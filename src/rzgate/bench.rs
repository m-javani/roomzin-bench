// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use crate::query::QueryConfig;
use crate::rzgate::client::HTTPClient;
use crate::rzgate::model::SearchAvailPayload;
use bytes::Bytes;
use serde_json::json;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Benchmark RzGate HTTP API with precise, open-loop constant rate load
///
/// Sends `requests` uniformly distributed over `duration` seconds.
/// Automatically uses as much concurrency as needed to sustain the target rate.
pub async fn benchmark_rzgate(
    url: String,
    token: String,
    connections: u32,
    duration: u32,
    query_config: Option<QueryConfig>,
) -> Result<(), Box<dyn Error>> {
    let connections = connections.max(1) as usize;
    let duration = duration.max(1) as usize;

    println!("Starting RzGate benchmark:");
    println!(" URL: {}", url);
    println!(" Total connections: {}", connections);
    println!(" duration: {} seconds", duration);
    println!();

    // === Build payload from QueryConfig ===
    let query_config = query_config.ok_or("query.yml is required for RzGate benchmark")?;
    let sa = query_config
        .search_avail
        .ok_or("query.yml must contain 'search_avail' section for RzGate benchmark")?;
    let segment = sa
        .segment
        .ok_or("search_avail.segment is required in query.yml")?;
    let room_type = sa
        .room_type
        .ok_or("search_avail.room_type is required in query.yml")?;
    let dates = sa
        .dates
        .ok_or("search_avail.dates is required in query.yml")?;
    let limit = sa.limit.unwrap_or(300);

    let payload = SearchAvailPayload {
        segment,
        room_type,
        area: sa.area.clone(),
        property_id: sa.property_id.clone(),
        property_type: sa.property_type.clone(),
        stars: sa.stars,
        category: sa.category.clone(),
        amenities: sa.amenities.clone().unwrap_or_default(),
        longitude: sa.longitude,
        latitude: sa.latitude,
        date: dates,
        availability: sa.availability,
        final_price: sa.final_price,
        rate_feature: sa.rate_feature.clone().unwrap_or_default(),
        limit: Some(limit as u64),
    };

    // === Preflight request ===
    println!("Running preflight request...");
    let start = Instant::now();
    let preflight_client = if url.contains("https://") {
        HTTPClient::new_http2_client(&url, &token)
    } else {
        HTTPClient::new_http_client(&url, &token)
    };
    let preflight_resp = preflight_client
        .search_avail(&payload)
        .await
        .map_err(|e| format!("Preflight failed: {e}"))?;

    let pretty_json = serde_json::to_string_pretty(&preflight_resp)?;
    let mut file = File::create("http.json")?;
    file.write_all(pretty_json.as_bytes())?;
    println!(
        "Preflight response saved to http.json. {:?}ms",
        start.elapsed().as_millis()
    );
    println!();

    // === Pre-marshal JSON wrapper ===
    let wrapper = json!({
        "command": "SEARCHAVAIL",
        "body": payload
    });
    let body_bytes = Bytes::from(serde_json::to_vec(&wrapper)?);

    // === Benchmark setup ===
    let (latency_tx, mut latency_rx) = mpsc::unbounded_channel::<(Duration, bool)>(); // (duration, success)

    let benchmark_start = Instant::now();

    // === Collect latencies ===
    let stat: Vec<(Duration, bool)> = Vec::with_capacity(connections * 1000);
    let results = Arc::new(tokio::sync::Mutex::new(stat));
    let results_clone = results.clone();
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();
    // Spawn one independent task per request
    let mut task_handles = Vec::with_capacity(connections);

    let recv_task = tokio::spawn(async move {
        while let Some(lat) = latency_rx.recv().await {
            results_clone.lock().await.push(lat);
            if benchmark_start.elapsed().as_secs() >= duration as u64 {
                cancel_token_clone.cancel();
            }
        }
    });

    // problem: with one connection the throuput is 25 but average latency is 2ms while
    // with 500 connections average latency is 7ms
    // it does not add up. it means we kill time here in bench
    for _conn_id in 1..=connections {
        // http client build latency: 26.24µs
        let client = if url.contains("https://") {
            HTTPClient::new_http2_client(&url, &token)
        } else {
            HTTPClient::new_http_client(&url, &token)
        };
        // request payload clone latency: 261ns
        let body_clone = body_bytes.clone();

        let latency_tx = latency_tx.clone();
        let cancel_token_clone = cancel_token.clone();
        let handle = tokio::spawn(async move {
            // Build request
            loop {
                if cancel_token_clone.is_cancelled() {
                    break;
                }

                // request build latency: 20.672µs
                let mut req = reqwest::Request::new(
                    reqwest::Method::POST,
                    format!("{}/api", client.base_url).parse().unwrap(),
                );
                req.headers_mut().insert(
                    reqwest::header::CONTENT_TYPE,
                    "application/json".parse().unwrap(),
                );
                if !client.token.is_empty() {
                    req.headers_mut().insert(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", client.token).parse().unwrap(),
                    );
                }
                *req.body_mut() = Some(reqwest::Body::from(body_clone.clone()));

                // Execute
                // server response takes less than 4ms
                // but throughput limited to 24 because of slow read
                let start = Instant::now();
                let response = client.client.execute(req).await;
                let latency = start.elapsed();

                let success = match response {
                    Ok(resp) => {
                        let status = resp.status();
                        // response read latency: ~40ms
                        // if we set client.http2_max_frame_size(2097152) // 2^24=256KB it becomes ~1-2ms
                        // let start = Instant::now();
                        let body = resp.bytes().await.unwrap_or_default(); // drain body
                        let _ = serde_json::to_string(&body.to_vec());

                        // println!("response read latency: {:?}\n", start.elapsed());
                        status.is_success()
                    }
                    Err(_) => false,
                };
                // latency: 3.982µs
                if let Err(_) = latency_tx.send((latency, success)) {
                    break;
                }
            }
        });

        task_handles.push(handle);
    }

    for task in task_handles {
        let _ = task.await;
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    let _ = recv_task.abort();

    // === Wait for all requests to complete ===
    let mut stat = results.lock().await.clone();
    stat.sort_unstable();

    let total_success = stat.iter().filter(|(_, success)| *success).count();

    let count = stat.len();
    if count == 0 {
        return Err("No latency data collected".into());
    }

    let min = stat[0].0;
    // let max = stat[count - 1].0;
    let p50 = stat[((count as f64) * 0.50) as usize].0;
    let p95 = stat[((count as f64) * 0.95) as usize].0;
    let p99 = stat[((count as f64) * 0.99) as usize].0;
    let sum: Duration = stat.iter().map(|(d, _)| d).copied().sum();
    let mean = sum / count as u32;

    // === Final Report ===
    println!("=== Benchmark Results ===");
    println!("Total requests sent: {}", count);
    println!("Successful (2xx): {}", total_success);
    println!("Failed: {}", count - total_success);
    println!("Total time: {:.2?}", duration);
    println!("Achieved RPS: {:.2}", count / duration);
    println!();
    println!("Latency statistics (ms):");
    println!(" Min: {:8.2} ms", min.as_micros() as f64 / 1000.0);
    println!(" P50: {:8.2} ms", p50.as_micros() as f64 / 1000.0);
    println!(" P95: {:8.2} ms", p95.as_micros() as f64 / 1000.0);
    println!(" P99: {:8.2} ms", p99.as_micros() as f64 / 1000.0);
    // println!(" Max: {:8.2} ms", max.as_micros() as f64 / 1000.0);
    println!(" Mean: {:8.2} ms", mean.as_micros() as f64 / 1000.0);

    Ok(())
}

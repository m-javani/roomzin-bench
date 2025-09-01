// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

// src/rzproxy/bench.rs  (or wherever this lives)
use crate::query::QueryConfig;
use crate::rzproxy::client::HTTPClient;
use crate::rzproxy::model::SearchAvailPayload;
use bytes::Bytes;
use serde_json::json;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Benchmark RzProxy HTTP API with precise, open-loop constant rate load
pub async fn benchmark_rzproxy(
    url: String,
    connections: u32,
    duration: u32,
    query_config: Option<QueryConfig>,
) -> Result<(), Box<dyn Error>> {
    let connections = connections.max(1) as usize;
    let duration = duration.max(1) as usize;

    println!("Starting RzProxy benchmark:");
    println!(" URL: {}", url);
    println!(" Total connections: {}", connections);
    println!(" duration: {} seconds", duration);
    println!();

    // === Load queries (new format) ===
    let query_config = query_config.ok_or("query.yml is required for RzProxy benchmark")?;
    if query_config.queries.is_empty() {
        return Err("query.yml contains no queries".into());
    }

    // Helper: turn a SearchRequest into the HTTP payload
    fn to_payload(sa: &crate::query::SearchRequest) -> Result<SearchAvailPayload, Box<dyn Error>> {
        let segment = sa
            .segment
            .clone()
            .ok_or("query is missing required field 'segment'")?;
        let room_type = sa
            .room_type
            .clone()
            .ok_or("query is missing required field 'room_type'")?;
        let dates = sa
            .dates
            .clone()
            .ok_or("query is missing required field 'dates'")?;

        Ok(SearchAvailPayload {
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
            limit: Some(sa.limit.unwrap_or(300) as u64),
        })
    }

    // === Preflight with the first query ===
    println!("Running preflight request (first query)...");
    let start = Instant::now();
    let preflight_payload = to_payload(&query_config.queries[0])?;

    let preflight_client = if url.contains("https://") {
        HTTPClient::new_http2_client(&url)
    } else {
        HTTPClient::new_http_client(&url)
    };

    let preflight_resp = preflight_client
        .search_avail(&preflight_payload)
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

    // === Pre-marshal one JSON body per query (so we can cycle) ===
    let mut bodies: Vec<Bytes> = Vec::with_capacity(query_config.queries.len());
    for q in &query_config.queries {
        let payload = to_payload(q)?;
        let wrapper = json!({
            "command": "SEARCHAVAIL",
            "segment": payload.segment.clone(),
            "body": payload
        });
        bodies.push(Bytes::from(serde_json::to_vec(&wrapper)?));
    }
    let bodies = Arc::new(bodies);

    // === Benchmark setup ===
    let (latency_tx, mut latency_rx) = mpsc::unbounded_channel::<(Duration, bool)>();
    let benchmark_start = Instant::now();

    let results = Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(
        connections * 1000,
    )));
    let results_clone = results.clone();
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    let recv_task = tokio::spawn(async move {
        while let Some(lat) = latency_rx.recv().await {
            results_clone.lock().await.push(lat);
            if benchmark_start.elapsed().as_secs() >= duration as u64 {
                cancel_token_clone.cancel();
            }
        }
    });

    let mut task_handles = Vec::with_capacity(connections);

    for conn_id in 0..connections {
        let client = if url.contains("https://") {
            HTTPClient::new_http2_client(&url)
        } else {
            HTTPClient::new_http_client(&url)
        };

        let bodies = bodies.clone();
        let latency_tx = latency_tx.clone();
        let cancel_token_clone = cancel_token.clone();

        let handle = tokio::spawn(async move {
            let mut req_idx = conn_id; // start each connection at a different offset

            loop {
                if cancel_token_clone.is_cancelled() {
                    break;
                }

                let body = bodies[req_idx % bodies.len()].clone();
                req_idx += 1;

                let mut req = reqwest::Request::new(
                    reqwest::Method::POST,
                    format!("{}/api", client.base_url).parse().unwrap(),
                );
                req.headers_mut().insert(
                    reqwest::header::CONTENT_TYPE,
                    "application/json".parse().unwrap(),
                );
                *req.body_mut() = Some(reqwest::Body::from(body));

                let start = Instant::now();
                let response = client.client.execute(req).await;
                let latency = start.elapsed();

                let success = match response {
                    Ok(resp) => {
                        let status = resp.status();
                        let _ = resp.bytes().await; // drain body
                        status.is_success()
                    }
                    Err(_) => false,
                };

                if latency_tx.send((latency, success)).is_err() {
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

    // === Stats ===
    let mut stat = results.lock().await.clone();
    stat.sort_unstable();
    let total_success = stat.iter().filter(|(_, success)| *success).count();
    let count = stat.len();

    if count == 0 {
        return Err("No latency data collected".into());
    }

    let min = stat[0].0;
    let p50 = stat[((count as f64) * 0.50) as usize].0;
    let p95 = stat[((count as f64) * 0.95) as usize].0;
    let p99 = stat[((count as f64) * 0.99) as usize].0;
    let sum: Duration = stat.iter().map(|(d, _)| *d).sum();
    let mean = sum / count as u32;

    println!("=== Benchmark Results ===");
    println!("Total requests sent: {}", count);
    println!("Successful (2xx): {}", total_success);
    println!("Failed: {}", count - total_success);
    println!("Total time: {:.2?}", duration);
    println!("Achieved RPS: {:.2}", count as f64 / duration as f64);
    println!();
    println!("Latency statistics (ms):");
    println!(" Min: {:8.2} ms", min.as_micros() as f64 / 1000.0);
    println!(" P50: {:8.2} ms", p50.as_micros() as f64 / 1000.0);
    println!(" P95: {:8.2} ms", p95.as_micros() as f64 / 1000.0);
    println!(" P99: {:8.2} ms", p99.as_micros() as f64 / 1000.0);
    println!(" Mean: {:8.2} ms", mean.as_micros() as f64 / 1000.0);

    Ok(())
}

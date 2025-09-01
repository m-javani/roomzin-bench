// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use ahash::AHashMap;
use std::error::Error;

use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, split};
use tokio::net::TcpStream;
use tokio::sync::{Barrier, broadcast, mpsc};
use tokio::time::{Duration, sleep, timeout};

use crate::codecs::Codecs;
use crate::config::Config;
use crate::error::CacheError;
use crate::model::SearchAvailRequest;
use crate::parser::deserialize_get_codecs;
use crate::persist::persist_response;
use crate::serializer::{serialize_fetch_codecs, serialize_login};

pub async fn run_connection(
    conn_id: usize,
    addr: String,
    token: String,
    events: Vec<(u32, u32, u64)>, // (clrid, second, offset_micros)
    payloads: Arc<Vec<Vec<u8>>>,
    barrier: Arc<Barrier>,
    write_tx: mpsc::UnboundedSender<(u32, Instant)>,
    read_tx: mpsc::UnboundedSender<(u32, Instant, bool, Option<bytes::Bytes>)>,
    mut cancel_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    let stream = timeout(Duration::from_secs(2), TcpStream::connect(&addr)).await??;

    stream.set_nodelay(true)?;
    let (mut reader, mut writer) = split(stream);

    // login
    let login_data = serialize_login(&token);
    writer.write_all(&login_data).await?;
    let mut buf = [0u8; 32];
    let n = reader.read(&mut buf).await?;

    // Check login response
    if n == 0 || &buf[..n] != b"LOGIN OK" {
        // Still wait on barrier to avoid deadlock, but then return error
        barrier.wait().await;
        return Err(format!("Connection {} login failed", conn_id).into());
    }

    // Signal that this connection is ready
    timeout(Duration::from_secs(5), barrier.wait())
        .await
        .map_err(|_| "Barrier wait timeout: connections not ready within 5 seconds")?;

    // Writer
    let wtx = write_tx.clone();
    let pw = payloads.clone();
    let writer_task = tokio::spawn(async move {
        let t0 = Instant::now();

        for (_i, (clrid, sec, offset)) in events.iter().enumerate() {
            let deadline = t0 + Duration::from_secs(*sec as u64) + Duration::from_micros(*offset);
            let now = Instant::now();

            if deadline > now {
                let sleep_duration = deadline - now;
                sleep(sleep_duration).await;
            }

            if writer.write_all(&pw[*clrid as usize]).await.is_ok() {
                let _ = wtx.send((*clrid, Instant::now()));
            } else {
                // println!("Connection {} failed to send request {}", conn_id, i);
            }
        }
    });

    // Reader
    let rtx = read_tx.clone();

    let reader_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        let mut pending = Vec::new();

        loop {
            tokio::select! {
                // Original read operation
                result = reader.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            // println!("Connection {} reader got EOF", conn_id);
                            break;
                        }
                        Ok(n) => {
                            // println!("Connection {} reader got {} bytes", conn_id, n);
                            pending.extend_from_slice(&buf[..n]);
                        }
                        Err(_e) => {
                            // println!("Connection {} reader error: {}", conn_id, e);
                            break;
                        }
                    }
                }
                // Cancellation signal - ADD THIS
                _ = cancel_rx.recv() => {
                    // println!("Connection {} reader received cancellation signal", conn_id);
                    break;
                }
            }

            while pending.len() >= 9 && pending[0] == 0xFF {
                let clrid = u32::from_le_bytes([pending[1], pending[2], pending[3], pending[4]]);
                let total_len =
                    u32::from_le_bytes([pending[5], pending[6], pending[7], pending[8]]) as usize;
                let frame_len = 9 + total_len;

                if pending.len() < frame_len {
                    // println!(
                    //     "Connection {} reader: incomplete frame (have {}, need {})",
                    //     conn_id,
                    //     pending.len(),
                    //     frame_len
                    // );
                    break;
                }

                let now = Instant::now();
                let payload = &pending[9..frame_len];
                let name_len = payload[0] as usize;

                if payload.len() < 1 + name_len {
                    // println!("Connection {} reader: malformed payload", conn_id);
                    pending.splice(..frame_len, []);
                    continue;
                }

                let msg_type = &payload[1..1 + name_len];
                let success = msg_type == b"SUCCESS";
                let payload_opt = if success {
                    Some(bytes::Bytes::copy_from_slice(&pending[0..frame_len]))
                    // None
                } else {
                    Some(bytes::Bytes::copy_from_slice(&pending[0..frame_len]))
                };

                // println!(
                //     "Connection {} reader got response for clrid {}",
                //     conn_id, clrid
                // );
                let _ = rtx.send((clrid, now, success, payload_opt));
                pending.splice(..frame_len, []);
            }
        }
        // println!("Connection {} reader exited", conn_id);
    });

    let _ = writer_task.await;
    let _ = reader_task.await;

    Ok(())
}

pub fn calculate_stats(
    write_map: &AHashMap<u32, Instant>,
    read_map: &AHashMap<u32, (Instant, bool, Option<bytes::Bytes>)>,
    total_requests: usize,
    config: Config,
    codecs: Arc<Codecs>,
) {
    let num_connections = config.connections;
    let duration_secs = config.duration_secs;
    let spike_enabled = config.spike_enabled;

    let mut latencies: Vec<u128> = Vec::new();
    let mut errors = 0usize;
    let mut per_second: AHashMap<u32, (Vec<u128>, usize)> = AHashMap::new();

    if spike_enabled {
        // For spike mode: use time-based bucketing
        let benchmark_start = write_map.values().min().cloned().unwrap_or(Instant::now());

        for (clrid, send_time) in write_map {
            if let Some((recv_time, success, _)) = read_map.get(clrid) {
                let latency = recv_time.duration_since(*send_time).as_micros();

                // Calculate which second this request belongs to based on actual time
                let time_offset_secs = send_time.duration_since(benchmark_start).as_secs() as u32;
                let sec = time_offset_secs.min(duration_secs - 1);

                let entry = per_second.entry(sec).or_insert_with(|| (Vec::new(), 0));
                entry.0.push(latency);
                latencies.push(latency);

                if !*success {
                    errors += 1;
                    entry.1 += 1;
                }
            }
        }
    } else {
        // For regular mode: use count-based bucketing
        let total_requests_u32 = total_requests as u32;
        let base_per_sec = total_requests_u32 / duration_secs;
        let rem_per_sec = total_requests_u32 % duration_secs;

        let expected_per_second: Vec<u32> = (0..duration_secs)
            .map(|sec| {
                if sec < rem_per_sec {
                    base_per_sec + 1
                } else {
                    base_per_sec
                }
            })
            .collect();

        // Sort requests by send time
        let mut sorted_requests: Vec<(u32, Instant)> = write_map
            .iter()
            .map(|(&clrid, &time)| (clrid, time))
            .collect();
        sorted_requests.sort_by_key(|&(_, time)| time);

        let mut current_second = 0;
        let mut requests_in_current_second = 0;
        let mut saved_response = false;

        for (clrid, send_time) in sorted_requests {
            if let Some((recv_time, success, payload_opt)) = read_map.get(&clrid) {
                let latency = recv_time.duration_since(send_time).as_micros();

                // persist results
                if !saved_response && *success {
                    if let Some(payload_bytes) = payload_opt {
                        // deserialize
                        saved_response = persist_response(
                            clrid,
                            &config.command_type,
                            payload_bytes.as_ref(),
                            codecs.clone(),
                        )
                    }
                }

                let entry = per_second
                    .entry(current_second)
                    .or_insert_with(|| (Vec::new(), 0));
                entry.0.push(latency);
                latencies.push(latency);

                if !*success {
                    errors += 1;
                    entry.1 += 1;
                }

                requests_in_current_second += 1;

                // Move to next second if we've reached the expected count
                if requests_in_current_second
                    >= expected_per_second[current_second as usize] as usize
                    && current_second < duration_secs - 1
                {
                    current_second += 1;
                    requests_in_current_second = 0;
                }
            }
        }
    }

    // Rest of your display code remains the same...
    println!("\n--- Stats Per Second ----\n");
    println!("Second    RPS     Min (ms)    Max (ms)    Error (%)");
    println!("-------  -----  ----------  ----------  ----------");

    // Sort seconds for consistent output
    let mut sorted_seconds: Vec<_> = per_second.keys().collect();
    sorted_seconds.sort();

    for sec in sorted_seconds {
        let (vals, sec_errors) = &per_second[sec];
        if vals.is_empty() {
            continue;
        }

        let rps = vals.len();
        let min = *vals.iter().min().unwrap_or(&0) as f64 / 1000.0;
        let max = *vals.iter().max().unwrap_or(&0) as f64 / 1000.0;
        let err_rate = (*sec_errors as f64 / rps as f64) * 100.0;

        println!(
            "{:<7}   {:<5}   {:<10.3}   {:<10.3}   {:<10.2}",
            sec + 1,
            rps,
            min,
            max,
            err_rate
        );
    }

    if latencies.is_empty() {
        println!("No successful responses recorded.");
        return;
    }

    latencies.sort();
    let total = latencies.len();
    let p = |percent: f64| {
        let index = (total as f64 * percent).ceil() as usize - 1;
        latencies[index.min(total - 1)]
    };

    println!("\n--- Benchmark Summary ---\n");
    println!("Connections:   {:<6}", num_connections);
    println!("Total requests:    {:<6}", total_requests);
    println!("success: {:<6}", total);
    println!("errors:  {:<6}", errors);
    println!("");
    println!("Min:           {:<6.3}ms", latencies[0] as f64 / 1000.0);
    println!("P50:           {:<6.3}ms", p(0.5) as f64 / 1000.0);
    println!("P95:           {:<6.3}ms", p(0.95) as f64 / 1000.0);
    println!("P99:           {:<6.3}ms", p(0.99) as f64 / 1000.0);
    println!("P99.9:         {:<6.3}ms", p(0.999) as f64 / 1000.0);
    println!(
        "Max:           {:<6.3}ms",
        latencies[total - 1] as f64 / 1000.0
    );
    println!("\n====================================================\n");
}

/// Scheduler module for deterministic, precomputed benchmark schedules.

#[derive(Debug, Clone)]
pub struct ConnEvent {
    /// Global connection id
    pub clrid: usize,
    /// second index relative to benchmark start (0..duration_secs-1)
    pub second: u32,
    /// delay from the start of that second in microseconds (0..1_000_000)
    pub start_offset_us: u64,
    /// number of requests to send in this event
    pub req_count: u32,
    /// gap between consecutive requests in microseconds within this event
    /// If 0, send back-to-back.
    pub gap_us: u64,
}

#[derive(Debug, Clone)]
pub struct ConnectionPlan {
    pub conn_id: usize,
    pub events: Vec<ConnEvent>, // sorted by (second, offset)
}

pub type Schedule = Vec<ConnectionPlan>;

/// Build a deterministic, count-based schedule.
pub fn build_count_based_schedule(
    connections: usize,
    requests_per_connection: u32,
    duration_secs: u32,
) -> Schedule {
    assert!(connections > 0, "connections must be > 0");
    assert!(duration_secs > 0, "duration_secs must be > 0");

    let total_requests: u128 = (connections as u128) * (requests_per_connection as u128);

    let base_per_sec = (total_requests / duration_secs as u128) as u64;
    let rem_per_sec = (total_requests % duration_secs as u128) as u32;

    let mut plans: Vec<ConnectionPlan> = (0..connections)
        .map(|i| ConnectionPlan {
            conn_id: i,
            events: Vec::new(),
        })
        .collect();

    for sec in 0..duration_secs {
        let mut sec_total = base_per_sec as u32;
        if (sec as u32) < rem_per_sec {
            sec_total += 1;
        }

        if sec_total == 0 {
            continue;
        }

        let base_per_conn = sec_total as usize / connections;
        let rem_per_conn = sec_total as usize % connections;

        for conn in 0..connections {
            let mut assigned = base_per_conn as u32;
            if conn < rem_per_conn {
                assigned += 1;
            }
            if assigned == 0 {
                continue;
            }

            let offset_micros = ((conn as u128) * 1_000_000u128 / (connections as u128)) as u64;
            let gap_micros = if assigned > 0 {
                1_000_000u64 / (assigned as u64)
            } else {
                0
            };

            plans[conn].events.push(ConnEvent {
                clrid: conn,
                second: sec,
                start_offset_us: offset_micros,
                req_count: assigned,
                gap_us: gap_micros,
            });
        }
    }

    for p in plans.iter_mut() {
        p.events.sort_by_key(|e| (e.second, e.start_offset_us));
    }

    plans
}

/// Build a spike schedule.
pub fn build_spike_schedule(connections: usize, spike_reqs: u32) -> Schedule {
    assert!(connections > 0, "connections must be > 0");
    assert!(spike_reqs > 0, "spike_reqs must be > 0");

    let mut plans: Vec<ConnectionPlan> = (0..connections)
        .map(|i| ConnectionPlan {
            conn_id: i,
            events: Vec::new(),
        })
        .collect();

    let sec = 0;

    let base_per_conn = (spike_reqs as usize) / connections;
    let rem_per_conn = (spike_reqs as usize) % connections;

    for conn in 0..connections {
        let mut assigned = base_per_conn as u32;
        if conn < rem_per_conn {
            assigned += 1;
        }
        if assigned == 0 {
            continue;
        }

        // CHANGE: Use the same start offset for ALL connections to make them concurrent
        let offset_micros = 0; // All start at the same time

        plans[conn].events.push(ConnEvent {
            clrid: conn,
            second: sec,
            start_offset_us: offset_micros,
            req_count: assigned,
            gap_us: 0, // back-to-back in spikes
        });
    }

    for p in plans.iter_mut() {
        p.events.sort_by_key(|e| (e.second, e.start_offset_us));
    }

    plans
}

pub async fn validate_search_query(
    config: &Config,
    req: &SearchAvailRequest,
    codecs: Arc<Codecs>,
) -> Result<(), CacheError> {
    use crate::serializer::serialize_login;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        time::{Duration, timeout},
    };

    let stream = timeout(
        Duration::from_secs(5),
        TcpStream::connect(config.server_addr()),
    )
    .await
    .map_err(|_| CacheError::Validation("Connection timeout".into()))?
    .map_err(|e| CacheError::Validation(format!("Connection failed: {e}")))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Login
    writer
        .write_all(&serialize_login(&config.token))
        .await
        .map_err(|e| CacheError::Validation(format!("Login failed: {e}")))?;
    let mut buf = [0u8; 32];
    let n = reader
        .read(&mut buf)
        .await
        .map_err(|_| CacheError::Validation("Login read failed".into()))?;
    if n == 0 || &buf[..n] != b"LOGIN OK" {
        return Err(CacheError::Validation("Login failed".into()));
    }

    // Send one request
    let payload = crate::serializer::serialize_search_avail(req.clone(), 9999, codecs.clone())?;
    writer
        .write_all(&payload)
        .await
        .map_err(|_| CacheError::Validation("Send failed".into()))?;

    // Read response frame
    let mut header = [0u8; 9];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|_| CacheError::Validation("No response".into()))?;
    if header[0] != 0xFF {
        return Err(CacheError::Validation("Invalid frame".into()));
    }
    let _clrid = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
    let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;

    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| CacheError::Validation("Incomplete payload".into()))?;

    let name_len = body[0] as usize;
    let msg_type = std::str::from_utf8(&body[1..1 + name_len]).unwrap_or("");
    if msg_type != "SUCCESS" {
        let error_msg = String::from_utf8_lossy(&body[1 + name_len..]).to_string();
        return Err(CacheError::Validation(format!(
            "Server rejected query: {msg_type} — {error_msg}"
        )));
    }
    Ok(())
}

fn load_codecs_from_file(path: &str) -> Result<Codecs, CacheError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| CacheError::Validation(format!("Failed to read {}: {}", path, e)))?;

    let yaml: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| CacheError::Validation(format!("Invalid YAML: {}", e)))?;

    let rate_features = yaml["rate_features"]
        .as_sequence()
        .ok_or_else(|| CacheError::Validation("Missing 'rate_features' list".into()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

    Ok(Codecs { rate_features })
}

pub async fn fetch_codecs(
    config: &Config,
    codecs_path: Option<String>,
) -> Result<Codecs, CacheError> {
    // 1. If codecs_path is provided, try to load from file first
    if let Some(path) = codecs_path {
        if let Ok(codecs) = load_codecs_from_file(&path) {
            return Ok(codecs);
        } else {
            // Log warning but continue to try server
            eprintln!(
                "Warning: Could not load codecs from {}, falling back to server",
                path
            );
        }
    }

    use crate::serializer::serialize_login;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        time::{Duration, timeout},
    };

    let stream = timeout(
        Duration::from_secs(5),
        TcpStream::connect(config.server_addr()),
    )
    .await
    .map_err(|_| CacheError::Validation("Connection timeout".into()))?
    .map_err(|e| CacheError::Validation(format!("Connection failed: {e}")))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Login
    writer
        .write_all(&serialize_login(&config.token))
        .await
        .map_err(|e| CacheError::Validation(format!("Login failed: {e}")))?;
    let mut buf = [0u8; 32];
    let n = reader
        .read(&mut buf)
        .await
        .map_err(|_| CacheError::Validation("Login read failed".into()))?;
    if n == 0 || &buf[..n] != b"LOGIN OK" {
        return Err(CacheError::Validation("Login failed".into()));
    }

    // Send GETCODECS request
    let payload = serialize_fetch_codecs(9999);
    writer
        .write_all(&payload)
        .await
        .map_err(|e| CacheError::Validation(format!("Failed to send GETCODECS: {e}")))?;

    // Read response frame
    let mut header = [0u8; 9];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|_| CacheError::Validation("No response".into()))?;

    if header[0] != 0xFF {
        return Err(CacheError::Validation("Invalid frame".into()));
    }

    let clrid = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
    let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;

    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| CacheError::Validation("Incomplete payload".into()))?;

    // Combine header + body for parse_response
    let mut full_response = Vec::with_capacity(9 + len);
    full_response.extend_from_slice(&header);
    full_response.extend_from_slice(&body);

    // Deserialize the response
    let rate_features = deserialize_get_codecs(&full_response, clrid)?;

    Ok(Codecs { rate_features })
}

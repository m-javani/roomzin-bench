// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

pub mod codecs;
pub mod command;
pub mod config;
pub mod error;
pub mod generate;
pub mod model;
pub mod parser;
pub mod persist;
pub mod query;
pub mod roomzin;
pub mod rzgate;
pub mod serializer;

use crate::command::get_serialized_commands;
use crate::config::Config;
use crate::query::QueryConfig;
use crate::roomzin::{
    build_count_based_schedule, build_spike_schedule, calculate_stats, fetch_codecs,
    run_connection, validate_search_query,
};
use crate::rzgate::bench::benchmark_rzgate;
use ahash::AHashMap;
use clap::{Parser, Subcommand};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Barrier, broadcast, mpsc};
use tokio::time::{Duration, timeout}; // New

#[derive(Parser)]
#[command(author, version, about = "Benchmark tool for Roomzin and RzGate")]
struct Cli {
    #[command(subcommand)]
    target: Target,
}

#[derive(Subcommand)]
enum Target {
    /// Generate test dataset (properties.csv + packages.csv)
    Generate {
        #[arg(long, default_value_t = 2)]
        segments: usize,

        #[arg(long, default_value_t = 2500)]
        props_per_segment: usize,

        #[arg(long, default_value_t = 10)]
        room_types: usize,

        #[arg(long, default_value_t = 60)]
        days: usize,

        #[arg(long, default_value = "./config")]
        config_dir: String,

        #[arg(long, default_value = "./data")]
        data_dir: String,

        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Benchmark Roomzin directly (TCP/binary protocol)
    #[command(name = "roomzin")]
    Roomzin {
        #[command(subcommand)]
        mode: RoomzinMode,
    },
    /// Benchmark RzGate (HTTP/JSON gateway)
    #[command(name = "rzgate")]
    RzGate {
        /// Base URL of RzGate (e.g. http://localhost:8777)
        #[arg(long, default_value = "http://0.0.0.0:8777")]
        url: String,

        /// Authorization token
        #[arg(short = 't', long, default_value = "rzgate123")]
        token: String,

        /// Number of concurrent connections
        #[arg(short = 'c', long, default_value_t = 10)]
        connections: usize,

        /// Benchmark duration in seconds
        #[arg(long, default_value_t = 60)]
        duration: u32,

        /// Path to dataset directory containing query.yml
        #[arg(short = 'd', long, default_value = "./data")]
        data_dir: String,

        #[arg(long, default_value = "./queries")]
        queries_dir: String,
    },
}

#[derive(Subcommand)]
enum RoomzinMode {
    /// Regular benchmark mode with evenly distributed load
    Regular {
        #[command(subcommand)]
        bench_type: BenchType,
        /// Path to dataset directory (codecs.yml + query.yml)
        #[arg(short = 'd', long, default_value = "./data")]
        data_dir: String,
        /// Path to codecs.yml file (optional, overrides codecs in data_dir)
        #[arg(long)]
        codecs: Option<String>,
        #[arg(long, default_value = "./queries")]
        queries_dir: String,
        /// Server host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Server port
        #[arg(long, default_value_t = 7777)]
        port: u16,
        /// Authentication token
        #[arg(short = 't', long)]
        token: String,
        /// Concurrent connections
        #[arg(short = 'c', long, default_value_t = 50)]
        connections: usize,
        /// Total requests to send
        #[arg(short = 'n', long, default_value_t = 100)]
        requests: u32,
        /// Benchmark duration in seconds
        #[arg(long, default_value_t = 1)]
        duration: u32,
    },
    /// Spike / burst mode
    Spike {
        #[command(subcommand)]
        bench_type: BenchType,
        #[arg(short = 'd', long, default_value = "./data")]
        data_dir: String,
        #[arg(long, default_value = "./queries")]
        queries_dir: String,
        /// Path to codecs.yml file (optional, overrides codecs in data_dir)
        #[arg(long)]
        codecs: Option<String>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 7777)]
        port: u16,
        #[arg(short = 't', long)]
        token: String,
        #[arg(short = 'c', long, default_value_t = 50)]
        connections: usize,
        #[arg(long, default_value_t = 2000)]
        spike_reqs: u32,
        #[arg(long, default_value_t = 10)]
        duration: u32,
    },
}

#[derive(Subcommand)]
enum BenchType {
    Search,
    Update,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.target {
        Target::Generate {
            segments,
            props_per_segment,
            room_types,
            days,
            config_dir,
            data_dir,
            seed,
        } => {
            generate::run(
                segments,
                props_per_segment,
                room_types,
                days,
                &config_dir,
                &data_dir,
                seed,
            )?;
            println!("✅ Dataset generation completed successfully!");
            Ok(())
        }
        Target::RzGate {
            url,
            token,
            connections,
            duration,
            data_dir: _,
            queries_dir,
        } => {
            // Load query config
            let query_config = QueryConfig::load(&format!("{}/query.yml", queries_dir))?;

            // Call the fully self-contained RzGate benchmark
            benchmark_rzgate(url, token, connections as u32, duration, query_config).await?;

            Ok(())
        }

        Target::Roomzin { mode } => {
            // === Everything below is the original Roomzin logic, only slightly restructured ===

            let (config, query_config, _data_dir) = match mode {
                RoomzinMode::Regular {
                    bench_type,
                    data_dir,
                    host,
                    port,
                    token,
                    connections,
                    requests,
                    duration,
                    codecs,
                    queries_dir,
                } => {
                    let command_type = match bench_type {
                        BenchType::Search => "search_avail",
                        BenchType::Update => "set_room_avl",
                    };
                    let cfg = Config {
                        host,
                        port,
                        token,
                        command_type: command_type.to_string(),
                        connections,
                        duration_secs: duration,
                        requests: Some(requests),
                        spike_enabled: false,
                        spike_reqs: None,
                        codecs_path: codecs,
                        data_dir: data_dir.clone(),
                    };
                    let qc = QueryConfig::load(&format!("{}/query.yml", queries_dir))?;
                    (cfg, qc, data_dir)
                }
                RoomzinMode::Spike {
                    bench_type,
                    data_dir,
                    host,
                    port,
                    token,
                    connections,
                    spike_reqs,
                    duration,
                    codecs,
                    queries_dir,
                } => {
                    let command_type = match bench_type {
                        BenchType::Search => "search_avail",
                        BenchType::Update => "set_room_avl",
                    };
                    let cfg = Config {
                        host,
                        port,
                        token,
                        command_type: command_type.to_string(),
                        connections,
                        duration_secs: duration,
                        requests: None,
                        spike_enabled: true,
                        spike_reqs: Some(spike_reqs),
                        codecs_path: codecs,
                        data_dir: data_dir.clone(),
                    };
                    let qc = QueryConfig::load(&format!("{}/query.yml", queries_dir))?;
                    (cfg, qc, data_dir)
                }
            };

            config.validate();

            // Preflight fetch codecs
            let codecs = match fetch_codecs(&config, config.codecs_path.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("fetching codecs failed: {e}");
                    return Ok(());
                }
            };
            let codecs = Arc::new(codecs);

            // Pre-flight validation for search_avail
            if config.command_type == "search_avail" {
                if let Some(qc) = &query_config {
                    if let Some(base_req) = qc.clone().into_search_request(codecs.clone())? {
                        println!("Validating query.yml against server...");
                        if let Err(e) =
                            validate_search_query(&config, &base_req, codecs.clone()).await
                        {
                            eprintln!("Query validation failed: {e}");
                            return Ok(());
                        } else {
                            println!("Query is valid");
                        }
                    }
                }
            }

            let _ = std::fs::remove_file("responses.json");
            println!("\n====================================================\n");

            // Build schedule
            let schedule = if config.spike_enabled {
                build_spike_schedule(config.connections, config.spike_reqs.unwrap())
            } else {
                let requests_per_connection = config.requests.unwrap() / config.connections as u32;
                build_count_based_schedule(
                    config.connections,
                    requests_per_connection,
                    config.duration_secs,
                )
            };

            let total_requests: usize = schedule
                .iter()
                .map(|plan| {
                    plan.events
                        .iter()
                        .map(|e| e.req_count as usize)
                        .sum::<usize>()
                })
                .sum();

            println!("> Scheduling requests...");

            // Pre-serialize all payloads
            let payloads = get_serialized_commands(
                total_requests,
                &config.command_type,
                codecs.clone(),
                query_config,
            )?;
            let payloads = Arc::new(payloads);

            // Channels
            let (write_tx, mut write_rx) = mpsc::unbounded_channel::<(u32, Instant)>();
            let (read_tx, mut read_rx) =
                mpsc::unbounded_channel::<(u32, Instant, bool, Option<bytes::Bytes>)>();

            // Consumers for latency maps
            let write_consumer = tokio::spawn(async move {
                let mut map: AHashMap<u32, Instant> = AHashMap::new();
                while let Some((clrid, t)) = write_rx.recv().await {
                    map.insert(clrid, t);
                }
                map
            });

            let read_consumer = tokio::spawn(async move {
                let mut map: AHashMap<u32, (Instant, bool, Option<bytes::Bytes>)> = AHashMap::new();
                while let Some((clrid, t, ok, payload)) = read_rx.recv().await {
                    map.insert(clrid, (t, ok, payload));
                }
                map
            });

            let barrier = Arc::new(Barrier::new(config.connections + 1));
            let (cancel_tx, _) = broadcast::channel(1);

            // Spawn connections
            let mut handles = vec![];
            let mut next_clrid: u32 = 0;
            for plan in schedule {
                let cancel_rx = cancel_tx.subscribe();
                let addr = config.server_addr();
                let barrier = barrier.clone();
                let write_tx = write_tx.clone();
                let read_tx = read_tx.clone();
                let payloads = payloads.clone();
                let token = config.token.clone();
                let conn_id = plan.conn_id;
                let mut expanded_events = Vec::new();
                for ev in plan.events {
                    for i in 0..ev.req_count {
                        expanded_events.push((
                            next_clrid,
                            ev.second,
                            ev.start_offset_us + (i as u64 * ev.gap_us),
                        ));
                        next_clrid += 1;
                    }
                }
                let handle = tokio::spawn(async move {
                    match run_connection(
                        conn_id,
                        addr,
                        token,
                        expanded_events,
                        payloads,
                        barrier,
                        write_tx,
                        read_tx,
                        cancel_rx,
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(e) => eprintln!("Connection {} failed: {}", conn_id, e),
                    }
                });
                handles.push(handle);
            }

            // Wait for all connections to connect
            timeout(Duration::from_secs(5), barrier.wait())
                .await
                .map_err(|_| "Timeout: not all connections ready within 5s")?;

            println!("> Opening connections...");
            println!("> Sending requests...");

            tokio::time::sleep(Duration::from_secs((config.duration_secs + 2) as u64)).await;

            // Shutdown
            let _ = cancel_tx.send(());
            for h in handles {
                let _ = h.await;
            }

            drop(write_tx);
            drop(read_tx);

            let write_map = write_consumer.await.unwrap();
            let read_map = read_consumer.await.unwrap();

            calculate_stats(
                &write_map,
                &read_map,
                total_requests,
                config,
                codecs.clone(),
            );

            Ok(())
        }
    }
}

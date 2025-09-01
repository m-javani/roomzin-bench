// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub command_type: String,
    pub connections: usize,
    pub duration_secs: u32,
    pub requests: Option<u32>,
    pub spike_enabled: bool,
    pub spike_reqs: Option<u32>,
    pub codecs_path: Option<String>,
    pub data_dir: String,
}

impl Config {
    pub fn validate(&self) {
        assert!(self.connections > 0, "connections must be > 0");
        assert!(self.duration_secs > 0, "duration_secs must be > 0");
        if self.spike_enabled {
            assert!(self.spike_reqs.unwrap_or(0) > 0, "spike_reqs must be > 0");
        } else {
            assert!(self.requests.unwrap_or(0) > 0, "requests must be > 0");
        }
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt, time::Duration};

use crate::rzgate::model::{PropertyAvail, SearchAvailPayload, SearchAvailResponse};

#[derive(Debug, Clone)]
pub struct HTTPClient {
    pub base_url: String,
    pub token: String,
    pub client: Client,
}

impl fmt::Display for HTTPClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HTTPClient(base_url: {}, has_token: {})",
            self.base_url,
            !self.token.is_empty()
        )
    }
}

impl HTTPClient {
    pub fn new_http2_client(base_url: &str, token: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .danger_accept_invalid_certs(true) // REMOVE in production!
            .http2_prior_knowledge()
            .pool_max_idle_per_host(100)
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_max_frame_size(2097152) // 2^24=256KB
            .build()
            .expect("Failed to create HTTP client");

        HTTPClient {
            base_url: base_url.to_string(),
            token: token.to_string(),
            client,
        }
    }
    pub fn new_http_client(base_url: &str, token: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        HTTPClient {
            base_url: base_url.to_string(),
            token: token.into(),
            client,
        }
    }

    pub async fn do_request<B, R>(
        &self,
        cmd: &str,
        payload: &B,
    ) -> Result<R, Box<dyn std::error::Error>>
    where
        B: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let wrapper = json!({
            "command": cmd,
            "body": payload
        });

        let mut request = self
            .client
            .post(format!("{}/api", self.base_url))
            .header("Content-Type", "application/json")
            .json(&wrapper);

        request = request.bearer_auth(self.token.clone());

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(format!("HTTP error {}: {}", status, error_body).into());
        }

        let result: R = response.json().await?;
        Ok(result)
    }

    pub async fn search_avail(
        &self,
        payload: &SearchAvailPayload,
    ) -> Result<Vec<PropertyAvail>, Box<dyn std::error::Error>> {
        let response: SearchAvailResponse = self.do_request("SEARCHAVAIL", payload).await?;

        if response.status == "error" {
            return Err(response
                .message
                .unwrap_or_else(|| "Server returned error".to_string())
                .into());
        }

        Ok(response.properties)
    }
}

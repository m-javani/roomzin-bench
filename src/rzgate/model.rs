// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchAvailPayload {
    pub segment: String,
    pub room_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_type: Option<String>, // `type` is a Rust keyword, so we use raw identifier syntax

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stars: Option<u8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub amenities: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,

    pub date: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<u8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_price: Option<u32>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "rate_features")]
    pub rate_feature: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DayAvail {
    pub date: String,
    pub availability: u8,
    pub final_price: u32,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rate_feature: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyAvail {
    pub property_id: String,
    pub days: Vec<DayAvail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchAvailResponse {
    pub status: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PropertyAvail>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

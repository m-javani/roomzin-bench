// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use serde::{Deserialize, Serialize};

#[allow(unused)]
/// A single day inside a property group.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ClientDayAvail {
    pub date: String,
    pub availability: u8,
    pub final_price: u32,
    pub rate_feature: String,
}

/// One property + all of its days.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ClientPropertyAvail {
    pub property_id: String,
    pub days: Vec<ClientDayAvail>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct SearchAvailRequest {
    pub segment: String,
    pub room_type: String,
    pub area: Option<String>,
    pub property_id: Option<String>,
    pub property_type: Option<String>,
    pub stars: Option<u8>,
    pub category: Option<String>,
    pub amenities: Option<Vec<String>>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
    pub dates: Vec<String>,
    pub availability: Option<u8>,
    pub final_price: Option<u32>,
    pub rate_feature: Option<Vec<String>>,
    pub limit: Option<usize>,
}

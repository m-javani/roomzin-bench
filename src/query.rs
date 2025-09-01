// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

// src/query.rs
use crate::{
    codecs::Codecs, error::CacheError, model::SearchAvailRequest, serializer::UpdRoomAvlRequest,
};
use serde::Deserialize;
use std::{fs, sync::Arc};

#[derive(Deserialize, Debug, Clone)]
pub struct QueryConfig {
    #[serde(default)]
    pub search_avail: Option<SearchRequest>,
    #[serde(default)]
    pub set_room_avl: Option<UpdateRequest>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct SearchRequest {
    pub segment: Option<String>,
    pub room_type: Option<String>,
    pub area: Option<String>,
    pub property_id: Option<String>,
    pub property_type: Option<String>,
    pub stars: Option<u8>,
    pub category: Option<String>,
    pub amenities: Option<Vec<String>>,
    pub rate_feature: Option<Vec<String>>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
    pub dates: Option<Vec<String>>,
    pub availability: Option<u8>,
    pub final_price: Option<u32>,
    pub limit: Option<usize>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct UpdateRequest {
    pub property_id: Option<String>,
    pub room_type: Option<String>,
    pub date: Option<String>,
    pub amount: Option<u8>,
}

impl QueryConfig {
    pub fn load(path: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if !fs::metadata(path).is_ok() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)?;
        let config: QueryConfig = serde_yaml::from_str(&content)?;
        Ok(Some(config))
    }

    /// Convert to concrete request structs using loaded codecs
    pub fn into_search_request(
        self,
        _codecs: Arc<Codecs>,
    ) -> Result<Option<SearchAvailRequest>, CacheError> {
        let search = match self.search_avail {
            Some(s) => s,
            None => return Ok(None),
        };

        Ok(Some(SearchAvailRequest {
            segment: search.segment.unwrap_or_default(),
            room_type: search.room_type.unwrap_or_else(|| "room_1".into()),
            area: search.area,
            property_id: search.property_id,
            property_type: search.property_type,
            stars: search.stars,
            category: search.category,
            amenities: search.amenities,
            longitude: search.longitude,
            latitude: search.latitude,
            dates: search.dates.unwrap_or_default(),
            availability: search.availability,
            final_price: search.final_price,
            rate_feature: search.rate_feature,
            limit: search.limit,
        }))
    }

    pub fn into_update_request(self) -> Option<UpdRoomAvlRequest> {
        self.set_room_avl.map(|u| UpdRoomAvlRequest {
            property_id: u.property_id.unwrap_or_default(),
            room_type: u.room_type.unwrap_or_else(|| "room_1".into()),
            date: u.date.unwrap_or_default(),
            amount: u.amount.unwrap_or(1),
        })
    }
}

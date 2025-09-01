// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

// src/command.rs
use crate::{
    codecs::Codecs,
    error::CacheError,
    query::{QueryConfig, UpdateRequest},
    serializer::{UpdRoomAvlRequest, serialize_search_avail, serialize_set_room_avl},
};

use std::{fs, sync::Arc};

pub fn get_serialized_commands(
    total_requests: usize,
    command_str: &str,
    codecs: Arc<Codecs>,
    query_config: Option<QueryConfig>,
    update_path: &str, // new: path to update.yml (only needed for set_room_avl)
) -> Result<Vec<Vec<u8>>, CacheError> {
    match command_str {
        "search_avail" => {
            let requests = query_config
                .map(|qc| qc.queries)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    CacheError::Validation("no search queries found in query.yml".into())
                })?;

            let mut payloads = Vec::with_capacity(total_requests);

            for clrid in 0..total_requests {
                // Cycle through the list of generated queries
                let search = &requests[clrid % requests.len()];

                let req = crate::model::SearchAvailRequest {
                    segment: search.segment.clone().unwrap_or_default(),
                    room_type: search.room_type.clone().unwrap_or_else(|| "room_1".into()),
                    area: search.area.clone(),
                    property_id: search.property_id.clone(),
                    property_type: search.property_type.clone(),
                    stars: search.stars,
                    category: search.category.clone(),
                    amenities: search.amenities.clone(),
                    longitude: search.longitude,
                    latitude: search.latitude,
                    dates: search.dates.clone().unwrap_or_default(),
                    availability: search.availability,
                    final_price: search.final_price,
                    rate_feature: search.rate_feature.clone(),
                    limit: search.limit.or(Some(300)),
                };

                let payload = serialize_search_avail(req, clrid as u32, codecs.clone())?;
                payloads.push(payload);
            }
            Ok(payloads)
        }

        "set_room_avl" => {
            if !fs::metadata(update_path).is_ok() {
                return Err(CacheError::Validation(format!("missing update.yml")));
            }

            let content = fs::read_to_string(update_path)?;
            let u: UpdateRequest = serde_yaml::from_str(&content)
                .map_err(|e| CacheError::Validation(format!("load update.yml: {e}")))?;

            let pid = u.property_id.unwrap();
            let room_type = u.room_type.unwrap_or_default();
            let date = u.date.unwrap_or_default();
            if pid.is_empty() || room_type.is_empty() || date.is_empty() {
                return Err(CacheError::Validation("invalid update.yml".into()));
            }

            let req = UpdRoomAvlRequest {
                property_id: pid,
                room_type: room_type,
                date: date,
                amount: u.amount.unwrap_or(1),
            };

            let mut payloads = Vec::with_capacity(total_requests);
            for clrid in 0..total_requests {
                // Re-use the exact same request for every clrid.
                // (If you later want slight variation you can do it deliberately,
                //  but never silently overwrite the generated fields.)
                let payload = serialize_set_room_avl(req.clone(), clrid as u32)?;
                payloads.push(payload);
            }
            Ok(payloads)
        }

        _ => Err(CacheError::Validation("unsupported command".into())),
    }
}

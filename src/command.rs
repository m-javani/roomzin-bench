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
    query::QueryConfig,
    serializer::{UpdRoomAvlRequest, serialize_search_avail, serialize_set_room_avl},
};
use chrono::Utc;
use std::sync::Arc;

pub fn get_serialized_commands(
    total_requests: usize,
    command_str: &str,
    codecs: Arc<Codecs>,
    query_config: Option<QueryConfig>,
) -> Result<Vec<Vec<u8>>, CacheError> {
    match command_str {
        "search_avail" => {
            let base_req = query_config
                .and_then(|qc| qc.into_search_request(codecs.clone()).ok().flatten())
                .unwrap_or_default();

            let mut payloads = Vec::with_capacity(total_requests);
            for clrid in 0..total_requests {
                let mut req = base_req.clone();

                if req.limit.is_none() {
                    req.limit = Some(300);
                }

                let payload = serialize_search_avail(req, clrid as u32, codecs.clone())?;
                payloads.push(payload);
            }
            Ok(payloads)
        }
        "set_room_avl" => {
            let base_update = query_config.and_then(|qc| qc.into_update_request());

            let mut payloads = Vec::with_capacity(total_requests);
            for clrid in 0..total_requests {
                let mut req = base_update.clone().unwrap_or(UpdRoomAvlRequest {
                    property_id: format!("prop_{}", (clrid % 256) + 1),
                    room_type: "room_1".into(),
                    date: (Utc::now().date_naive() + chrono::Duration::days(1))
                        .format("%Y-%m-%d")
                        .to_string(),
                    amount: 88,
                });

                // Allow query.yml to fix property_id
                if req.property_id.is_empty() {
                    req.property_id = format!("prop_{}", (clrid % 256) + 1);
                }

                let payload = serialize_set_room_avl(req, clrid as u32)?;
                payloads.push(payload);
            }
            Ok(payloads)
        }
        _ => Err(CacheError::Validation("unsupported command".into())),
    }
}

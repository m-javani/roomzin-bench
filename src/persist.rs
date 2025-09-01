// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use std::{fs::OpenOptions, sync::Arc};
use std::io::Write;

use crate::codecs::Codecs;
use crate::parser::{deserialize_search_avail, deserialize_upd_room_avl};

pub fn persist_response(clrid: u32, command_str: &str, payload: &[u8],codecs: Arc<Codecs>) -> bool {
    match command_str {
        "search_avail" => {
            match deserialize_search_avail(payload, clrid, codecs.clone()) {
                Ok(results) => {
                    let json = serde_json::to_string_pretty(&results).unwrap();
                    let mut f = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open("responses.json")
                        .unwrap();
                    writeln!(f, "{}", json).unwrap();
                    true
                }
                Err(e) => {
                    /* ignore bad frames */
                    println!("deserialize error: {:?}", e);
                    false
                }
            }
        }

        "set_room_avl" => {
            match deserialize_upd_room_avl(payload, clrid) {
                Ok(results) => {
                    println!("\n----------------------------------------------------\n");
                    println!("set_room_avl result: {}", results);
                    println!("\n----------------------------------------------------\n");
                    true
                }
                Err(e) => {
                    /* ignore bad frames */
                    println!("deserialize error: {:?}", e);
                    false
                }
            }
        }
        _ => {
            println!("unsupported command response");
            true
        }
    }
}

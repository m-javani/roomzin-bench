// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use std::sync::Arc;

use chrono::{Datelike, NaiveDate, Utc};
use uuid::Uuid;

use crate::{
    codecs::Codecs,
    error::CacheError,
    model::{ClientDayAvail, ClientPropertyAvail},
    serializer::Field,
};

// Helper function to parse response structure
fn parse_response(data: &[u8], clrid: u32) -> Result<(String, Vec<Field>), CacheError> {
    if data.is_empty() || data[0] != 0xFF {
        return Err(CacheError::ParseError("Invalid magic byte".into()));
    }
    if data.len() < 9 {
        return Err(CacheError::ParseError("Response too short".into()));
    }

    let response_clrid = u32::from_le_bytes(
        data[1..5]
            .try_into()
            .map_err(|_| CacheError::ParseError("Invalid clrid format".into()))?,
    );
    if response_clrid != clrid {
        return Err(CacheError::ParseError("Mismatched clrid".into()));
    }

    let mut cursor = 9; // 0xFF + clrid(4) + total_length(4)

    if cursor >= data.len() {
        return Err(CacheError::ParseError("Missing command length".into()));
    }
    let cmd_len = data[cursor] as usize;
    cursor += 1;

    if data.len() < cursor + cmd_len + 2 {
        return Err(CacheError::ParseError("Invalid command length".into()));
    }

    let response_type = std::str::from_utf8(&data[cursor..cursor + cmd_len])
        .map_err(|e| CacheError::Utf8Error(e))?
        .to_string();
    cursor += cmd_len;

    let field_count = u16::from_le_bytes(
        data[cursor..cursor + 2]
            .try_into()
            .map_err(|_| CacheError::ParseError("Invalid field count".into()))?,
    ) as usize;
    cursor += 2;

    let mut fields = Vec::new();
    for _ in 0..field_count {
        if cursor + 7 > data.len() {
            return Err(CacheError::ParseError("Incomplete field data".into()));
        }

        let id = u16::from_le_bytes(
            data[cursor..cursor + 2]
                .try_into()
                .map_err(|_| CacheError::ParseError("Invalid field ID".into()))?,
        );
        let field_type = data[cursor + 2];
        let data_len = u32::from_le_bytes(
            data[cursor + 3..cursor + 7]
                .try_into()
                .map_err(|_| CacheError::ParseError("Invalid data length".into()))?,
        ) as usize;
        cursor += 7;

        if cursor + data_len > data.len() {
            return Err(CacheError::ParseError("Data length exceeds buffer".into()));
        }

        let field_data = data[cursor..cursor + data_len].to_vec();
        fields.push(Field {
            id,
            field_type,
            data: field_data,
        });
        cursor += data_len;
    }

    Ok((response_type, fields))
}

// Helper function to parse error message (UNCHANGED)
fn parse_error(fields: Vec<Field>) -> CacheError {
    if fields.len() != 1 || fields[0].id != 0x01 || fields[0].field_type != 0x01 {
        return CacheError::ParseError("Invalid error field".into());
    }
    match std::str::from_utf8(&fields[0].data) {
        Ok(message) => CacheError::ParseError(message.to_string()),
        Err(e) => CacheError::Utf8Error(e),
    }
}

// Helper function to convert u16 to date (UNCHANGED)
fn u16_to_date_naive(packed: u16) -> Result<NaiveDate, CacheError> {
    let year_offset = ((packed >> 9) & 0b111) as i32;
    let month = ((packed >> 5) & 0b1111) + 1;
    let day = (packed & 0b11111) + 1;

    let base_year = Utc::now().naive_utc().date().year();
    NaiveDate::from_ymd_opt(base_year + year_offset, month as u32, day as u32)
        .ok_or_else(|| CacheError::Validation("unpacked date is invalid".to_string()))
}

// Helper function to convert u16 to date string (UNCHANGED)
pub fn u16_to_date(packed: u16) -> Result<String, CacheError> {
    let date = u16_to_date_naive(packed)?;
    Ok(date.format("%Y-%m-%d").to_string())
}

// Deserializes a SEARCHAVAIL response (expects Vec<ClientSearchAvailResult>)
pub fn deserialize_search_avail(
    data: &[u8],
    clrid: u32,
    codecs: Arc<Codecs>,
) -> Result<Vec<ClientPropertyAvail>, CacheError> {
    let (response_type, fields) = parse_response(data, clrid)?;
    match response_type.as_str() {
        "SUCCESS" => {
            if fields.is_empty() {
                return Ok(Vec::new());
            }

            // Read num_days field first (expected at idx=0, field_id=1, type=0x02, len=2)
            if fields[0].id != 1 || fields[0].field_type != 0x02 || fields[0].data.len() != 2 {
                return Err(CacheError::ParseError("expected num_days field".into()));
            }
            let num_days = u16::from_le_bytes(
                fields[0]
                    .data
                    .as_slice()
                    .try_into()
                    .map_err(|_| CacheError::ParseError("invalid num_days".into()))?,
            ) as usize;
            let mut idx = 1; // Start after num_days field

            if idx >= fields.len() {
                return Ok(Vec::new()); // Only num_days field present, no properties
            }

            use std::collections::HashMap;
            let mut map: HashMap<String, Vec<ClientDayAvail>> = HashMap::new();

            while idx < fields.len() {
                /* ---- property-id field ---- */
                let prop_field = &fields[idx];
                if prop_field.field_type != 0x01 {
                    return Err(CacheError::ParseError("expected property-id field".into()));
                }
                let property_id = bytes_to_property_id(&prop_field.data);
                idx += 1;

                /* ---- days vector field (type 0x08) ---- */
                if idx >= fields.len() {
                    return Err(CacheError::ParseError("expected days vector field".into()));
                }
                let days_field = &fields[idx];
                if days_field.field_type != 0x08 {
                    return Err(CacheError::ParseError(
                        "expected days vector field type 0x08".into(),
                    ));
                }

                let days_data = &days_field.data;
                if days_data.len() < 2 {
                    return Err(CacheError::ParseError("days vector too short".into()));
                }

                let days_count = u16::from_le_bytes(
                    days_data[0..2]
                        .try_into()
                        .map_err(|_| CacheError::ParseError("invalid days count".into()))?,
                ) as usize;

                if days_count != num_days {
                    return Err(CacheError::ParseError(
                        "days count mismatch with num_days".into(),
                    ));
                }

                let expected_data_len = 2 + (11 * days_count);
                if days_data.len() != expected_data_len {
                    return Err(CacheError::ParseError("days vector length mismatch".into()));
                }

                let mut days = Vec::with_capacity(days_count);
                let mut data_cursor = 2;

                for _ in 0..days_count {
                    if data_cursor + 8 > days_data.len() {
                        return Err(CacheError::ParseError("insufficient day data".into()));
                    }

                    let date = u16_to_date(u16::from_le_bytes(
                        days_data[data_cursor..data_cursor + 2].try_into().unwrap(),
                    ))?;
                    data_cursor += 2;

                    let availability = days_data[data_cursor];
                    data_cursor += 1;

                    let final_price = u32::from_le_bytes(
                        days_data[data_cursor..data_cursor + 4].try_into().unwrap(),
                    );
                    data_cursor += 4;

                    let rc = u32::from_le_bytes(
                        days_data[data_cursor..data_cursor + 4].try_into().unwrap(),
                    );
                    let rate_feature = codecs.bitmask_to_rate_feature_string(rc);
                    data_cursor += 4;

                    days.push(ClientDayAvail {
                        date,
                        availability,
                        final_price,
                        rate_feature,
                    });
                }

                map.entry(property_id).or_default().extend(days);
                idx += 1;
            }

            // Ensure all fields consumed
            if idx != fields.len() {
                return Err(CacheError::ParseError(
                    "extra fields after properties".into(),
                ));
            }

            Ok(map
                .into_iter()
                .map(|(property_id, days)| ClientPropertyAvail { property_id, days })
                .collect())
        }
        "ERROR" => Err(parse_error(fields)),
        _ => Err(CacheError::ParseError("invalid response type".into())),
    }
}

pub fn bytes_to_property_id(data: &[u8]) -> String {
    // 1. Too short → return empty
    if data.len() < 7 {
        return String::new();
    }

    // 2. Short string marker
    if data[6] == 0xF0 {
        // Left segment: 0..5
        let left_len = data.iter().take(6).take_while(|&&b| b != 0).count();

        // Right segment: 7..15
        let right_len = data.iter().skip(7).take_while(|&&b| b != 0).count();

        // Reconstruct original string
        let mut result = Vec::with_capacity(left_len + right_len);
        result.extend_from_slice(&data[..left_len]);
        result.extend_from_slice(&data[7..7 + right_len]);
        return String::from_utf8_lossy(&result).to_string();
    }

    // 3. UUID detection (valid version)
    let version = (data[6] & 0xF0) >> 4;
    if matches!(version, 1 | 2 | 3 | 4 | 5 | 7) {
        // Pad to 16 bytes if needed
        let mut uuid_bytes = [0u8; 16];
        let copy_len = data.len().min(16);
        uuid_bytes[..copy_len].copy_from_slice(&data[..copy_len]);

        if let Ok(uuid) = Uuid::from_slice(&uuid_bytes) {
            return uuid.to_string();
        }
    }

    // This should never happen with proper server data
    String::new()
}

// Deserializes a UPDROOMAVL response (expects u8)
pub fn deserialize_upd_room_avl(data: &[u8], clrid: u32) -> Result<u8, CacheError> {
    let (response_type, fields) = parse_response(data, clrid)?;
    match response_type.as_str() {
        "SUCCESS" => {
            if fields.len() != 1 {
                return Err(CacheError::ParseError(
                    "Expected one field in UPDROOMAVL response".into(),
                ));
            }
            let field = &fields[0];
            if field.id != 0x01 || field.field_type != 0x02 || field.data.len() != 1 {
                return Err(CacheError::ParseError(
                    "Invalid field in UPDROOMAVL response".into(),
                ));
            }
            Ok(field.data[0])
        }
        "ERROR" => Err(parse_error(fields)),
        _ => Err(CacheError::ParseError("Invalid response type".into())),
    }
}

pub fn deserialize_get_codecs(data: &[u8], clrid: u32) -> Result<Vec<String>, CacheError> {
    let (response_type, fields) = parse_response(data, clrid)?;
    match response_type.as_str() {
        "SUCCESS" => {
            if fields.len() != 1 {
                return Err(CacheError::ParseError(format!(
                    "invalid field count: expected 1 field, got {}",
                    fields.len()
                )));
            }

            let field = &fields[0];
            if field.field_type != 0x09 {
                return Err(CacheError::ParseError(format!(
                    "expected field type 0x09, got type {}",
                    field.field_type
                )));
            }

            let data_str = std::str::from_utf8(&field.data)
                .map_err(|e| CacheError::ParseError(format!("Invalid UTF-8: {}", e)))?;

            let rate_features: Vec<String> = data_str.split(',').map(|s| s.to_string()).collect();

            Ok(rate_features)
        }
        "ERROR" => Err(parse_error(fields)),
        _ => Err(CacheError::ParseError("Invalid response type".into())),
    }
}

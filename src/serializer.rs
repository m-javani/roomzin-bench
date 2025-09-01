#![allow(unused)]
// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.


use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use crate::{
    codecs::Codecs,
    error::CacheError,
    model::{ClientDayAvail, ClientPropertyAvail, SearchAvailRequest},
};

// Field structure
#[derive(Debug, Clone)]
pub struct Field {
    pub id: u16,
    pub field_type: u8,
    pub data: Vec<u8>,
}

pub fn serialize_login(token: &str) -> Vec<u8> {
    let cmd = b"LOGIN";
    let token_b = token.as_bytes();

    // Field structure: [id(2)][type(1)][len(4)][data(n)]
    let field_bytes = [
        &0x01u16.to_le_bytes()[..],                // ← 2 bytes for u16 ID
        &[0x01],                                   // ← 1 byte for type
        &(token_b.len() as u32).to_le_bytes()[..], // 4-byte length
        token_b,                                   // token
    ]
    .concat();

    let cmd_len = cmd.len() as u8;

    // total_len represents the length AFTER the total_len field itself
    let total_len = 1 +             // cmd_len
        cmd.len() +                 // cmd  
        2 +                         // field_count
        field_bytes.len(); // fields data

    let mut frame = vec![0xFF]; // magic byte

    // CLRID (4 bytes)
    frame.extend_from_slice(&0u32.to_le_bytes());

    // Total length (4 bytes)
    frame.extend_from_slice(&(total_len as u32).to_le_bytes());

    // Command length and name
    frame.push(cmd_len);
    frame.extend_from_slice(cmd);

    // Field count (2 bytes)
    frame.extend_from_slice(&1u16.to_le_bytes());

    // Field data
    frame.extend_from_slice(&field_bytes);

    frame
}

/// Entry-point for SDKs / tests.
pub fn serialize_search_avail(
    req: SearchAvailRequest,
    clrid: u32,
    codecs: Arc<Codecs>,
) -> Result<Vec<u8>, CacheError> {
    // Build the wire frame
    let mut buffer = Vec::new();
    buffer.push(0xFF);
    buffer.extend_from_slice(&clrid.to_le_bytes());
    buffer.extend_from_slice(&0u32.to_le_bytes()); // length placeholder

    let cmd_name = "SEARCHAVAIL";
    buffer.push(cmd_name.len() as u8);
    buffer.extend_from_slice(cmd_name.as_bytes());

    let field_count_offset = buffer.len();
    buffer.extend_from_slice(&0u16.to_le_bytes());

    let mut fields = Vec::new();

    // 0x01 segment (required)
    if req.segment.is_empty() {
        return Err(CacheError::Validation("Segment is required".into()));
    }
    fields.push(Field {
        id: 0x01,
        field_type: 0x01,
        data: req.segment.into_bytes(),
    });

    // 0x02 room_type (required)
    if req.room_type.is_empty() {
        return Err(CacheError::Validation("Room type is required".into()));
    }
    fields.push(Field {
        id: 0x02,
        field_type: 0x01,
        data: req.room_type.into_bytes(),
    });

    // 0x03 area
    if let Some(a) = req.area {
        fields.push(Field {
            id: 0x03,
            field_type: 0x01,
            data: a.into_bytes(),
        });
    }

    // 0x04 property_id
    if let Some(pid) = req.property_id {
        fields.push(Field {
            id: 0x04,
            field_type: 0x01,
            data: pid.into_bytes(),
        });
    }

    // 0x05 property_type
    if let Some(pt) = req.property_type {
        fields.push(Field {
            id: 0x05,
            field_type: 0x01,
            data: pt.into_bytes(),
        });
    }

    // 0x06 stars
    if let Some(s) = req.stars {
        fields.push(Field {
            id: 0x06,
            field_type: 0x02,
            data: vec![s],
        });
    }

    // 0x07 category
    if let Some(c) = req.category {
        fields.push(Field {
            id: 0x07,
            field_type: 0x01,
            data: c.into_bytes(),
        });
    }

    // 0x08 amenities
    if let Some(amenities) = req.amenities {
        fields.push(Field {
            id: 0x08,
            field_type: 0x01,
            data: amenities.join(",").into_bytes(),
        });
    }

    // 0x09 longitude
    if let Some(lon) = req.longitude {
        fields.push(Field {
            id: 0x09,
            field_type: 0x03,
            data: lon.to_le_bytes().to_vec(),
        });
    }

    // 0x0A latitude
    if let Some(lat) = req.latitude {
        fields.push(Field {
            id: 0x0A,
            field_type: 0x03,
            data: lat.to_le_bytes().to_vec(),
        });
    }

    // 0x0B dates
    if !req.dates.is_empty() {
        let s = req.dates.join(",");
        fields.push(Field {
            id: 0x0B,
            field_type: 0x01,
            data: s.into_bytes(),
        });
    }

    // 0x0C availability
    if let Some(av) = req.availability {
        fields.push(Field {
            id: 0x0C,
            field_type: 0x02,
            data: vec![av],
        });
    }

    // 0x0D final_price
    if let Some(fp) = req.final_price {
        fields.push(Field {
            id: 0x0D,
            field_type: 0x03,
            data: fp.to_le_bytes().to_vec(),
        });
    }

    // 0x0E rate_feature
    if let Some(rc) = req.rate_feature {
        fields.push(Field {
            id: 0x0E,
            field_type: 0x01,
            data: rc.join(",").into_bytes(),
        });
    }

    // 0x0F limit
    if let Some(lim) = req.limit {
        fields.push(Field {
            id: 0x0F,
            field_type: 0x03,
            data: (lim as u64).to_le_bytes().to_vec(),
        });
    }

    // update field count
    let field_count = fields.len() as u16;
    buffer[field_count_offset..field_count_offset + 2].copy_from_slice(&field_count.to_le_bytes());

    // append fields
    for f in fields {
        buffer.extend_from_slice(&f.id.to_le_bytes());
        buffer.push(f.field_type);
        buffer.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        buffer.extend_from_slice(&f.data);
    }

    // back-patch total length
    let total_len = buffer.len() - 9;
    buffer[5..9].copy_from_slice(&(total_len as u32).to_le_bytes());

    Ok(buffer)
}

#[derive(Debug, Clone)]
pub struct UpdRoomAvlRequest {
    pub property_id: String,
    pub room_type: String,
    pub date: String,
    pub amount: u8,
}

// Serializes a UpdRoomAvlRequest into a TCP packet buffer
pub fn serialize_dec_room_avl(
    payload: UpdRoomAvlRequest,
    clrid: u32,
) -> Result<Vec<u8>, CacheError> {
    let mut buffer = Vec::new();

    // Magic byte
    buffer.push(0xFF);

    // Client request ID (clrid)
    buffer.extend_from_slice(&clrid.to_le_bytes());

    // Placeholder for total length (4 bytes, updated later)
    buffer.extend_from_slice(&0u32.to_le_bytes());

    // Command name
    let cmd_name = "DECROOMAVL";
    let cmd_len = cmd_name.len() as u8;
    buffer.push(cmd_len);
    buffer.extend_from_slice(cmd_name.as_bytes());

    // Field count (updated later)
    let field_count_offset = buffer.len();
    buffer.extend_from_slice(&0u16.to_le_bytes());

    // Serialize fields
    let mut fields = Vec::new();

    // Property ID (ID: 0x01, Type: 0x01 - String)
    if !payload.property_id.is_empty() {
        fields.push(Field {
            id: 0x01,
            field_type: 0x01,
            data: payload.property_id.as_bytes().to_vec(),
        });
    } else {
        return Err(CacheError::Validation("Property ID is required".into()));
    }

    // Room Type (ID: 0x02, Type: 0x01 - String)
    if !payload.room_type.is_empty() {
        fields.push(Field {
            id: 0x02,
            field_type: 0x01,
            data: payload.room_type.as_bytes().to_vec(),
        });
    } else {
        return Err(CacheError::Validation("Room type is required".into()));
    }

    // Date (ID: 0x03, Type: 0x01 - String)
    if !payload.date.is_empty() {
        fields.push(Field {
            id: 0x03,
            field_type: 0x01,
            data: payload.date.into_bytes(),
        });
    } else {
        return Err(CacheError::Validation("Date is required".into()));
    }

    // Amount (ID: 0x04, Type: 0x02 - u8)
    if payload.amount != 0 {
        fields.push(Field {
            id: 0x04,
            field_type: 0x02,
            data: vec![payload.amount],
        });
    }

    // Update field count
    let field_count = fields.len() as u16;
    buffer[field_count_offset..field_count_offset + 2].copy_from_slice(&field_count.to_le_bytes());

    // Serialize fields
    for field in fields {
        buffer.extend_from_slice(&field.id.to_le_bytes());
        buffer.push(field.field_type);
        let data_len = field.data.len() as u32;
        buffer.extend_from_slice(&data_len.to_le_bytes());
        buffer.extend_from_slice(&field.data);
    }

    // Update total length (from cmd_len onwards)
    let total_len = buffer.len() - 9;
    buffer[5..9].copy_from_slice(&(total_len as u32).to_le_bytes());

    Ok(buffer)
}

// Serializes a SetRoomAvlPayload into a TCP packet buffer
pub fn serialize_set_room_avl(
    payload: UpdRoomAvlRequest,
    clrid: u32,
) -> Result<Vec<u8>, CacheError> {
    let mut buffer = Vec::new();

    // Magic byte
    buffer.push(0xFF);

    // Client request ID (clrid)
    buffer.extend_from_slice(&clrid.to_le_bytes());

    // Placeholder for total length (4 bytes, updated later)
    buffer.extend_from_slice(&0u32.to_le_bytes());

    // Command name
    let cmd_name = "SETROOMAVL";
    let cmd_len = cmd_name.len() as u8;
    buffer.push(cmd_len);
    buffer.extend_from_slice(cmd_name.as_bytes());

    // Field count (updated later)
    let field_count_offset = buffer.len();
    buffer.extend_from_slice(&0u16.to_le_bytes());

    // Serialize fields
    let mut fields = Vec::new();

    // Property ID (ID: 0x01, Type: 0x01 - String)
    if !payload.property_id.is_empty() {
        fields.push(Field {
            id: 0x01,
            field_type: 0x01,
            data: payload.property_id.as_bytes().to_vec(),
        });
    } else {
        return Err(CacheError::Validation("Property ID is required".into()));
    }

    // Room Type (ID: 0x02, Type: 0x01 - String)
    if !payload.room_type.is_empty() {
        fields.push(Field {
            id: 0x02,
            field_type: 0x01,
            data: payload.room_type.as_bytes().to_vec(),
        });
    } else {
        return Err(CacheError::Validation("Room type is required".into()));
    }

    // Date (ID: 0x03, Type: 0x01 - String)
    if !payload.date.is_empty() {
        fields.push(Field {
            id: 0x03,
            field_type: 0x01,
            data: payload.date.as_bytes().to_vec(),
        });
    } else {
        return Err(CacheError::Validation("Date is required".into()));
    }

    // Amount (ID: 0x04, Type: 0x02 - u8)
    if payload.amount != 0 {
        fields.push(Field {
            id: 0x04,
            field_type: 0x02,
            data: vec![payload.amount],
        });
    }

    // Update field count
    let field_count = fields.len() as u16;
    buffer[field_count_offset..field_count_offset + 2].copy_from_slice(&field_count.to_le_bytes());

    // Serialize fields
    for field in fields {
        buffer.extend_from_slice(&field.id.to_le_bytes());
        buffer.push(field.field_type);
        let data_len = field.data.len() as u32;
        buffer.extend_from_slice(&data_len.to_le_bytes());
        buffer.extend_from_slice(&field.data);
    }

    // Update total length (from cmd_len onwards)
    let total_len = buffer.len() - 9;
    buffer[5..9].copy_from_slice(&(total_len as u32).to_le_bytes());

    Ok(buffer)
}

pub fn serialize_fetch_codecs(clrid: u32) -> Vec<u8> {
    let mut buffer = Vec::new();

    // Magic byte
    buffer.push(0xFF);

    // Client request ID (clrid)
    buffer.extend_from_slice(&clrid.to_le_bytes());

    // Placeholder for total length (4 bytes, updated later)
    buffer.extend_from_slice(&0u32.to_le_bytes());

    // Command name
    let cmd_name = "GETCODECS";
    let cmd_len = cmd_name.len() as u8;
    buffer.push(cmd_len);
    buffer.extend_from_slice(cmd_name.as_bytes());

    // Field count = 0
    buffer.extend_from_slice(&0u16.to_le_bytes());

    // Update total length (from cmd_len onwards)
    let total_len = buffer.len() - 9;
    buffer[5..9].copy_from_slice(&(total_len as u32).to_le_bytes());

    buffer
}

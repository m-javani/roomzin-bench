// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use crate::error::CacheError;
use csv::Writer;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::Path;

/// Generate test dataset: properties.csv and packages.csv
pub fn run(
    segments: usize,
    props_per_segment: usize,
    room_types: usize,
    days: usize,
    config_dir: &str,
    data_dir: &str,
    seed: u64,
) -> Result<(), CacheError> {
    // Ensure directories exist
    fs::create_dir_all(config_dir)?;
    fs::create_dir_all(data_dir)?;

    let codecs_path = Path::new(config_dir).join("codecs.yml");
    let props_path = Path::new(data_dir).join("properties.csv");
    let pkgs_path = Path::new(data_dir).join("packages.csv");

    // Load codecs
    let codecs = load_codecs(&codecs_path)?;
    let rate_features = get_rate_features(&codecs)?;

    let mut rng = SmallRng::seed_from_u64(seed);
    let start_date = chrono::Local::now().naive_local();

    let amenities = "wifi|pool|breakfast|spa|restaurant|bar";

    println!("Generating dataset with seed {}", seed);
    println!(
        " - Segments: {}, Props per segment: {}",
        segments, props_per_segment
    );
    println!(" - Room types: {}, Days: {}", room_types, days);

    // Generate properties.csv
    let num_props = {
        let mut writer = Writer::from_path(&props_path)?;
        gen_properties(segments, props_per_segment, amenities, &mut writer)?;
        num_props_from_params(segments, props_per_segment)
    };

    // Generate packages.csv
    {
        let mut writer = Writer::from_path(&pkgs_path)?;
        gen_packages(
            num_props,
            room_types,
            days,
            &rate_features,
            &mut writer,
            &mut rng,
            start_date.into(),
        )?;
    }

    println!("✅ Dataset generation completed successfully!");
    println!("   Properties → {}", props_path.display());
    println!("   Packages   → {}", pkgs_path.display());
    println!("   Total properties: {}", num_props);
    println!("   Rate features: {}", rate_features.len());

    Ok(())
}

fn load_codecs(path: &Path) -> Result<HashMap<String, Value>, CacheError> {
    let content = fs::read_to_string(path)
        .map_err(|e| CacheError::Validation(format!("Failed to read {}: {}", path.display(), e)))?;

    serde_yaml::from_str(&content)
        .map_err(|e| CacheError::Validation(format!("Invalid YAML in {}: {}", path.display(), e)))
}

fn get_rate_features(codecs: &HashMap<String, Value>) -> Result<Vec<String>, CacheError> {
    let rate_features = codecs
        .get("rate_features")
        .and_then(|v| v.as_sequence())
        .ok_or_else(|| {
            CacheError::Validation("Missing or invalid 'rate_features' in codecs.yml".into())
        })?;

    let features: Vec<String> = rate_features
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if features.is_empty() {
        return Err(CacheError::Validation("rate_features list is empty".into()));
    }

    Ok(features)
}

fn num_props_from_params(segments: usize, props_per_segment: usize) -> usize {
    segments * props_per_segment
}

fn gen_properties(
    num_segments: usize,
    props_per_segment: usize,
    amenities: &str,
    writer: &mut Writer<File>,
) -> Result<(), CacheError> {
    writer.write_record([
        "PropertyID",
        "Segment",
        "Area",
        "PropertyType",
        "Category",
        "Stars",
        "Latitude",
        "Longitude",
        "Amenities",
    ])?;

    let mut prop_counter = 0usize;

    for seg_i in 0..num_segments {
        let segment = format!("segment_{}", seg_i + 1);
        let area = format!("area_{}", seg_i + 1);
        let base_lat = 40.7128 + seg_i as f64 * 0.5;
        let base_lon = -74.0060 + seg_i as f64 * 0.5;

        for _ in 0..props_per_segment {
            prop_counter += 1;
            let prop_id = format!("prop_{}", prop_counter);
            let stars = 4 + (prop_counter % 2);
            let lat = base_lat + (prop_counter % 10) as f64 * 0.001;
            let lon = base_lon + (prop_counter % 10) as f64 * 0.001;

            writer.write_record([
                &prop_id,
                &segment,
                &area,
                "hotel",
                "test",
                &stars.to_string(),
                &format!("{:.6}", lat),
                &format!("{:.6}", lon),
                amenities,
            ])?;
        }
    }
    Ok(())
}

fn gen_packages(
    num_props: usize,
    room_types: usize,
    days: usize,
    rate_features: &[String],
    writer: &mut Writer<File>,
    rng: &mut SmallRng,
    start_date: chrono::NaiveDate,
) -> Result<(), CacheError> {
    writer.write_record([
        "PropertyID",
        "RoomType",
        "Date",
        "Availability",
        "FinalPrice",
        "RateFeature",
    ])?;

    for prop_num in 1..=num_props {
        let prop_id = format!("prop_{}", prop_num);

        for j in 1..=room_types {
            let room_type = format!("room_{}", j);
            let availability = 5 + (prop_num + j) % 11;
            let final_price = 100 + (prop_num + j) * 10;
            let rc_cnt = 2 + ((prop_num + j) % 4);

            let rate_feature = pick_unique(rate_features, rc_cnt, rng);

            for d in 0..days {
                let cur_date = start_date + chrono::Duration::days(d as i64);
                writer.write_record([
                    &prop_id,
                    &room_type,
                    &cur_date.format("%Y-%m-%d").to_string(),
                    &availability.to_string(),
                    &final_price.to_string(),
                    &rate_feature,
                ])?;
            }
        }
    }
    Ok(())
}

use rand::seq::IteratorRandom;

fn pick_unique(items: &[String], k: usize, rng: &mut SmallRng) -> String {
    if items.is_empty() {
        return String::new();
    }

    if items.len() < k {
        // Need repetition
        let mut selected = Vec::with_capacity(k);
        while selected.len() < k {
            let remaining = k - selected.len();
            let sample: Vec<&String> = items
                .iter()
                .choose_multiple(rng, remaining.min(items.len()));
            selected.extend(sample.into_iter().cloned());
        }
        selected.truncate(k);
        selected.join("|")
    } else {
        // Unique selection
        items
            .iter()
            .choose_multiple(rng, k)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("|")
    }
}

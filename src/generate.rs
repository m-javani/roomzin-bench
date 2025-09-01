// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

use crate::error::CacheError;
use csv::Writer;
use indicatif::{ProgressBar, ProgressStyle};
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

    println!("Generating benchmark dataset...\n");
    println!("This creates a synthetic benchmark dataset.");
    println!("Please be patient. Large datasets may take around a minute to generate.\n");

    let pb = ProgressBar::new(100);

    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {percent:>3}%  {msg}  ETA {eta_precise}")
            .unwrap()
            .progress_chars("█░"),
    );

    pb.set_message("Writing properties...");

    let total_properties = segments * props_per_segment;
    let total_packages = segments * props_per_segment * room_types * days;

    // Generate properties.csv
    let _num_props = {
        let mut writer = Writer::from_path(&props_path)?;
        gen_properties(segments, props_per_segment, amenities, &mut writer, &pb)?;
        num_props_from_params(segments, props_per_segment)
    };

    // Generate packages.csv
    {
        let mut writer = Writer::from_path(&pkgs_path)?;
        gen_packages(
            segments,
            props_per_segment,
            room_types,
            days,
            &rate_features,
            &mut writer,
            &mut rng,
            start_date.into(),
            &pb,
        )?;
    }

    pb.set_position(100);
    pb.finish_with_message("Benchmark dataset generated.");

    println!();
    println!("✓ Benchmark dataset generated.");
    println!("✓ Properties: {}", total_properties);
    println!("✓ Packages: {}", total_packages);
    println!("✓ Output written to {}", data_dir);

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
    pb: &ProgressBar,
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

    let property_types = ["hotel", "hostel", "resort"];
    let categories = ["standard", "premium", "budget"];

    let mut written = 0usize;
    let total = num_segments * props_per_segment;

    for seg_num in 1..=num_segments {
        let segment = format!("segment_{}", seg_num);
        let area = format!("area_{}", seg_num);
        let base_lat = 40.7128 + (seg_num - 1) as f64 * 0.5;
        let base_lon = -74.0060 + (seg_num - 1) as f64 * 0.5;

        for prop_in_segment in 1..=props_per_segment {
            let prop_id = format!("s{}_p_{}", seg_num, prop_in_segment);
            let stars = 4 + (prop_in_segment % 2);
            let lat = base_lat + (prop_in_segment % 10) as f64 * 0.001;
            let lon = base_lon + (prop_in_segment % 10) as f64 * 0.001;
            let prop_type = property_types[(prop_in_segment - 1) % property_types.len()];
            let category = categories[(prop_in_segment - 1) % categories.len()];

            written += 1;
            if written % 1000 == 0 || written == total {
                pb.set_position((written * 20 / total) as u64);
            }

            writer.write_record([
                &prop_id,
                &segment,
                &area,
                prop_type,
                category,
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
    num_segments: usize,
    props_per_segment: usize,
    room_types: usize,
    days: usize,
    rate_features: &[String],
    writer: &mut Writer<File>,
    rng: &mut SmallRng,
    start_date: chrono::NaiveDate,
    pb: &ProgressBar,
) -> Result<(), CacheError> {
    pb.set_message("Writing packages...");

    let total = num_segments * props_per_segment * room_types * days;

    let mut written = 0usize;

    writer.write_record([
        "PropertyID",
        "RoomType",
        "Date",
        "Availability",
        "FinalPrice",
        "RateFeature",
    ])?;

    for seg_num in 1..=num_segments {
        for prop_in_segment in 1..=props_per_segment {
            let prop_id = format!("s{}_p_{}", seg_num, prop_in_segment); // ← match properties.csv

            for j in 1..=room_types {
                let room_type = format!("room_{}", j);
                let availability = 5 + (prop_in_segment + j) % 11;
                let final_price = 100 + (prop_in_segment + j) * 10;
                let rc_cnt = 2 + ((prop_in_segment + j) % 4);
                let rate_feature = pick_unique(rate_features, rc_cnt, rng);

                for d in 0..days {
                    written += 1;
                    if written % 100_000 == 0 || written == total {
                        pb.set_position(20 + (written * 80 / total) as u64);
                    }

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

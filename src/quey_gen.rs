// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

// Three streaming passes over packages.csv.
// Pass 1: select lowest-hash candidates per (segment, room_type).
// Pass 2: collect package days for those candidates only.
// Pass 3: compute exact expected_count.
use crate::error::CacheError;
use ahash::{AHashMap, AHashSet};
use chrono::NaiveDate;
use indicatif::{ProgressBar, ProgressStyle};
use lazycsv::Csv;
use memmap2::Mmap;
use serde::Serialize;
use std::arch::x86_64::*;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::str;
use std::time::Instant;

const DEFAULT_MAX_EXAMPLES: usize = 2;

#[derive(Clone)]
struct PropMeta {
    segment: String,
    area: String,
    category: String,
    property_type: String,
    stars: u8,
    amenities: Vec<String>,
}

struct PackageDay {
    date: String,
    day_num: i32,
    avail: u8,
    price: u32,
    rate_feature: Vec<String>,
}

struct Candidate {
    property_id: String,
    meta: PropMeta,
    days: Vec<PackageDay>,
}

/// Pass-1 state: keep only the lowest-hash property IDs (online top-k).
struct PairSelection {
    /// (hash, prop_id) sorted ascending by hash, length ≤ max_examples
    selected: Vec<(u64, String)>,
}

#[derive(Serialize)]
struct BenchmarkQuery {
    segment: String,
    room_type: String,
    category: String,
    property_type: String,
    area: String,
    stars: u8,
    amenities: Vec<String>,
    rate_feature: Vec<String>,
    availability: u8,
    final_price: u32,
    dates: Vec<String>,
    expected_count: u64,
    limit: usize,
}

#[derive(Serialize)]
struct Output {
    queries: Vec<BenchmarkQuery>,
}

struct PendingQuery {
    query: BenchmarkQuery,
    stay_day_nums: Vec<i32>,
}

pub struct BenchmarkQueryGenerator {
    pub input_dir: String,
    pub output_dir: String,
    pub max_examples: usize,
    pub limit: usize,
}

impl BenchmarkQueryGenerator {
    pub fn new(input_dir: &str, output_dir: &str, limit: usize) -> Self {
        let _ = std::fs::create_dir_all(output_dir);
        Self {
            input_dir: input_dir.to_string(),
            output_dir: output_dir.to_string(),
            max_examples: DEFAULT_MAX_EXAMPLES,
            limit,
        }
    }

    pub fn run(&self) -> Result<(), CacheError> {
        let start = Instant::now();

        println!("Generating benchmark queries...\n");
        println!(
            "This scans the benchmark dataset once to generate reproducible benchmark queries."
        );
        println!("Please be patient. This may take around 1–2 minutes on large datasets.\n");

        let properties = self.load_properties()?;

        let packages_path = format!("{}/packages.csv", self.input_dir);

        let pb = ProgressBar::new(100);

        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40.cyan/blue} {percent:>3}%  {msg}  ETA {eta_precise}")
                .unwrap()
                .progress_chars("█░"),
        );

        pb.set_message("Scanning benchmark dataset...");

        // ------------------------------------------------------------------
        // Pass 1: select lowest-hash property IDs per (segment, room_type)
        // ------------------------------------------------------------------
        let file = File::open(&packages_path)
            .map_err(|e| CacheError::Validation(format!("open packages.csv: {e}")))?;
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| CacheError::Validation(format!("mmap packages.csv: {e}")))?;

        let total_rows = mmap
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
            .count() as u64;

        let mut selections: AHashMap<(String, String), PairSelection> = AHashMap::new();
        let mut pkg_count = 0u64;

        for line in mmap
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            let Some(fields) = parse_line_fast(line) else {
                continue;
            };
            let prop_id = fields[0];
            let Some(meta) = properties.get(prop_id) else {
                continue;
            };
            let room_type = fields[1];
            let key = (meta.segment.clone(), room_type.to_string());

            let pair = selections.entry(key).or_insert_with(|| PairSelection {
                selected: Vec::with_capacity(self.max_examples),
            });

            // Already selected? nothing to do.
            if pair.selected.iter().any(|(_, id)| id == prop_id) {
                // still count the row
            } else {
                let h = prop_hash(prop_id);
                if pair.selected.len() < self.max_examples {
                    // insert keeping sorted order by hash
                    let pos = pair.selected.partition_point(|&(hh, _)| hh < h);
                    pair.selected.insert(pos, (h, prop_id.to_string()));
                } else if h < pair.selected.last().unwrap().0 {
                    // better than current worst → replace
                    pair.selected.pop();
                    let pos = pair.selected.partition_point(|&(hh, _)| hh < h);
                    pair.selected.insert(pos, (h, prop_id.to_string()));
                }
            }

            pkg_count += 1;
            if pkg_count % 100_000 == 0 {
                pb.set_position(pkg_count * 33 / total_rows);
            }
        }

        pb.set_message("Building benchmark queries...");
        pb.set_position(33);

        // Build lookup: (seg, room) → set of selected prop_ids
        let mut selected_lookup: AHashMap<(String, String), AHashSet<String>> = AHashMap::new();
        for (key, sel) in &selections {
            let set: AHashSet<String> = sel.selected.iter().map(|(_, id)| id.clone()).collect();
            selected_lookup.insert(key.clone(), set);
        }
        drop(selections);

        // ------------------------------------------------------------------
        // Pass 2: collect PackageDays only for the selected properties
        // ------------------------------------------------------------------
        let mut update_property_id: Option<String> = None;
        let mut update_room_type: Option<String> = None;
        let mut update_date: Option<String> = None;

        let mut candidates: AHashMap<(String, String), Vec<Candidate>> = AHashMap::new();

        let file2 = File::open(&packages_path)
            .map_err(|e| CacheError::Validation(format!("open packages.csv (pass 2): {e}")))?;
        let mmap2 = unsafe { Mmap::map(&file2) }
            .map_err(|e| CacheError::Validation(format!("mmap packages.csv (pass 2): {e}")))?;

        pkg_count = 0;

        for line in mmap2
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            let Some(fields) = parse_line_fast(line) else {
                continue;
            };
            let prop_id = fields[0];
            let Some(meta) = properties.get(prop_id) else {
                continue;
            };
            let room_type = fields[1];
            let key = (meta.segment.clone(), room_type.to_string());

            let Some(sel_set) = selected_lookup.get(&key) else {
                continue;
            };
            if !sel_set.contains(prop_id) {
                continue;
            }

            let date_str = fields[2];
            let Some(day_num) = date_to_day_num(date_str) else {
                continue;
            };
            let avail = parse_u8_fast(fields[3].as_bytes()).unwrap_or(0);
            let price = parse_u32_fast(fields[4].as_bytes()).unwrap_or(0);
            let rate_feature = split_pipe_list(fields[5]);

            let pair_cands = candidates.entry(key).or_default();
            if let Some(cand) = pair_cands.iter_mut().find(|c| c.property_id == prop_id) {
                cand.days.push(PackageDay {
                    date: date_str.to_string(),
                    day_num,
                    avail,
                    price,
                    rate_feature: rate_feature,
                });
            } else {
                pair_cands.push(Candidate {
                    property_id: prop_id.to_string(),
                    meta: meta.clone(),
                    days: vec![PackageDay {
                        date: date_str.to_string(),
                        day_num,
                        avail,
                        price,
                        rate_feature,
                    }],
                });
            }

            pkg_count += 1;
            if pkg_count % 100_000 == 0 {
                pb.set_position(33 + (pkg_count * 33 / total_rows));
            }
        }

        drop(selected_lookup);

        // Build pending queries
        let mut pending: Vec<PendingQuery> = Vec::new();
        let mut sorted_keys: Vec<_> = candidates.keys().cloned().collect();
        sorted_keys.sort();

        for (seg, room) in sorted_keys {
            let cands = candidates.remove(&(seg.clone(), room.clone())).unwrap();
            let seed_base = simple_hash(&seg, &room);

            for (ex_idx, cand) in cands.into_iter().enumerate() {
                let seed = seed_base.wrapping_add(ex_idx as u64 * 0x9E3779B97F4A7C15);
                let mut days = cand.days;
                days.sort_by_key(|d| d.day_num);
                days.dedup_by_key(|d| d.day_num);

                let (check_in, check_out, stay_days) = select_stay(&days, seed);
                if stay_days.is_empty() {
                    continue;
                }
                // Capture the first valid candidate for the update.yml
                if update_property_id.is_none() {
                    update_property_id = Some(cand.property_id.clone());
                    update_room_type = Some(room.clone());
                    update_date = Some(check_in.clone()); // or stay_days[0].date.clone()
                }

                let amenities = select_subset(&cand.meta.amenities, seed, 3);

                let mut rate_feature = stay_days[0].rate_feature.clone();
                for d in stay_days.iter().skip(1) {
                    rate_feature.retain(|r| d.rate_feature.iter().any(|x| x == r));
                }
                let rate_feature = select_subset(&rate_feature, seed.wrapping_add(1), 2);

                let final_price = stay_days.iter().map(|d| d.price).max().unwrap_or(0);
                let stay_day_nums: Vec<i32> = stay_days.iter().map(|d| d.day_num).collect();

                pending.push(PendingQuery {
                    query: BenchmarkQuery {
                        segment: seg.clone(),
                        room_type: room.clone(),
                        category: cand.meta.category,
                        property_type: cand.meta.property_type,
                        area: cand.meta.area,
                        stars: cand.meta.stars,
                        amenities,
                        rate_feature,
                        availability: 1,
                        final_price,
                        dates: vec![check_in, check_out],
                        expected_count: 0,
                        limit: self.limit,
                    },
                    stay_day_nums,
                });
            }
        }

        pb.set_message("Computing expected results...");
        pb.set_position(66);

        // ------------------------------------------------------------------
        // Pass 3: exact expected_count
        // ------------------------------------------------------------------
        let mut group: AHashMap<(String, String), Vec<usize>> = AHashMap::new();
        for (idx, pq) in pending.iter().enumerate() {
            group
                .entry((pq.query.segment.clone(), pq.query.room_type.clone()))
                .or_default()
                .push(idx);
        }

        // u16 bitmask – safe up to 16-night stays
        let mut coverage: Vec<AHashMap<String, u16>> =
            (0..pending.len()).map(|_| AHashMap::new()).collect();

        let file3 = File::open(&packages_path)
            .map_err(|e| CacheError::Validation(format!("open packages.csv (pass 3): {e}")))?;
        let mmap3 = unsafe { Mmap::map(&file3) }
            .map_err(|e| CacheError::Validation(format!("mmap packages.csv (pass 3): {e}")))?;

        pkg_count = 0;

        for line in mmap3
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            let Some(fields) = parse_line_fast(line) else {
                continue;
            };
            let prop_id = fields[0];
            let Some(meta) = properties.get(prop_id) else {
                continue;
            };
            let room_type = fields[1];
            let key = (meta.segment.clone(), room_type.to_string());

            let Some(indices) = group.get(&key) else {
                continue;
            };

            // Tiny optimization: skip rate parsing when nobody needs it
            let need_rates = indices
                .iter()
                .any(|&i| !pending[i].query.rate_feature.is_empty());
            let day_rates = if need_rates {
                split_pipe_list(fields[5])
            } else {
                Vec::new()
            };

            let date_str = fields[2];
            let Some(day_num) = date_to_day_num(date_str) else {
                continue;
            };
            let avail = parse_u8_fast(fields[3].as_bytes()).unwrap_or(0);
            let price = parse_u32_fast(fields[4].as_bytes()).unwrap_or(0);

            for &idx in indices {
                let pq = &pending[idx];

                if !meta_matches(meta, &pq.query) {
                    continue;
                }

                let Some(bit) = pq.stay_day_nums.iter().position(|&dn| dn == day_num) else {
                    continue;
                };

                let rates_ok = pq.query.rate_feature.is_empty()
                    || pq
                        .query
                        .rate_feature
                        .iter()
                        .all(|req| day_rates.iter().any(|r| r == req));

                if avail >= pq.query.availability && price <= pq.query.final_price && rates_ok {
                    let mask = coverage[idx].entry(prop_id.to_string()).or_insert(0);
                    *mask |= 1u16 << bit;
                }
            }

            pkg_count += 1;
            if pkg_count % 100_000 == 0 {
                pb.set_position(66 + (pkg_count * 34 / total_rows));
            }
        }

        // Finalize counts
        for (idx, pq) in pending.iter_mut().enumerate() {
            let full_mask = (1u16 << pq.stay_day_nums.len()) - 1;
            let count = coverage[idx].values().filter(|&&m| m == full_mask).count() as u64;
            pq.query.expected_count = count;
        }

        let queries: Vec<BenchmarkQuery> = pending.into_iter().map(|pq| pq.query).collect();
        let out = Output { queries };
        let yaml = serde_yaml::to_string(&out)
            .map_err(|e| CacheError::Validation(format!("yaml: {e}")))?;
        let output_path = format!("{}/query.yml", self.output_dir);
        let mut w = BufWriter::new(File::create(&output_path)?);
        w.write_all(yaml.as_bytes())?;
        w.flush()?;

        pb.set_position(100);
        pb.finish_with_message("Benchmark queries generated.");

        // ------------------------------------------------------------------
        // Also write a simple update.yml next to the search queries
        // ------------------------------------------------------------------
        if let (Some(prop_id), Some(room), Some(date)) =
            (update_property_id, update_room_type, update_date)
        {
            #[derive(Serialize)]
            struct UpdateRecord {
                property_id: String,
                room_type: String,
                date: String,
                amount: u8,
            }

            let update = UpdateRecord {
                property_id: prop_id,
                room_type: room,
                date,
                amount: 5,
            };

            let update_path = std::path::PathBuf::from(format!("{}/update.yml", self.output_dir));
            let update_yaml = serde_yaml::to_string(&update)
                .map_err(|e| CacheError::Validation(format!("update yaml: {e}")))?;
            fs::write(&update_path, update_yaml)?;
        }

        println!();
        println!("✓ Generated {} benchmark queries.", out.queries.len());
        println!("✓ Output written to {}", self.output_dir);
        println!("Completed in {:.2}s.", start.elapsed().as_secs_f64());

        Ok(())
    }

    fn load_properties(&self) -> Result<AHashMap<String, PropMeta>, CacheError> {
        let path = format!("{}/properties.csv", self.input_dir);
        let data = fs::read(&path).map_err(|e| CacheError::Validation(e.to_string()))?;
        let csv = Csv::new(&data).skip_rows(1);
        let mut map = AHashMap::new();
        for result in csv.into_rows::<9>() {
            let record = result.map_err(|e| CacheError::Validation(e.to_string()))?;
            let prop_id = record[0]
                .try_as_str()
                .map_err(|e| CacheError::Validation(e.to_string()))?
                .to_string();
            let amenities = split_pipe_list(
                &record[8]
                    .try_as_str()
                    .map_err(|e| CacheError::Validation(e.to_string()))?,
            );
            map.insert(
                prop_id,
                PropMeta {
                    segment: record[1]
                        .try_as_str()
                        .map_err(|e| CacheError::Validation(e.to_string()))?
                        .to_string(),
                    area: record[2]
                        .try_as_str()
                        .map_err(|e| CacheError::Validation(e.to_string()))?
                        .to_string(),
                    property_type: record[3]
                        .try_as_str()
                        .map_err(|e| CacheError::Validation(e.to_string()))?
                        .to_string(),
                    category: record[4]
                        .try_as_str()
                        .map_err(|e| CacheError::Validation(e.to_string()))?
                        .to_string(),
                    stars: parse_u8_fast(record[5].buf).unwrap_or(0),
                    amenities,
                },
            );
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline]
fn meta_matches(m: &PropMeta, q: &BenchmarkQuery) -> bool {
    m.area == q.area
        && m.category == q.category
        && m.property_type == q.property_type
        && m.stars == q.stars
        && q.amenities
            .iter()
            .all(|a| m.amenities.iter().any(|x| x == a))
}

#[inline]
fn prop_hash(prop_id: &str) -> u64 {
    let mut h: u64 = 0x517cc1b727220a95;
    for byte in prop_id.bytes() {
        h = h.wrapping_mul(0x100000001b3).wrapping_add(byte as u64);
    }
    h
}

#[inline]
fn parse_u8_fast(bytes: &[u8]) -> Option<u8> {
    parse_u32_fast(bytes).and_then(|n| if n <= 255 { Some(n as u8) } else { None })
}

#[inline]
fn parse_u32_fast(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || bytes.len() > 10 {
        return None;
    }
    let mut result = 0u32;
    for &byte in bytes {
        let digit = byte.wrapping_sub(b'0');
        if digit >= 10 {
            return None;
        }
        result = result * 10 + digit as u32;
    }
    Some(result)
}

#[inline]
fn parse_line_fast(line: &[u8]) -> Option<[&str; 6]> {
    let comma_pos = find_first_5_commas(line)?;
    let fields = unsafe {
        [
            str::from_utf8_unchecked(&line[0..comma_pos[0]]),
            str::from_utf8_unchecked(&line[comma_pos[0] + 1..comma_pos[1]]),
            str::from_utf8_unchecked(&line[comma_pos[1] + 1..comma_pos[2]]),
            str::from_utf8_unchecked(&line[comma_pos[2] + 1..comma_pos[3]]),
            str::from_utf8_unchecked(&line[comma_pos[3] + 1..comma_pos[4]]),
            str::from_utf8_unchecked(&line[comma_pos[4] + 1..]),
        ]
    };
    if fields[1].len() > 64 || fields[5].matches('|').count() > 7 {
        return None;
    }
    Some(fields)
}

#[inline]
fn find_first_5_commas(line: &[u8]) -> Option<[usize; 5]> {
    let mut pos = [0; 5];
    let mut found = 0;
    let mut offset = 0;
    while found < 5 && offset < line.len() {
        let remaining_len = line.len() - offset;
        if remaining_len < 16 {
            for (i, &b) in line[offset..].iter().enumerate() {
                if b == b',' {
                    pos[found] = offset + i;
                    found += 1;
                    if found == 5 {
                        return Some(pos);
                    }
                }
            }
            break;
        }
        let ptr = unsafe { line.as_ptr().add(offset) } as *const __m128i;
        let chunk = unsafe { _mm_loadu_si128(ptr) };
        let comma = unsafe { _mm_set1_epi8(b',' as i8) };
        let eq = unsafe { _mm_cmpeq_epi8(chunk, comma) };
        let mask = unsafe { _mm_movemask_epi8(eq) } as u32;
        if mask == 0 {
            offset += 16;
            continue;
        }
        let mut local_mask = mask;
        while local_mask != 0 && found < 5 {
            let bit_pos = local_mask.trailing_zeros() as usize;
            pos[found] = offset + bit_pos;
            found += 1;
            local_mask &= local_mask - 1;
        }
        offset += 16;
    }
    if found == 5 { Some(pos) } else { None }
}

fn split_pipe_list(s: &str) -> Vec<String> {
    s.split('|')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect()
}

fn date_to_day_num(s: &str) -> Option<i32> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
    Some(d.signed_duration_since(epoch).num_days() as i32)
}

fn simple_hash(a: &str, b: &str) -> u64 {
    let mut h: u64 = 0x517cc1b727220a95;
    for byte in a.bytes().chain(b.bytes()) {
        h = h.wrapping_mul(0x100000001b3).wrapping_add(byte as u64);
    }
    h
}

fn select_subset<T: Clone>(items: &[T], seed: u64, max_k: usize) -> Vec<T> {
    if items.is_empty() {
        return Vec::new();
    }
    let k = ((seed % (max_k as u64 + 1)) as usize).min(items.len());
    if k == 0 {
        return Vec::new();
    }
    let mut selected = Vec::with_capacity(k);
    let mut used = AHashSet::with_capacity(k);
    let mut s = seed;
    let max_attempts = items.len() * 4 + 16;
    for _ in 0..max_attempts {
        if selected.len() >= k {
            break;
        }
        s = s
            .wrapping_mul(0x100000001b3)
            .wrapping_add(0x9E3779B97F4A7C15);
        let idx = (s as usize) % items.len();
        if used.insert(idx) {
            selected.push(items[idx].clone());
        }
        if used.len() == items.len() {
            break;
        }
    }
    selected
}

fn select_stay(days: &[PackageDay], seed: u64) -> (String, String, Vec<&PackageDay>) {
    if days.is_empty() {
        return (String::new(), String::new(), Vec::new());
    }
    let desired = 2 + (seed % 6) as usize; // 2..=7
    let mut runs: Vec<Vec<&PackageDay>> = Vec::new();
    let mut cur: Vec<&PackageDay> = Vec::new();
    for d in days {
        if d.avail == 0 {
            if !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if cur.is_empty() || d.day_num == cur.last().unwrap().day_num + 1 {
            cur.push(d);
        } else {
            runs.push(std::mem::take(&mut cur));
            cur.push(d);
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    let mut best: Option<&[&PackageDay]> = None;
    let mut best_len = 0;
    for run in &runs {
        if run.len() >= desired {
            return (
                run[0].date.clone(),
                run[desired - 1].date.clone(),
                run[..desired].to_vec(),
            );
        }
        if run.len() > best_len {
            best_len = run.len();
            best = Some(run.as_slice());
        }
    }
    if let Some(run) = best {
        let len = run.len().min(7).max(1);
        return (
            run[0].date.clone(),
            run[len - 1].date.clone(),
            run[..len].to_vec(),
        );
    }
    (days[0].date.clone(), days[0].date.clone(), vec![&days[0]])
}

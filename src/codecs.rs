// // SPDX-License-Identifier: BUSL-1.1
// // Copyright (c) 2026 M. Javani
// //
// // This file is part of roomzin-bench.
// //
// // Use of this software is governed by the Business Source License 1.1
// // included in the LICENSE file in the root of this repository.

#[derive(Debug, Clone)]
pub struct Codecs {
    pub rate_features: Vec<String>,
}

impl Codecs {
    pub fn rate_feature_index(&self, name: &str) -> Option<usize> {
        self.rate_features
            .iter()
            .position(|r| r.eq_ignore_ascii_case(name))
    }

    /// Convert bitmask → comma-separated list of rate_feature names
    pub fn bitmask_to_rate_feature_string(&self, bitmask: u32) -> String {
        let mut list = Vec::with_capacity(4);
        for (i, rc) in self.rate_features.iter().enumerate() {
            if i >= 24 {
                break;
            }
            if (bitmask & (1u32 << i)) != 0 {
                list.push(rc.as_str());
            }
        }
        list.join(",")
    }
}

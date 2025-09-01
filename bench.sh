#!/usr/bin/env bash
# // SPDX-License-Identifier: BUSL-1.1
# // Copyright (c) 2026 M. Javani
# //
# // This file is part of roomzin-bench.
# //
# // Use of this software is governed by the Business Source License 1.1
# // included in the LICENSE file in the root of this repository.

set -euo pipefail

TOKEN="abc123"
DURATION=1

# Default configurations
SEGMENTS=4
PROPS=25000
ROOM_TYPES=10
DATA_DAYS=60

CONNS_STR="50 100 500 1000"
REQS_STR="10000 15000 20000 25000 30000 35000" # 2000 4000 6000 8000 
SEARCH_DAYS_STR="3 7 14"
LIMITS_STR="300 500"

# Parse command-line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --segments) SEGMENTS="$2"; shift 2 ;;
    --props) PROPS="$2"; shift 2 ;;
    --room-types) ROOM_TYPES="$2"; shift 2 ;;
    --data-days) DATA_DAYS="$2"; shift 2 ;;
    --token) TOKEN="$2"; shift 2 ;;
    --duration) DURATION="$2"; shift 2 ;;
    --conns) CONNS_STR="$2"; shift 2 ;;
    --reqs) REQS_STR="$2"; shift 2 ;;
    --search-days) SEARCH_DAYS_STR="$2"; shift 2 ;;
    --limits) LIMITS_STR="$2"; shift 2 ;;
    *) echo "Unknown option $1"; exit 1 ;;
  esac
done

# Convert strings to arrays
CONNS=($CONNS_STR)
REQS=($REQS_STR)
SEARCH_DAYS=($SEARCH_DAYS_STR)
LIMITS=($LIMITS_STR)

# Calculate approximate records in millions
RECORDS=$((SEGMENTS * PROPS * ROOM_TYPES * DATA_DAYS / 1000000))M

LOG="bench_${RECORDS}_${SEGMENTS}_${PROPS}.log"
> "$LOG"

log_with_header(){
  local phase=$1; shift
  echo "==========  $phase  ==========" | tee -a "$LOG"
  "$@" | tee -a "$LOG"
  echo "" | tee -a "$LOG"
}

# Function to run a benchmark
run_bench() {
  local c=$1 r=$2 d=$3 l=$4
  local header="Benchmark: connections=$c requests=$r search_days=$d limit=$l (RPS approx=$((r / DURATION)))"
  log_with_header "$header" \
    ./rzbench benchmark regular --token "$TOKEN" \
    --connections "$c" --requests "$r" \
    --num-segments "$SEGMENTS" --duration-secs "$DURATION" \
    search --num-days "$d" --limit "$l"
}

# ============================================================================
# Test Section 1: days:3, limit:300, connections:50,100,500,1000, requests: 10k
# ============================================================================
echo "Section 1: days=3, limit=300, requests=10000, varying connections" | tee -a "$LOG"
for c in "${CONNS[@]}"; do
  run_bench "$c" "10000" "3" "300"
done

# ============================================================================
# Test Section 2: days: 7, 14, limit:300, connections: 50, requests: 2k, 4k, 6k, 8k, 10k
# ============================================================================
echo "Section 2: connections=50, limit=300, varying days and requests" | tee -a "$LOG"
for d in "7" "14"; do
  for r in "${REQS[@]}"; do
    run_bench "50" "$r" "$d" "300"
  done
done

# ============================================================================
# Test Section 3: days: 3, 7, limit: 500, connections: 50, requests: 2k, 4k, 6k, 8k, 10k
# ============================================================================
echo "Section 3: connections=50, limit=500, varying days and requests" | tee -a "$LOG"
for d in "3" "7"; do
  for r in "${REQS[@]}"; do
    run_bench "50" "$r" "$d" "500"
  done
done

# ============================================================================
# Test Section: 4 rounds with fixed connections=50, days=3, limit=300
# ============================================================================
# echo "Section: connections=50, days=3, limit=300, varying requests" | tee -a "$LOG"
# for r in "${REQS[@]}"; do
#   run_bench "50" "$r" "3" "300"
# done

echo "Benchmarks complete. Results: $LOG"
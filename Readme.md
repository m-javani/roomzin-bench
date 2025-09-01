# Roomzin Benchmark Tool

A comprehensive benchmarking suite for [Roomzin](https://m-javani.github.io/roomzin-doc/) inventory engine and [RzProxy](https://m-javani.github.io/roomzin-doc/rzproxy.html) HTTP/JSON proxy.

## Features

- **Direct Roomzin Benchmark** – measure TCP/binary protocol performance
- **RzProxy Benchmark** – measure HTTP/JSON proxy performance
- **CSV Data Generator** – create realistic test datasets
- **Query Generator** – automatically generate realistic search and update queries
- **Regular & Spike Load Patterns** – test sustained and burst traffic
- **Detailed Statistics** – latency percentiles, throughput, success rates

## Quick Start

### One-Step Setup

Run the setup script to get everything you need:

```bash
curl -sSL https://raw.githubusercontent.com/m-javani/roomzin-bench/main/scripts/setup.sh | bash
```

This will:
- Download `rzbench`, `roomzin`, and `rzproxy` binaries
- Download all configuration files and scripts
- Create the complete `benchmark-project/` directory structure
- Make all binaries executable
- Verify downloads with checksums

After the script completes, you'll have a ready-to-use benchmark environment.

### 1. Directory Structure

The script creates this directory layout:

```
benchmark-project/
├── config/
│   ├── roomzin.yml        # Roomzin server config
│   └── codecs.yml         # Rate features definition
├── csv/                   # Generated CSV data
│   ├── properties.csv
│   └── packages.csv
├── queries/               # Generated query files
│   ├── query.yml          # Search queries
│   └── update.yml         # Update query
├── data/                  # Snapshot files produced by roomzin
├── scripts/
├── snapshots/
├── roomzin                # Roomzin server binary
├── rzbench                # Benchmark binary
└── rzproxy                 # RzProxy binary
```

---

### 2. Generate Test Data

```bash
cd benchmark-project

./rzbench generate \
    --segments 2 \
    --props-per-segment 2500 \
    --room-types 10 \
    --days 60 \
    --data-dir ./csv
```

**Dataset Structure:**

| File | Fields | Pattern |
|------|--------|---------|
| `properties.csv` | PropertyID, Segment, Area, PropertyType, Category, Stars, Lat, Lon, Amenities | `s{1..N}_p_{1..M}` |
| `packages.csv` | PropertyID, RoomType, Date, Availability, FinalPrice, RateFeature | One row per (property × room_type × day) |

**Naming Rules:**
- **Segments**: `segment_1`, `segment_2`, ... `segment_{segments}`
- **Property IDs**: `s{N}_p_{M}` where N=segment number, M=local property number
- **Room types**: `room_1` through `room_{room-types}`
- **Dates**: Start from today, go forward `days` days
- **Rate features**: From `rate_features` list in `codecs.yml` (combined with `|`)

**Property Attributes** (cycle per segment):
- **PropertyType**: `hotel` → `hostel` → `resort` → repeats
- **Category**: `standard` → `premium` → `budget` → repeats
- **Stars**: Alternates 4, 5, 4, 5...

**Size Formula:**
- Properties = segments × props-per-segment
- Packages = properties × room-types × days

Example: 2 × 2,500 × 10 × 60 = **3,000,000 rows**

### 3. Generate Benchmark Queries

After generating the CSVs, create realistic search and update queries:

```bash
./rzbench gen-queries \
    --csv-dir ./csv \
    --queries-dir ./queries \
    --limit 300 \
    --query-per-key 2
```

Note: limit is the max records returned in each response


This produces:
- `queries/query.yml` – multiple search queries (one per segment/room_type)
- `queries/update.yml` – one realistic update target

**How it works:**
- Streams `packages.csv` twice (discovery + exact expected_count)
- Selects lowest-hash properties per (segment, room_type)
- Builds consecutive available stays
- Writes deterministic, server-compatible queries

### 4. Build Snapshot

```bash
./roomzin build-snapshot \
    --shard-id 1 \
    --input-path ./csv \
    --output-path ./data \
    --codecs ./config/codecs.yml
```

### 5. Run Roomzin Server

```bash
./roomzin run \
    --config ./config/roomzin.yml \
    --codecs ./config/codecs.yml \
    --data-dir ./data
```

Make sure `./config/roomzin.yml` has `data_dir: "./data"`.

### 6. Run RzProxy

RzProxy supports standalone mode for single Roomzin instances:

```bash
./rzproxy
```

> **Note:** For detailed RzProxy configuration and setup, refer to the [RzProxy repository documentation](https://github.com/m-javani/rzproxy).

### 7. Run Benchmarks

#### Roomzin (Direct TCP)

**Regular Mode** – sustained load:

```bash
# Search benchmark
./rzbench roomzin regular \
    --data-dir ./data \
    --queries-dir ./queries \
    --codecs ./config/codecs.yml \
    --connections 1000 \
    --requests 20000 \
    --duration 1 \
    search

# Update benchmark
./rzbench roomzin regular \
    --data-dir ./data \
    --queries-dir ./queries \
    --codecs ./config/codecs.yml \
    --connections 100 \
    --requests 1000 \
    update
```

**Spike Mode** – burst load:

```bash
./rzbench roomzin spike \
    --data-dir ./data \
    --queries-dir ./queries \
    --codecs ./config/codecs.yml \
    --connections 100 \
    --spike-reqs 500 \
    search
```

#### RzProxy (HTTP/JSON Proxy)

**Prerequisites:**
- Roomzin running (standalone mode is fine)
- RzProxy configured and running (see Step 6 above)
- Query files in `./queries/`

```bash
./rzbench rzproxy \
    --url http://127.0.0.1:8777 \
    --connections 200 \
    --duration 2 \
    --data-dir ./data \
    --queries-dir ./queries
```

## Command Reference

### Generate Dataset

| Flag | Description | Default |
|------|-------------|---------|
| `--segments` | Number of segments | Required |
| `--props-per-segment` | Properties per segment | Required |
| `--room-types` | Room types per property | Required |
| `--days` | Number of days | Required |
| `--data-dir` | Output directory for CSVs | `./csv` |
| `--seed` | RNG seed | Random |

### Generate Queries

| Flag | Description | Default |
|------|-------------|---------|
| `-d, --csv-dir` | Directory containing properties.csv + packages.csv | Required |
| `-o, --queries-dir` | Directory for query.yml + update.yml | Required |
| `--query-per-key` | Max queries per (segment, room_type) | 2 |

### Roomzin Benchmark

| Flag | Description | Default |
|------|-------------|---------|
| `--data-dir` | Snapshot directory | `./data` |
| `--queries-dir` | Directory containing query.yml + update.yml | Required |
| `--codecs` | Path to codecs.yml | Required |
| `-c, --connections` | Concurrent connections | 50 |
| `-n, --requests` | Total requests (regular mode) | 1000 |
| `--duration` | Duration in seconds | 1 |
| `--spike-reqs` | Requests per spike (spike mode) | Required |

### RzProxy Benchmark

| Flag | Description | Default |
|------|-------------|---------|
| `--url` | RzProxy endpoint | Required |
| `--connections` | Concurrent connections | 10 |
| `--duration` | Duration in seconds | 60 |
| `--queries-dir` | Directory containing query.yml | Required |

## Customizing Queries

The generated `./queries/query.yml` contains realistic search queries. You can modify it to match your testing scenarios:

```yaml
# Example query configuration
- segment: "segment_1"
  room_type: "room_1"
  dates: ["2025-01-01", "2025-01-02"]
  price_min: 100
  price_max: 300
  amenities: ["wifi", "pool"]
  stars: [4, 5]
  limit: 100
```

The `update.yml` file contains a realistic update target with:
- Specific property ID
- Room type
- Date range
- Availability values

The tool automatically validates query configurations before benchmarking.

## Results Output

- **`responses.json`** – Sample responses from the first successful search requests
- **`http.json`** – Sample HTTP responses (RzProxy benchmarks)
- **Console Output** – Real-time statistics including:
  - Requests per second (RPS)
  - Latency percentiles (p50, p95, p99)
  - Success/failure rates
  - Total requests processed

## Performance Tuning

### Core Configuration

In `./config/roomzin.yml`:

```yaml
core_config:
  sys_cores_count: 0  # Auto (~50% of cores)
  # Or set explicitly based on your workload
```

**Key Considerations:**
- **Too few system cores** → routing/serialization bottlenecks → higher tail latency
- **Too many system cores** → fewer processor cores → lower search throughput
- **Sweet spot** (usually 40–60%) depends on workload; auto default works well for most cases

### TCP Buffer Configuration

```yaml
tcp:
  tcp_recv_buffer_size: 262144   # 256KB default
  tcp_send_buffer_size: 131072  # 128KB default
```

Adjust these based on your dataset size and client reading speed.

## Building from Source (Optional)

If you prefer to build from source instead of using the pre-built binary:

```bash
git clone https://github.com/m-javani/roomzin-bench
cd roomzin-bench
chmod +x ./build.sh
./build.sh
```

## Troubleshooting

### "codecs.yml not found"

Ensure you're using the `--codecs` flag to point to the correct location:
```bash
./rzbench roomzin regular search --codecs ./config/codecs.yml ...
```

### "Query validation failed"

Re-run `gen-queries` so the dates, room types and filters match the current CSV data. Do not edit `query.yml` by hand unless you know the exact data ranges.

### "missing update.yml" / "invalid update.yml"

Run `gen-queries` first. The update target is generated automatically from real property/room/date combinations present in the CSVs.

### RzProxy Connection Issues

Ensure:
- Roomzin is running
- RzProxy is started


## Support

- **Licensing**: mehdy.javany@gmail.com

## License

This tool is licensed under the Business Source License 1.1.
See the LICENSE file in the root of this repository for details.
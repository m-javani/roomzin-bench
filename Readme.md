# Roomzin Benchmark Tool

A comprehensive benchmarking suite for [Roomzin](https://m-javani.github.io/roomzin-doc/) inventory engine and [RzGate](https://m-javani.github.io/roomzin-doc/rzgate.html) HTTP/JSON proxy.

## Features

- **Direct Roomzin Benchmark** – measure TCP/binary protocol performance
- **RzGate Benchmark** – measure HTTP/JSON proxy performance
- **CSV Data Generator** – create realistic test datasets
- **Regular & Spike Load Patterns** – test sustained and burst traffic
- **Custom Query Configuration** – use your own `query.yml` with specific filters
- **Detailed Statistics** – latency percentiles, throughput, success rates

## Quick Start

### One-Step Setup

Run the setup script to get everything you need:

```bash
curl -sSL https://raw.githubusercontent.com/m-javani/roomzin-bench/main/scripts/setup.sh | bash
```

This will:
- Download `rzbench`, `roomzin`, and `rzgate` binaries
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
│   ├── auth.yml           # Authentication tokens
│   └── codecs.yml         # Rate features definition
├── data/                  # Generated CSV data (created by script)
├── queries/
│   └── query.yml          # Search query configuration
├── rzgconf/
│   ├── rzgate.yml         # RzGate config
│   └── auth.yml           # RzGate authentication tokens
├── scripts/
├── snapshots/             # Generated snapshot files (created by script)
├── roomzin                # Roomzin server binary
├── rzbench                # Benchmark binary
└── rzgate                 # RzGate binary
```

### 2. Generate Test Data

Navigate to the benchmark directory and generate test data:

```bash
cd benchmark-project
./rzbench generate \
        --segments 2 \
        --props-per-segment 2500 \
        --room-types 10 \
        --days 60 \
        --data-dir ./csv
```

This reads `codecs.yml` from `./config/` and writes CSVs to `./data/`.

**Dataset Size Formula:**
- Properties = segments × props-per-segment
- Packages = properties × room-types × days

Example: 2 × 2,500 × 10 × 60 = **3,000,000 rows**

### 3. Build Snapshot

```bash
./roomzin build-snapshot \
        --shard-id 1 \
        --input-path ./csv \
        --output-path ./data \
        --codecs ./config/codecs.yml
```

### 4. Run Roomzin Server

```bash
./roomzin run \
        --config ./config/roomzin.yml \
        --codecs ./config/codecs.yml \
        --auth-file ./config/auth.yml \
        --data-dir ./data
```

Make sure `./config/roomzin.yml` has `data_dir: "./snapshots"`.

### 5. Run RzGate

RzGate supports standalone mode for single Roomzin instances:

```bash
./rzgate \
    --config ./rzgconf/rzgate.yml \
    --tokens-path ./rzgconf/auth.yml \
    --http true \
    --https false \
    --auth-enabled true \
    --no-cluster true \
    --roomzin-standalone-host 127.0.0.1
```

> **Note:** For detailed RzGate configuration and setup, refer to the [RzGate repository documentation](https://github.com/m-javani/rzgate).

### 6. Run Benchmarks

#### Roomzin (Direct TCP)

**Regular Mode** – sustained load:

```bash
./rzbench roomzin regular \
    --data-dir ./data \
    --queries-dir ./queries \
    --codecs ./config/codecs.yml \
    --token abc123 \
    --connections 1000 \
    --requests 20000 \
    --duration 1 \
    search
```

```bash
./rzbench roomzin regular \
    --data-dir ./data \
    --queries-dir ./queries \
    --codecs ./config/codecs.yml \
    --token abc123 \
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
    --token abc123 \
    --connections 100 \
    --spike-reqs 500 \
    search
```

#### RzGate (HTTP/JSON Proxy)

**Prerequisites:**
- Roomzin running (standalone mode is fine)
- RzGate configured and running (see Step 5 above)
- Query file in `./queries/query.yml`

```bash
./rzbench rzgate \
    --url http://127.0.0.1:8777 \
    --token rzgate123 \
    --connections 200 \
    --duration 2 \
    --data-dir ./data \
    --queries-dir ./queries
```

## Command Reference

### Roomzin Benchmark Options

| Flag | Description | Default |
|------|-------------|---------|
| `--data-dir` | Directory containing CSV data | `./data` |
| `--codecs` | Path to codecs.yml file | None (fetches from server) |
| `-t, --token` | Authentication token | Required |
| `-c, --connections` | Number of concurrent connections | 50 |
| `-n, --requests` | Total requests to send | 1000 |
| `--duration` | Benchmark duration in seconds | 1 |
| `--spike-reqs` | Requests per spike (spike mode) | None |

### RzGate Benchmark Options

| Flag | Description | Default |
|------|-------------|---------|
| `--url` | RzGate endpoint | Required |
| `--token` | Authentication token | Required |
| `--connections` | Number of concurrent connections | 10 |
| `--duration` | Benchmark duration in seconds | 60 |
| `--data-dir` | Directory containing query.yml | `./data` |

## Customizing Queries

Edit `./queries/query.yml` to match your testing scenarios:

```yaml
# Example query configuration
search_avail:
  segment: "segment_1"
  room_type: "single"
  dates: ["2025-01-01", "2025-01-02"]
  price_min: 100
  price_max: 300
  amenities: ["wifi", "pool"]
  stars: [4, 5]
  limit: 100
```

The tool automatically validates the query configuration before benchmarking.

## Results Output

- **`responses.json`** – Sample responses from the first successful search requests
- **`http.json`** – Sample HTTP responses (RzGate benchmarks)
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

### Prerequisites

- Rust 1.70+
- Cargo

### Build

```bash
git clone https://github.com/m-javani/roomzin-bench
cd roomzin-bench
cargo build --release
```

The binary will be at `target/release/rzbench`.

## Troubleshooting

### "codecs.yml not found"

Ensure you're using the `--codecs` flag to point to the correct location:
```bash
./rzbench roomzin search --codecs ./config/codecs.yml --data-dir ./data ...
```

### "Login failed"

Verify your authentication token matches the one in `./config/auth.yml`:
```yaml
tokens:
  - token: abc123
    role: client
```

### "Connection refused"

Make sure Roomzin is running and the port is correct:
```bash
./roomzin run --config ./config/roomzin.yml --codecs ./config/codecs.yml
```

### "Query validation failed"

Check that the filters in `query.yml` match your actual dataset:
- Property IDs exist
- Room types are valid
- Date ranges are within the data window

### RzGate Connection Issues

Ensure:
- Roomzin is running
- RzGate is started with `--no-cluster` and `--roomzin-standalone-host`
- The URL in the benchmark command matches your RzGate setup (`http://127.0.0.1:8777`)
- If using auth, ensure the token matches `./rzgconf/auth.yml`

### Setup Script Issues

If the setup script fails:
- Ensure you have `curl` and `tar` installed
- Check your internet connection
- Try running the script with `bash -x setup.sh` for debug output

---
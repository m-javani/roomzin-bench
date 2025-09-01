#!/bin/bash
# // SPDX-License-Identifier: BUSL-1.1
# // Copyright (c) 2026 M. Javani
# //
# // This file is part of roomzin-bench.
# //
# // Use of this software is governed by the Business Source License 1.1
# // included in the LICENSE file in the root of this repository.

# Roomzin Benchmark Setup Script
# Downloads all binaries and assets needed for benchmarking

set -e

# ============================================
# VERSION - Update this with each release
# ============================================
LATEST_VERSION="v1.0.0"
# ============================================

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}===============================================================================${NC}"
echo -e "${BLUE}                      ROOMZIN BENCHMARK SETUP${NC}"
echo -e "${BLUE}===============================================================================${NC}"

# Check dependencies
for cmd in curl tar; do
    if ! command -v $cmd &> /dev/null; then
        echo -e "${RED}Error: $cmd is required but not installed.${NC}"
        exit 1
    fi
done

# Create benchmark directory
PROJECT_DIR="benchmark-project"
mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

echo -e "${YELLOW}Downloading assets...${NC}"
# Download assets tarball
curl -L "https://github.com/m-javani/roomzin-bench/releases/download/$LATEST_VERSION/benchmark-assets-$LATEST_VERSION.tar.gz" -o assets.tar.gz

# Extract assets
tar -xzf assets.tar.gz
rm assets.tar.gz

echo -e "${YELLOW}Downloading rzbench binary...${NC}"
# Download rzbench binary
curl -L "https://github.com/m-javani/roomzin-bench/releases/download/$LATEST_VERSION/rzbench" -o rzbench
chmod +x rzbench

echo -e "${YELLOW}Downloading roomzin binary...${NC}"
# Download roomzin binary from roomzin-doc
curl -L "https://github.com/m-javani/roomzin-doc/releases/download/$LATEST_VERSION/roomzin-$LATEST_VERSION" -o roomzin
chmod +x roomzin

echo -e "${YELLOW}Downloading rzgate binary...${NC}"
# Download rzgate binary
curl -L "https://github.com/m-javani/rzgate/releases/download/$LATEST_VERSION/rzgate" -o rzgate
chmod +x rzgate

echo -e "${GREEN}===============================================================================${NC}"
echo -e "${GREEN}✓ Setup complete!${NC}"
echo -e "${GREEN}===============================================================================${NC}"
echo ""
echo -e "Your benchmark environment is ready at: ${BLUE}./$PROJECT_DIR${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "  1. cd $PROJECT_DIR"
echo "  2. ./rzbench generate --segments 2 --props-per-segment 2500 --room-types 10 --days 60"
echo "  3. ./roomzin build-snapshot --shard-id 1 --input-path ./data --output-path ./snapshots --codecs ./config/codecs.yml"
echo "  4. ./roomzin run --config ./config/roomzin.yml --codecs ./config/codecs.yml --auth-file ./config/auth.yml --sys-cores-count 4"
echo ""
echo -e "For detailed instructions, read: ${BLUE}benchmark_guide.txt${NC}"
echo -e "${GREEN}===============================================================================${NC}"
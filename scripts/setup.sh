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

# Download function
download() {
    local url=$1
    local output=$2
    
    echo -e "${YELLOW}Downloading ${output}...${NC}"
    if curl -L -# "$url" -o "$output"; then
        if [[ "$output" != *.tar.gz ]]; then
            chmod +x "$output"
        fi
        echo -e "${GREEN}✓ Downloaded ${output}${NC}"
        return 0
    else
        echo -e "${RED}✗ Failed to download ${output}${NC}"
        return 1
    fi
}

# Download assets
download "https://github.com/m-javani/roomzin-bench/releases/latest/download/benchmark-assets.tar.gz" "assets.tar.gz"

# Extract assets
echo -e "${YELLOW}Extracting assets...${NC}"
tar -xzf assets.tar.gz
rm assets.tar.gz
echo -e "${GREEN}✓ Extracted assets${NC}"

# Download binaries
download "https://github.com/m-javani/roomzin-bench/releases/latest/download/rzbench" "rzbench"
download "https://github.com/m-javani/roomzin-doc/releases/latest/download/roomzin" "roomzin"
download "https://github.com/m-javani/rzproxy/releases/latest/download/rzproxy" "rzproxy"

echo -e "${GREEN}===============================================================================${NC}"
echo -e "${GREEN}✓ Setup complete!${NC}"
echo -e "${GREEN}===============================================================================${NC}"
echo ""
echo -e "Your benchmark environment is ready at: ${BLUE}./$PROJECT_DIR${NC}"
echo ""
echo -e "Binaries downloaded:"
echo -e "  - rzbench (benchmark tool)"
echo -e "  - roomzin (database)"
echo -e "  - rzproxy (HTTP proxy)"
echo -e "  - assets (benchmark data and configs)"
echo ""
echo -e "${YELLOW}Next step:${NC}"
echo -e "For detailed instructions, read: ${BLUE}benchmark_guide.txt${NC}"
echo -e "${GREEN}===============================================================================${NC}"
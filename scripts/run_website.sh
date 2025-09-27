#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBSITE_DIR="$SCRIPT_DIR/../website"
docker build \
    --no-cache \
    -t scout4all-website \
    "$WEBSITE_DIR" \
    --progress=plain
docker run -it -p 1313:1313 scout4all-website
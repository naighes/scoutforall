#!/usr/bin/env bash
# The tests as the CI runs them: in several time zones, since the runners are
# in UTC and our machines are not (a test passing only in our own time zone
# broke the build once). Built once, run in each.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo nextest run --workspace --locked --all-features --no-run
for tz in UTC Europe/Rome Asia/Tokyo America/Los_Angeles; do
  echo "== TZ=$tz"
  TZ="$tz" cargo nextest run --workspace --locked --all-features "$@"
done

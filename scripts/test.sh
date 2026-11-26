#!/usr/bin/env bash
# The tests as the CI runs them: in several time zones, since the runners are
# in UTC and our machines are not (a test passing only in our own time zone
# broke the build once). Built once, run in each.
# Then the tests of the Android app (scripts/test-android.sh), only for a whole
# run on our machines: the CI has a job of its own for them, and arguments
# pick Rust tests.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo nextest run --workspace --locked --all-features --no-run
for tz in UTC Europe/Rome Asia/Tokyo America/Los_Angeles; do
  echo "== TZ=$tz"
  TZ="$tz" cargo nextest run --workspace --locked --all-features "$@"
done
case "${CI:-}:$#" in
  :0) scripts/test-android.sh ;;
esac

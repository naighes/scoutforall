#!/usr/bin/env bash
# Takes the stale build artifacts out of target/. Cargo never does: every
# change of features, flags or profile (and every run of rust-analyzer,
# clippy or the tests) makes new binaries with another hash and leaves the
# old ones, and target/ grows into hundreds of GB (200 GB once, with dozens
# of 300 MB copies of the desktop executable, most of it written in a few
# days).
#
#     scripts/clean-target.sh              what would go, and how much: it
#                                          removes nothing
#     scripts/clean-target.sh --apply      removes it
#     scripts/clean-target.sh --days N     the age of what is stale, by the
#                                          time of the last change (default
#                                          7)
#     scripts/clean-target.sh --all        `cargo clean`, the whole target/
#                                          (with --apply)
#
# What is stale, in every profile and target (debug, release, mobile, the
# Android ones, ...): the files of deps/ and examples/ older than N days, and
# the sessions of incremental/ older than N days. build/ and .fingerprint/
# stay: cargo rebuilds what is missing from deps/, and a build script is
# never run again for a result it believes is there. The next build
# recompiles what was taken out (the dependencies, if they are among it).
#
# The target/ is CARGO_TARGET_DIR if set.
set -euo pipefail
cd "$(dirname "$0")/.."

days=7
apply=0
all=0
while [ $# -gt 0 ]; do
  case "$1" in
    --apply) apply=1 ;;
    --all) all=1 ;;
    --days)
      shift
      days="${1:-}"
      case "$days" in
        '' | *[!0-9]*) echo "--days wants a number of days" >&2; exit 2 ;;
      esac
      ;;
    -h | --help) sed -n '2,/^set /p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1 (see the top of $0)" >&2; exit 2 ;;
  esac
  shift
done

target="${CARGO_TARGET_DIR:-target}"
if [ ! -d "$target" ]; then
  echo "nothing to clean: no $target"
  exit 0
fi

# size of the paths, in GB
size_of() { du -sk "$@" 2>/dev/null | awk '{s+=$1} END {printf "%.1f", s/1048576}'; }

before=$(du -sk "$target" | awk '{print $1}')
echo "$target: $(size_of "$target") GB"

if [ "$all" -eq 1 ]; then
  if [ "$apply" -eq 1 ]; then
    cargo clean
  else
    echo "would run \`cargo clean\` ($(size_of "$target") GB)"
  fi
else
  # deps/ and examples/ sit at target/<profile>/ or target/<triple>/<profile>/
  stale_files=0
  while IFS= read -r -d '' dir; do
    n=$(find "$dir" -type f -mtime +"$days" | wc -l | tr -d ' ')
    if [ "$n" -gt 0 ]; then
      kb=$(find "$dir" -type f -mtime +"$days" -print0 | xargs -0 du -sk | awk '{s+=$1} END {print s+0}')
      printf '%-55s %7d files %7.1f GB\n' "$dir" "$n" "$(echo "$kb" | awk '{print $1/1048576}')"
      stale_files=$((stale_files + n))
      if [ "$apply" -eq 1 ]; then
        find "$dir" -type f -mtime +"$days" -delete
      fi
    fi
  done < <(find "$target" -maxdepth 3 -type d \( -name deps -o -name examples \) -print0)

  # incremental/<crate>-<hash>/<session>: the old sessions are garbage, cargo
  # starts a new one from the last
  stale_sessions=0
  session_kb=0
  while IFS= read -r -d '' inc; do
    while IFS= read -r -d '' session; do
      session_kb=$((session_kb + $(du -sk "$session" | awk '{print $1}')))
      stale_sessions=$((stale_sessions + 1))
      if [ "$apply" -eq 1 ]; then
        rm -rf "$session"
      fi
    done < <(find "$inc" -mindepth 2 -maxdepth 2 -type d -mtime +"$days" -print0)
  done < <(find "$target" -maxdepth 3 -type d -name incremental -print0)
  printf '%-55s %7d sessions %5.1f GB\n' "incremental/" "$stale_sessions" "$(echo "$session_kb" | awk '{print $1/1048576}')"

  echo "stale: $stale_files files and $stale_sessions incremental sessions older than $days days"
fi

if [ "$apply" -eq 1 ]; then
  after=$(du -sk "$target" 2>/dev/null | awk '{print $1}')
  echo "freed $(echo "$before $after" | awk '{printf "%.1f", ($1-$2)/1048576}') GB; $target: $(size_of "$target") GB"
else
  echo "nothing removed: --apply does it"
fi

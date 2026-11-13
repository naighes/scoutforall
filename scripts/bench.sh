#!/usr/bin/env bash
# How long the app takes on large archives (the bench crate): the reads of
# the teams and the matches, the statistics of a team, the scouting of a
# match in progress. The archives are made of real matches, the DataVolley
# files of the 2021 MEVZA league (men and women), downloaded once into a
# cache and never committed: they aren't ours to share.
#
#     scripts/bench.sh                     the table of the times, on the
#                                          archives 1x300 and 10x100
#     scripts/bench.sh generate DIR [TxM]  an archive of T teams of M matches
#                                          each (default 1x300) kept in DIR,
#                                          e.g. to try the apps on it
#     scripts/bench.sh measure DIR         the reads on DIR, e.g. a copy of
#                                          ~/.scout4all: the cache of the
#                                          lists is emptied (and rebuilt),
#                                          to time them without it
#     scripts/bench.sh scouting DIR        the scouting of the match in
#                                          progress of an archive made by
#                                          `generate` (it changes its last
#                                          event back and forth)
#     scripts/bench.sh reports DIR OUT     the statistics of every match and
#                                          team of DIR, as files in OUT: run
#                                          before and after a change, then
#                                          `diff -r` (e.g. on an archive of
#                                          every real match, `generate DIR
#                                          162x1`, and on a copy of
#                                          ~/.scout4all); the lists too, and
#                                          the analysis of the last 3 and 10
#                                          matches and of October 2026. The
#                                          matches migrated are named by
#                                          their id of before the database,
#                                          so OUT compares with reports taken
#                                          on the folders
#
# DIR is opened as the apps open it: a folder of before the database is
# migrated into it (its backup zip beside it), so give a copy.
#
# The table is Markdown, to paste where the times are compared. Built in
# release, as the apps; run it with the machine otherwise idle, and compare
# only times taken on the same machine.
# The files: $SCOUT4ALL_DVW, else ~/.cache/scout4all/dvw (or under
# $XDG_CACHE_HOME). Needs: cargo, curl.
set -euo pipefail
cd "$(dirname "$0")/.."

dvw="${SCOUT4ALL_DVW:-${XDG_CACHE_HOME:-$HOME/.cache}/scout4all/dvw}"
# the download pages of the MEVZA, women and men
pages=(https://mevza.volleynet.at/Download/58 https://mevza.volleynet.at/Download/59)

download() {
  mkdir -p "$dvw"
  local page link file
  for page in "${pages[@]}"; do
    # the links are /dl/<the path of the file in base64>
    for link in $(curl -sfL -A scout4all-bench "$page" | grep -o 'href="[^"]*/dl/[^"]*"' | cut -d '"' -f 2); do
      file="$(printf '%s' "${link##*/dl/}" | base64 -d 2>/dev/null || true)"
      case "$file" in
        *.dvw) ;;
        *) continue ;;
      esac
      file="$(basename "$file" | tr -d '&')"
      [ -s "$dvw/$file" ] && continue
      curl -sfL -A scout4all-bench -o "$dvw/$file.part" "$link" && mv "$dvw/$file.part" "$dvw/$file"
    done
  done
}

case "${1:-}" in
  "" | generate) [ -n "$(ls "$dvw"/*.dvw 2>/dev/null)" ] || download ;;
  measure | scouting | reports) ;;
  *) sed -n '2,/^set /p' "$0" | sed '$d' | cut -c 3-; exit 2 ;;
esac

cargo build --release --locked --package scout4all-bench
bench=target/release/scout4all-bench
case "${1:-}" in
  "") "$bench" "$dvw" ;;
  generate) shift; "$bench" generate "$dvw" "$@" ;;
  measure | scouting | reports) "$bench" "$@" ;;
esac

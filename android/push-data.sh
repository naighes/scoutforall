#!/bin/sh
# Copies a data folder (default: ~/.scout4all) into the debug app on the
# device or emulator, in place of its data: the layout is the same on every
# platform. The database is copied with SQLite's backup (the desktop may
# have it open), the recordings of the desktop are left out; a folder of
# before the database is migrated by the app when it starts.
# Needs: sqlite3, rsync.
set -eu

src="${1:-$HOME/.scout4all}"
app="io.github.naighes.scout4all"
adb="${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb"
tmp="/data/local/tmp/scout4all"
db="scout4all.db"

[ -d "$src" ] || { echo "$src: no data folder" >&2; exit 1; }
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
rsync -a --exclude "/$db" --exclude "/$db-wal" --exclude "/$db-shm" --exclude "/$db-journal" \
  --exclude /recordings "$src/" "$stage/scout4all/"
[ ! -f "$src/$db" ] || sqlite3 "$src/$db" ".backup '$stage/scout4all/$db'"

"$adb" shell rm -rf "$tmp"
"$adb" push "$stage/scout4all" "$tmp"
"$adb" shell run-as "$app" sh -c "'rm -rf files/scout4all && mkdir -p files && cp -r $tmp files/scout4all'"
"$adb" shell rm -rf "$tmp"
"$adb" shell am force-stop "$app"
echo "copied $src into $app"

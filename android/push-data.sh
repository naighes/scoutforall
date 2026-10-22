#!/bin/sh
# Copies a data folder (default: ~/.scout4all) into the debug app on the
# device or emulator: the layout is the same on every platform.
set -eu

src="${1:-$HOME/.scout4all}"
app="io.github.naighes.scout4all"
adb="${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb"
tmp="/data/local/tmp/scout4all"

"$adb" shell rm -rf "$tmp"
"$adb" push "$src" "$tmp"
"$adb" shell run-as "$app" sh -c "'rm -rf files/scout4all && mkdir -p files && cp -r $tmp files/scout4all'"
"$adb" shell rm -rf "$tmp"
"$adb" shell am force-stop "$app"
echo "copied $src into $app"

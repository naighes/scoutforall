#!/usr/bin/env bash
# Builds the Android app and installs it on the phone connected over adb (USB
# debugging on), keeping its data, then opens it. The release build by default:
# the debug one is far slower. Needs what android/README.md asks to build.
# Usage: scripts/install-android.sh [--debug]
# With more than one device, choose it with ANDROID_SERIAL (see `adb devices`).
set -euo pipefail
cd "$(dirname "$0")/../android"

case "${1:-}" in
  "") variant=release ;;
  --debug) variant=debug ;;
  *) echo "usage: $0 [--debug]" >&2; exit 2 ;;
esac

adb=$(command -v adb || echo "${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb")
[ -x "$adb" ] || { echo "adb not found: set ANDROID_HOME" >&2; exit 1; }
# before building: no point without a phone
"$adb" get-state >/dev/null || { echo "no phone ready (or more than one): see 'adb devices'" >&2; exit 1; }

case "$variant" in
  release) ./gradlew assembleRelease ;;
  debug) ./gradlew assembleDebug ;;
esac

# a signature different from the installed one makes this fail: uninstalling
# would lose the data, so it's left to who runs the script
"$adb" install -r "app/build/outputs/apk/$variant/app-$variant.apk"
"$adb" shell monkey -p io.github.naighes.scout4all -c android.intent.category.LAUNCHER 1 >/dev/null
echo "installed ($variant) and opened"

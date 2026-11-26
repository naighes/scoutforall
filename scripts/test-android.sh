#!/usr/bin/env bash
# The tests of the Android app as the CI runs them: on the JVM with
# Robolectric, no emulator, with the core built for this machine. Needs a JDK
# and the Android SDK. Arguments go to Gradle (e.g. --tests '*PlayerFormTest').
set -euo pipefail
cd "$(dirname "$0")/../android"
./gradlew testDebugUnitTest "$@"

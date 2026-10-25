# scout4all for Android

A prototype: the app lists the teams. The domain is the Rust core of the
workspace (`core/`), exposed to Kotlin by the `mobile/` crate with
[UniFFI](https://mozilla.github.io/uniffi-rs/); the UI is Jetpack Compose.

## Requirements

- Android Studio, with the NDK (SDK Manager → SDK Tools → NDK)
- Rust (rustup) with the Android target and cargo-ndk:

  ```sh
  rustup target add aarch64-linux-android
  cargo install cargo-ndk
  ```

## Build and run

Open this folder in Android Studio and run the `app` configuration, or:

```sh
./gradlew installDebug
```

The build runs cargo: `cargoNdk` builds the core for Android (`arm64-v8a`)
and `uniffiBindgen` generates the Kotlin bindings, both under `app/build`.

## Tests

```sh
./gradlew testDebugUnitTest
```

The screens run on the JVM with Robolectric, no emulator: they call the real
core, built for this machine by `uniffiBindgen` and loaded by JNA, with the
data of each test in an empty folder.

## Data

The app keeps its data in `files/scout4all` in its sandbox, with the same
layout as `~/.scout4all`. To try it with the data of the TUI (debug builds
only):

```sh
./push-data.sh            # or ./push-data.sh <folder>
```

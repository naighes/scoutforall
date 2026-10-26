# scout4all for Android

The app scouts matches on the phone, as the CLI does in the terminal. The domain is the Rust core of the
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

## Release

The version is the one of the workspace (`[workspace.package]` of
`Cargo.toml`): the app is released with the CLI, from the same tag, and the
versionCode follows from it (0.9.0 is 900). The release is shrunk by R8
(`app/proguard-rules.pro` keeps what JNA and UniFFI find by name).

```sh
./gradlew bundleRelease     # the bundle for Play
./gradlew assembleRelease   # an apk, to try it on a phone
```

Both are signed with the upload key, from these entries of
`local.properties` (or the environment variables `UPLOAD_STORE_FILE`,
`UPLOAD_STORE_PASSWORD`, `UPLOAD_KEY_ALIAS`, `UPLOAD_KEY_PASSWORD`):

```properties
upload.store.file=/path/to/upload.jks
upload.store.password=…
upload.key.alias=upload
upload.key.password=…
```

Without them the debug key signs the release: it installs over a debug
build, keeping its data, and Play refuses it. From a tag, the workflow
`publish-android.yml` builds the signed bundle with the secrets of the
repository.

The texts of the Play listing, the icon and the feature graphic are in
`fastlane/metadata/android` (`fastlane/graphics.py` draws the images from
the vectors of the app).

## Data

The app keeps its data in `files/scout4all` in its sandbox, with the same
layout as `~/.scout4all`. To try it with the data of the TUI (debug builds
only):

```sh
./push-data.sh            # or ./push-data.sh <folder>
```

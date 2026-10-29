# Mobile samples

Each sample is a native app project that builds one of the repo's examples with cargo and runs it
as the whole app. Both need the `mobile` feature, which the projects turn on.

| | Project | App code | Entry point |
| --- | --- | --- | --- |
| Android | [`android/`](android/) (Gradle) | [`examples/android_demo.rs`](../examples/android_demo.rs) | `aurea_main`, called by `AureaActivity` |
| iOS and iPadOS | [`ios/`](ios/) (Xcode) | [`examples/ios_demo.rs`](../examples/ios_demo.rs) | `aurea_main`, called by `main.m` |

## Android

Needs the Android SDK with the NDK version `app/build.gradle.kts` pins, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`.

```bash
cd mobile/android
./gradlew installDebug                    # CPU drawing
./gradlew installDebug -Paurea.gpu=true   # GPU through ZenGPU's Vulkan, the CPU if it cannot start
```

The `aurea` module holds `dev.aurea.AureaActivity`, the Activity an app runs in. An app's own
Activity subclasses it and names its native library; the library defines `aurea_main`, which calls
`aurea::android::run`.

An app's Cargo.toml has a `cdylib` example that is the library, and `aurea/aurea-cargo.gradle`
builds it with cargo for each ABI into `jniLibs`. The sample's is
[`examples/android_demo.rs`](../examples/android_demo.rs).

An app's identity comes from its Cargo.toml table `[package.metadata.aurea]`: `name` (label), `id`
(application ID), `version` (`"auto"` for the package version, or a string) and optional `build`
(versionCode). `aurea/aurea-identity.gradle` reads the table with `cargo metadata`.

An app's `settings.gradle.kts` includes the `aurea` module from its aurea crate, found through the
app's Cargo.toml (this repo's copy resolves to `mobile/android/aurea`):

```kotlin
import groovy.json.JsonSlurper

// ...pluginManagement, dependencyResolutionManagement, rootProject.name...
include(":app")

val aureaCargo = JsonSlurper().parseText(
    providers.exec {
        commandLine("cargo", "metadata", "--format-version", "1", "--manifest-path", file("../../Cargo.toml").path)
    }.standardOutput.asText.get(),
) as Map<*, *>
val aurea = (aureaCargo["packages"] as List<*>).map { it as Map<*, *> }.first { it["name"] == "aurea" }
include(":aurea")
project(":aurea").projectDir = File(aurea["manifest_path"] as String).parentFile.resolve("mobile/android/aurea")
```

The app's `app/build.gradle.kts` sets the cargo build and identity, then applies both scripts from
the `aurea` module:

```kotlin
extra["aurea.manifest"] = rootProject.file("../../Cargo.toml")
apply(from = project(":aurea").file("aurea-identity.gradle"))

extra["aurea.cargo.manifest"] = rootProject.file("../../Cargo.toml")
extra["aurea.cargo.lib"] = "android_demo"
extra["aurea.cargo.features"] = if (findProperty("aurea.gpu") == "true") "mobile,zengpu" else "mobile"
extra["aurea.cargo.abis"] = listOf("arm64-v8a", "x86_64")
extra["aurea.cargo.profile"] = "release"
apply(from = project(":aurea").file("aurea-cargo.gradle"))
```

`aurea.cargo.abis` defaults to `arm64-v8a`; `aurea-cargo.gradle` knows `arm64-v8a` and `x86_64`.

## iOS and iPadOS

Needs Xcode and the Rust targets for where it runs: `aarch64-apple-ios` for devices,
`aarch64-apple-ios-sim` for the simulator on Apple silicon (`x86_64-apple-ios` on Intel). A build
phase runs cargo and links the example as a static library.

Open `mobile/ios/AureaSmoke.xcodeproj`, pick your team under Signing & Capabilities, and run. A
free personal team works on your own devices; its apps last seven days, and the first launch
needs Settings → General → VPN & Device Management → Trust. From the command line:

```bash
cd mobile/ios
xcodebuild -scheme AureaSmoke -destination 'platform=iOS,name=<device>' \
    -allowProvisioningUpdates DEVELOPMENT_TEAM=<team id> build
```

Pass `AUREA_FEATURES=zengpu` to build it drawing on the GPU.

If the bundle ID `com.aurea.smoke` is taken for your team, pass another with
`PRODUCT_BUNDLE_IDENTIFIER=`.

### What the iOS host gives the app

`aurea::ios::run` draws the UI into the screen's safe area; the theme's background fills the rest.
With the `zengpu` feature it draws on the GPU through ZenGPU's Metal backend, and on the CPU
otherwise, or when the GPU cannot start (`aurea::ios::use_gpu(false)` asks for the CPU);
`aurea::ios::gpu_context()` shares the GPU with the app. The CPU path uploads only the tiles a frame
changed. The app hears, through `aurea::ios::on_event`:

- **Touches as the mouse.** An Apple Pencil presses at once. A finger presses once it moves, stays
  down a moment, or lifts (a tap), so a second finger can still turn it into a gesture.
- **Two-finger gestures.** Moving together scrolls (`MouseWheel`, content following the fingers);
  spreading or closing zooms (`Magnify`), and turning rotates (`Rotate`). `ui::PanZoom` handles all three, turning only with `rotate_with_gestures` set.
- **Apple Pencil samples** as `Pen`, with pressure, tilt, twist and hover, and its double tap or
  Pencil Pro squeeze as `PenTap`, with what the user set it to do in Settings.
- **A hardware keyboard** as `KeyInput` and `TextInput`.
- **The on-screen keyboard**, while `aurea::ios::show_keyboard(true)` holds it up, as `TextInput`,
  with delete and return as Backspace and Enter, and what an input method such as Korean or
  Japanese is still composing as `ImePreedit`. `keyboard_height()` says how much it covers, and
  `set_text_area` where candidate lists should show.
- **The lifecycle.** `Focused` and `Unfocused`; `Minimized` on going to the background, where iOS
  may stop the app without warning, so save there; `Restored`; and `MemoryWarning`.

Work on other threads asks for a frame with `aurea::ios::request_frame()`, or through
`aurea::ios::canvas_id()` with `request_canvas_redraw` like a desktop canvas. The display link
pauses when nothing changes. `aurea::ios::scale()` and `safe_area()` describe the screen.

`aurea::ios::pick_files` and `export_files` show the system document picker, `share_files` the
share sheet, and `alert` an alert with the same buttons as a desktop `MessageDialog`. None of them
blocks: `done` hears the answer later. Files picked to open are copies in the sandbox. The
clipboard carries text, images and the app's own byte formats, as on the desktop.

`aurea::config_dir`, `data_dir` and `cache_dir` live in the app's `Library`, and `documents_dir()`
in its `Documents`, which the Files app shows when the Info.plist sets `UIFileSharingEnabled` and
`LSSupportsOpeningDocumentsInPlace`.

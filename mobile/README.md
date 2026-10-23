# Mobile samples

Each sample is a native app project that builds one of the repo's examples with cargo and runs it
as the whole app. Both need the `mobile` feature, which the projects turn on.

| | Project | App code | Entry point |
| --- | --- | --- | --- |
| Android | [`android/`](android/) (Gradle) | [`examples/android_demo.rs`](../examples/android_demo.rs) | JNI functions the Activity calls |
| iOS and iPadOS | [`ios/`](ios/) (Xcode) | [`examples/ios_demo.rs`](../examples/ios_demo.rs) | `aurea_main`, called by `main.m` |

## Android

Needs the Android SDK with the NDK version `app/build.gradle.kts` pins, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`.

```bash
cd mobile/android
./gradlew installDebug
```

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

If the bundle ID `com.aurea.smoke` is taken for your team, pass another with
`PRODUCT_BUNDLE_IDENTIFIER=`.

### What the iOS host gives the app

`aurea::ios::run` draws the UI on the CPU into the screen's safe area; the theme's background fills
the rest. The app hears, through `aurea::ios::on_event`:

- **Touches as the mouse.** An Apple Pencil presses at once. A finger presses once it moves, stays
  down a moment, or lifts (a tap), so a second finger can still turn it into a gesture.
- **Two-finger gestures.** Moving together scrolls (`MouseWheel`, content following the fingers);
  spreading or closing zooms (`Magnify`). `ui::PanZoom` handles both.
- **Apple Pencil samples** as `Pen`, with pressure, tilt, twist and hover.
- **A hardware keyboard** as `KeyInput` and `TextInput`.
- **The lifecycle.** `Focused` and `Unfocused`; `Minimized` on going to the background, where iOS
  may stop the app without warning, so save there; `Restored`; and `MemoryWarning`.

Work on other threads asks for a frame with `aurea::ios::request_frame()`, or through
`aurea::ios::canvas_id()` with `request_canvas_redraw` like a desktop canvas. The display link
pauses when nothing changes. `aurea::ios::scale()` and `safe_area()` describe the screen.

`aurea::config_dir`, `data_dir` and `cache_dir` live in the app's `Library`, and `documents_dir()`
in its `Documents`, which the Files app shows when the Info.plist sets `UIFileSharingEnabled` and
`LSSupportsOpeningDocumentsInPlace`.

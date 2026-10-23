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

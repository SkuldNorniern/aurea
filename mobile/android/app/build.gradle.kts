plugins {
    id("com.android.application")
}

// The app's identity (id, name, version) comes from this Cargo manifest's [package.metadata.aurea].
extra["aurea.manifest"] = rootProject.file("../../Cargo.toml")
apply(from = project(":aurea").file("aurea-identity.gradle"))

// `-Paurea.gpu=true` builds with the `zengpu` feature: Vulkan drawing, the CPU when it can't start.
extra["aurea.cargo.manifest"] = rootProject.file("../../Cargo.toml")
extra["aurea.cargo.lib"] = "android_demo"
extra["aurea.cargo.features"] = if (findProperty("aurea.gpu") == "true") "mobile,zengpu" else "mobile"
extra["aurea.cargo.abis"] = listOf("arm64-v8a", "x86_64")
extra["aurea.cargo.profile"] = "release"
apply(from = project(":aurea").file("aurea-cargo.gradle"))

android {
    namespace = "com.aurea.smoke"
    compileSdk = 37
    ndkVersion = "30.0.16248370"

    defaultConfig {
        minSdk = 24
        targetSdk = 37
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(project(":aurea"))
}

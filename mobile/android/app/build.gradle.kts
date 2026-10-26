import javax.inject.Inject

plugins {
    id("com.android.application")
}

// The app's identity (id, name, version) comes from this Cargo manifest's [package.metadata.aurea].
extra["aurea.manifest"] = rootProject.file("../../Cargo.toml")
apply(from = "../aurea/aurea-identity.gradle")

// Rust targets per Android ABI, built with the NDK this project pins.
val rustAbis = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android",
)
val androidMinApi = 24
// `-Paurea.gpu=true` builds with the `zengpu` feature: Vulkan drawing, the CPU when it can't start.
val cargoFeatures = if (findProperty("aurea.gpu") == "true") "mobile,zengpu" else "mobile"

android {
    namespace = "com.aurea.smoke"
    compileSdk = 37
    ndkVersion = "30.0.16248370"

    defaultConfig {
        minSdk = androidMinApi
        targetSdk = 37
        ndk {
            abiFilters += rustAbis.keys
        }
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
    sourceSets.getByName("main").jniLibs.directories.add(
        layout.buildDirectory.dir("rustJniLibs").get().asFile.path,
    )
}

/** Builds the demo app's library for every ABI with cargo and the NDK's clang. */
abstract class CargoBuild @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:Internal abstract val repoRoot: DirectoryProperty
    @get:Internal abstract val ndkDir: DirectoryProperty
    @get:Internal abstract val outDir: DirectoryProperty
    @get:Internal abstract val abis: MapProperty<String, String>
    @get:Internal abstract val minApi: Property<Int>
    @get:Internal abstract val features: Property<String>

    @TaskAction
    fun build() {
        val os = System.getProperty("os.name").lowercase()
        val (host, script, exe) = when {
            os.contains("windows") -> Triple("windows-x86_64", ".cmd", ".exe")
            os.contains("mac") -> Triple("darwin-x86_64", "", "")
            else -> Triple("linux-x86_64", "", "")
        }
        val bin = ndkDir.get().asFile.resolve("toolchains/llvm/prebuilt/$host/bin")
        val root = repoRoot.get().asFile
        val targetDir = root.resolve("target/android")
        for ((abi, triple) in abis.get()) {
            val clang = bin.resolve("$triple${minApi.get()}-clang$script").path
            val env = triple.replace('-', '_')
            exec.exec {
                workingDir = root
                commandLine(
                    "cargo", "build", "--example", "android_demo", "--release",
                    "--features", features.get(), "--target", triple,
                )
                environment("CC_$env", clang)
                environment("AR_$env", bin.resolve("llvm-ar$exe").path)
                environment("CARGO_TARGET_${env.uppercase()}_LINKER", clang)
                environment("CARGO_TARGET_DIR", targetDir.path)
            }
            val so = targetDir.resolve("$triple/release/examples/libandroid_demo.so")
            val dest = outDir.get().asFile.resolve(abi)
            dest.deleteRecursively()
            dest.mkdirs()
            so.copyTo(dest.resolve("libandroid_demo.so"), overwrite = true)
        }
    }
}

val cargoBuild = tasks.register<CargoBuild>("cargoBuild") {
    repoRoot.set(rootProject.layout.projectDirectory.dir("../.."))
    ndkDir.set(androidComponents.sdkComponents.ndkDirectory)
    outDir.set(layout.buildDirectory.dir("rustJniLibs"))
    abis.set(rustAbis)
    minApi.set(androidMinApi)
    features.set(cargoFeatures)
}

dependencies {
    implementation(project(":aurea"))
}

tasks.named("preBuild") {
    dependsOn(cargoBuild)
}

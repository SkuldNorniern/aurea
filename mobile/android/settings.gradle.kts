import groovy.json.JsonSlurper

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "aurea-smoke"
include(":app")

// Includes aurea's Android module from the aurea crate that the app's Cargo.toml depends on.
val aureaCargo = JsonSlurper().parseText(
    providers.exec {
        commandLine("cargo", "metadata", "--format-version", "1", "--manifest-path", file("../../Cargo.toml").path)
    }.standardOutput.asText.get(),
) as Map<*, *>
val aurea = (aureaCargo["packages"] as List<*>).map { it as Map<*, *> }.first { it["name"] == "aurea" }
include(":aurea")
project(":aurea").projectDir = File(aurea["manifest_path"] as String).parentFile.resolve("mobile/android/aurea")

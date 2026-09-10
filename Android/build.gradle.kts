plugins {
    id("com.android.application") version "9.4.0" apply false
}

// CodeQL Default Setup uses Gradle autobuild for Kotlin and invokes the generic
// testClasses task. Android Gradle Plugin does not create that lifecycle task,
// so map it to an Android debug build that compiles the Java/Kotlin sources.
tasks.register("testClasses") {
    group = "verification"
    description = "Build Android JVM/Kotlin classes for CodeQL autobuild"
    dependsOn(":app:assembleDebug")
}

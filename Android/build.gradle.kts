buildscript {
    repositories {
        google()
        mavenCentral()
    }
    configurations.classpath {
        resolutionStrategy.force(
            "org.bouncycastle:bcprov-jdk18on:1.86",
            "org.bouncycastle:bcpkix-jdk18on:1.86",
            "org.bouncycastle:bcutil-jdk18on:1.86",
        )
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.4.1")
    }
}

// Android Gradle Plugin 9.4.1 currently resolves Bouncy Castle 1.80.2 on the
// build-tool classpath. Keep AGP stable while overriding only its vulnerable
// build-time crypto modules. This does not add Bouncy Castle to the app runtime.

// CodeQL Default Setup uses Gradle autobuild for Kotlin and invokes the generic
// testClasses task. Android Gradle Plugin does not create that lifecycle task,
// so map it to an Android debug build that compiles the Java/Kotlin sources.
tasks.register("testClasses") {
    group = "verification"
    description = "Build Android JVM/Kotlin classes for CodeQL autobuild"
    dependsOn(":app:assembleDebug")
}

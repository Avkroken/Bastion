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
            "org.bitbucket.b_c:jose4j:0.9.7",
            "org.jdom:jdom2:2.0.6.1",
        )
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.4.1")
    }
}

// Android Gradle Plugin 9.4.1 currently requests vulnerable build-tool
// transitive dependencies (Bouncy Castle 1.80.2, jose4j 0.9.5 and JDOM 2.0.6). Keep AGP
// stable while forcing patched buildscript-classpath versions (including JDOM 2.0.6.1). These overrides
// do not add either library to the application runtime.

// CodeQL Default Setup uses Gradle autobuild for Kotlin and invokes the generic
// testClasses task. Android Gradle Plugin does not create that lifecycle task,
// so map it to an Android debug build that compiles the Java/Kotlin sources.
tasks.register("testClasses") {
    group = "verification"
    description = "Build Android JVM/Kotlin classes for CodeQL autobuild"
    dependsOn(":app:assembleDebug")
}

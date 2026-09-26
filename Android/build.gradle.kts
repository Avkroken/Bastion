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

        // AGP exposes these as transitive build-tool dependencies, but GitHub
        // dependency submission can alert on transitive Gradle dependencies
        // without Dependabot being able to update them in the repository.
        // Keep them explicit so security fixes remain both enforced and
        // machine-updatable without removing dependency-graph coverage.
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.20")
        classpath("org.apache.commons:commons-lang3:3.18.0")
    }
}

// Android Gradle Plugin 9.4.1 currently requests build-tool transitive
// dependencies with known vulnerabilities. Keep AGP stable while selecting
// patched buildscript-classpath versions. Explicit classpath entries above
// make Kotlin Gradle Plugin and Commons Lang directly updateable by Dependabot;
// the forced entries cover the remaining build-only transitives. None of these
// dependencies are added to the application runtime.

// CodeQL Default Setup uses Gradle autobuild for Kotlin and invokes the generic
// testClasses task. Android Gradle Plugin does not create that lifecycle task,
// so map it to an Android debug build that compiles the Java/Kotlin sources.
tasks.register("testClasses") {
    group = "verification"
    description = "Build Android JVM/Kotlin classes for CodeQL autobuild"
    dependsOn(":app:assembleDebug")
}

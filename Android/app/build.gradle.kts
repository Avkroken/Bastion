plugins {
    id("com.android.application")
}

android {
    namespace = "se.denied.bastion"
    compileSdk = 35

    defaultConfig {
        applicationId = "se.denied.bastion"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // sshd-core och sshd-common bäddar båda in en identisk
    // META-INF/DEPENDENCIES-fil — Androids resursmerge tillåter inte två
    // jar:ar med samma META-INF-sökväg utan en explicit regel (verifierat
    // i CI: "2 files found with path 'META-INF/DEPENDENCIES'"). Innehållet
    // är bara en läslig beroendelista, ingen kodfunktion — säkert att bara
    // droppa filen helt istället för att välja en av de två godtyckligt.
    packaging {
        resources {
            excludes += "META-INF/DEPENDENCIES"
        }
    }
}

// kotlinOptions{jvmTarget} (KGP:s gamla android-plugin-DSL) är inte
// registrerad utan det separata org.jetbrains.kotlin.android-pluginet,
// trots att Googles egen migreringsguide (kotl.in/gradle/agp-built-in-kotlin)
// säger att den ska vara bakåtkompatibel — verifierat fel empiriskt
// (CI: "Unresolved reference 'kotlinOptions'"), inte antaget. jvmToolchain
// är AGP 9:s inbyggda Kotlin-stöds egen, rekommenderade ersättning.
kotlin {
    jvmToolchain(17)
}

dependencies {
    // SSH-motorn — samma princip som SSHCore (Swift-sidan) bygger på
    // swift-nio-ssh istället för att implementera SSH-protokollet från
    // grunden: Apache MINA SSHD är en mogen, väl underhållen Java/Kotlin-
    // SSH-implementation (klient OCH server, den senare används bara i
    // testerna nedan för en riktig, självständig round-trip-verifiering
    // utan att röra systemets egna sshd).
    implementation("org.apache.sshd:sshd-core:2.19.0")
    implementation("org.apache.sshd:sshd-common:2.19.0")

    testImplementation("org.apache.sshd:sshd-scp:2.19.0")
    testImplementation(kotlin("test"))
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.bouncycastle:bcprov-jdk18on:1.85.2")
}

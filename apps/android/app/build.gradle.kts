plugins { id("com.android.application"); id("org.jetbrains.kotlin.plugin.compose") }
android {
    namespace = "ru.hl100.drop"
    compileSdk = 37
    defaultConfig {
        applicationId = "ru.hl100.drop"
        minSdk = 31
        targetSdk = 35
        versionCode = 6
        versionName = "1.0.0"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }
    buildFeatures { compose = true; buildConfig = true }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    signingConfigs {
        create("distribution") {
            val keyPath = System.getenv("HL100_KEYSTORE")
            if (keyPath != null) {
                storeFile = file(keyPath)
                storePassword = System.getenv("HL100_STORE_PASSWORD")
                keyAlias = System.getenv("HL100_KEY_ALIAS")
                keyPassword = System.getenv("HL100_KEY_PASSWORD")
            }
        }
    }
    buildTypes { release { isMinifyEnabled = false; if (System.getenv("HL100_KEYSTORE") != null) signingConfig = signingConfigs.getByName("distribution") } }
}
dependencies {
    testImplementation("junit:junit:4.13.2")
    implementation(platform("androidx.compose:compose-bom:2025.08.00"))
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.documentfile:documentfile:1.1.0")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.9.4")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
}

import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Release signing for Play: put these in app_android/keystore.properties (not committed):
//   storeFile=/path/to/upload-key.jks  storePassword=…  keyAlias=upload  keyPassword=…
val keystoreProps = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use(::load)
}

android {
    namespace = "dev.vamsi.planner"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.vamsi.planner"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "1.0.0"
    }

    signingConfigs {
        if (keystoreProps.isNotEmpty()) {
            create("upload") {
                storeFile = file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // R8: shrink, optimize and obfuscate code; drop unused resources.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("upload")
        }
        debug {
            applicationIdSuffix = ".debug"
        }
    }

    buildFeatures {
        compose = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.kotlinx.serialization.json)
    debugImplementation(libs.compose.ui.tooling)
    testImplementation(libs.junit)
}

// Finished binaries go to app_android/dist/ (separate from the desktop app's target/),
// named planner-<version>-<variant>.<apk|aab>.
val distDir = rootProject.layout.projectDirectory.dir("dist")
val appVersion = android.defaultConfig.versionName
listOf(
    Triple("assembleDebug", "apk/debug", "debug.apk"),
    Triple("assembleRelease", "apk/release", "release.apk"),
    Triple("bundleRelease", "bundle/release", "release.aab"),
).forEach { (buildTask, outDir, suffix) ->
    val copy = tasks.register<Copy>("dist${buildTask.replaceFirstChar(Char::uppercase)}") {
        from(layout.buildDirectory.dir("outputs/$outDir")) { include("*.apk", "*.aab") }
        into(distDir)
        rename(".*", "planner-$appVersion-$suffix") // plain strings: configuration-cache safe
    }
    tasks.matching { it.name == buildTask }.configureEach { finalizedBy(copy) }
}

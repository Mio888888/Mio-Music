group = "com.vant.mio_music.native_host"
version = "1.0"
plugins { id("com.android.library") }
android {
    namespace = "com.vant.mio_music.native_host"
    compileSdk = 36
    defaultConfig { minSdk = 26 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
dependencies { implementation("androidx.core:core:1.13.1") }

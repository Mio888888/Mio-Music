import { readFileSync, writeFileSync, existsSync } from 'node:fs'

const mainPath = 'src-tauri/gen/android/app/src/main/java/com/vant/Mio/Music/MainActivity.kt'

if (!existsSync(mainPath)) {
  console.warn('[patch-android-native-audio] MainActivity.kt not found, skipping')
  process.exit(0)
}

let main = readFileSync(mainPath, 'utf8')
if (!main.includes('external fun initAndroidContext')) {
  main = main.replace(
    'class MainActivity : TauriActivity() {',
    'class MainActivity : TauriActivity() {\n  private external fun initAndroidContext(activity: android.app.Activity)',
  )
}
if (!main.includes('initAndroidContext(this)')) {
  main = main.replace(
    '  override fun onCreate(savedInstanceState: Bundle?) {\n    enableEdgeToEdge()',
    '  override fun onCreate(savedInstanceState: Bundle?) {\n    initAndroidContext(this)\n    enableEdgeToEdge()',
  )
}
writeFileSync(mainPath, main)
console.log('[patch-android-native-audio] Patched MainActivity.kt')

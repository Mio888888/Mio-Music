import { cpSync, existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const mainPath = 'src-tauri/gen/android/app/src/main/java/com/vant/Mio/Music/MainActivity.kt'
const iconSourceDir = 'src-tauri/icons/android'
const resDir = 'src-tauri/gen/android/app/src/main/res'

if (!existsSync(mainPath)) {
  console.warn('[patch-android] MainActivity.kt not found, skipping native audio patch')
} else {
  let main = readFileSync(mainPath, 'utf8')
  if (!main.includes('companion object {')) {
    main = main.replace(
      'class MainActivity : TauriActivity() {',
      'class MainActivity : TauriActivity() {\n  companion object {\n    init { System.loadLibrary("mio_lib") }\n  }',
    )
  }
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
  console.log('[patch-android] Patched MainActivity.kt')
}

if (!existsSync(iconSourceDir)) {
  console.warn('[patch-android] Android icon source directory not found, skipping icon patch')
} else if (!existsSync(resDir)) {
  console.warn('[patch-android] Generated Android res directory not found, skipping icon patch')
} else {
  cpSync(iconSourceDir, resDir, { recursive: true, force: true })
  console.log(`[patch-android] Copied Android icons from ${iconSourceDir} to ${resDir}`)
}

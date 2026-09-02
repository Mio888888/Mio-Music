# AGENTS.md

本文件记录在本项目中协作时必须遵守的项目经验和约束。

## 版本发布

- 更新应用版本时，必须同步更新 Android 生成工程中的 `src-tauri/gen/android/app/tauri.properties`。
- Tauri 默认按语义化版本计算 `tauri.android.versionCode`：
  ```
  versionCode = major * 1000000 + minor * 1000 + patch
  ```
  例如 `0.3.2` 对应 `3002`。
- 不要只更新桌面端配置和 package 版本；仓库提交的 Android 工程会在 CI 中被 Tauri 重写。若提交的 `versionCode` 落后，`.github/workflows/build.yml` 的 “Verify Android project unchanged” 会因生成文件变化而失败。
- 发布前用下面的检查确认没有遗漏：
  ```bash
  git diff --exit-code -- src-tauri/gen ':!src-tauri/gen/android/tauri.settings.gradle'
  ```

## Android 生成文件格式

- `src-tauri/gen/android/app/src/main/assets/tauri.conf.json` 和 `src-tauri/gen/android/app/tauri.properties` 是 Tauri 生成文件；当前 CI 的生成结果不带文件末尾换行。编辑器、脚本或格式化工具不得自动补回 EOF 换行。
- 重新生成或升级 Tauri CLI 后，必须以 CI 的实际生成结果为准按字节核对这两个文件；不要只检查可见文本或版本号是否一致。

## 跨平台更新与 Android 发布

- Tauri 更新器仅支持 Linux、macOS 和 Windows；Android/iOS 不得调用 `check()`、下载或安装更新，移动端应引导用户前往 GitHub Release 页面下载对应安装包。
- `.github/workflows/build.yml` 的 `publish-release` 必须等待 `build-android` 完成，确保 APK 上传后再公开 Release。

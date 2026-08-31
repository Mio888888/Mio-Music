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

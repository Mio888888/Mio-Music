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
- `updater:default` 权限必须放在带 `platforms: ["linux", "macOS", "windows"]` 的 capability 中，不能放入移动端也会加载的默认 capability。
- `.github/workflows/build.yml` 的 `publish-release` 必须等待 `build-android` 完成，确保 APK 上传后再公开 Release。
- 并行桌面构建不得各自创建 Release；必须由独立 job 创建唯一草稿，并通过同一个 `releaseId` 上传所有平台产物，发布步骤也必须使用该 ID。

## 插件导入与音质

- Android 文件选择器可返回 `FilePath::Url(content://...)`；必须将原始 `FilePath` 交给已注册的 `tauri-plugin-fs` 读取，不得要求它能转成普通文件路径。添加原生插件依赖后要同步生成 Android Gradle 依赖与各端权限 schema。
- 音质应以插件实际声明为准，兼容 `qualitys` 和带引号的 `"qualitys"` 字段；运行时元数据只在 Worker 中获取并校验，静态解析作为兜底，不得给所有插件强行添加 FLAC。
- 插件切换及同 ID 替换必须使过期的元数据请求和执行缓存失效；取消/失败的导入不得丢弃正在进行的有效选择。
- 全局音质是所有音源的交集，单音源音质是该音源自己的列表。不同格式的同名插件可能声明不同音质，不能假定 Ceru 与 LX 列表相同。
- 含密钥的插件样本仅用于本地验证，不得将访问链接、密钥或原始样本提交到仓库或 Issue。

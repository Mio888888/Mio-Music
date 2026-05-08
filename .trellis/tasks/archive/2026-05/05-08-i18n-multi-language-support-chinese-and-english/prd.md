# i18n 多语言支持：初始化中文和英语

## Goal

为 Mio Music 桌面音乐播放器添加国际化（i18n）基础设施，初始化支持中文（zh-CN）和英语（en-US），使用户可在设置中切换语言，并跟随系统语言自动检测。一次性全量迁移 92 个文件。

## Requirements

### 基础设施

- 安装 `vue-i18n@^11` + `@intlify/unplugin-vue-i18n`
- 在 `vite.config.ts` 中配置 i18n 预编译插件
- 创建 i18n 实例并在 `main.ts` 中注册
- 创建 `/src/locales/` 目录，按模块组织翻译文件

### 语言管理

- 在 Settings store 中添加 `language` 字段，存入 localStorage
- 默认跟随系统语言（`@tauri-apps/plugin-os` → `locale()`），回退到 zh-CN
- 用户可在设置中手动切换语言
- 语言变更即时生效，无需刷新

### UI 框架同步

- TDesign `ConfigProvider` 国际化与 vue-i18n locale 同步
- Naive UI `n-config-provider` 国际化与 vue-i18n locale 同步

### 文本迁移

- 92 个文件（58 .vue + 34 .ts）的硬编码中文全量提取
- 组件使用 `const { t } = useI18n()` + `t('key')`
- 翻译 key 按模块组织（`settings.appearance.title`）

## Acceptance Criteria

- [ ] vue-i18n 正确集成，Vite 构建正常
- [ ] 支持 zh-CN 和 en-US
- [ ] 设置页有语言切换选项
- [ ] 默认跟随系统语言
- [ ] 切换语言后所有 UI 文本即时更新
- [ ] TDesign / Naive UI 组件语言同步
- [ ] TypeScript 类型检查通过
- [ ] 所有原有功能不受影响

## Definition of Done

- `vue-tsc --noEmit && vite build` 通过
- 中英文切换流畅无闪烁
- 新增语言只需添加翻译文件

## Out of Scope

- 桌面歌词窗口国际化（独立 Tauri window，后续处理）
- 更多语言（仅 zh-CN / en-US）
- 后端 Rust 代码国际化
- 翻译 key 自动提取工具

## Technical Approach

### 方案：vue-i18n 11.x + @intlify/unplugin-vue-i18n

**新增依赖**:
```
vue-i18n@^11
@intlify/unplugin-vue-i18n   (devDep)
```

**翻译文件结构**:
```
src/locales/
├── index.ts              # createI18n + 导出
├── zh-CN/
│   ├── common.ts         # 通用：按钮、状态、时间
│   ├── nav.ts            # 导航、侧边栏
│   ├── settings.ts       # 设置页
│   ├── play.ts           # 播放器
│   ├── music.ts          # 发现、歌单、搜索
│   ├── playlist.ts       # 歌单管理
│   ├── ai.ts             # AI 助手
│   ├── auth.ts           # 用户认证
│   ├── backup.ts         # 备份恢复
│   ├── download.ts       # 下载
│   ├── plugin.ts         # 插件
│   └── error.ts          # 错误、提示信息
├── en-US/
│   └── (同结构)
```

**集成点**:

1. `vite.config.ts` — 添加 `VueI18nPlugin`，配置 `localeDir` 路径
2. `main.ts` — `app.use(i18n)` 注册
3. `src/store/Settings.ts` — 添加 `language` 字段 + `updateLanguage()` 方法
4. `App.vue` — TDesign `<t-config-provider :locale="tdLocale">` + Naive `<n-config-provider :locale="nLocale">`
5. 设置页 — 添加语言选择组件
6. 所有 .vue/.ts 文件 — 提取硬编码中文 → `t('key')`

**Settings store 改动**:
- `SettingsState` 增加 `language?: string` 字段
- `loadSettings()` 中读取 `parsed.language`
- 新增 `computed currentLocale`：优先 `settings.language` → 系统 locale → `'zh-CN'`
- 新增 `updateLanguage(lang)` 方法

## Implementation Plan

### Step 1: 基础设施搭建
- 安装 vue-i18n + unplugin
- 创建 `src/locales/index.ts`
- 配置 vite.config.ts
- 注册 main.ts

### Step 2: Settings store 改造
- 添加 language 字段
- 添加 locale 计算 + 语言更新方法
- 系统语言检测

### Step 3: 翻译文件创建
- 逐模块提取所有 92 个文件的中文文本
- 创建 zh-CN 和 en-US 翻译文件

### Step 4: 全量文本替换
- 所有 .vue 组件中 `{{ '中文' }}` → `{{ t('key') }}`
- 所有 .ts 中硬编码中文 → `t('key')`
- TDesign / Naive UI locale 同步

### Step 5: 设置页语言选择 + 验证
- 设置页添加语言切换 UI
- 构建验证 + 功能测试

## Research References

- [`research/vue3-i18n-approach.md`](research/vue3-i18n-approach.md) — vue-i18n 11.x 方案评估与推荐

## Technical Notes

- Settings store 当前用 `localStorage.setItem('appSettings', ...)` 手动持久化（非 Pinia persist 插件）
- `@tauri-apps/plugin-os` 已安装，`locale()` 可用
- 桌面歌词窗口是独立 Tauri window，暂不处理
- 音乐插件返回的文本保持原样
- Vite 6.4.2，需 `@intlify/unplugin-vue-i18n` 兼容版本
- auto-import 已配置 Vue，`useI18n` 可考虑加入 auto-imports

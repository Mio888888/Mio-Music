# Research: Vue 3 + Tauri 2 桌面音乐播放器 i18n 方案

- **Query**: Vue 3 + Tauri 2 桌面音乐播放器最佳 i18n 方案调研
- **Scope**: mixed (内部代码分析 + 外部库调研)
- **Date**: 2026-05-08

## Findings

### 1. vue-i18n (intlify) — 标准方案

**基本信息**

| 属性 | 值 |
|---|---|
| npm 包名 | `vue-i18n` |
| 当前版本 | 11.4.2 |
| 许可证 | MIT |
| 仓库 | https://github.com/intlify/vue-i18n |
| Peer 依赖 | `vue: ^3.0.0` |
| 解压大小 | ~1.6 MB (含所有变体) |
| 生产 bundle | ~25-35 KB (gzip 后约 10-12 KB，仅导入 runtime 版本) |
| 最近更新 | 2026-05-07 (活跃维护中) |
| 首次发布 | 2014-05-04 |

**工作原理**

vue-i18n 11.x 是 Vue 3 的官方 i18n 解决方案，提供以下核心能力：

- `createI18n()` — 创建 i18n 实例，注册为 Vue 插件
- `useI18n()` — Composition API composable，返回 `{ t, locale, messages, ... }`
- `t(key)` — 翻译函数，支持插值、复数、列表格式化
- 消息格式支持：具名插值 `@.camelCase`、列表插值、ICU Message Format（通过 `@intlify/message-format`）
- 懒加载支持：异步加载 locale 消息文件

**Composition API 设置模式**

```typescript
// src/i18n/index.ts
import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN'

const i18n = createI18n({
  legacy: false,            // 使用 Composition API 模式
  locale: 'zh-CN',          // 默认语言
  fallbackLocale: 'zh-CN',  // 回退语言
  messages: {
    'zh-CN': zhCN,
  },
})

export default i18n

// main.ts
import i18n from './i18n'
app.use(i18n)
```

```typescript
// 组件中使用
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const { t } = useI18n()
</script>
<template>
  <span>{{ t('settings.title') }}</span>
</template>
```

**TypeScript 支持**

vue-i18n 11.x 提供了完善的 TypeScript 支持：

1. **Schema 类型安全** — 可通过泛型参数 `createI18n<{ message: Schema }>()` 让 `t()` 函数获得 key 的类型提示和校验
2. **导出类型** — `VueI18n`、`ExportLocaleMessage`、`Locale` 等类型定义完整
3. **Composition API 类型** — `useI18n()` 返回值完全类型化

```typescript
// 类型安全的 Schema 定义
import type { ExportLocaleMessage } from 'vue-i18n'

type MessageSchema = {
  settings: {
    title: string
    autoUpdate: string
    cacheLimit: string
  }
  common: {
    confirm: string
    cancel: string
    save: string
    delete: string
  }
}

const i18n = createI18n<[MessageSchema], 'zh-CN' | 'en'>({
  legacy: false,
  messages: { ... }
})
// t('settings.title') — 自动补全且类型安全
```

**Lazy-loading Locale Messages**

```typescript
// 异步加载语言包
const loadLocaleMessages = async (locale: string) => {
  const messages = await import(`./locales/${locale}.json`)
  i18n.global.setLocaleMessage(locale, messages.default)
  i18n.global.locale.value = locale
}
```

**Setup 复杂度**: 低。安装 `vue-i18n` + 创建 i18n 实例 + 在 main.ts 注册即可。

**Bundle Size 影响**: 中等。运行时版本 gzip 后约 10-12 KB。使用 `@intlify/unplugin-vue-i18n` 可预编译消息，移除运行时消息编译器，进一步减小体积。

**社区采纳度**: 非常高。vue-i18n 是 Vue 生态事实上的 i18n 标准，npm 周下载量远超其他方案，持续维护超过 12 年。

---

### 2. @intlify/unplugin-vue-i18n — Vite 构建优化插件

**基本信息**

| 属性 | 值 |
|---|---|
| npm 包名 | `@intlify/unplugin-vue-i18n` |
| 当前版本 | 11.1.2 |
| Peer 依赖 | `vue: ^3.2.25`, `vite: ^6.0.0 \|\| ^7.0.0 \|\| ^8.0.0`, `vue-i18n: *` 或 `petite-vue-i18n: *` |
| 解压大小 | ~122 KB |
| 作为 devDependency | 是 |

**工作原理**

该 unplugin 提供两个核心功能：

1. **SFC `<i18n>` 自定义块** — 允许在 `.vue` 文件中直接定义组件级 i18n 消息
   ```vue
   <i18n lang="json">
   { "hello": "你好" }
   </i18n>
   ```

2. **消息预编译** — 在构建时将 JSON/YAML 消息文件预编译为 JavaScript，消除运行时消息编译器依赖，减小 bundle 体积

**Vite 配置方式**

```typescript
// vite.config.ts
import VueI18nPlugin from '@intlify/unplugin-vue-i18n/vite'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  plugins: [
    vue(),
    VueI18nPlugin({
      /* 选项 */
      include: resolve(dirname(fileURLToPath(import.meta.url)), './src/i18n/locales/**'),
      // 默认消息格式，支持 json / yaml / json5
      defaultSFCLang: 'json',
      // 全局注入 Vue I18n 的 SFC
      globalSFCScope: true,
    }),
  ],
})
```

**与本项目的兼容性**

- 项目使用 Vite 6.4.2 → unplugin 要求 `^6.0.0 || ^7.0.0 || ^8.0.0` → **完全兼容**
- 项目使用 Vue 3.5.13 → unplugin 要求 `^3.2.25` → **完全兼容**
- 可直接集成到现有 `vite.config.ts`，与已有的 `AutoImport`、`Components` 插件并列

**Setup 复杂度**: 低。仅需在 vite.config.ts 添加一个插件配置。

**Bundle Size 影响**: 正面。通过预编译可移除 vue-i18n 的运行时消息编译器，生产 bundle 中 vue-i18n 部分可减小约 40-50%。

**社区采纳度**: 高。vue-i18n 官方推荐配合使用的构建工具，Vite 项目的标准配置。

---

### 3. 备选方案

#### 3a. petite-vue-i18n

| 属性 | 值 |
|---|---|
| npm 包名 | `petite-vue-i18n` |
| 当前版本 | 11.4.2 (与 vue-i18n 同步) |
| 解压大小 | ~1.0 MB |
| 生产 bundle | ~15-20 KB (gzip 约 6-8 KB) |

**描述**: vue-i18n 的精简版，API 完全兼容（`createI18n`、`useI18n`、`t()`），但移除了以下功能：
- Legacy API（Options API 兼容）
- 消息编译器（必须配合 unplugin 预编译）
- 组件内 `<i18n>` / `<i18n-t>` / `<i18n-d>` / `<i18n-n>` 等内置组件
- 部分高级格式化功能

**优点**: 更小的 bundle 体积
**缺点**: 丢失内置翻译组件（需自行使用 `t()` 函数），功能受限
**评估**: 对于本项目，由于需要大量使用 `t()` 函数替换硬编码中文文本，petite-vue-i18n 完全满足需求。但 vue-i18n 完整版配合 unplugin 预编译后体积差异不大（运行时编译器已被移除），因此不建议刻意选择 petite 版本。如果将来项目稳定后需要极致优化，可无缝切换。

#### 3b. 自定义轻量方案

使用 Vue 3 的 `provide/inject` + reactive 对象实现极简 i18n：

```typescript
// 简单示例
const i18n = reactive({ locale: 'zh-CN', messages: { ... } })
provide('i18n', i18n)
const t = (key: string) => i18n.messages[i18n.locale][key] ?? key
```

**评估**: 不推荐。自行实现会缺少复数处理、插值、嵌套 key 解析、回退机制等基本功能，且无法利用生态工具（unplugin 预编译、编辑器插件、翻译文件管理等）。对于 69 个 Vue 文件、44 个含中文文本的组件的中等规模项目，维护成本远超收益。

---

### 4. Vue 3 + Vite + TypeScript i18n 最佳实践

#### 推荐文件结构

```
src/
├── i18n/
│   ├── index.ts              # createI18n 实例 + 语言加载逻辑
│   ├── types.ts              # MessageSchema 类型定义
│   └── locales/
│       ├── zh-CN/
│       │   ├── common.json    # 通用文本（按钮、标签等）
│       │   ├── settings.json  # 设置页相关
│       │   ├── player.json    # 播放器相关
│       │   ├── playlist.json  # 播放列表相关
│       │   └── ...
│       └── en/
│           ├── common.json
│           ├── settings.json
│           ├── player.json
│           ├── playlist.json
│           └── ...
```

**按功能域拆分消息文件的原因**：
- 项目有 44 个含中文的 Vue 组件，涉及设置、播放、搜索、下载等多个功能域
- 拆分后每个 JSON 文件保持在 50-100 行，易于维护和翻译
- 可实现按路由懒加载语言包

#### 类型安全的 Translation Key

```typescript
// src/i18n/types.ts
export interface MessageSchema {
  common: {
    confirm: string
    cancel: string
    save: string
    delete: string
    add: string
    search: string
    download: string
    loading: string
    error: string
    success: string
    retry: string
    close: string
  }
  settings: {
    title: string
    autoUpdate: string
    cacheSizeLimit: string
    // ...
  }
  // ...
}
```

#### 懒加载实现

```typescript
// src/i18n/index.ts
import { createI18n } from 'vue-i18n'
import type { MessageSchema } from './types'

const i18n = createI18n<[MessageSchema], 'zh-CN' | 'en'>({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {}
})

// 合并加载某个语言的所有消息文件
export async function loadLocale(locale: string) {
  const modules = import.meta.glob(`./locales/${locale}/*.json`)
  const messages: Record<string, any> = {}
  for (const [path, loader] of Object.entries(modules)) {
    const key = path.split('/').pop()!.replace('.json', '')
    const mod = await loader() as { default: any }
    messages[key] = mod.default
  }
  i18n.global.setLocaleMessage(locale, messages)
}

// 初始加载默认语言
await loadLocale('zh-CN')

export default i18n
```

---

### 5. 桌面应用 (Tauri 2) 特定考虑

#### 系统语言检测

项目已安装 `@tauri-apps/plugin-os`（版本 2.3.2），该插件提供了 `locale()` 函数：

```typescript
import { locale } from '@tauri-apps/plugin-os'

const systemLocale = await locale()
// 返回 BCP-47 语言标签，如 'zh-CN', 'en-US', 'zh-TW', 'ja' 等
// 如果无法获取则返回 null
```

**映射 BCP-47 到应用语言**：

```typescript
function mapLocaleToAppLang(bcp47: string | null): 'zh-CN' | 'en' {
  if (!bcp47) return 'zh-CN'  // 默认中文
  if (bcp47.startsWith('zh')) return 'zh-CN'
  return 'en'
}
```

#### 语言偏好持久化

项目现有两种持久化方式：

1. **Pinia + pinia-plugin-persistedstate** — 部分 store 使用 `persist: true`（如 `AudioEffects`、`playSetting`、`Equalizer`）
2. **localStorage 直接操作** — `Settings` store 手动读写 `localStorage`

**推荐方式**：创建独立的 locale store，使用 `pinia-plugin-persistedstate` 持久化：

```typescript
// src/store/locale.ts
import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useLocaleStore = defineStore('locale', () => {
  const locale = ref<'zh-CN' | 'en'>('zh-CN')
  return { locale }
}, { persist: true })
```

这与项目现有 store 模式一致。

#### Tauri 桌面端的特殊注意事项

1. **无服务端** — 所有语言文件必须打包到应用内或本地加载，不能依赖 CDN 动态拉取
2. **单次加载即可** — 桌面应用通常启动一次长期运行，无需复杂的按需加载策略，初始化时加载所有语言包即可
3. **语言切换不需要刷新** — vue-i18n 的 reactive 机制使得切换语言后所有 `t()` 调用自动更新
4. **多窗口** — 项目有 `/desktop-lyric` 独立窗口，该窗口也需独立注册 i18n 实例或共享 locale 状态（通过 localStorage 同步）

---

### 6. 项目现状分析

| 指标 | 数值 |
|---|---|
| Vue 文件总数 | 69 |
| 含中文文本的 Vue 文件 | 44 (约 64%) |
| Vue 文件中的中文字符数 | 约 10,335 |
| TS 文件中的中文字符数 | 约 3,486 |
| 硬编码中文总量 | 约 13,821 个字符 |
| UI 框架 | TDesign + Naive UI (辅助) |
| 状态管理 | Pinia + pinia-plugin-persistedstate |
| Tauri OS 插件 | 已安装，提供 `locale()` API |
| Vite 版本 | 6.4.2 (与 unplugin 兼容) |

**需要注意**：TDesign 组件库本身支持国际化（`tdesign-vue-next` 提供 `ConfigProvider` 和语言包），部分组件（如日期选择、分页、对话框确认文本）可通过 TDesign 的 `ConfigProvider` 设置语言。这意味着 TDesign 组件内部的中文文本可能不需要手动翻译，而是通过 TDesign 自身的 i18n 机制处理。

---

### 7. 最终推荐

**推荐方案**: **vue-i18n 11.x + @intlify/unplugin-vue-i18n**

理由：

1. **生态标准** — vue-i18n 是 Vue 3 官方 i18n 方案，社区采纳度最高，文档最完善，持续活跃维护
2. **TypeScript 一等公民** — 通过 Schema 泛型实现翻译 key 的类型安全和自动补全
3. **Composition API 原生支持** — `useI18n()` + `t()` 函数完美契合项目现有的 `<script setup lang="ts">` 模式
4. **unplugin 预编译** — 构建时预编译消息，消除运行时编译器，优化 bundle 体积
5. **Tauri 系统语言检测已就绪** — 项目已安装 `@tauri-apps/plugin-os`，其 `locale()` 函数直接返回 BCP-47 标签
6. **Pinia 持久化模式成熟** — 语言偏好可无缝集成到现有 store 架构
7. **实施风险低** — 逐步迁移策略：先搭建 i18n 基础设施，然后按页面/组件逐步替换硬编码中文

**安装命令**:
```bash
npm install vue-i18n
npm install -D @intlify/unplugin-vue-i18n
```

**预期工作量**:
- 基础设施搭建 (i18n 实例 + 类型 + 系统语言检测): 1-2 小时
- 翻译文件编写 (提取所有中文字符串为 JSON): 4-6 小时
- 组件迁移 (替换硬编码为 `t()` 调用): 44 个文件，约 6-8 小时
- TDesign ConfigProvider 集成: 0.5 小时
- 测试与调整: 2-3 小时

---

## Related Specs

- `.trellis/tasks/05-08-i18n-multi-language-support-chinese-and-english/` — 当前任务目录

## Caveats / Not Found

1. **TDesign i18n 联动** — 需要进一步确认 TDesign 的 `ConfigProvider` 国际化与 vue-i18n 的协调方式（可能需要监听 locale 变化同步更新 TDesign 的 `globalConfig`）
2. **Naive UI i18n** — 项目使用了 Naive UI 的 `useDialog`、`useMessage` 等，Naive UI 也有自己的 `n-config-provider` 国际化机制，需要分别配置
3. **翻译质量** — 技术翻译需要注意术语一致性（如 "播放列表" vs "歌单" 等），建议维护术语表
4. **RTL 布局** — 中英文均为 LTR，暂无需考虑 RTL 支持
5. **日期/数字格式化** — vue-i18n 提供 `n()` 和 `d()` 函数处理数字和日期格式，但 Tauri 桌面应用中可能用不到复杂的本地化格式

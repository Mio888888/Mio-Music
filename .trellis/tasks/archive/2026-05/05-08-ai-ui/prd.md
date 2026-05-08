# 优化 AI 助手弹窗 UI — 主题适配与移动端优化

## Goal

将 AIChat.vue 的聊天窗口从硬编码样式升级为项目的 Liquid Glass 设计语言，支持 light/dark 主题自动切换，并在移动端使用底部弹出模式。

## What I already know

* **当前文件**: `src/components/AI/AIChat.vue` — 悬浮球 + 聊天窗口，所有颜色硬编码
* **项目主题系统**: `data-theme` 属性切换，`:root[data-theme='light']` / `:root[data-theme='dark']`
* **CSS 变量体系**: `--td-*`（TDesign）、`--glass-*`（glass morphism）、`--mobile-*`（移动端）
* **Liquid Glass 模式**: `AddToPlaylistDialog.vue` 已实现完整示例 — `color-mix()`、`backdrop-filter`、动画边框
* **移动端变量**: `--mobile-safe-top/bottom`、`--mobile-card-radius: 22px`、`--mobile-glass-bg`、`--mobile-touch-target: 44px`
* **断点惯例**: 768px 用于移动端布局切换

## Assumptions (temporary)

* 悬浮球功能（拖拽、自动隐藏、位置记忆）保持不变
* 流式消息、Markdown 渲染逻辑不变
* 只改 UI 层（样式 + 少量模板结构调整），不改业务逻辑

## Open Questions

*(暂无阻塞问题 — 技术方案已明确)*

## Requirements

### 1. 主题适配 (Light/Dark)
- 聊天窗口背景、文字、边框全部使用 CSS 变量
- 消息气泡颜色跟随主题
- Markdown 渲染内容（代码块、引用、表格等）跟随主题
- 悬浮球关闭按钮适配主题
- 过渡动画保持平滑

### 2. 移动端优化 (≤768px)
- 聊天窗口改为全屏底部弹出模式（Liquid Glass 风格）
- 使用 safe-area-inset 适配刘海屏
- 触控目标 ≥ 44px
- 输入区域适配移动端键盘弹出
- 悬浮球尺寸和隐藏距离调整

### 3. 设计一致性
- 聊天窗口使用 Liquid Glass 效果（backdrop-filter、渐变边框）
- 与 AddToPlaylistDialog 的视觉风格保持一致
- 使用项目的 motion 系统变量

## Acceptance Criteria

- [ ] 聊天窗口在 light/dark 主题下均显示正确，切换无闪烁
- [ ] 移动端 (≤768px) 聊天窗口以底部弹出方式展示，全宽
- [ ] Markdown 内容（代码块、引用、链接、表格）在两种主题下可读性良好
- [ ] 悬浮球拖拽、自动隐藏、位置记忆功能不受影响
- [ ] 消息气泡在两种主题下对比度充足
- [ ] 触控元素在移动端 ≥ 44px

## Definition of Done

* 桌面端和移动端手动验证通过
* Light/Dark 主题切换无视觉异常
* 无 console 错误
* 现有功能（流式响应、Markdown、拖拽）回归正常

## Out of Scope

* 悬浮球本身的视觉重新设计
* AI 功能逻辑变更（API、流式处理）
* 新增功能（如多轮对话历史持久化）
* 国际化内容变更

## Technical Notes

* **关键参考文件**: `src/components/AddToPlaylistDialog.vue` — Liquid Glass 完整实现
* **主题变量来源**: `src/assets/main.css`
* **关键 CSS 技术**: `color-mix(in srgb, ...)` 用于主题感知的半透明色
* **Glass 效果**: `backdrop-filter: blur(var(--glass-blur-panel)) saturate(200%)`
* **移动端底部弹出**: `align-items: flex-end` + `max-height: min(76dvh, 620px)`

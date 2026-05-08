# 优化下载页面移动端 UI

## Goal

优化下载管理页面在手机端的显示和交互体验，使其更符合项目移动端设计规范。

## What I already know

* **当前文件**: `src/views/download/index.vue` — 下载管理页，含搜索、批量操作、标签页、任务列表
* **已有移动端适配**: 768px 断点已存在，基础布局已调整（flex-direction: column 等）
* **现有问题**:
  - header settings 在移动端布局较拥挤，并发数和批量操作混在一起
  - task-item 卡片未使用项目的 Liquid Glass 风格
  - task-info 在桌面端 width: 350px 硬编码
  - 底部无 safe-area-inset 处理
  - 操作按钮在小屏上可能太密集

## Requirements

### 1. 卡片设计优化
- task-item 使用 Liquid Glass 风格卡片设计
- 使用 color-mix() 实现主题感知的半透明背景和边框
- 进度条在移动端更醒目

### 2. 移动端布局改进
- header settings 在移动端更合理的分组排列
- 并发数控制移到更合适的位置（或折叠）
- 搜索框在移动端全宽
- 底部 padding 加入 safe-area-inset

### 3. 交互优化
- 触控目标 ≥ 44px
- 任务操作按钮在移动端更易点击
- 空状态样式优化

## Acceptance Criteria

- [ ] 下载页在移动端 (≤768px) 布局合理，操作便捷
- [ ] 卡片风格与项目整体 Liquid Glass 设计一致
- [ ] 桌面端布局不受影响或得到改善
- [ ] 触控目标 ≥ 44px
- [ ] Light/Dark 主题下均显示正常

## Out of Scope

* 下载功能逻辑变更
* 新增功能
* 国际化内容变更

## Technical Notes

* **关键文件**: `src/views/download/index.vue`
* **参考**: AIChat.vue Liquid Glass 实现、main.css 移动端变量
* **移动端变量**: `--mobile-page-gutter`、`--mobile-page-top-gutter`、`--mobile-card-radius-small`、`--mobile-touch-target`

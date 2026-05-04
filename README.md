# Mio Music

一款简洁优雅的跨平台音乐播放器，基于 [CeruMusic](https://github.com/timeshiftsauce/CeruMusic) 复刻开发。

## 特性

- 基于合规插件获取公开音乐信息与播放
- 多音源支持（网易云音乐、QQ音乐、酷狗音乐、酷我音乐、咪咕音乐等）
- Hi-Res 高品质音频播放
- Apple Music 风格歌词滚动显示
- 本地音乐管理与标签编辑
- 音乐下载与离线播放
- 桌面歌词
- Three.js 粒子动画启动闪屏

## 技术栈

**前端**: Vue 3 · TypeScript · TDesign · Pinia · Three.js · Vite

**后端**: Rust · Tauri 2 · Rodio · Symphonia · Rusqlite

## 开发

```bash
# 安装依赖
npm install

# 启动开发模式
npm run tauri dev

# 构建发布
npm run tauri build
```

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Tauri CLI](https://tauri.app/start/prerequisites/) v2

### 推荐 IDE

- [VS Code](https://code.visualstudio.com/) + [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## 致谢

本项目基于 [CeruMusic](https://github.com/timeshiftsauce/CeruMusic) 开发，感谢原作者 **时迁酱** 的开源贡献。

- CeruMusic 官网: [ceru.docs.shiqianjiang.cn](https://ceru.docs.shiqianjiang.cn/)

## 声明

本项目仅供学习交流使用，不直接获取、存储、传输任何音乐数据或版权内容，仅提供插件运行框架。用户通过插件获取的所有数据，其合法性由插件提供者及用户自行负责。禁止用于任何商业运营或侵犯第三方权益的场景。

## License

本项目遵循原项目 CeruMusic 的开源协议。

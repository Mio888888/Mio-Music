export interface SearchItem {
  id: string
  category: string
  title: string
  description: string
  keywords: string[]
}

export const searchItems: SearchItem[] = [
  // 外观设置
  { id: 'appearance-titlebar', category: 'appearance', title: '标题栏风格', description: '选择标题栏控制按钮风格', keywords: ['标题栏', 'titlebar', '风格', 'windows', '红绿灯'] },
  { id: 'appearance-close-behavior', category: 'appearance', title: '关闭按钮行为', description: '设置点击窗口关闭按钮时的行为', keywords: ['关闭', '托盘', '退出', 'close', 'tray'] },
  { id: 'appearance-theme', category: 'appearance', title: '应用主题色', description: '选择应用的主题颜色', keywords: ['主题', '颜色', 'theme', 'color', '暗色', 'dark'] },
  { id: 'appearance-lyric-font', category: 'appearance', title: '歌词字体设置', description: '配置歌词显示字体、大小和字重', keywords: ['歌词', '字体', 'font', '大小', '字重'] },
  { id: 'appearance-desktop-lyric', category: 'appearance', title: '桌面歌词样式', description: '配置桌面歌词窗口的显示样式', keywords: ['桌面歌词', '样式', '颜色', '字体', 'desktop', 'lyric'] },

  // AI 功能
  { id: 'ai-api-config', category: 'ai', title: 'DeepSeek API 配置', description: '配置 DeepSeek API Key 以使用 AI 功能', keywords: ['deepseek', 'api', 'key', 'ai', '人工智能'] },
  { id: 'ai-floatball', category: 'ai', title: 'AI 浮球设置', description: '控制 AI 悬浮球的显示与隐藏', keywords: ['浮球', '悬浮球', 'ai', 'float', 'ball'] },

  // 播放设置
  { id: 'playback-playlist', category: 'playlist', title: '播放列表管理', description: '导入、导出和清空播放列表', keywords: ['播放列表', '导入', '导出', '清空', 'playlist'] },
  { id: 'playback-audio-output', category: 'playlist', title: '音频输出', description: '选择音频输出设备和 DLNA 投屏', keywords: ['音频', '输出', '设备', 'dlna', '投屏', 'audio', 'output'] },
  { id: 'playback-equalizer', category: 'playlist', title: '音频均衡器', description: '调节均衡器预设和频段增益', keywords: ['均衡器', 'eq', 'equalizer', '音效', '频段'] },
  { id: 'playback-audio-effect', category: 'playlist', title: '高级音效处理', description: '低音增强、环绕音效、声道平衡', keywords: ['音效', '低音', '环绕', '声道', 'bass', 'surround', 'balance'] },
  { id: 'playback-performance', category: 'playlist', title: '全屏播放-性能优化', description: '跳动歌词、背景动画、音频可视化', keywords: ['性能', '跳动歌词', '背景动画', '可视化', 'visualizer'] },

  // 音乐源
  { id: 'music-source', category: 'music', title: '音乐源选择', description: '选择音乐来源', keywords: ['音乐源', 'source', '音源'] },
  { id: 'music-quality', category: 'music', title: '音质选择', description: '选择播放和下载的音质', keywords: ['音质', 'quality', '无损', 'flac', 'hires'] },

  // 存储管理
  { id: 'storage-directory', category: 'storage', title: '存储目录', description: '配置缓存和下载目录', keywords: ['目录', '存储', '缓存', '下载', 'directory', 'cache'] },
  { id: 'storage-cache', category: 'storage', title: '缓存管理', description: '查看和清除本地歌曲缓存', keywords: ['缓存', '清除', 'cache', 'clean'] },
  { id: 'storage-cache-strategy', category: 'storage', title: '缓存策略', description: '自动缓存音乐设置', keywords: ['缓存', '策略', '自动'] },
  { id: 'storage-filename', category: 'storage', title: '文件名格式设置', description: '设置下载歌曲的文件名格式', keywords: ['文件名', '格式', '模板', 'filename', 'template'] },
  { id: 'storage-tags', category: 'storage', title: '标签写入设置', description: '设置下载歌曲时要写入的标签信息', keywords: ['标签', 'tag', 'id3', '封面', '歌词'] },

  // 快捷键
  { id: 'hotkey-settings', category: 'hotkeys', title: '快捷键设置', description: '配置系统级全局快捷键', keywords: ['快捷键', 'hotkey', 'shortcut', '全局'] },

  // 插件
  { id: 'plugin-settings', category: 'plugins', title: '插件管理', description: '安装、配置和管理插件', keywords: ['插件', 'plugin', '扩展', '音源插件'] },

  // 关于
  { id: 'about-version', category: 'about', title: '版本信息', description: '查看应用版本和检查更新', keywords: ['版本', '更新', 'version', 'update'] },
  { id: 'about-tech', category: 'about', title: '技术栈', description: '查看应用使用的技术栈', keywords: ['技术', 'tech', '框架', 'framework'] },
  { id: 'about-team', category: 'about', title: '开发团队', description: '了解开发团队成员', keywords: ['团队', '开发者', 'team', 'developer'] },
  { id: 'about-legal', category: 'about', title: '法律声明', description: '法律声明和使用条款', keywords: ['法律', '声明', '版权', 'legal', 'copyright'] },
]

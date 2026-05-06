<script setup lang="ts">
import { ref } from 'vue'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { platform as getPlatform } from '@tauri-apps/plugin-os'
import { MessagePlugin } from 'tdesign-vue-next'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const autoUpdate = ref(settings.value.autoUpdate)
const updateAutoUpdate = () => {
  settingsStore.updateSettings({ autoUpdate: autoUpdate.value })
}

const appVersion = ref('1.0.0')

type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'up-to-date' | 'error'
const updateStatus = ref<UpdateStatus>('idle')
const isChecking = ref(false)
const newVersion = ref('')
const updateBody = ref('')
const downloadPercent = ref(0)
const downloadDetail = ref('')
const errorMsg = ref('')

const isMacOS = ref(false)
try {
  isMacOS.value = getPlatform() === 'macos'
} catch {}

const getAppVersion = async () => {
  try {
    const { getVersion } = await import('@tauri-apps/api/app')
    appVersion.value = await getVersion()
  } catch { appVersion.value = '1.0.0' }
}
getAppVersion()

const handleCheckUpdate = async () => {
  isChecking.value = true
  updateStatus.value = 'checking'
  errorMsg.value = ''
  newVersion.value = ''
  downloadPercent.value = 0
  downloadDetail.value = ''

  try {
    const update = await check()
    if (!update) {
      updateStatus.value = 'up-to-date'
      return
    }

    newVersion.value = update.version
    updateBody.value = update.body || ''
    updateStatus.value = 'available'
  } catch (e: any) {
    console.error('检查更新错误:', e)
    updateStatus.value = 'error'
    errorMsg.value = e?.message || e?.toString?.() || JSON.stringify(e) || '检查更新失败，请稍后重试'
  } finally {
    isChecking.value = false
  }
}

const handleDownloadAndInstall = async () => {
  if (isMacOS.value) return

  updateStatus.value = 'downloading'
  downloadPercent.value = 0
  downloadDetail.value = ''

  try {
    const update = await check()
    if (!update) {
      updateStatus.value = 'up-to-date'
      return
    }

    let contentLength = 0
    let downloaded = 0

    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          contentLength = event.data.contentLength ?? 0
          break
        case 'Progress':
          downloaded += event.data.chunkLength
          if (contentLength > 0) {
            downloadPercent.value = Math.round(downloaded / contentLength * 100)
            const mb = (downloaded / 1024 / 1024).toFixed(1)
            const total = (contentLength / 1024 / 1024).toFixed(1)
            downloadDetail.value = `${mb} MB / ${total} MB`
          }
          break
        case 'Finished':
          break
      }
    })

    updateStatus.value = 'downloaded'
  } catch (e: any) {
    updateStatus.value = 'error'
    errorMsg.value = e?.message || '下载更新失败'
  }
}

const handleRelaunch = async () => {
  try {
    await relaunch()
  } catch {
    MessagePlugin.error('重启失败，请手动重启应用')
  }
}

const openLink = async (url: string) => {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl(url)
  } catch { window.open(url, '_blank') }
}
</script>

<template>
  <div class="settings-section">
    <div class="setting-group app-header-group">
      <div class="app-header">
        <div class="app-logo"><img src="/icon.png" alt="Mio Music" /></div>
        <div class="app-info">
          <h2 class="app-name">Mio Music</h2>
          <span class="app-version">v{{ appVersion }}</span>
        </div>
      </div>
      <p class="app-description">一款简洁优雅的跨平台音乐播放器，支持基于合规插件获取公开音乐信息与播放功能。</p>
    </div>

    <div id="about-version" class="setting-group">
      <h3>版本信息</h3>
      <div class="version-section">
        <div class="update-actions">
          <div class="update-option">
            <t-switch v-model:value="autoUpdate" @change="updateAutoUpdate"></t-switch>
            <div>应用启动时检查更新</div>
          </div>
          <t-button
            theme="primary"
            :loading="isChecking"
            :disabled="updateStatus === 'downloading'"
            @click="handleCheckUpdate"
          >
            {{ isChecking ? '检查中...' : '检查更新' }}
          </t-button>
        </div>

        <!-- 已是最新版本 -->
        <div v-if="updateStatus === 'up-to-date'" class="update-card success">
          <span class="update-icon">✓</span> 当前已是最新版本
        </div>

        <!-- 发现新版本 -->
        <div v-if="updateStatus === 'available'" class="update-card">
          <div class="update-header">
            <span class="update-icon new">↑</span>
            <span>发现新版本 <strong>v{{ newVersion }}</strong></span>
          </div>
          <div v-if="updateBody" class="update-notes">{{ updateBody }}</div>

          <div class="update-actions-row">
            <!-- macOS: 引导手动下载 -->
            <t-button
              v-if="isMacOS"
              theme="primary"
              @click="openLink('https://github.com/Mio888888/Mio-Music/releases/latest')"
            >
              前往 GitHub 下载
            </t-button>
            <!-- Windows/Linux: 自动下载安装 -->
            <t-button
              v-else
              theme="primary"
              @click="handleDownloadAndInstall"
            >
              下载并安装
            </t-button>
            <t-button variant="text" @click="updateStatus = 'idle'">稍后提醒</t-button>
          </div>
        </div>

        <!-- 下载中 -->
        <div v-if="updateStatus === 'downloading'" class="update-card">
          <div class="update-header">
            <span class="update-icon downloading">↓</span>
            <span>正在下载更新 v{{ newVersion }}...</span>
          </div>
          <t-progress :percentage="downloadPercent" theme="plump" :label="`${downloadPercent}%`" />
          <div v-if="downloadDetail" class="progress-detail">{{ downloadDetail }}</div>
        </div>

        <!-- 下载完成 -->
        <div v-if="updateStatus === 'downloaded'" class="update-card success">
          <div class="update-header">
            <span class="update-icon">✓</span>
            <span>更新已下载完成，重启应用即可安装</span>
          </div>
          <t-button theme="primary" @click="handleRelaunch">立即重启安装</t-button>
        </div>

        <!-- 错误 -->
        <div v-if="updateStatus === 'error'" class="update-card error">
          <span class="update-icon err">!</span> {{ errorMsg }}
        </div>
      </div>
    </div>

    <div id="about-tech" class="setting-group">
      <h3>技术栈</h3>
      <div class="tech-stack">
        <div class="tech-item"><span class="tech-name">Tauri 2</span><span class="tech-desc">跨平台桌面应用框架</span></div>
        <div class="tech-item"><span class="tech-name">Rust</span><span class="tech-desc">后端核心语言</span></div>
        <div class="tech-item"><span class="tech-name">Vue 3</span><span class="tech-desc">前端响应式框架</span></div>
        <div class="tech-item"><span class="tech-name">TypeScript</span><span class="tech-desc">类型安全的 JavaScript</span></div>
        <div class="tech-item"><span class="tech-name">Pinia</span><span class="tech-desc">状态管理</span></div>
        <div class="tech-item"><span class="tech-name">Vite</span><span class="tech-desc">前端构建工具</span></div>
        <div class="tech-item"><span class="tech-name">TDesign</span><span class="tech-desc">UI 组件库</span></div>
        <div class="tech-item"><span class="tech-name">Three.js</span><span class="tech-desc">3D 粒子动画渲染</span></div>
        <div class="tech-item"><span class="tech-name">Rodio</span><span class="tech-desc">Rust 音频播放引擎</span></div>
        <div class="tech-item"><span class="tech-name">Symphonia</span><span class="tech-desc">音频解码 (FLAC/MP3/AAC)</span></div>
        <div class="tech-item"><span class="tech-name">Lofty</span><span class="tech-desc">音频元数据读写</span></div>
        <div class="tech-item"><span class="tech-name">Rusqlite</span><span class="tech-desc">本地数据库存储</span></div>
        <div class="tech-item link" style="cursor:pointer" @click="openLink('https://github.com/Steve-xmh/applemusic-like-lyrics')"><span class="tech-name">AMLL</span><span class="tech-desc">Apple Music 风格歌词</span></div>
      </div>
    </div>

    <div id="about-legal" class="setting-group">
      <h3>法律声明</h3>
      <div class="legal-notice">
        <div class="notice-item"><h4>🔒 数据与内容责任</h4><p>本项目不直接获取、存储、传输任何音乐数据或版权内容，仅提供插件运行框架。用户通过插件获取的所有数据，其合法性由插件提供者及用户自行负责。</p></div>
        <div class="notice-item"><h4>⚖️ 版权合规要求</h4><p>用户承诺仅通过合规插件获取音乐相关信息，且获取、使用版权内容的行为符合《中华人民共和国著作权法》及相关法律法规，不侵犯任何第三方合法权益。</p></div>
        <div class="notice-item"><h4>🚫 使用限制</h4><p>本项目仅允许用于非商业、纯技术学习目的，禁止用于任何商业运营、盈利活动，禁止修改后用于侵犯第三方权益的场景。</p></div>
      </div>
      <h3 style="margin-top: 2rem">关于我们</h3>
      <div class="about-us">
        <p class="about-intro">
          Mio Music 基于
          <a class="about-link" @click="openLink('https://github.com/timeshiftsauce/CeruMusic')">CeruMusic</a>
          项目复刻开发，感谢原作者
          <strong>时迁酱</strong>
          的开源贡献与持续维护。如果你也喜欢原项目，欢迎访问
          <a class="about-link" @click="openLink('https://ceru.docs.shiqianjiang.cn/')">CeruMusic 官方文档</a>
          了解更多。
        </p>
        <div class="sponsor-card">
          <p class="sponsor-text">以下是原项目作者的赞助二维码，如果你愿意支持 CeruMusic 的开发，可以请作者喝杯奶茶 ☕</p>
          <div class="sponsor-qr">
            <img
              src="https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png"
              alt="原项目作者赞赏码"
            />
          </div>
          <p class="sponsor-hint">所有赞助将直接支持原项目作者时迁酱</p>
        </div>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.settings-section { animation: fadeInUp 0.4s ease-out; animation-fill-mode: both; }
.setting-group {
  background: var(--settings-group-bg, var(--td-bg-color-container));
  border-radius: 0.75rem; padding: 1.5rem; margin-bottom: 1.5rem;
  border: 1px solid var(--settings-group-border, var(--td-border-level-1-color));
  box-shadow: 0 1px 3px var(--settings-group-shadow);
  animation: fadeInUp 0.4s ease-out; animation-fill-mode: both;
  h3 { margin: 0 0 0.5rem; font-size: 1.125rem; font-weight: 600; color: var(--td-text-color-primary); }
  > p { margin: 0 0 1.5rem; color: var(--td-text-color-secondary); font-size: 0.875rem; }
}
.app-header-group {
  display: flex; flex-direction: column; align-items: center; text-align: center; padding: 2.5rem 1.5rem;
  .app-header {
    display: flex; align-items: center; gap: 1rem; margin-bottom: 1rem;
    .app-logo {
      width: 3.5rem; height: 3.5rem; flex-shrink: 0;
      img { width: 100%; height: 100%; object-fit: contain; border-radius: 0.75rem; }
    }
    .app-info {
      display: flex; align-items: baseline; gap: 0.75rem;
      .app-name { margin: 0; font-size: 1.75rem; font-weight: 800; color: var(--td-text-color-primary); letter-spacing: -0.5px; }
    }
  }
  .app-version {
    background: var(--td-brand-color-1); color: var(--td-brand-color-6);
    padding: 0.2rem 0.6rem; border-radius: 1rem; font-size: 0.75rem; font-weight: 600;
    border: 1px solid var(--td-brand-color-3);
  }
  .app-description {
    margin: 0; color: var(--td-text-color-secondary); line-height: 1.6; font-size: 0.9rem; max-width: 420px;
  }
}
.version-section {
  .update-actions {
    display: flex; justify-content: space-between; align-items: center;
    .update-option {
      display: flex; align-items: center; gap: 0.5rem;
      color: var(--td-text-color-primary);
    }
  }
}
.update-card {
  margin-top: 0.75rem; padding: 1rem 1.25rem;
  background: var(--td-bg-color-page); border: 1px solid var(--td-border-level-1-color);
  border-radius: 0.75rem; font-size: 0.875rem;
  display: flex; flex-direction: column; gap: 0.5rem;

  &.success {
    border-color: var(--td-success-color);
    color: var(--td-success-color);
    flex-direction: row; align-items: center; gap: 0.5rem;
  }
  &.error {
    border-color: var(--td-error-color);
    color: var(--td-error-color);
    flex-direction: row; align-items: center; gap: 0.5rem;
  }
}
.update-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 1.25rem; height: 1.25rem; border-radius: 50%; font-size: 0.75rem;
  font-weight: 700; flex-shrink: 0;
  background: var(--td-success-color); color: var(--td-text-color-anti);

  &.new { background: var(--td-brand-color); }
  &.downloading { background: var(--td-brand-color); will-change: opacity, transform; animation: pulse 1.5s infinite; }
  &.err { background: var(--td-error-color); }
}
.update-header {
  display: flex; align-items: center; gap: 0.5rem;
  strong { color: var(--td-brand-color); }
}
.update-notes {
  color: var(--td-text-color-secondary); font-size: 0.8rem; line-height: 1.5;
  margin: 0.25rem 0 0.25rem 1.75rem; white-space: pre-wrap;
}
.update-actions-row {
  display: flex; gap: 0.5rem; margin-top: 0.25rem; padding-left: 1.75rem;
}
.progress-detail {
  font-size: 0.75rem; color: var(--td-text-color-secondary); margin-top: 0.25rem;
}
.tech-stack {
  display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 0.75rem;
  .tech-item {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.75rem; background: var(--td-bg-color-page); border-radius: 0.5rem;
    border: 1px solid var(--td-border-level-1-color); transition: 0.3s; gap: 1rem;
    .tech-name { font-weight: 600; flex-shrink: 0; color: var(--td-text-color-primary); }
    .tech-desc { font-size: 0.875rem; color: var(--td-text-color-secondary); }
    &.link:hover { background-color: var(--td-brand-color-1); border-color: var(--td-brand-color); }
  }
}
.legal-notice {
  .notice-item {
    margin-bottom: 1.5rem; &:last-child { margin-bottom: 0; }
    h4 { margin: 0 0 0.5rem; font-size: 0.875rem; font-weight: 600; color: var(--td-text-color-primary); }
    p { margin: 0; font-size: 0.875rem; color: var(--td-text-color-secondary); line-height: 1.5; }
  }
}
.about-us {
  margin-top: 0.5rem;
  .about-intro {
    margin: 0 0 1.25rem; color: var(--td-text-color-secondary); font-size: 0.9rem; line-height: 1.7;
    .about-link {
      color: var(--td-brand-color); cursor: pointer; font-weight: 500;
      text-decoration: underline; text-decoration-color: var(--td-brand-color-3);
      text-underline-offset: 2px;
      &:hover { text-decoration-color: var(--td-brand-color); }
    }
  }
  .sponsor-card {
    background: var(--td-bg-color-page); border: 1px solid var(--td-border-level-1-color);
    border-radius: 0.75rem; padding: 1.25rem; display: flex; flex-direction: column; align-items: center;
    .sponsor-text { margin: 0 0 1rem; color: var(--td-text-color-secondary); font-size: 0.85rem; line-height: 1.5; text-align: center; }
    .sponsor-qr {
      width: 180px; height: 180px; border-radius: 0.75rem; overflow: hidden;
      border: 1px solid var(--td-border-level-1-color);
      img { width: 100%; height: 100%; object-fit: cover; }
    }
    .sponsor-hint { margin: 0.75rem 0 0; font-size: 0.75rem; color: var(--td-text-color-disabled); }
  }
}
@media (max-width: 768px) {
  .setting-group {
    padding: 14px;
    margin-bottom: 10px;

    h3 {
      font-size: 16px;
      line-height: 1.35;
    }
  }

  .app-header-group {
    padding: 22px 14px;

    .app-header {
      flex-direction: column;
      gap: 10px;
      margin-bottom: 10px;
    }

    .app-logo {
      width: 48px;
      height: 48px;
    }

    .app-info {
      flex-direction: column;
      align-items: center;
      gap: 6px;

      .app-name {
        font-size: 24px;
      }
    }

    .app-description {
      font-size: 13px;
    }
  }

  .version-section .update-actions {
    flex-direction: column;
    gap: 10px;
    align-items: stretch;

    .update-option {
      justify-content: space-between;
      padding: 10px 12px;
      border-radius: 8px;
      background: var(--settings-feature-bg, var(--td-bg-color-container));
      border: 1px solid var(--settings-feature-border, var(--td-border-level-1-color));
      color: var(--td-text-color-primary);
    }
  }

  .update-card {
    padding: 12px;
  }

  .update-actions-row {
    padding-left: 0;
    flex-wrap: wrap;
  }

  .update-notes {
    margin-left: 0;
  }

  .tech-stack {
    grid-template-columns: 1fr;
    gap: 8px;

    .tech-item {
      align-items: flex-start;
      flex-direction: column;
      gap: 4px;
      padding: 12px;
    }
  }

  .legal-notice .notice-item {
    margin-bottom: 14px;

    p {
      font-size: 13px;
      line-height: 1.55;
    }
  }

  .about-us {
    .about-intro {
      font-size: 13px;
      line-height: 1.6;
    }

    .sponsor-card {
      padding: 14px;

      .sponsor-qr {
        width: min(58vw, 180px);
        height: min(58vw, 180px);
      }
    }
  }
}
@keyframes fadeInUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.6; } }
</style>

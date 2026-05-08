<script setup lang="ts">
import { computed, ref } from 'vue'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { platform as getPlatform } from '@tauri-apps/plugin-os'
import { MessagePlugin } from 'tdesign-vue-next'

const { t } = useI18n()

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
    errorMsg.value = e?.message || e?.toString?.() || JSON.stringify(e) || t('settings.about.checkUpdateFailed')
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
    errorMsg.value = e?.message || t('settings.about.downloadUpdateFailed')
  }
}

const handleRelaunch = async () => {
  try {
    await relaunch()
  } catch {
    MessagePlugin.error(t('settings.about.restartFailed'))
  }
}

const openLink = async (url: string) => {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl(url)
  } catch { window.open(url, '_blank') }
}

const aboutIntroHtml = computed(() => {
  const link1 = `<a class="about-link" data-url="https://github.com/timeshiftsauce/CeruMusic">CeruMusic</a>`
  const link2 = '<strong>时迁酱</strong>'
  const link3 = `<a class="about-link" data-url="https://ceru.docs.shiqianjiang.cn/">CeruMusic</a>`
  return t('settings.about.aboutIntro', [link1, link2, link3])
})

const handleAboutClick = (e: MouseEvent) => {
  const target = e.target as HTMLElement
  if (target.classList.contains('about-link')) {
    const url = target.dataset.url
    if (url) openLink(url)
  }
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
      <p class="app-description">{{ t('settings.about.description') }}</p>
    </div>

    <div id="about-version" class="setting-group">
      <h3>{{ t('settings.about.versionInfo') }}</h3>
      <div class="version-section">
        <div class="update-actions">
          <div class="update-option">
            <t-switch v-model:value="autoUpdate" @change="updateAutoUpdate"></t-switch>
            <div>{{ t('settings.about.checkUpdateOnStart') }}</div>
          </div>
          <t-button
            class="update-check-button"
            theme="primary"
            :disabled="isChecking || updateStatus === 'downloading'"
            :aria-busy="isChecking"
            @click="handleCheckUpdate"
          >
            <span class="update-check-content">
              <span v-if="isChecking" class="update-check-spinner" aria-hidden="true" />
              {{ isChecking ? t('settings.about.checking') : t('settings.about.checkUpdate') }}
            </span>
          </t-button>
        </div>

        <!-- 已是最新版本 -->
        <div v-if="updateStatus === 'up-to-date'" class="update-card success">
          <span class="update-icon">✓</span> {{ t('settings.about.upToDate') }}
        </div>

        <!-- 发现新版本 -->
        <div v-if="updateStatus === 'available'" class="update-card">
          <div class="update-header">
            <span class="update-icon new">↑</span>
            <span>{{ t('settings.about.newVersionFound') }} <strong>v{{ newVersion }}</strong></span>
          </div>
          <div v-if="updateBody" class="update-notes">{{ updateBody }}</div>

          <div class="update-actions-row">
            <!-- macOS: 引导手动下载 -->
            <t-button
              v-if="isMacOS"
              theme="primary"
              @click="openLink('https://github.com/Mio888888/Mio-Music/releases/latest')"
            >
              {{ t('settings.about.goToGithub') }}
            </t-button>
            <!-- Windows/Linux: 自动下载安装 -->
            <t-button
              v-else
              theme="primary"
              @click="handleDownloadAndInstall"
            >
              {{ t('settings.about.downloadAndInstall') }}
            </t-button>
            <t-button variant="text" @click="updateStatus = 'idle'">{{ t('settings.about.remindLater') }}</t-button>
          </div>
        </div>

        <!-- 下载中 -->
        <div v-if="updateStatus === 'downloading'" class="update-card">
          <div class="update-header">
            <span class="update-icon downloading">↓</span>
            <span>{{ t('settings.about.downloading') }} v{{ newVersion }}...</span>
          </div>
          <t-progress :percentage="downloadPercent" theme="plump" :label="`${downloadPercent}%`" />
          <div v-if="downloadDetail" class="progress-detail">{{ downloadDetail }}</div>
        </div>

        <!-- 下载完成 -->
        <div v-if="updateStatus === 'downloaded'" class="update-card success">
          <div class="update-header">
            <span class="update-icon">✓</span>
            <span>{{ t('settings.about.updateDownloaded') }}</span>
          </div>
          <t-button theme="primary" @click="handleRelaunch">{{ t('settings.about.restartNow') }}</t-button>
        </div>

        <!-- 错误 -->
        <div v-if="updateStatus === 'error'" class="update-card error">
          <span class="update-icon err">!</span> {{ errorMsg }}
        </div>
      </div>
    </div>

    <div id="about-tech" class="setting-group">
      <h3>{{ t('settings.about.techStack') }}</h3>
      <div class="tech-stack">
        <div class="tech-item"><span class="tech-name">Tauri 2</span><span class="tech-desc">{{ t('settings.about.techTauri2') }}</span></div>
        <div class="tech-item"><span class="tech-name">Rust</span><span class="tech-desc">{{ t('settings.about.techRust') }}</span></div>
        <div class="tech-item"><span class="tech-name">Vue 3</span><span class="tech-desc">{{ t('settings.about.techVue3') }}</span></div>
        <div class="tech-item"><span class="tech-name">TypeScript</span><span class="tech-desc">{{ t('settings.about.techTypeScript') }}</span></div>
        <div class="tech-item"><span class="tech-name">Pinia</span><span class="tech-desc">{{ t('settings.about.techPinia') }}</span></div>
        <div class="tech-item"><span class="tech-name">Vite</span><span class="tech-desc">{{ t('settings.about.techVite') }}</span></div>
        <div class="tech-item"><span class="tech-name">TDesign</span><span class="tech-desc">{{ t('settings.about.techTDesign') }}</span></div>
        <div class="tech-item"><span class="tech-name">Three.js</span><span class="tech-desc">{{ t('settings.about.techThreejs') }}</span></div>
        <div class="tech-item"><span class="tech-name">Rodio</span><span class="tech-desc">{{ t('settings.about.techRodio') }}</span></div>
        <div class="tech-item"><span class="tech-name">Symphonia</span><span class="tech-desc">{{ t('settings.about.techSymphonia') }}</span></div>
        <div class="tech-item"><span class="tech-name">Lofty</span><span class="tech-desc">{{ t('settings.about.techLofty') }}</span></div>
        <div class="tech-item"><span class="tech-name">Rusqlite</span><span class="tech-desc">{{ t('settings.about.techRusqlite') }}</span></div>
        <div class="tech-item link" style="cursor:pointer" @click="openLink('https://github.com/Steve-xmh/applemusic-like-lyrics')"><span class="tech-name">AMLL</span><span class="tech-desc">{{ t('settings.about.techAmll') }}</span></div>
      </div>
    </div>

    <div id="about-legal" class="setting-group">
      <h3>{{ t('settings.about.legalNotice') }}</h3>
      <div class="legal-notice">
        <div class="notice-item"><h4>{{ t('settings.about.dataResponsibility') }}</h4><p>{{ t('settings.about.dataResponsibilityDesc') }}</p></div>
        <div class="notice-item"><h4>{{ t('settings.about.copyrightCompliance') }}</h4><p>{{ t('settings.about.copyrightComplianceDesc') }}</p></div>
        <div class="notice-item"><h4>{{ t('settings.about.usageRestriction') }}</h4><p>{{ t('settings.about.usageRestrictionDesc') }}</p></div>
      </div>
      <h3 style="margin-top: 2rem">{{ t('settings.about.aboutUs') }}</h3>
      <div class="about-us">
        <p class="about-intro" v-html="aboutIntroHtml" @click="handleAboutClick"></p>
        <div class="sponsor-card">
          <p class="sponsor-text">{{ t('settings.about.sponsorText') }} ☕</p>
          <div class="sponsor-qr">
            <img
              src="https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png"
              :alt="t('settings.about.sponsorImageAlt')"
            />
          </div>
          <p class="sponsor-hint">{{ t('settings.about.sponsorHint') }}</p>
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
.update-check-button {
  .update-check-content {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }

  .update-check-spinner {
    width: 14px;
    height: 14px;
    margin-right: 6px;
    border: 2px solid color-mix(in srgb, currentColor 30%, transparent);
    border-top-color: currentColor;
    border-radius: 50%;
    will-change: transform;
    animation: update-check-spin 1s linear infinite;
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
@keyframes update-check-spin { to { transform: rotate(360deg); } }
</style>

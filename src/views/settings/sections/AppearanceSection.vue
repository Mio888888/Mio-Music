<script setup lang="ts">
import { ref } from 'vue'
import { storeToRefs } from 'pinia'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'
import TitleBarControls from '@/components/TitleBarControls.vue'
import ThemeSelector from '@/components/ThemeSelector.vue'
import LyricFontSettings from '@/components/Settings/LyricFontSettings.vue'
import DesktopLyricStyle from '@/components/Settings/DesktopLyricStyle.vue'
import GlobalBackgroundSettings from '@/components/Settings/GlobalBackgroundSettings.vue'

const userStore = LocalUserDetailStore()
const { userInfo } = storeToRefs(userStore)
const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const currentStyle = ref<'windows' | 'traffic-light'>(
  userInfo.value.topBarStyle ? 'traffic-light' : 'windows'
)

const switchStyle = (style: 'windows' | 'traffic-light'): void => {
  currentStyle.value = style
  userInfo.value.topBarStyle = style === 'traffic-light'
}
</script>

<template>
  <div class="settings-section">
    <t-card title="基础外观" class="setting-card" hover-shadow>
      <div id="appearance-titlebar" class="setting-group-item mobile-hidden">
        <div class="setting-label">
          <h4>标题栏风格</h4>
          <p>选择您喜欢的标题栏控制按钮风格</p>
        </div>
        <div class="style-buttons">
          <t-button :theme="currentStyle === 'windows' ? 'primary' : 'default'" @click="switchStyle('windows')">
            Windows 风格
          </t-button>
          <t-button :theme="currentStyle === 'traffic-light' ? 'primary' : 'default'" @click="switchStyle('traffic-light')">
            红绿灯风格
          </t-button>
        </div>
        <div class="style-preview">
          <div class="preview-item">
            <h4>Windows 风格</h4>
            <div class="mock-titlebar">
              <div class="mock-title">Windows 风格标题栏</div>
              <TitleBarControls control-style="windows" />
            </div>
          </div>
          <div class="preview-item">
            <h4>红绿灯风格 (macOS)</h4>
            <div class="mock-titlebar">
              <div class="mock-title">红绿灯风格标题栏</div>
              <TitleBarControls control-style="traffic-light" />
            </div>
          </div>
        </div>
      </div>

      <t-divider class="mobile-hidden" />

      <div id="appearance-close-behavior" class="setting-group-item mobile-hidden">
        <div class="setting-label">
          <h4>关闭按钮行为</h4>
          <p>设置点击窗口关闭按钮时的行为</p>
        </div>
        <div class="setting-control" style="display: flex; align-items: center; gap: 10px">
          <t-switch
            :value="settings.closeToTray"
            @change="(val: any) => settingsStore.updateSettings({ closeToTray: Boolean(val) })"
          />
          <span class="setting-text">{{ settings.closeToTray ? '最小化到托盘' : '直接退出应用' }}</span>
        </div>
      </div>

      <t-divider class="mobile-hidden" />

      <div id="appearance-theme" class="setting-group-item">
        <div class="setting-label">
          <h4>应用主题色</h4>
          <p>选择应用的主题颜色</p>
        </div>
        <ThemeSelector />
      </div>

      <t-divider />

      <div v-if="settingsStore.shouldUseSpringFestivalTheme()" id="appearance-festival-theme" class="setting-group-item">
        <div class="setting-label">
          <h4>节日主题(限时体验)</h4>
          <p>当前为春节主题，您可以选择关闭</p>
        </div>
        <div class="setting-control" style="display: flex; align-items: center; gap: 10px">
          <t-button v-if="!settings.springFestivalDisabled" theme="danger" @click="settingsStore.updateSettings({ springFestivalDisabled: true })">
            关闭春节主题
          </t-button>
          <template v-else>
            <t-tag theme="default">春节主题已关闭</t-tag>
            <t-button theme="success" variant="outline" @click="settingsStore.updateSettings({ springFestivalDisabled: false })">
              开启春节主题
            </t-button>
          </template>
        </div>
      </div>
    </t-card>

    <div class="setting-spacer"></div>
    <t-card title="全局背景" class="setting-card" hover-shadow>
      <GlobalBackgroundSettings />
    </t-card>

    <div class="setting-spacer"></div>
    <div id="appearance-lyric-font">
      <LyricFontSettings />
    </div>

    <div class="setting-spacer"></div>
    <div id="appearance-desktop-lyric">
      <DesktopLyricStyle />
    </div>
  </div>
</template>

<style lang="scss" scoped>
.settings-section {
  animation: fadeInUp 0.4s ease-out;
  animation-fill-mode: both;
}

.setting-group-item {
  margin-bottom: 24px;
  &:last-child { margin-bottom: 0; }
}

.setting-label {
  margin-bottom: 16px;
  h4 { margin: 0 0 4px; font-size: 14px; font-weight: 600; color: var(--td-text-color-primary); }
  p { margin: 0; font-size: 12px; color: var(--td-text-color-secondary); }
}

.style-buttons {
  display: flex;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
  .t-button { transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease; &:hover { transform: translateY(-1px); } }
}

.style-preview {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;

  .preview-item {
    background: var(--td-bg-color-page);
    padding: 1rem;
    border-radius: 0.5rem;
    border: 1px solid var(--td-border-level-1-color);
    h4 { margin: 0 0 0.75rem; font-size: 0.875rem; font-weight: 600; color: var(--td-text-color-primary); }
  }
}

.mock-titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  background: var(--td-bg-color-container);
  border-radius: 0.375rem;
  border: 1px solid var(--td-border-level-1-color);
  .mock-title { font-weight: 500; color: var(--td-text-color-primary); font-size: 0.875rem; }
}

.setting-spacer { height: 24px; }

@keyframes fadeInUp {
  from { opacity: 0; transform: translateY(20px); }
  to { opacity: 1; transform: translateY(0); }
}

@media (max-width: 768px) {
  .mobile-hidden {
    display: none !important;
  }

  .setting-group-item {
    margin-bottom: 16px;
  }

  .setting-label {
    margin-bottom: 12px;
  }

  .style-buttons {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-bottom: 12px;

    .t-button {
      min-width: 0;
      padding: 0 10px;
      font-size: 13px;
    }
  }

  .style-preview {
    grid-template-columns: 1fr;
    gap: 10px;

    .preview-item {
      padding: 12px;

      h4 {
        margin-bottom: 8px;
        font-size: 13px;
      }
    }
  }

  .mock-titlebar {
    padding: 10px 12px;

    .mock-title {
      max-width: 120px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-size: 12px;
    }
  }
}

@media (max-width: 380px) {
  .style-buttons {
    grid-template-columns: 1fr;
  }
}
</style>

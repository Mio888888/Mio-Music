<script setup lang="ts">
import { ref, computed, nextTick, watch } from 'vue'
import { useRoute } from 'vue-router'
import TitleBarControls from '@/components/TitleBarControls.vue'
import {
  PaletteIcon,
  ApiIcon,
  PlayCircleIcon,
  KeyboardIcon,
  TreeRoundDotIcon,
  MusicIcon,
  SaveIcon,
  InfoCircleIcon
} from 'tdesign-icons-vue-next'

import AppearanceSection from './sections/AppearanceSection.vue'
import AISection from './sections/AISection.vue'
import PlaybackSection from './sections/PlaybackSection.vue'
import HotkeySection from './sections/HotkeySection.vue'
import PluginSection from './sections/PluginSection.vue'
import MusicSourceSection from './sections/MusicSourceSection.vue'
import StorageSection from './sections/StorageSection.vue'
import AboutSection from './sections/AboutSection.vue'
import SettingsSearch from '@/components/SettingsSearch.vue'
import type { SearchItem } from './searchIndex'

const activeCategory = ref<string>('appearance')
const route = useRoute()
const contentPanelRef = ref<HTMLElement>()
const scrollPositions = ref<Record<string, number>>({})

const settingsCategories = [
  {
    key: 'appearance',
    label: '外观设置',
    icon: PaletteIcon,
    description: '主题、标题栏风格等外观配置'
  },
  {
    key: 'ai',
    label: 'AI 功能',
    icon: ApiIcon,
    description: 'DeepSeek API 配置和 AI 相关功能'
  },
  {
    key: 'playlist',
    label: '播放设置',
    icon: PlayCircleIcon,
    description: '播放列表，歌词管理和相关设置'
  },
  {
    key: 'hotkeys',
    label: '快捷键',
    icon: KeyboardIcon,
    description: '全局快捷键配置'
  },
  {
    key: 'plugins',
    label: '插件管理',
    icon: TreeRoundDotIcon,
    description: '插件安装、配置和管理'
  },
  {
    key: 'music',
    label: '音乐源',
    icon: MusicIcon,
    description: '音乐源选择和音质配置'
  },
  {
    key: 'storage',
    label: '存储管理',
    icon: SaveIcon,
    description: '缓存管理和存储设置'
  },
  {
    key: 'about',
    label: '关于',
    icon: InfoCircleIcon,
    description: '版本信息和功能说明'
  }
]

const sectionComponents: Record<string, any> = {
  appearance: AppearanceSection,
  ai: AISection,
  playlist: PlaybackSection,
  hotkeys: HotkeySection,
  plugins: PluginSection,
  music: MusicSourceSection,
  storage: StorageSection,
  about: AboutSection
}

const currentComponent = computed(() => sectionComponents[activeCategory.value])

const switchCategory = async (categoryKey: string) => {
  if (activeCategory.value === categoryKey) return
  if (contentPanelRef.value) {
    scrollPositions.value[activeCategory.value] = contentPanelRef.value.scrollTop
  }
  activeCategory.value = categoryKey
  await nextTick()
  if (contentPanelRef.value) {
    contentPanelRef.value.scrollTop = scrollPositions.value[categoryKey] || 0
  }
}

function scrollToSection(sectionId?: string) {
  if (!sectionId) return
  let attempts = 0
  const maxAttempts = 20
  const tryScroll = () => {
    const el = document.getElementById(sectionId)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' })
      el.classList.remove('highlight-flash')
      void el.offsetWidth
      el.classList.add('highlight-flash')
      setTimeout(() => el.classList.remove('highlight-flash'), 2000)
    } else if (attempts < maxAttempts) {
      attempts++
      setTimeout(tryScroll, 100)
    }
  }
  tryScroll()
}

watch(
  () => route.query,
  async (q) => {
    const category = String(q.category || '')
    const section = String(q.section || '')
    if (category && category !== activeCategory.value) {
      await switchCategory(category)
      await nextTick()
    }
    if (section) {
      await nextTick()
      scrollToSection(section)
    }
  },
  { immediate: true, deep: true }
)

const handleSearchSelect = async (item: SearchItem) => {
  if (activeCategory.value !== item.category) {
    await switchCategory(item.category)
    await nextTick()
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
  let attempts = 0
  const maxAttempts = 10
  const tryScroll = () => {
    const element = document.getElementById(item.id)
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'start' })
      element.classList.remove('highlight-flash')
      void element.offsetWidth
      element.classList.add('highlight-flash')
      setTimeout(() => element.classList.remove('highlight-flash'), 2000)
    } else if (attempts < maxAttempts) {
      attempts++
      setTimeout(tryScroll, 100)
    }
  }
  tryScroll()
}
</script>

<template>
  <div class="main-container">
    <div class="header" data-tauri-drag-region>
      <TitleBarControls title="设置" :show-back="true" :show-account="false">
        <template #extra>
          <div style="flex-shrink: 0">
            <SettingsSearch @select="handleSearchSelect" />
          </div>
        </template>
      </TitleBarControls>
    </div>
    <div class="settings-layout">
      <div class="sidebar">
        <nav class="sidebar-nav">
          <div
            v-for="category in settingsCategories"
            :id="`settings-nav-${category.key}`"
            :key="category.key"
            class="nav-item"
            :class="{ active: activeCategory === category.key }"
            @click="switchCategory(category.key)"
          >
            <div class="nav-icon">
              <component :is="category.icon" />
            </div>
            <div class="nav-content">
              <div class="nav-label">{{ category.label }}</div>
              <div class="nav-description">{{ category.description }}</div>
            </div>
          </div>
        </nav>
      </div>
      <div class="content-panel">
        <div ref="contentPanelRef" class="panel-content">
          <KeepAlive>
            <component
              :is="currentComponent"
              :key="activeCategory"
              @switch-category="switchCategory"
            />
          </KeepAlive>
        </div>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.main-container {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--settings-main-bg);
}

.header {
  display: flex;
  align-items: center;
  background: var(--settings-header-bg);
  padding: 1.5rem;
  z-index: 1000;
}

.settings-layout {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.sidebar {
  width: 280px;
  background: var(--settings-sidebar-bg);
  padding-right: 5px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  padding-left: 5px;

  .sidebar-nav {
    flex: 1;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.875rem 1.5rem;
    margin-top: 5px;
    cursor: pointer;
    transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
    border-left: 3px solid transparent;
    border-radius: 5px;

    &:hover {
      background: var(--settings-nav-hover-bg);
    }

    &.active {
      background: var(--settings-nav-active-bg);
      border-left-color: var(--settings-nav-active-border);

      .nav-icon {
        color: var(--settings-nav-icon-active);
      }

      .nav-label {
        color: var(--settings-nav-label-active);
        font-weight: 600;
      }
    }

    .nav-icon {
      width: 20px;
      height: 20px;
      color: var(--settings-nav-icon-color);
      display: flex;
      justify-content: center;
      align-items: center;

      svg {
        width: 100%;
        height: 100%;
      }

      transition: color 0.2s ease;
    }

    .nav-content {
      flex: 1;
      min-width: 0;

      .nav-label {
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--settings-nav-label-color);
        margin-bottom: 0.125rem;
        transition: color 0.2s ease;
      }

      .nav-description {
        font-size: 0.75rem;
        color: var(--settings-nav-desc-color);
        line-height: 1.3;
      }
    }
  }
}

.content-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 10px;

  .panel-content {
    flex: 1;
    overflow-y: auto;
    background: var(--settings-main-bg);
    scroll-behavior: smooth;
  }
}

@media (max-width: 768px) {
  .main-container {
    height: 100dvh;
    min-height: 100dvh;
    background: var(--mobile-page-bg, var(--settings-main-bg));
    overflow: hidden;
  }

  .header {
    padding: calc(var(--mobile-safe-top) + 10px) var(--mobile-page-gutter) 10px;
    background: var(--mobile-glass-bg-strong, var(--settings-header-bg));
    backdrop-filter: saturate(var(--mobile-glass-saturate)) blur(var(--mobile-glass-blur));
    -webkit-backdrop-filter: saturate(var(--mobile-glass-saturate)) blur(var(--mobile-glass-blur));
  }

  .header :deep(.settings-search) {
    width: min(46vw, 220px);
  }

  .settings-layout {
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .sidebar {
    width: 100%;
    max-height: none;
    flex: 0 0 auto;
    padding: 6px var(--mobile-page-gutter) 8px;
    background: transparent;
    border-bottom: none;
    overflow: visible;
  }

  .sidebar .sidebar-nav {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding: 0;
    -webkit-overflow-scrolling: touch;
    scroll-snap-type: x proximity;
  }

  .sidebar .nav-item {
    min-width: auto;
    flex: 0 0 auto;
    min-height: var(--mobile-touch-target);
    margin: 0;
    padding: 0 14px;
    border: 0.5px solid var(--mobile-glass-border);
    border-radius: var(--mobile-control-radius);
    background: var(--mobile-glass-bg);
    scroll-snap-align: start;
    touch-action: manipulation;
  }

  .sidebar .nav-item.active {
    border-bottom-color: var(--td-brand-color);
    background: var(--td-brand-color-light);
  }

  .sidebar .nav-icon {
    width: 18px;
    height: 18px;
  }

  .sidebar .nav-description {
    display: none;
  }

  .sidebar .nav-label {
    margin: 0;
    font-size: 14px;
    white-space: nowrap;
  }

  .content-panel {
    min-height: 0;
    padding: 0 var(--mobile-page-gutter) calc(var(--mobile-content-bottom-inset) + 12px);
    overflow: hidden;
  }

  .content-panel .panel-content {
    min-height: 0;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    border-radius: var(--mobile-card-radius);
  }

  .content-panel :deep(.setting-group),
  .content-panel :deep(.section) {
    padding: 16px;
    margin-bottom: 12px;
    border-radius: var(--mobile-card-radius-small);
  }

  .content-panel :deep(.setting-item),
  .content-panel :deep(.hotkey-row),
  .content-panel :deep(.tag-option) {
    min-height: var(--mobile-touch-target);
    padding: 12px;
    border-radius: var(--mobile-card-radius-small);
  }

  .content-panel :deep(.setting-item),
  .content-panel :deep(.hotkey-row) {
    align-items: flex-start;
    flex-direction: column;
    gap: 10px;
  }

  .content-panel :deep(.hotkey-actions),
  .content-panel :deep(.template-tip),
  .content-panel :deep(.preview-container),
  .content-panel :deep(.status-summary) {
    width: 100%;
    flex-wrap: wrap;
  }

  .content-panel :deep(.t-button),
  .content-panel :deep(.t-input),
  .content-panel :deep(.t-input-number),
  .content-panel :deep(.t-select) {
    min-height: var(--mobile-touch-target);
  }
}

.fade-slide-enter-active,
.fade-slide-leave-active {
  transition: background-color 0.3s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.3s cubic-bezier(0.4, 0, 0.2, 1), color 0.3s cubic-bezier(0.4, 0, 0.2, 1), box-shadow 0.3s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.3s cubic-bezier(0.4, 0, 0.2, 1), transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.fade-slide-enter-from {
  opacity: 0;
  transform: translateX(20px);
}

.fade-slide-leave-to {
  opacity: 0;
  transform: translateX(-20px);
}

.fade-slide-enter-to,
.fade-slide-leave-from {
  opacity: 1;
  transform: translateX(0);
}

.sidebar,
.panel-content {
  &::-webkit-scrollbar {
    width: 0;
    height: 0;
  }
  &::-webkit-scrollbar-track {
    background: transparent;
  }
  &::-webkit-scrollbar-thumb {
    background: transparent;
  }
  scrollbar-width: none;
  -ms-overflow-style: none;
}

:deep(.highlight-flash) {
  position: relative;
}

:deep(.highlight-flash::after) {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  pointer-events: none;
  animation: flashHighlight 2s ease-out;
  z-index: 10;
}

@keyframes flashHighlight {
  0% {
    background-color: rgba(var(--td-brand-color-rgb), 0.2);
    box-shadow: 0 0 0 2px var(--td-brand-color);
  }
  50% {
    background-color: rgba(var(--td-brand-color-rgb), 0.1);
  }
  100% {
    background-color: transparent;
    box-shadow: none;
  }
}
</style>

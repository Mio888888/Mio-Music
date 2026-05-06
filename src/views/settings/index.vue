<script setup lang="ts">
import { ref, computed, nextTick, watch, onMounted, onBeforeUnmount } from 'vue'
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
const isMobile = ref(false)
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

const visibleSettingsCategories = computed(() =>
  isMobile.value
    ? settingsCategories.filter((category) => category.key !== 'hotkeys')
    : settingsCategories
)

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

const sidebarNavRef = ref<HTMLElement>()
const navCanScrollLeft = ref(false)
const navCanScrollRight = ref(false)

const updateNavScrollState = () => {
  const navEl = sidebarNavRef.value
  if (!navEl) return
  navCanScrollLeft.value = navEl.scrollLeft > 4
  navCanScrollRight.value = navEl.scrollLeft + navEl.clientWidth < navEl.scrollWidth - 4
}

const scrollNavToActive = (categoryKey: string) => {
  const navEl = sidebarNavRef.value
  if (!navEl) return
  const activeEl = document.getElementById(`settings-nav-${categoryKey}`)
  if (!activeEl) return
  const navRect = navEl.getBoundingClientRect()
  const elRect = activeEl.getBoundingClientRect()
  const offset = elRect.left - navRect.left - (navRect.width / 2) + (elRect.width / 2)
  navEl.scrollBy({ left: offset, behavior: 'smooth' })
  window.setTimeout(updateNavScrollState, 300)
}

const updateIsMobile = () => {
  isMobile.value = window.matchMedia('(max-width: 768px)').matches
  if (isMobile.value && activeCategory.value === 'hotkeys') {
    activeCategory.value = 'appearance'
  }
}

const switchCategory = async (categoryKey: string) => {
  if (isMobile.value && categoryKey === 'hotkeys') return
  if (activeCategory.value === categoryKey) return
  if (contentPanelRef.value) {
    scrollPositions.value[activeCategory.value] = contentPanelRef.value.scrollTop
  }
  activeCategory.value = categoryKey
  await nextTick()
  if (contentPanelRef.value) {
    contentPanelRef.value.scrollTop = scrollPositions.value[categoryKey] || 0
  }
  scrollNavToActive(categoryKey)
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
    if (category && category !== activeCategory.value && !(isMobile.value && category === 'hotkeys')) {
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

onMounted(async () => {
  updateIsMobile()
  await nextTick()
  updateNavScrollState()
  scrollNavToActive(activeCategory.value)
  window.addEventListener('resize', updateIsMobile)
  window.addEventListener('resize', updateNavScrollState)
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', updateIsMobile)
  window.removeEventListener('resize', updateNavScrollState)
})

const handleSearchSelect = async (item: SearchItem) => {
  if (isMobile.value && item.category === 'hotkeys') return
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
            <SettingsSearch :hidden-categories="isMobile ? ['hotkeys'] : []" @select="handleSearchSelect" />
          </div>
        </template>
      </TitleBarControls>
    </div>
    <div class="settings-layout">
      <div
        class="sidebar"
        :class="{
          'can-scroll-left': navCanScrollLeft,
          'can-scroll-right': navCanScrollRight
        }"
      >
        <nav ref="sidebarNavRef" class="sidebar-nav" @scroll="updateNavScrollState">
          <div
            v-for="category in visibleSettingsCategories"
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
    position: relative;
  }

  .sidebar::before,
  .sidebar::after {
    content: '';
    position: absolute;
    top: 6px;
    bottom: 8px;
    width: 24px;
    z-index: 2;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.3s ease;
  }

  .sidebar.can-scroll-left::before {
    left: var(--mobile-page-gutter);
    background: linear-gradient(to right, var(--mobile-page-bg, var(--settings-main-bg)), transparent);
    opacity: 1;
  }

  .sidebar.can-scroll-right::after {
    right: var(--mobile-page-gutter);
    background: linear-gradient(to left, var(--mobile-page-bg, var(--settings-main-bg)), transparent);
    opacity: 1;
  }

  .sidebar .sidebar-nav {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 2px 0;
    -webkit-overflow-scrolling: touch;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
  }

  .sidebar .nav-item {
    min-width: auto;
    flex: 0 0 auto;
    min-height: var(--mobile-touch-target);
    margin: 0;
    padding: 0 14px;
    border: 1px solid var(--mobile-glass-border);
    border-radius: 20px;
    background: var(--mobile-glass-bg);
    scroll-snap-align: start;
    touch-action: manipulation;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .sidebar .nav-item.active {
    background: var(--td-brand-color);
    border-color: var(--td-brand-color);
    box-shadow: 0 2px 8px rgba(var(--td-brand-color-rgb), 0.3);
  }

  .sidebar .nav-item.active .nav-icon {
    color: #fff;
  }

  .sidebar .nav-item.active .nav-label {
    color: #fff;
    font-weight: 600;
  }

  .sidebar .nav-icon {
    width: 16px;
    height: 16px;
  }

  .sidebar .nav-description {
    display: none;
  }

  .sidebar .nav-label {
    margin: 0;
    font-size: 13px;
    white-space: nowrap;
  }

  .content-panel {
    flex: 1 1 0;
    height: 0;
    min-height: 0;
    padding: 0 var(--mobile-page-gutter);
    overflow: hidden;
  }

  .content-panel .panel-content {
    flex: 1;
    height: 100%;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    border: 1px solid var(--mobile-glass-border);
    border-radius: var(--mobile-card-radius);
    background: var(--mobile-glass-bg, var(--settings-main-bg));
    padding: 10px 10px calc(var(--mobile-content-bottom-inset) + 12px);
    box-sizing: border-box;
  }

  .content-panel :deep(.settings-section) {
    min-height: 100%;
  }

  .content-panel :deep(.setting-group),
  .content-panel :deep(.section) {
    padding: 14px;
    margin-bottom: 10px;
    border-radius: var(--mobile-card-radius-small);
  }

  .content-panel :deep(.setting-card) {
    margin-bottom: 10px;
    border-radius: var(--mobile-card-radius-small);
    overflow: hidden;
  }

  .content-panel :deep(.setting-item),
  .content-panel :deep(.hotkey-row),
  .content-panel :deep(.tag-option) {
    min-height: var(--mobile-touch-target);
    padding: 10px 12px;
    border-radius: var(--mobile-card-radius-small);
  }

  .content-panel :deep(.setting-item),
  .content-panel :deep(.hotkey-row) {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }

  .content-panel :deep(.setting-item .setting-control),
  .content-panel :deep(.setting-item .style-buttons),
  .content-panel :deep(.setting-item .setting-info) {
    width: 100%;
  }

  .content-panel :deep(.hotkey-actions),
  .content-panel :deep(.template-tip),
  .content-panel :deep(.preview-container),
  .content-panel :deep(.status-summary) {
    width: 100%;
    flex-wrap: wrap;
  }

  .content-panel :deep(.t-input),
  .content-panel :deep(.t-input-number),
  .content-panel :deep(.t-select),
  .content-panel :deep(.t-textarea) {
    width: 100% !important;
  }

  .content-panel :deep(.t-button),
  .content-panel :deep(.t-input),
  .content-panel :deep(.t-input-number),
  .content-panel :deep(.t-select) {
    min-height: var(--mobile-touch-target);
  }

  .content-panel :deep(.t-slider) {
    width: 100% !important;
  }

  .content-panel :deep(.t-radio-group) {
    flex-wrap: wrap;
    width: 100%;
  }

  .content-panel :deep(.t-radio-button) {
    flex: 1;
    min-width: 0;
    text-align: center;
  }

  .content-panel :deep(.t-card) {
    border-radius: var(--mobile-card-radius-small);
    overflow: hidden;
  }

  .content-panel :deep(.t-card__header) {
    padding: 14px 14px 8px;
  }

  .content-panel :deep(.t-card__body) {
    padding: 8px 14px 14px;
  }

  .content-panel :deep(.t-card__actions) {
    margin-left: 8px;
  }

  .content-panel :deep(.t-space) {
    flex-wrap: wrap;
  }

  .content-panel :deep(.setting-label),
  .content-panel :deep(.setting-info),
  .content-panel :deep(.item-info) {
    width: 100%;
    min-width: 0;
  }

  .content-panel :deep(.setting-label h4),
  .content-panel :deep(.setting-title),
  .content-panel :deep(.item-title) {
    font-size: 14px;
    line-height: 1.35;
  }

  .content-panel :deep(.setting-label p),
  .content-panel :deep(.setting-desc),
  .content-panel :deep(.item-desc),
  .content-panel :deep(.option-desc) {
    font-size: 12px;
    line-height: 1.45;
  }

  .content-panel :deep(.file-path) {
    max-width: 100%;
  }

  .content-panel :deep(.font-preview),
  .content-panel :deep(.preview-container),
  .content-panel :deep(.visualizer-container) {
    width: 100%;
    min-width: 0;
  }

  .content-panel :deep(.tag-options) {
    gap: 10px;
  }

  .content-panel :deep(.lyric-format-options .t-radio-group),
  .content-panel :deep(.presets .t-radio-group),
  .content-panel :deep(.control-group .t-radio-group) {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .content-panel :deep(.visual-balance) {
    font-size: 18px;
  }

  .content-panel :deep(.effect-card) {
    padding: 12px;
  }

  .content-panel :deep(.card-header) {
    gap: 10px;
  }

  .content-panel :deep(.card-header .title) {
    font-size: 14px;
    line-height: 1.35;
  }


  .content-panel :deep(.t-divider) {
    margin: 12px 0;
  }

  .content-panel :deep(.setting-spacer) {
    height: 10px;
  }

  .content-panel :deep(.setting-group-item) {
    margin-bottom: 16px;
  }

  .content-panel :deep(.effects-grid) {
    grid-template-columns: 1fr;
    gap: 12px;
  }

  .content-panel :deep(.sliders-container) {
    height: 200px;
    padding: 10px 0;
  }

  .content-panel :deep(.eq-content .controls-row) {
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
  }

  .content-panel :deep(.eq-content .preset-controls) {
    width: 100%;
  }

  .content-panel :deep(.eq-content .preset-select) {
    flex: 1;
    min-width: 0;
  }

  .content-panel :deep(.eq-content .action-buttons) {
    width: 100%;
    justify-content: flex-start;
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

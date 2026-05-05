<script setup lang="ts">
import TitleBarControls from '@/components/TitleBarControls.vue'
import BackupRestore from '@/components/BackupRestore/BackupRestore.vue'
import { onMounted, onUnmounted, ref, watchEffect, computed, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'
import { useRouter, useRoute } from 'vue-router'
import { searchValue as useSearchStore } from '@/store/search'

let stopWatchEffect: (() => void) | null = null

onMounted(() => {
  const LocalUserDetail = LocalUserDetailStore()
  stopWatchEffect = watchEffect(() => {
    source.value = sourceicon[LocalUserDetail.userSource.source || 'wy']
  })
})

onUnmounted(() => {
  if (stopWatchEffect) {
    stopWatchEffect()
    stopWatchEffect = null
  }
})

const sourceicon: Record<string, string> = {
  kg: 'kugouyinle',
  wy: 'wangyiyun',
  mg: 'mg',
  tx: 'tx',
  kw: 'kw',
  bd: 'kw',
  xm: 'xm',
  git: 'git'
}
const source = ref('kugouyinle')

interface MenuItem {
  name: string
  icon: string
  path: string
}

const menuList: MenuItem[] = [
  { name: '发现', icon: 'icon-faxian', path: '/home/find' },
  { name: '歌单', icon: 'icon-yanchu', path: '/home/songlist' },
  { name: '本地', icon: 'icon-shouye', path: '/home/local' },
  { name: '下载', icon: 'icon-xiazai', path: '/home/download' }
]

const menuActive = ref(0)
const router = useRouter()
const route = useRoute()
const source_list_show = ref(false)

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)
const isDetailPage = computed(() => /\/(list|singer)\//.test(route.path))
const showNewYear = computed(
  () => settingsStore.shouldUseSpringFestivalTheme() && !settings.value.springFestivalDisabled
)

watch(
  () => route.path,
  (newPath) => {
    const index = menuList.findIndex((item) => newPath.startsWith(item.path))
    menuActive.value = index
  },
  { immediate: true }
)

const hasPluginData = computed(() => {
  const LocalUserDetail = LocalUserDetailStore()
  return !!(
    LocalUserDetail.userInfo.pluginId &&
    LocalUserDetail.userInfo.supportedSources &&
    Object.keys(LocalUserDetail.userInfo.supportedSources).length > 0
  )
})

const sourceNames: Record<string, string> = {
  wy: '网易云音乐',
  kg: '酷狗音乐',
  mg: '咪咕音乐',
  tx: 'QQ音乐',
  kw: '酷我音乐',
  bd: '波点音乐',
  xm: '虾米音乐',
  git: 'GitCode'
}

const sourceList = computed(() => {
  const LocalUserDetail = LocalUserDetailStore()
  const supportedSources = LocalUserDetail.userInfo.supportedSources
  if (!supportedSources) return []
  return Object.keys(supportedSources).map((key) => ({
    key,
    name: sourceNames[key] || key,
    icon: sourceicon[key] || key
  }))
})

const toggleSourceList = () => {
  source_list_show.value = !source_list_show.value
}

const selectSource = (sourceKey: string) => {
  if (!hasPluginData.value) return
  const LocalUserDetail = LocalUserDetailStore()
  LocalUserDetail.userInfo.selectSources = sourceKey

  const sourceDetail = LocalUserDetail.userInfo.supportedSources?.[sourceKey]
  if (!LocalUserDetail.userInfo.sourceQualityMap) {
    LocalUserDetail.userInfo.sourceQualityMap = {}
  }
  if (sourceDetail && sourceDetail.qualitys && sourceDetail.qualitys.length > 0) {
    const saved = LocalUserDetail.userInfo.sourceQualityMap[sourceKey]
    const useQuality =
      saved && sourceDetail.qualitys.includes(saved)
        ? saved
        : sourceDetail.qualitys[sourceDetail.qualitys.length - 1]
    LocalUserDetail.userInfo.sourceQualityMap[sourceKey] = useQuality
    LocalUserDetail.userInfo.selectQuality = useQuality
  }

  source.value = sourceicon[sourceKey]
  source_list_show.value = false
}

const handleMaskClick = () => {
  source_list_show.value = false
}

const handleClick = (index: number): void => {
  menuActive.value = index
  router.push(menuList[index].path)
}

const goBack = (): void => {
  router.go(-1)
}

const goForward = (): void => {
  router.go(1)
}

const SearchStore = useSearchStore()
const inputRef = ref<any>(null)

const handleSearch = async () => {
  if (!SearchStore.getValue.trim()) return
  try {
    router.push({ path: '/home/search' })
  } catch (error) {
    console.error('搜索失败:', error)
  }
}

const handleKeyDown = () => {
  handleSearch()
  inputRef.value?.blur?.()
}
</script>

<template>
  <t-layout class="home-container">
    <t-aside class="sidebar">
      <div class="sidebar-content">
        <div class="logo-section" data-tauri-drag-region>
          <div class="logo-icon" :class="{ 'spring-logo': showNewYear }">
            <img src="/icon.png" alt="Mio Music" />
          </div>
          <p class="app-title">
            <span style="font-weight: 800">Mio Music</span>
          </p>
        </div>

        <!-- 春节装饰横幅 -->
        <div v-if="showNewYear" class="spring-banner">
          <span class="spring-banner-text">新年快乐</span>
          <span class="spring-banner-year">2026</span>
        </div>

        <nav class="nav-section">
          <t-button
            v-for="(item, index) in menuList"
            :key="index"
            :variant="menuActive == index ? 'base' : 'text'"
            :class="menuActive == index ? 'nav-button active' : 'nav-button'"
            block
            @click="handleClick(index)"
          >
            <i :class="`iconfont ${item.icon} nav-icon`"></i>
            {{ item.name }}
          </t-button>
        </nav>
      </div>
    </t-aside>

    <t-layout style="flex: 1">
      <t-content>
        <div class="content">
          <div class="header" :class="{ 'detail-page': isDetailPage }" data-tauri-drag-region>
            <t-button shape="circle" theme="default" class="nav-btn desktop-only" @click="goBack">
              <i class="iconfont icon-xiangzuo"></i>
            </t-button>
            <t-button shape="circle" theme="default" class="nav-btn desktop-only" @click="goForward">
              <i class="iconfont icon-xiangyou"></i>
            </t-button>

            <div class="search-container">
              <div class="search-input">
                <div class="source-selector" @click="toggleSourceList">
                  <svg class="icon" aria-hidden="true">
                    <use :xlink:href="`#icon-${source}`"></use>
                  </svg>
                </div>
                <transition name="mask">
                  <div v-if="source_list_show" class="source-mask" @click="handleMaskClick"></div>
                </transition>
                <transition name="source">
                  <div v-if="source_list_show" class="source-list">
                    <div class="items">
                      <div
                        v-for="item in sourceList"
                        :key="item.key"
                        class="source-item"
                        :class="{ active: source === item.icon }"
                        @click="selectSource(item.key)"
                      >
                        <svg class="source-icon" aria-hidden="true">
                          <use :xlink:href="`#icon-${item.icon}`"></use>
                        </svg>
                        <span class="source-name">{{ item.name }}</span>
                      </div>
                    </div>
                  </div>
                </transition>
                <t-input
                  ref="inputRef"
                  v-model="SearchStore.value"
                  placeholder="搜索音乐、歌手"
                  style="width: 100%"
                  @enter="handleKeyDown"
                  @focus="SearchStore.setFocus(true)"
                  @blur="SearchStore.setFocus(false)"
                >
                  <template #suffix>
                    <t-button
                      theme="default"
                      variant="text"
                      shape="circle"
                      style="display: flex; align-items: center; justify-content: center"
                      @click="handleSearch"
                    >
                      <template #icon><t-icon name="search" style="font-size: 16px" /></template>
                    </t-button>
                  </template>
                </t-input>
              </div>

              <t-button
                shape="circle"
                theme="default"
                variant="text"
                class="nav-btn desktop-only"
                style="width: 32px; height: 32px; margin: 0; flex-shrink: 0"
                @click="router.push('/home/recognize')"
              >
                <template #icon><i class="iconfont icon-shengyin" style="font-size: 16px"></i></template>
              </t-button>

              <TitleBarControls>
                <template #before-settings>
                  <BackupRestore />
                </template>
              </TitleBarControls>
            </div>
          </div>

          <div class="mainContent">
            <slot name="body"></slot>
          </div>
        </div>
      </t-content>
    </t-layout>

    <!-- 移动端底部导航（Apple Music 风格） -->
    <nav class="mobile-bottom-nav">
      <button
        v-for="(item, index) in menuList"
        :key="index"
        class="mobile-nav-item"
        :class="{ active: menuActive === index }"
        @click="handleClick(index)"
      >
        <div class="mobile-nav-dot"></div>
        <i :class="`iconfont ${item.icon}`"></i>
        <span class="mobile-nav-label">{{ item.name }}</span>
      </button>
    </nav>
  </t-layout>
</template>

<style scoped>
.source-enter-active,
.source-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.source-enter-from {
  opacity: 0;
  transform: translateY(-0.5rem);
}
.source-leave-to {
  opacity: 0;
  transform: translateY(-0.5rem);
}

.mask-enter-active,
.mask-leave-active {
  transition: opacity 0.2s ease;
}
.mask-enter-from,
.mask-leave-to {
  opacity: 0;
}

.home-container {
  height: calc(100dvh - var(--play-bottom-height, 70px));
  overflow: hidden;
  position: relative;
}

.icon {
  width: 1.5rem;
  height: 1.5rem;
}

.sidebar {
  width: 15rem;
  background-image: linear-gradient(
    to bottom,
    var(--td-brand-color-4) -140vh,
    var(--td-bg-color-container) 30vh
  );
  border-right: 0.0625rem solid var(--td-border-level-1-color);
  flex-shrink: 0;
}

.sidebar-content {
  padding: 1rem;
}

.logo-section {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1.5rem;
}

.logo-icon {
  width: 2rem;
  height: 2rem;
  display: flex;
  align-items: center;
  justify-content: center;
}

.logo-icon .iconfont,
.logo-icon img {
  font-size: 1.25rem;
  color: #fff;
  width: 100%;
  height: 100%;
  border-radius: 0.625rem;
}

.app-title {
  font-weight: 500;
  font-size: 1.125rem;
  color: var(--td-text-color-primary);
}

.nav-section {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.nav-button {
  justify-content: flex-start;
  height: 2.4rem;
  text-align: left;
  padding: 0.7rem 1rem;
  border-radius: 0.5rem;
  border: none;
  transition: background-color 0.25s ease, color 0.25s ease;
}

.nav-button .nav-icon {
  margin-right: 0.75rem;
  font-size: 1rem;
}

.nav-button.active {
  background-color: var(--td-brand-color-4);
  color: var(--td-text-color-anti);
}

.nav-button.active:hover {
  background-color: var(--td-brand-color-5) !important;
}

.nav-button:not(.active) {
  color: var(--td-text-color-secondary);
}

.nav-button:not(.active):hover {
  color: var(--td-text-color-primary);
  background-color: var(--td-bg-color-component-hover);
}

:deep(.t-layout__content) {
  height: 100%;
  min-width: 0;
  display: flex;
}

.content {
  padding: 0;
  background-image: linear-gradient(
    to bottom,
    var(--td-brand-color-4) -110vh,
    var(--td-bg-color-container) 15vh
  );
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
}

.header {
  display: flex;
  align-items: center;
  padding: 1.5rem;
}

@supports (-webkit-touch-callout: none) {
  .header {
    padding-top: 2.5rem;
  }
}

.nav-btn {
  margin-right: 0.5rem;
}

.nav-btn .iconfont {
  font-size: 1rem;
  color: var(--td-text-color-secondary);
}

.nav-btn:hover .iconfont {
  color: var(--td-text-color-primary);
}

.search-container {
  display: flex;
  flex: 1;
  position: relative;
  align-items: center;
  justify-content: space-between;
}

.search-input {
  display: flex;
  align-items: center;
  padding: 0 0.5rem;
  width: min(18.75rem, 400px);
  margin-right: 0.5rem;
  border-radius: 1.25rem;
  background-color: var(--td-bg-color-container);
  overflow: visible;
  position: relative;
  transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

.search-input:has(input:focus) {
  width: max(18.75rem, 400px);
}

.source-selector {
  display: flex;
  align-items: center;
  cursor: pointer;
  box-sizing: border-box;
  padding: 0.25rem;
  aspect-ratio: 1 / 1;
  border-radius: 999px;
  overflow: hidden;
  transition: background-color 0.2s;
}

.source-selector:hover {
  background-color: var(--td-bg-color-component-hover);
}

.source-mask {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 9999999;
  background: transparent;
  cursor: pointer;
}

.source-list {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: 10000000;
  background: var(--td-bg-color-container);
  border: 1px solid var(--td-border-level-1-color);
  border-radius: 0.5rem;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  min-width: 10rem;
  overflow-y: hidden;
  margin-top: 0.25rem;
  padding: 0.5em;
}

.source-list .items {
  max-height: 12rem;
  overflow-y: auto;
}

.source-item {
  border-radius: 5px;
  display: flex;
  align-items: center;
  padding: 0.5rem 0.75rem;
  margin-bottom: 5px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.source-item:last-child {
  margin: 0;
}

.source-item:hover {
  background-color: var(--td-bg-color-component-hover);
}

.source-item.active {
  background-color: var(--td-brand-color-1);
  color: var(--td-brand-color);
}

.source-icon {
  width: 1rem;
  height: 1rem;
  margin-right: 0.5rem;
}

.source-name {
  font-size: 0.875rem;
  white-space: nowrap;
}

:deep(.t-input) {
  border-radius: 0 !important;
  border: none;
  box-shadow: none;
}

:deep(.t-input--suffix) {
  padding-right: 0 !important;
  background-color: transparent;
}

.mainContent {
  flex: 1;
  overflow: hidden;
  position: relative;
  height: 0;
  min-width: 0;
}

.settings-btn .iconfont {
  font-size: 1rem;
  color: var(--td-text-color-secondary);
}

.settings-btn:hover .iconfont {
  color: var(--td-text-color-primary);
}

/* 春节装饰 */
.spring-logo {
  background: linear-gradient(135deg, #ff4d4d, #ffd700) !important;
}

.spring-banner {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.35rem 0.75rem;
  margin-bottom: 1rem;
  border-radius: 8px;
  background: linear-gradient(135deg, rgba(255, 77, 77, 0.1), rgba(255, 215, 0, 0.1));
  border: 1px solid rgba(255, 215, 0, 0.15);
  position: relative;
  overflow: hidden;
}

.spring-banner::after {
  content: '';
  position: absolute;
  inset: -40% -60%;
  background: radial-gradient(circle, rgba(255, 215, 0, 0.2) 0 1px, transparent 2px);
  opacity: 0.3;
  will-change: transform, opacity; animation: bannerSparkle 2.5s linear infinite;
}

.spring-banner-text {
  position: relative;
  z-index: 1;
  font-size: 0.8rem;
  font-weight: 700;
  color: rgba(255, 50, 50, 0.85);
  letter-spacing: 1px;
}

.spring-banner-year {
  position: relative;
  z-index: 1;
  font-size: 0.8rem;
  font-weight: 900;
  background: linear-gradient(180deg, #fff0b3, #ffd65a);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

@keyframes bannerSparkle {
  0% { transform: translateX(-10%) rotate(0deg); }
  100% { transform: translateX(10%) rotate(180deg); }
}

/* ============ 移动端底部导航（Apple Music 风格） ============ */
.mobile-bottom-nav {
  display: none;
}

.desktop-only {
  display: inline-flex;
}

@media (max-width: 768px) {
  .desktop-only {
    display: none !important;
  }

  .sidebar {
    display: none !important;
  }

  .home-container {
    height: calc(100dvh - var(--mobile-content-bottom-inset));
    min-height: 0;
  }

  .home-container:has(.header.detail-page) {
    height: calc(100dvh - var(--mobile-content-bottom-inset));
  }

  .content {
    background: var(--mobile-page-bg);
  }

  .mainContent {
    overflow: hidden;
    min-height: 0;
  }

  .header {
    padding: calc(var(--mobile-safe-top) + var(--mobile-page-top-gutter)) var(--mobile-page-gutter) var(--mobile-page-top-gutter);
  }

  .header.detail-page {
    display: none;
  }

  .header.detail-page + .mainContent {
    height: 100%;
  }

  .search-input {
    width: 100%;
    min-width: 0;
    flex: 1;
  }

  .search-input:has(input:focus) {
    width: 100%;
  }

  .source-selector {
    min-width: var(--mobile-touch-target);
    min-height: var(--mobile-touch-target);
    justify-content: center;
  }

  .source-item {
    min-height: var(--mobile-touch-target);
  }

  :deep(.t-input) {
    min-height: var(--mobile-touch-target);
  }

  :deep(.t-input__wrap),
  :deep(.t-input__inner) {
    min-height: var(--mobile-touch-target);
  }

  :deep(.title-controls) {
    width: auto;
    flex-shrink: 0;
  }

  .mobile-bottom-nav {
    display: flex;
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: var(--mobile-bottom-layer-z);
    align-items: flex-start;
    justify-content: space-around;
    height: var(--mobile-nav-total-height);
    padding: 6px max(10px, var(--mobile-page-gutter)) var(--mobile-safe-bottom);
    background: var(--mobile-glass-bg);
    border-top: 0.5px solid var(--mobile-glass-border);
    box-shadow: 0 -10px 30px rgba(15, 23, 42, 0.08);
    backdrop-filter: saturate(var(--mobile-glass-saturate)) blur(var(--mobile-glass-blur));
    -webkit-backdrop-filter: saturate(var(--mobile-glass-saturate)) blur(var(--mobile-glass-blur));
  }

  .mobile-nav-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    flex: 1;
    min-width: var(--mobile-touch-target);
    min-height: var(--mobile-touch-target);
    height: var(--mobile-nav-height);
    background: none;
    border: none;
    border-radius: var(--mobile-control-radius);
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
    padding: 5px 0 4px;
    position: relative;
    transition: background-color var(--motion-duration-quick) var(--motion-ease-standard), transform var(--motion-duration-quick) var(--motion-ease-standard);

    &:active {
      transform: scale(0.96);
      background: color-mix(in srgb, var(--td-brand-color) 10%, transparent);
    }

    .iconfont {
      font-size: 22px;
      color: var(--td-text-color-placeholder);
      transition: color 0.2s ease;
    }

    .mobile-nav-label {
      font-size: 10px;
      line-height: 1;
      color: var(--td-text-color-placeholder);
      transition: color 0.2s ease;
    }

    .mobile-nav-dot {
      width: 5px;
      height: 5px;
      border-radius: 50%;
      background: transparent;
      margin-bottom: 1px;
      transition: background-color 0.2s ease, transform 0.2s ease;
    }
  }

  .mobile-nav-item.active {
    .iconfont {
      color: var(--td-brand-color);
    }

    .mobile-nav-label {
      color: var(--td-brand-color);
      font-weight: 600;
    }

    .mobile-nav-dot {
      background: var(--td-brand-color);
      transform: scale(1.15);
    }
  }

  /* 暗色模式 */
  :global([data-theme="dark"]) .mobile-bottom-nav {
    background: var(--mobile-glass-bg);
    border-top-color: var(--mobile-glass-border);
  }
}
</style>

<script setup lang="ts">
import { ref, onMounted, watch, onActivated, onDeactivated } from 'vue'
import { useRouter } from 'vue-router'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { storeToRefs } from 'pinia'
import { musicSdk } from '@/services/musicSdk'

const router = useRouter()
const localUserStore = LocalUserDetailStore()
const { userSource } = storeToRefs(localUserStore)

const playlists = ref<any[]>([])
const loading = ref(true)
const error = ref('')

const tags = ref<any[]>([])
const hotTag = ref<any[]>([])
const activeCategoryName = ref('热门')
const activeTagId = ref('')
const showMore = ref(false)
const activeGroupIndex = ref(0)
const activeGroupName = ref('')

const page = ref(1)
const limit = ref(30)
const total = ref(0)
const loadingMore = ref(false)
const noMore = ref(false)

const categoryCache: Record<string, { list: any[]; page: number; total: number }> = {}

const scrollTop = ref(0)
const scrollRef = ref<HTMLDivElement>()

const fetchTags = async () => {
  try {
    const res = await musicSdk.getPlaylistTags()
    tags.value = res?.tags || []
    hotTag.value = res?.hotTag || []
    activeGroupIndex.value = 0
    activeGroupName.value = tags.value[0]?.name || ''
  } catch (e) {
    console.error('获取歌单标签失败:', e)
  }
}

const fetchCategoryPlaylists = async (reset = false) => {
  if (loadingMore.value) return
  if (reset) {
    page.value = 1
    noMore.value = false
    loading.value = true
    playlists.value = []
  }

  const cacheKey = (activeTagId.value || 'hot') + ':' + (userSource.value.source || 'kw')

  if (reset && categoryCache[cacheKey]) {
    const cache = categoryCache[cacheKey]
    playlists.value = cache.list
    page.value = cache.page
    total.value = cache.total
    loading.value = false
    return
  }

  loadingMore.value = true
  try {
    const res = await musicSdk.getCategoryPlaylists('hot', activeTagId.value, page.value, limit.value)
    const list = Array.isArray(res?.list) ? res.list : []
    total.value = res?.total || 0

    const mapped = list.map((item: any) => ({
      id: item.id,
      title: item.name,
      description: item.desc || '精选歌单',
      cover: item.img,
      playCount: item.play_count || item.playCount,
      author: item.author,
      total: item.total,
      source: item.source
    }))

    playlists.value = reset ? mapped : [...playlists.value, ...mapped]
    noMore.value = playlists.value.length >= total.value
    if (!noMore.value) page.value += 1

    categoryCache[cacheKey] = {
      list: playlists.value.slice(),
      page: page.value,
      total: total.value
    }
    error.value = ''
  } catch (e) {
    console.error('获取分类歌单失败:', e)
    error.value = '获取分类歌单失败，请稍后重试'
  } finally {
    loading.value = false
    loadingMore.value = false
  }
}

const onSelectTag = async (tagId: string, name: string) => {
  activeTagId.value = tagId
  activeCategoryName.value = name
  showMore.value = false
  await fetchCategoryPlaylists(true)
}

const onScroll = (e: Event) => {
  const el = e.target as HTMLElement
  if (el.scrollHeight - el.scrollTop - el.clientHeight < 100 && !noMore.value && !loadingMore.value) {
    fetchCategoryPlaylists(false)
  }
}

watch(activeGroupName, (name) => {
  const idx = tags.value.findIndex((g: any) => g.name === name)
  activeGroupIndex.value = idx >= 0 ? idx : 0
})

const formatCount = (count: any) => {
  const n = typeof count === 'number' ? count : parseInt(count) || 0
  if (n >= 10000) return (n / 10000).toFixed(1) + '万'
  return String(n)
}

const goToPlaylist = (item: any) => {
  router.push({
    name: 'list',
    params: { id: item.id },
    query: { title: item.title, source: item.source, cover: item.cover }
  })
}

onMounted(() => {
  watch(
    userSource,
    () => {
      loading.value = true
      error.value = ''
      Object.keys(categoryCache).forEach(k => delete categoryCache[k])
      fetchTags().then(() => {
        activeTagId.value = ''
        activeCategoryName.value = '热门'
        fetchCategoryPlaylists(true)
      })
    },
    { deep: true, immediate: true }
  )

  const onDocClick = (e: MouseEvent) => {
    const target = e.target as HTMLElement
    if (!target.closest('.category-bar') && showMore.value) showMore.value = false
  }
  document.addEventListener('click', onDocClick)
})

onActivated(() => {
  if (scrollRef.value) scrollRef.value.scrollTop = scrollTop.value
})

onDeactivated(() => {
  if (scrollRef.value) scrollTop.value = scrollRef.value.scrollTop
})
</script>

<template>
  <div ref="scrollRef" class="playlist-category" @scroll="onScroll">
    <div class="category-bar">
      <div class="hot-tags">
        <button
          class="tag-chip"
          :class="{ active: activeTagId === '' }"
          @click="onSelectTag('', '热门')"
        >
          热门
        </button>
        <button
          v-for="t in hotTag"
          :key="t.id"
          class="tag-chip"
          :class="{ active: activeTagId === t.id }"
          @click="onSelectTag(t.id, t.name)"
        >
          {{ t.name }}
        </button>

        <div
          class="more-category-wrapper"
          @mouseenter="showMore = true"
          @mouseleave="showMore = false"
        >
          <t-button class="tag-chip more" shape="round" variant="outline">
            更多分类
            <template #suffix>
              <t-icon name="chevron-down" size="14px" :class="{ rotate: showMore }" />
            </template>
          </t-button>

          <transition name="dropdown">
            <div v-if="showMore" class="more-panel">
              <div class="panel-inner">
                <div class="panel-content">
                  <t-tabs v-model:value="activeGroupName" size="medium">
                    <t-tab-panel
                      v-for="group in tags"
                      :key="group.name"
                      :value="group.name"
                      :label="group.name"
                    />
                  </t-tabs>
                  <div v-if="tags[activeGroupIndex]" class="panel-tags">
                    <button
                      v-for="t in tags[activeGroupIndex].list"
                      :key="t.id"
                      class="tag-chip"
                      :class="{ active: activeTagId === t.id }"
                      @click="onSelectTag(t.id, t.name)"
                    >
                      {{ t.name }}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </transition>
        </div>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title">{{ activeCategoryName }}歌单</h3>

      <div v-if="loading && playlists.length === 0" class="loading-container">
        <t-loading size="large" text="正在加载歌单..." />
      </div>

      <div v-else-if="error" class="error-container">
        <t-alert theme="error" :message="error" />
        <t-button theme="primary" style="margin-top: 1rem" @click="fetchCategoryPlaylists(true)">
          重新加载
        </t-button>
      </div>

      <div v-else class="playlist-grid">
        <div
          v-for="playlist in playlists"
          :key="playlist.id"
          class="playlist-card"
          @click="goToPlaylist(playlist)"
        >
          <div class="playlist-cover">
            <img :src="playlist.cover || '/default-cover.png'" :alt="playlist.title" loading="lazy" />
          </div>
          <div class="playlist-info">
            <h4 class="playlist-title">{{ playlist.title }}</h4>
            <p class="playlist-desc">{{ playlist.description }}</p>
            <div class="playlist-meta">
              <span class="play-count">
                <i class="iconfont icon-bofang"></i>
                {{ formatCount(playlist.playCount) }}
              </span>
              <span v-if="playlist.total" class="song-count">{{ playlist.total }}首</span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="loadingMore && playlists.length > 0" class="load-status">
        <t-loading size="small" text="加载更多..." />
      </div>
      <div v-else-if="noMore && playlists.length > 0" class="load-status">
        <span class="no-more">没有更多内容</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.playlist-category {
  height: 100%;
  overflow-y: auto;
  padding: 0 2rem;
}

.category-bar {
  margin-bottom: 1rem;
  position: relative;
}

.hot-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.tag-chip {
  padding: 4px 12px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--td-text-color-secondary);
  cursor: pointer;
  font-size: 13px;
  transition: all 0.2s;
}

.tag-chip:hover {
  color: var(--td-text-color-primary);
  background: var(--td-bg-color-secondarycontainer);
}

.tag-chip.active {
  background: var(--td-brand-color-light);
  color: var(--td-brand-color);
  font-weight: 600;
}

.tag-chip.more {
  display: flex;
  align-items: center;
  gap: 4px;
  background: var(--td-bg-color-secondarycontainer);
}

.tag-chip.more:hover {
  background: var(--td-bg-color-component-hover);
}

.more-category-wrapper {
  position: relative;
  display: inline-block;
}

.more-panel {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  z-index: 100;
  padding-top: 8px;
  width: 100%;
  transform-origin: top center;
}

.panel-inner {
  background: var(--td-bg-color-container);
  border-radius: 12px;
  box-shadow: 0 6px 30px rgba(0, 0, 0, 0.1);
  border: 1px solid var(--td-border-level-1-color);
  padding: 8px 16px 16px;
}

.panel-tags {
  display: flex;
  flex-wrap: wrap;
  padding-top: 12px;
  gap: 8px;
  max-height: 300px;
  overflow-y: auto;
}

.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.16s ease;
}
.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.98);
}

.section {
  margin-bottom: 3rem;
}

.section-title {
  color: var(--td-text-color-primary);
  font-size: 1.25rem;
  font-weight: 600;
  margin-bottom: 1.5rem;
}

.loading-container {
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 4rem 0;
}

.error-container {
  text-align: center;
  padding: 2rem;
}

.load-status {
  display: flex;
  justify-content: center;
  padding: 12px 0;
}

.no-more {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
}

/* 歌单网格 */
.playlist-grid {
  display: grid;
  gap: 1.25rem;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
}

@media (max-width: 480px) {
  .playlist-grid {
    gap: 0.75rem;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  }
}

@media (min-width: 481px) and (max-width: 768px) {
  .playlist-grid {
    gap: 1rem;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  }
}

@media (min-width: 769px) and (max-width: 1024px) {
  .playlist-grid {
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
  }
}

@media (min-width: 1200px) {
  .playlist-grid {
    gap: 1.5rem;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  }
}

/* 歌单卡片 — 对齐 CeruMusic-main 风格 */
.playlist-card {
  background: var(--td-bg-color-container);
  border-radius: 1rem;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
  position: relative;
}

.playlist-card:hover {
  transform: translateY(-4px) scale(1.02);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
}

.playlist-card:hover .playlist-cover::after {
  opacity: 1;
}

.playlist-card:active {
  transform: translateY(-2px) scale(1.01);
}

.playlist-cover {
  position: relative;
  aspect-ratio: 1;
  overflow: hidden;
}

/* 悬浮遮罩层 */
.playlist-cover::after {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.1) 0%, rgba(0, 0, 0, 0.3) 100%);
  opacity: 0;
  transition: opacity 0.3s ease;
  pointer-events: none;
}

.playlist-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  user-select: none;
  -webkit-user-drag: none;
  transition: transform 0.3s ease;
}

.playlist-card:hover .playlist-cover img {
  transform: scale(1.05);
}

.playlist-info {
  padding: 1.25rem 1rem;
  position: relative;
  background: var(--td-bg-color-container);
  transition: all 0.3s ease;
}

.playlist-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
  margin: 0 0 0.5rem;
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 2.8rem;
}

.playlist-desc {
  font-size: 0.875rem;
  color: var(--td-text-color-secondary);
  margin: 0 0 0.75rem;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 2.625rem;
  transition: color 0.3s ease;
}

.playlist-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  margin-top: auto;
  padding-top: 0.5rem;
  border-top: 1px solid var(--td-border-level-1-color);
  transition: color 0.3s ease;
}

.play-count {
  font-size: 0.75rem;
  color: var(--td-text-color-placeholder);
  display: flex;
  align-items: center;
  gap: 0.25rem;
  font-weight: 500;
  transition: color 0.3s ease;
}

.play-count .iconfont {
  font-size: 0.875rem;
  opacity: 0.8;
}

.song-count {
  font-size: 0.75rem;
  color: var(--td-text-color-placeholder);
  font-weight: 500;
  background: var(--td-bg-color-secondarycontainer);
  padding: 0.125rem 0.5rem;
  border-radius: 0.375rem;
  transition: color 0.3s ease;
}
</style>

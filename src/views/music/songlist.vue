<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { musicSdk, type PlaylistItem } from '@/services/musicSdk'
import PlaylistCategory from '@/components/Find/PlaylistCategory.vue'

const router = useRouter()
const searchKeyword = ref('')
const searchResults = ref<PlaylistItem[]>([])
const searching = ref(false)

const formatCount = (count: any) => {
  const n = typeof count === 'number' ? count : parseInt(count) || 0
  if (n >= 10000) return (n / 10000).toFixed(1) + '万'
  return String(n)
}

const goToPlaylist = (item: PlaylistItem) => {
  router.push({
    name: 'list',
    params: { id: item.id },
    query: { title: item.name, source: item.source, cover: item.img }
  })
}

let searchTimer: ReturnType<typeof setTimeout> | null = null
watch(searchKeyword, (kw) => {
  if (searchTimer) clearTimeout(searchTimer)
  if (!kw.trim()) {
    searchResults.value = []
    searching.value = false
    return
  }
  searchTimer = setTimeout(async () => {
    searching.value = true
    try {
      const res = await musicSdk.searchPlaylist(kw.trim())
      searchResults.value = Array.isArray(res?.list) ? res.list : []
    } catch (e) {
      console.error('搜索歌单失败:', e)
      searchResults.value = []
    } finally {
      searching.value = false
    }
  }, 400)
})
</script>

<template>
  <div class="songlist-page">
    <div class="page-header">
      <h2>推荐歌单</h2>
      <p>按分类浏览热门歌单</p>
    </div>
    <div class="search-bar">
      <t-input v-model="searchKeyword" placeholder="搜索歌单" clearable style="max-width: 360px">
        <template #prefix-icon><i class="iconfont icon-sousuo"></i></template>
      </t-input>
    </div>

    <div v-if="searchKeyword.trim()" class="search-section">
      <div v-if="searching" class="loading-container">
        <t-loading size="large" text="搜索中..." />
      </div>
      <div v-else-if="searchResults.length > 0" class="playlist-grid">
        <div
          v-for="pl in searchResults"
          :key="pl.id"
          class="playlist-card"
          @click="goToPlaylist(pl)"
        >
          <div class="playlist-cover">
            <img :src="pl.img || '/default-cover.png'" :alt="pl.name" loading="lazy" />
          </div>
          <div class="playlist-info">
            <h4 class="playlist-title">{{ pl.name }}</h4>
            <div class="playlist-meta">
              <span v-if="pl.playCount" class="play-count">
                <i class="iconfont icon-bofang"></i>
                {{ formatCount(pl.playCount) }}
              </span>
            </div>
          </div>
        </div>
      </div>
      <div v-else class="empty-search">
        <p>没有找到匹配的歌单</p>
      </div>
    </div>

    <PlaylistCategory v-show="!searchKeyword.trim()" />
  </div>
</template>

<style scoped>
.songlist-page {
  width: 100%;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.page-header {
  margin: 0 2rem 0.5rem;
}

.page-header h2 {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  color: var(--td-text-color-primary);
  margin-bottom: 0.5rem;
  font-size: 1.875rem;
  font-weight: 600;
}

.page-header p {
  color: var(--td-text-color-secondary);
  font-size: 0.85rem;
}

.search-bar {
  margin: 0 2rem 1rem;
}

.search-section {
  flex: 1;
  overflow-y: auto;
  padding: 0 2rem 2rem;
}

.loading-container {
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 4rem 0;
}

.empty-search {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4rem 0;
  color: var(--td-text-color-placeholder);
}

.playlist-grid {
  display: grid;
  gap: 1.25rem;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
}

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

.playlist-cover {
  position: relative;
  aspect-ratio: 1;
  overflow: hidden;
}

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

.playlist-card:hover .playlist-cover::after { opacity: 1; }

.playlist-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform 0.3s ease;
}

.playlist-card:hover .playlist-cover img { transform: scale(1.05); }

.playlist-info {
  padding: 1.25rem 1rem;
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

.playlist-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--td-border-level-1-color);
}

.play-count {
  font-size: 0.75rem;
  color: var(--td-text-color-placeholder);
  display: flex;
  align-items: center;
  gap: 0.25rem;
}
</style>

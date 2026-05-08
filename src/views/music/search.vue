<script setup lang="ts">
import { ref, watch, onActivated } from 'vue'
import { searchValue as useSearchStore } from '@/store/search'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { musicSdk, type MusicItem } from '@/services/musicSdk'
import SkeletonLoader from '@/components/SkeletonLoader.vue'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { useRouter } from 'vue-router'

const { t } = useI18n()
const searchStore = useSearchStore()
const router = useRouter()

const keyword = ref('')
const searchResults = ref<MusicItem[]>([])
const loading = ref(false)
const hasMore = ref(true)
const currentPage = ref(1)
const pageSize = 30
const totalItems = ref(0)
const activeTab = ref<'songs' | 'playlists'>('songs')

const playlistResults = ref<any[]>([])
const playlistLoading = ref(false)
const playlistPage = ref(1)
const playlistLimit = 30
const playlistTotal = ref(0)

const localUserStore = LocalUserDetailStore()

onActivated(async () => {
  if (searchStore.getValue.trim() === '') {
    router.push({ name: 'find' })
  }
})

watch(
  () => searchStore.getValue,
  async (val) => {
    if (searchStore.getFocus || val.trim() === keyword.value.trim()) return
    if (val.trim() === '') { router.push({ name: 'find' }); return }
    keyword.value = val
    resetResults()
    if (activeTab.value === 'songs') await performSearch(true)
    else await fetchPlaylists(true)
  },
  { immediate: true }
)

watch(
  () => localUserStore.userSource,
  async () => {
    if (keyword.value.trim()) {
      resetResults()
      if (activeTab.value === 'songs') await performSearch(true)
      else await fetchPlaylists(true)
    }
  },
  { deep: true }
)

watch(activeTab, async (val) => {
  if (!keyword.value.trim()) return
  if (val === 'songs' && searchResults.value.length === 0) await performSearch(true)
  else if (val === 'playlists' && playlistResults.value.length === 0) await fetchPlaylists(true)
})

function resetResults() {
  searchResults.value = []
  playlistResults.value = []
  currentPage.value = 1
  playlistPage.value = 1
}

const performSearch = async (reset = false) => {
  if (loading.value || !keyword.value.trim()) return
  if (reset) { currentPage.value = 1; searchResults.value = []; hasMore.value = true }
  if (!hasMore.value) return

  loading.value = true
  try {
    const result = await musicSdk.search(keyword.value, currentPage.value, pageSize)
    totalItems.value = result.total || 0
    const newSongs = (result.list || []).map((song, i) => ({
      ...song, id: song.songmid || `${currentPage.value}-${i}`
    }))
    searchResults.value = reset ? newSongs : [...searchResults.value, ...newSongs]
    currentPage.value += 1
    hasMore.value = searchResults.value.length < totalItems.value
  } catch (e) { console.error('搜索失败:', e) }
  finally { loading.value = false }
}

const fetchPlaylists = async (reset = false) => {
  if (playlistLoading.value || !keyword.value.trim()) return
  if (reset) { playlistPage.value = 1; playlistResults.value = [] }

  playlistLoading.value = true
  try {
    const res = await musicSdk.searchPlaylist(keyword.value, playlistPage.value, playlistLimit)
    playlistTotal.value = res?.total || 0
    const list = Array.isArray(res?.list) ? res.list : []
    const mapped = list.map((item: any) => ({
      id: item.id, title: item.name, description: item.desc || '',
      cover: item.img, playCount: item.playCount, author: item.author,
      total: item.total, source: item.source
    }))
    playlistResults.value = reset ? mapped : [...playlistResults.value, ...mapped]
    playlistPage.value += 1
  } catch (e) { console.error('歌单搜索失败:', e) }
  finally { playlistLoading.value = false }
}

const handlePlay = (song: MusicItem) => {
  const playStatus = useGlobalPlayStatusStore()
  playStatus.updatePlayerInfo(song as any)
  playSong(song as any)
}

const handleScroll = (event: Event) => {
  const target = event.target as HTMLElement
  const { scrollTop, scrollHeight, clientHeight } = target
  if (scrollHeight - scrollTop - clientHeight < 100 && !loading.value && hasMore.value) performSearch(false)
}

const routerToPlaylist = (playlist: any) => {
  router.push({ name: 'list', params: { id: playlist.id }, query: { title: playlist.title, source: playlist.source, cover: playlist.cover } })
}

const onPlaylistScroll = (event: Event) => {
  const target = event.target as HTMLElement
  const { scrollTop, scrollHeight, clientHeight } = target
  if (scrollHeight - scrollTop - clientHeight < 100 && playlistResults.value.length < playlistTotal.value && !playlistLoading.value) fetchPlaylists(false)
}

const unescape = (str: string) => str.replace(/&#(\d+);/g, (_, dec) => String.fromCharCode(dec))
</script>

<template>
  <div class="search-container">
    <div class="search-header">
      <div class="header-row">
        <h2 class="search-title">{{ t('music.search.searchKeyword') }}"<span class="keyword">{{ keyword }}</span>"</h2>
        <div class="result-info">
          <span v-if="activeTab === 'songs'">{{ t('music.search.foundSongs', { count: totalItems }) }}</span>
          <span v-else>{{ t('music.search.foundPlaylists', { count: playlistTotal }) }}</span>
        </div>
      </div>
      <n-tabs v-model:value="activeTab" type="line" size="small">
        <n-tab-pane name="songs" :tab="t('music.search.tabSongs')" />
        <n-tab-pane name="playlists" :tab="t('music.search.tabPlaylists')" />
      </n-tabs>
    </div>

    <div class="result-content">
      <div v-show="activeTab === 'songs'" class="song-tab" @scroll="handleScroll">
        <div v-if="searchResults.length > 0" class="song-list">
          <div v-for="(song, index) in searchResults" :key="song.songmid" class="song-item" @click="handlePlay(song)">
            <span class="song-index">{{ index + 1 }}</span>
            <div class="song-info">
              <span class="song-name">{{ song.name }}</span>
              <span class="song-singer">
                <span
                  v-if="song.singerId && song.source !== 'local'"
                  class="singer-link"
                  @click.stop="router.push({ name: 'singer', params: { id: song.singerId }, query: { source: song.source } })"
                >{{ song.singer }}</span>
                <template v-else>{{ song.singer }}</template>
                <template v-if="song.albumName"> - {{ song.albumName }}</template>
              </span>
            </div>
            <span class="song-duration">{{ song.interval || '--:--' }}</span>
          </div>
        </div>
        <div v-else-if="!loading" class="empty-state"><div class="empty-content"><h3>{{ t('music.search.noSongResults') }}</h3><p>{{ t('music.search.tryOther') }}</p></div></div>
        <SkeletonLoader v-if="loading && searchResults.length === 0" type="song-list" :rows="10" />
      </div>

      <div v-show="activeTab === 'playlists'" class="playlist-tab">
        <div class="grid-scroll-container" @scroll="onPlaylistScroll">
          <TransitionGroup v-if="playlistResults.length > 0" name="grid-fade" tag="div" class="playlist-grid">
            <div v-for="playlist in playlistResults" :key="playlist.id" class="playlist-card" @click="routerToPlaylist(playlist)">
              <div class="playlist-cover"><img :src="playlist.cover || '/default-cover.png'" :alt="playlist.title" loading="lazy" /></div>
              <div class="playlist-info">
                <h4 class="playlist-title">{{ unescape(playlist.title) }}</h4>
                <p class="playlist-desc">{{ playlist.description || t('music.search.featuredPlaylist') }}</p>
              </div>
            </div>
          </TransitionGroup>
          <div v-else-if="!playlistLoading" class="empty-state"><div class="empty-content"><h3>{{ t('music.search.noPlaylistResults') }}</h3><p>{{ t('music.search.tryOther') }}</p></div></div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-container { width: 100%; padding: 20px; height: 100%; display: flex; flex-direction: column; overflow: hidden; box-sizing: border-box; }
.search-header { flex-shrink: 0; margin-bottom: 12px; }
.header-row { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 8px; }
.search-title { font-size: 24px; font-weight: normal; color: var(--td-text-color-primary); margin: 0; border-left: 4px solid var(--td-brand-color); padding-left: 8px; }
.keyword { color: var(--td-brand-color); }
.result-info { font-size: 12px; color: var(--td-text-color-secondary); }
.result-content { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.song-tab, .playlist-tab { display: flex; flex-direction: column; flex: 1; min-height: 0; overflow-y: auto; }
.song-list { padding: 0; }
.song-item { display: flex; align-items: center; min-height: 44px; padding: 10px 12px; cursor: pointer; transition: background-color var(--motion-duration-instant) var(--motion-ease-standard); border-radius: 6px; box-sizing: border-box; }
.song-item:hover { background: var(--td-bg-color-component-hover); }
.song-index { width: 32px; font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; }
.song-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.song-name { font-size: 14px; color: var(--td-text-color-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.song-singer { font-size: 12px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.singer-link { cursor: pointer; &:hover { color: var(--td-brand-color); } }
.song-duration { font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; margin-left: 12px; }
.grid-scroll-container { flex: 1; min-height: 0; overflow: auto; padding: 8px; }
.playlist-grid { display: grid; gap: 12px; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); }
.playlist-card { background: var(--td-bg-color-container); border-radius: 12px; overflow: hidden; cursor: pointer; transition: background-color var(--motion-duration-quick) var(--motion-ease-standard), border-color var(--motion-duration-quick) var(--motion-ease-standard), color var(--motion-duration-quick) var(--motion-ease-standard), box-shadow var(--motion-duration-quick) var(--motion-ease-standard), opacity var(--motion-duration-quick) var(--motion-ease-standard), transform var(--motion-duration-quick) var(--motion-ease-standard); }
.playlist-card:hover { transform: translateY(-2px); box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1); }
.playlist-cover { position: relative; aspect-ratio: 1; overflow: hidden; }
.playlist-cover img { width: 100%; height: 100%; object-fit: cover; }
.playlist-info { padding: 12px; }
.playlist-title { font-size: 14px; font-weight: 600; color: var(--td-text-color-primary); margin: 0 0 6px; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.playlist-desc { font-size: 12px; color: var(--td-text-color-secondary); margin: 0; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.empty-state { display: flex; align-items: center; justify-content: center; min-height: 300px; }
.empty-content { text-align: center; }
.empty-content h3 { font-size: 16px; color: var(--td-text-color-primary); margin: 0 0 8px; font-weight: normal; }
.empty-content p { font-size: 12px; color: var(--td-text-color-secondary); margin: 0; }
.loading-state { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px; }
.loading-spinner { width: 40px; height: 40px; border: 3px solid var(--td-bg-color-component); border-top-color: var(--td-brand-color); border-radius: 50%; will-change: transform; animation: spin 1s linear infinite; margin-bottom: 12px; }
.loading-state p { font-size: 14px; color: var(--td-text-color-secondary); margin: 0; }
@keyframes spin { to { transform: rotate(360deg); } }
.grid-fade-enter-active, .grid-fade-leave-active { transition: background-color var(--motion-duration-standard) var(--motion-ease-standard), border-color var(--motion-duration-standard) var(--motion-ease-standard), color var(--motion-duration-standard) var(--motion-ease-standard), box-shadow var(--motion-duration-standard) var(--motion-ease-standard), opacity var(--motion-duration-standard) var(--motion-ease-standard), transform var(--motion-duration-standard) var(--motion-ease-standard); }
.grid-fade-enter-from, .grid-fade-leave-to { opacity: 0; transform: translateY(6px) scale(0.98); }

@media (max-width: 768px) {
  .search-container {
    padding: var(--mobile-page-top-gutter) var(--mobile-page-gutter) 0;
  }

  .search-header {
    margin-bottom: 1rem;
  }

  .header-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 0.35rem;
  }

  .search-title {
    max-width: 100%;
    border-left: none;
    padding-left: 0;
    font-size: clamp(1.8rem, 8vw, 2.35rem);
    line-height: 1.1;
    letter-spacing: -0.04em;
    word-break: break-word;
  }

  .result-info {
    font-size: 0.95rem;
  }

  :deep(.n-tabs-tab) {
    min-height: var(--mobile-touch-target);
  }

  .song-tab,
  .playlist-tab,
  .grid-scroll-container {
    min-height: 0;
    -webkit-overflow-scrolling: touch;
  }

  .song-item {
    min-height: 56px;
    padding: 8px 10px;
    border-radius: var(--mobile-card-radius-small);
    touch-action: manipulation;
  }

  .song-index {
    width: 28px;
  }

  .song-name {
    font-size: 15px;
  }

  .song-duration {
    margin-left: 8px;
  }

  .grid-scroll-container {
    padding: 0;
  }

  .playlist-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .playlist-card {
    border-radius: var(--mobile-card-radius-small);
  }

  .playlist-info {
    padding: 10px;
  }
}
</style>

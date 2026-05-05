<script setup lang="ts">
import { ref, onMounted, computed, onActivated } from 'vue'
import { useRoute } from 'vue-router'
import { musicSdk, type MusicItem, type SingerInfo, type SingerAlbumListResult } from '@/services/musicSdk'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { DialogPlugin } from 'tdesign-vue-next'
import SongVirtualList from '@/components/Music/SongVirtualList.vue'

const route = useRoute()
const playStatus = useGlobalPlayStatusStore()
const localUserStore = LocalUserDetailStore()

const singerId = computed(() => route.params.id as string)
const singerSource = computed(() => (route.query.source as string) || '')

const singerInfo = ref<SingerInfo | null>(null)
const songs = ref<MusicItem[]>([])
const albums = ref<SingerAlbumListResult['list']>([])
const loading = ref(false)
const loadingMore = ref(false)
const hasMore = ref(true)
const currentPage = ref(1)
const totalCount = ref(0)

const songListRef = ref<InstanceType<typeof SongVirtualList> | null>(null)
const currentSongId = computed(() => localUserStore.userInfo?.lastPlaySongId)

const bgImageLoaded = ref(false)
const isHeaderCompact = ref(false)
const activeTab = ref<'songs' | 'albums'>('songs')

async function fetchSingerInfo() {
  try {
    singerInfo.value = await musicSdk.getSingerInfo(singerId.value, singerSource.value)
  } catch (e) {
    console.error('获取歌手信息失败:', e)
  }
}

async function fetchSongs(reset = false) {
  if (loading.value) return
  if (reset) { currentPage.value = 1; songs.value = []; hasMore.value = true }
  if (!hasMore.value) return

  loading.value = true
  try {
    const res = await musicSdk.getSingerSongList(singerId.value, currentPage.value, 30, singerSource.value)
    const newSongs = res?.list || []
    songs.value = reset ? newSongs : [...songs.value, ...newSongs]
    totalCount.value = res?.total || 0
    hasMore.value = songs.value.length < (res?.total || 0)
    currentPage.value += 1

    const songsNeedPic = newSongs.filter(s => !s.img)
    if (songsNeedPic.length > 0) {
      const batch = songsNeedPic.slice(0, 50)
      Promise.all(batch.map(async (song) => {
        try {
          const url = await musicSdk.getPic(song)
          if (url) song.img = url
        } catch {}
      })).then(() => { songs.value = [...songs.value] })
    }
  } catch (e) {
    console.error('获取歌手歌曲失败:', e)
  } finally {
    loading.value = false
  }
}

async function fetchAlbums() {
  if (albums.value.length > 0) return
  loading.value = true
  try {
    const res = await musicSdk.getSingerAlbumList(singerId.value, 1, 30, singerSource.value)
    albums.value = res?.list || []
  } catch (e) {
    console.error('获取歌手专辑失败:', e)
  } finally {
    loading.value = false
  }
}

function handlePlay(song: MusicItem) {
  playStatus.updatePlayerInfo(song as any)
  playSong(song as any)
}

function handlePlayAll() {
  if (songs.value.length === 0) return
  const dialog = DialogPlugin.confirm({
    header: '播放全部',
    body: `确定要用歌手"${singerInfo.value?.info.name || ''}"的 ${songs.value.length} 首歌曲替换当前播放列表吗？`,
    confirmBtn: '确定替换',
    cancelBtn: '取消',
    onConfirm: () => {
      dialog.destroy()
      const sourceSongs = songListRef.value?.sortedSongs ?? songs.value
      localUserStore.replaceSongList(sourceSongs.map(s => ({
        songmid: s.songmid, name: s.name, singer: s.singer,
        albumName: s.albumName, img: s.img, source: s.source,
        url: '', interval: s.interval
      })) as any)
      playSong(sourceSongs[0] as any)
      playStatus.updatePlayerInfo(sourceSongs[0] as any)
    },
    onCancel: () => dialog.destroy()
  })
}

function switchTab(tab: 'songs' | 'albums') {
  activeTab.value = tab
  if (tab === 'albums') fetchAlbums()
}

function goBack() {
  window.history.back()
}

onMounted(() => {
  fetchSingerInfo()
  fetchSongs(true)
})

onActivated(() => {
  if (!singerInfo.value && singerId.value) {
    fetchSingerInfo()
    fetchSongs(true)
  }
})
</script>

<template>
  <div class="singer-page">
    <!-- Header -->
    <div class="singer-header" :class="{ compact: isHeaderCompact }">
      <div class="header-bg">
        <img
          v-if="singerInfo?.info.avatar"
          :src="singerInfo.info.avatar"
          :class="{ loaded: bgImageLoaded }"
          @load="bgImageLoaded = true"
        />
        <div class="bg-overlay" />
      </div>

      <div class="header-content">
        <div class="back-btn" @click="goBack">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <path d="M15.41 7.41L14 6L8 12L14 18L15.41 16.59L10.83 12L15.41 7.41Z" fill="currentColor" />
          </svg>
        </div>

        <div class="singer-meta">
          <div class="singer-cover">
            <img v-if="singerInfo?.info.avatar" :src="singerInfo.info.avatar" />
            <div v-else class="cover-placeholder" />
          </div>
          <div class="singer-info">
            <h1 class="singer-name">{{ singerInfo?.info.name || '加载中...' }}</h1>
            <div class="singer-stats" v-if="singerInfo">
              <span>{{ singerInfo.count.music }} 首歌曲</span>
              <span class="dot">·</span>
              <span>{{ singerInfo.count.album }} 张专辑</span>
            </div>
            <p class="singer-desc" v-if="singerInfo?.info.desc">{{ singerInfo.info.desc }}</p>
          </div>
        </div>

        <div class="header-actions" v-if="songs.length > 0">
          <button class="play-all-btn" @click="handlePlayAll">播放全部</button>
        </div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="singer-tabs">
      <div
        class="tab" :class="{ active: activeTab === 'songs' }"
        @click="switchTab('songs')"
      >
        歌曲({{ totalCount }})
      </div>
      <div
        class="tab" :class="{ active: activeTab === 'albums' }"
        @click="switchTab('albums')"
      >
        专辑({{ singerInfo?.count.album || 0 }})
      </div>
    </div>

    <!-- Song List -->
    <div v-show="activeTab === 'songs'" class="singer-content">
      <SongVirtualList
        ref="songListRef"
        :songs="songs"
        :currentSongId="currentSongId"
        :showAlbum="true"
        :showDuration="true"
        @play="handlePlay"
      />
      <div v-if="loading" class="loading-more">加载中...</div>
      <div v-if="!hasMore && songs.length > 0" class="no-more">没有更多了</div>
    </div>

    <!-- Album List -->
    <div v-show="activeTab === 'albums'" class="singer-content">
      <div v-if="loading" class="loading-more">加载中...</div>
      <div v-else-if="albums.length === 0" class="empty-state">暂无专辑</div>
      <div v-else class="album-grid">
        <div
          v-for="album in albums"
          :key="String(album.id)"
          class="album-card"
          @click="$router.push({ name: 'list', params: { id: String(album.id) }, query: { title: album.info.name, cover: album.info.img, source: singerSource, type: 'album' } })"
        >
          <div class="album-cover">
            <img v-if="album.info.img" :src="album.info.img" loading="lazy" />
            <div v-else class="cover-placeholder" />
          </div>
          <div class="album-title">{{ album.info.name }}</div>
          <div class="album-meta">{{ album.count }} 首</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.singer-page {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
}

.singer-header {
  position: relative;
  padding: 60px 24px 20px;
  min-height: 220px;
}

.header-bg {
  position: absolute;
  inset: 0;
  overflow: hidden;
  z-index: 0;
}

.header-bg img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  filter: blur(12px) brightness(0.4);
  transform: scale(1.2);
  opacity: 0;
  transition: opacity 0.5s;
}

.header-bg img.loaded {
  opacity: 1;
}

.bg-overlay {
  position: absolute;
  inset: 0;
  background: linear-gradient(transparent 0%, var(--td-bg-color-container) 100%);
}

.header-content {
  position: relative;
  z-index: 1;
}

.back-btn {
  position: absolute;
  top: -40px;
  left: 0;
  cursor: pointer;
  color: var(--td-text-color-primary);
  opacity: 0.8;
  transition: opacity 0.2s;
  padding: 4px;
}

.back-btn:hover {
  opacity: 1;
}

.singer-meta {
  display: flex;
  gap: 16px;
  align-items: flex-end;
}

.singer-cover {
  width: 100px;
  height: 100px;
  border-radius: 50%;
  overflow: hidden;
  flex-shrink: 0;
  background: var(--td-bg-color-secondarycontainer);
}

.singer-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.cover-placeholder {
  width: 100%;
  height: 100%;
  background: var(--td-bg-color-secondarycontainer);
}

.singer-info {
  flex: 1;
  min-width: 0;
}

.singer-name {
  font-size: 22px;
  font-weight: 700;
  margin: 0 0 4px;
  color: var(--td-text-color-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.singer-stats {
  font-size: 13px;
  color: var(--td-text-color-secondary);
  margin-bottom: 4px;
}

.singer-stats .dot {
  margin: 0 6px;
}

.singer-desc {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  margin: 4px 0 0;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.header-actions {
  margin-top: 16px;
}

.play-all-btn {
  padding: 8px 24px;
  border-radius: 20px;
  border: none;
  background: var(--td-brand-color);
  color: #fff;
  font-size: 14px;
  cursor: pointer;
  transition: opacity 0.2s;
}

.play-all-btn:hover {
  opacity: 0.85;
}

.singer-tabs {
  display: flex;
  padding: 0 24px;
  gap: 24px;
  border-bottom: 1px solid var(--td-border-level-1-color);
  position: sticky;
  top: 0;
  background: var(--td-bg-color-container);
  z-index: 10;
}

.tab {
  padding: 10px 0;
  font-size: 14px;
  color: var(--td-text-color-secondary);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
}

.tab.active {
  color: var(--td-brand-color);
  border-bottom-color: var(--td-brand-color);
}

.tab:hover {
  color: var(--td-text-color-primary);
}

.singer-content {
  padding: 0;
}

.loading-more,
.no-more,
.empty-state {
  text-align: center;
  padding: 24px;
  color: var(--td-text-color-placeholder);
  font-size: 13px;
}

.album-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 16px;
  padding: 16px 24px;
}

.album-card {
  cursor: pointer;
  transition: transform 0.2s;
}

.album-card:hover {
  transform: translateY(-2px);
}

.album-cover {
  width: 100%;
  aspect-ratio: 1;
  border-radius: 8px;
  overflow: hidden;
  background: var(--td-bg-color-secondarycontainer);
}

.album-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.album-title {
  font-size: 13px;
  margin-top: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--td-text-color-primary);
}

.album-meta {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  margin-top: 2px;
}
</style>

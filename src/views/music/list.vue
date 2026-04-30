<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { musicSdk, type MusicItem } from '@/services/musicSdk'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { storeToRefs } from 'pinia'

const route = useRoute()
const router = useRouter()
const playStatus = useGlobalPlayStatusStore()
const localUserStore = LocalUserDetailStore()
const { list } = storeToRefs(localUserStore)

const isLeaderboard = computed(() => route.query.isLeaderboard === 'true')
const playlistId = computed(() => route.params.id as string)
const playlistTitle = computed(() => (route.query.title as string) || '歌单')
const playlistCover = computed(() => (route.query.cover as string) || '/default-cover.png')
const playlistSource = computed(() => (route.query.source as string) || '')

const songs = ref<MusicItem[]>([])
const loading = ref(false)
const hasMore = ref(true)
const currentPage = ref(1)
const playlistInfo = ref<any>(null)

const fetchSongs = async (reset = false) => {
  if (loading.value) return
  if (reset) { currentPage.value = 1; songs.value = []; hasMore.value = true }
  if (!hasMore.value) return

  loading.value = true
  try {
    const res = isLeaderboard.value
      ? await musicSdk.getLeaderboardDetail(playlistId.value, currentPage.value)
      : await musicSdk.getPlaylistDetail(playlistId.value, currentPage.value)
    const newSongs = res?.list || []
    songs.value = reset ? newSongs : [...songs.value, ...newSongs]
    if (res?.info) playlistInfo.value = res.info
    hasMore.value = songs.value.length < (res?.total || 0)
    currentPage.value += 1
  } catch (e) { console.error('获取歌单详情失败:', e) }
  finally { loading.value = false }
}

const handlePlay = (song: MusicItem) => {
  playStatus.updatePlayerInfo(song as any)
  playSong(song as any)
}

const handlePlayAll = () => {
  if (songs.value.length === 0) return
  localUserStore.replaceSongList(songs.value.map(s => ({
    songmid: s.songmid, name: s.name, singer: s.singer,
    albumName: s.albumName, img: s.img, source: s.source,
    url: '', interval: s.interval
  })) as any)
  playSong(songs.value[0] as any)
  playStatus.updatePlayerInfo(songs.value[0] as any)
}

const handleScroll = (event: Event) => {
  const target = event.target as HTMLElement
  const { scrollTop, scrollHeight, clientHeight } = target
  if (scrollHeight - scrollTop - clientHeight < 100 && !loading.value && hasMore.value) fetchSongs()
}

onMounted(() => fetchSongs(true))
</script>

<template>
  <div class="list-container">
    <!-- 歌单头部 -->
    <div class="list-header">
      <div class="header-cover">
        <img :src="playlistCover" :alt="playlistTitle" />
      </div>
      <div class="header-info">
        <h2 class="header-title">{{ playlistTitle }}</h2>
        <div v-if="playlistSource" class="header-source">来源: {{ playlistSource }}</div>
        <div class="header-meta">
          <span class="song-count">共 {{ songs.length }} 首</span>
        </div>
        <div class="header-actions">
          <t-button theme="primary" @click="handlePlayAll" :disabled="songs.length === 0">
            <template #icon><i class="iconfont icon-bofang"></i></template>
            播放全部
          </t-button>
        </div>
      </div>
    </div>

    <!-- 歌曲列表 -->
    <div class="song-section" @scroll="handleScroll">
      <div class="song-list-header">
        <span class="col-index">#</span>
        <span class="col-name">歌曲</span>
        <span class="col-singer">歌手</span>
        <span class="col-album">专辑</span>
        <span class="col-duration">时长</span>
      </div>

      <div v-if="songs.length > 0" class="song-list">
        <div
          v-for="(song, index) in songs"
          :key="song.songmid || index"
          class="song-row"
          @click="handlePlay(song)"
        >
          <span class="col-index">{{ index + 1 }}</span>
          <div class="col-name">
            <span class="name-text">{{ song.name }}</span>
          </div>
          <span class="col-singer">{{ song.singer }}</span>
          <span class="col-album">{{ song.albumName }}</span>
          <span class="col-duration">{{ song.interval || '--:--' }}</span>
        </div>
      </div>

      <div v-if="loading" class="loading-state">
        <div class="loading-spinner"></div>
        <p>加载中...</p>
      </div>

      <div v-if="!loading && songs.length === 0" class="empty-state">
        <p>暂无歌曲</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.list-container { width: 100%; height: 100%; display: flex; flex-direction: column; overflow: hidden; }
.list-header { display: flex; gap: 20px; padding: 20px; flex-shrink: 0; }
.header-cover { width: 180px; height: 180px; border-radius: 12px; overflow: hidden; flex-shrink: 0; box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2); }
.header-cover img { width: 100%; height: 100%; object-fit: cover; }
.header-info { display: flex; flex-direction: column; justify-content: center; }
.header-title { font-size: 22px; font-weight: 700; color: var(--td-text-color-primary); margin: 0 0 8px; }
.header-source { font-size: 13px; color: var(--td-text-color-secondary); margin-bottom: 4px; }
.header-meta { font-size: 13px; color: var(--td-text-color-secondary); margin-bottom: 12px; }
.header-actions { display: flex; gap: 8px; }
.song-section { flex: 1; overflow-y: auto; padding: 0 20px 20px; }
.song-list-header { display: flex; align-items: center; padding: 8px 12px; font-size: 12px; color: var(--td-text-color-secondary); border-bottom: 1px solid var(--td-border-level-1-color); }
.song-row { display: flex; align-items: center; padding: 10px 12px; cursor: pointer; transition: background 0.15s; border-radius: 6px; }
.song-row:hover { background: var(--td-bg-color-component-hover); }
.col-index { width: 40px; font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; }
.col-name { flex: 3; min-width: 0; }
.name-text { font-size: 14px; color: var(--td-text-color-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; }
.col-singer { flex: 2; font-size: 13px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.col-album { flex: 2; font-size: 13px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.col-duration { width: 60px; font-size: 12px; color: var(--td-text-color-secondary); text-align: right; flex-shrink: 0; }
.loading-state { display: flex; flex-direction: column; align-items: center; padding: 40px; }
.loading-spinner { width: 40px; height: 40px; border: 3px solid var(--td-bg-color-component); border-top-color: var(--td-brand-color); border-radius: 50%; animation: spin 1s linear infinite; margin-bottom: 12px; }
.loading-state p { color: var(--td-text-color-secondary); }
@keyframes spin { to { transform: rotate(360deg); } }
.empty-state { display: flex; align-items: center; justify-content: center; min-height: 200px; }
.empty-state p { color: var(--td-text-color-secondary); }
</style>

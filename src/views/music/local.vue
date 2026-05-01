<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { useRouter } from 'vue-router'
import { MessagePlugin } from 'tdesign-vue-next'
import AddToPlaylistDialog from '@/components/Playlist/AddToPlaylistDialog.vue'

const router = useRouter()
const localUserStore = LocalUserDetailStore()
const playStatus = useGlobalPlayStatusStore()

const loading = ref(false)
const scanning = ref(false)
const tracks = ref<any[]>([])
const searchKeyword = ref('')
const coverCache = ref<Record<string, string>>({})

// Multi-select
const selectedIds = ref<Set<string>>(new Set())
const showAddToPlaylist = ref(false)
const songsToAdd = ref<any[]>([])

const hasSelection = computed(() => selectedIds.value.size > 0)

const isAllSelected = computed(() =>
  filteredTracks.value.length > 0 && filteredTracks.value.every(t => selectedIds.value.has(t.songmid))
)

const filteredTracks = computed(() => {
  if (!searchKeyword.value.trim()) return tracks.value
  const kw = searchKeyword.value.toLowerCase()
  return tracks.value.filter(t =>
    t.name.toLowerCase().includes(kw) || t.singer.toLowerCase().includes(kw) || t.albumName.toLowerCase().includes(kw)
  )
})

const fetchTracks = async () => {
  loading.value = true
  try {
    const res = await (window as any).api?.localMusic?.getList?.()
    if (res?.success) {
      tracks.value = res.data || []
      const ids = tracks.value.slice(0, 50).map((t: any) => t.songmid)
      if (ids.length) loadCovers(ids)
    }
  } catch (e) { console.error('获取本地音乐失败:', e) }
  finally { loading.value = false }
}

const loadCovers = async (ids: string[]) => {
  try {
    const res = await (window as any).api?.localMusic?.getCoversBase64?.(ids)
    if (res?.success && res.data) coverCache.value = { ...coverCache.value, ...res.data }
  } catch {}
}

const scanDirs = async () => {
  scanning.value = true
  try {
    const dirRes = await (window as any).api?.localMusic?.getDirs?.()
    const dirs = dirRes?.success ? (dirRes.data || []) : []
    if (dirs.length === 0) {
      MessagePlugin.warning('请先在设置中添加音乐目录')
      return
    }
    const scanRes = await (window as any).api?.localMusic?.scan?.(dirs)
    if (scanRes?.success) {
      const data = scanRes.data
      MessagePlugin.success(`扫描完成: ${data.scanned} 个文件, ${data.added} 首新增`)
      await fetchTracks()
    }
  } catch (e) { console.error('扫描失败:', e); MessagePlugin.error('扫描失败') }
  finally { scanning.value = false }
}

const handlePlay = (track: any) => {
  const song = {
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: coverCache.value[track.songmid] || '',
    source: 'local', url: track.url || '', interval: track.interval
  }
  playStatus.updatePlayerInfo(song)
  playSong(song)
  localUserStore.addSongToFirst(song)
}

const openTagEditor = (track: any) => {
  router.push({ name: 'local-tag-editor', query: { songmid: track.songmid } })
}

const formatDuration = (sec: number) => {
  if (!sec || !isFinite(sec)) return '--:--'
  const m = Math.floor(sec / 60), s = Math.floor(sec % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

const formatSize = (bytes: number) => {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / 1048576).toFixed(1) + ' MB'
}

// Selection
const toggleSelect = (songmid: string) => {
  const s = new Set(selectedIds.value)
  if (s.has(songmid)) s.delete(songmid)
  else s.add(songmid)
  selectedIds.value = s
}

const toggleSelectAll = () => {
  if (isAllSelected.value) {
    selectedIds.value = new Set()
  } else {
    selectedIds.value = new Set(filteredTracks.value.map(t => t.songmid))
  }
}

const clearSelection = () => { selectedIds.value = new Set() }

const batchPlay = () => {
  const selected = filteredTracks.value.filter(t => selectedIds.value.has(t.songmid))
  if (selected.length === 0) return
  const songList = selected.map(track => ({
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: coverCache.value[track.songmid] || '',
    source: 'local', url: track.url || '', interval: track.interval
  }))
  localUserStore.replaceSongList(songList as any)
  playSong(songList[0] as any)
  playStatus.updatePlayerInfo(songList[0] as any)
}

const batchAddToPlaylist = () => {
  const selected = filteredTracks.value.filter(t => selectedIds.value.has(t.songmid))
  if (selected.length === 0) return
  songsToAdd.value = selected.map(track => ({
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: coverCache.value[track.songmid] || '',
    source: 'local', url: track.url || '', interval: track.interval
  }))
  showAddToPlaylist.value = true
}

onMounted(() => fetchTracks())
</script>

<template>
  <div class="local-container">
    <div class="local-header">
      <h2>本地音乐 <span class="count">({{ tracks.length }} 首)</span></h2>
      <div class="header-actions">
        <t-input v-model="searchKeyword" placeholder="搜索本地音乐" clearable style="width: 240px" />
        <t-button theme="primary" :loading="scanning" @click="scanDirs">扫描音乐</t-button>
      </div>
    </div>

    <!-- Batch toolbar -->
    <div v-if="hasSelection" class="batch-toolbar">
      <span class="batch-info">已选择 {{ selectedIds.size }} 首</span>
      <t-button size="small" @click="batchPlay">播放选中</t-button>
      <t-button size="small" @click="batchAddToPlaylist">添加到歌单</t-button>
      <t-button size="small" variant="text" @click="clearSelection">取消选择</t-button>
    </div>

    <div v-if="loading" class="loading-state">
      <div class="loading-spinner"></div><p>加载中...</p>
    </div>
    <div v-else-if="filteredTracks.length > 0" class="track-list">
      <div class="list-header">
        <span class="col-check">
          <t-checkbox :checked="isAllSelected" @change="toggleSelectAll" />
        </span>
        <span class="col-name">歌曲</span>
        <span class="col-singer">歌手</span>
        <span class="col-album">专辑</span>
        <span class="col-duration">时长</span>
        <span class="col-size">大小</span>
        <span class="col-actions-header">操作</span>
      </div>
      <div v-for="track in filteredTracks" :key="track.songmid"
        class="track-row"
        :class="{ 'is-selected': selectedIds.has(track.songmid) }"
        @click="handlePlay(track)"
      >
        <div class="col-check" @click.stop>
          <t-checkbox :checked="selectedIds.has(track.songmid)" @change="toggleSelect(track.songmid)" />
        </div>
        <div class="col-name">
          <img v-if="coverCache[track.songmid]" :src="coverCache[track.songmid]" class="track-cover" />
          <img v-else src="/default-cover.png" class="track-cover" />
          <span>{{ track.name }}</span>
        </div>
        <span class="col-singer">{{ track.singer || '未知' }}</span>
        <span class="col-album">{{ track.albumName || '未知专辑' }}</span>
        <span class="col-duration">{{ formatDuration(track.duration) }}</span>
        <span class="col-size">{{ formatSize(track.size) }}</span>
        <div class="col-actions" @click.stop>
          <t-button variant="text" size="small" @click="openTagEditor(track)">编辑</t-button>
        </div>
      </div>
    </div>
    <div v-else class="empty-state">
      <p>{{ searchKeyword ? '没有匹配的音乐' : '还没有本地音乐，点击"扫描音乐"开始' }}</p>
    </div>

    <AddToPlaylistDialog
      v-model:visible="showAddToPlaylist"
      :songs="songsToAdd"
    />
  </div>
</template>

<style scoped>
.local-container { width: 100%; height: 100%; display: flex; flex-direction: column; overflow: hidden; padding: 20px; }
.local-header { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; margin-bottom: 16px; }
.local-header h2 {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
  margin: 0;
}
.count { font-size: 14px; font-weight: 400; color: var(--td-text-color-secondary); }
.header-actions { display: flex; gap: 8px; }
.batch-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  margin-bottom: 12px;
  background: var(--td-brand-color-light);
  border-radius: 8px;
  flex-shrink: 0;
}
.batch-info { font-size: 13px; color: var(--td-brand-color); font-weight: 500; margin-right: 4px; }
.track-list { flex: 1; overflow-y: auto; }
.list-header, .track-row { display: flex; align-items: center; padding: 8px 12px; }
.list-header { font-size: 12px; color: var(--td-text-color-secondary); border-bottom: 1px solid var(--td-border-level-1-color); }
.track-row { cursor: pointer; transition: background 0.15s; border-radius: 6px; }
.track-row:hover { background: var(--td-bg-color-component-hover); }
.track-row.is-selected { background: var(--td-brand-color-light); }
.col-check { width: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; }
.col-name { flex: 3; display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 14px; color: var(--td-text-color-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.col-singer { flex: 2; font-size: 13px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.col-album { flex: 2; font-size: 13px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.col-duration { width: 60px; font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; }
.col-size { width: 80px; font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; }
.col-actions { width: 60px; flex-shrink: 0; }
.col-actions-header { width: 60px; flex-shrink: 0; }
.track-cover { width: 36px; height: 36px; border-radius: 4px; object-fit: cover; flex-shrink: 0; }
.loading-state { display: flex; flex-direction: column; align-items: center; padding: 60px; }
.loading-spinner { width: 40px; height: 40px; border: 3px solid var(--td-bg-color-component); border-top-color: var(--td-brand-color); border-radius: 50%; animation: spin 1s linear infinite; margin-bottom: 12px; }
.loading-state p { color: var(--td-text-color-secondary); }
.empty-state { display: flex; align-items: center; justify-content: center; min-height: 300px; }
.empty-state p { color: var(--td-text-color-secondary); }
@keyframes spin { to { transform: rotate(360deg); } }
</style>

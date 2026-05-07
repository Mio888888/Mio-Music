<script setup lang="ts">
import { ref, shallowRef, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useVirtualList } from '@vueuse/core'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { useRouter } from 'vue-router'
import { MessagePlugin } from 'tdesign-vue-next'
import {
  ChevronRightIcon,
  RefreshIcon,
  EllipsisIcon,
  PlayCircleIcon,
  SearchIcon,
  FolderIcon
} from 'tdesign-icons-vue-next'
import AddToPlaylistDialog from '@/components/Playlist/AddToPlaylistDialog.vue'

const router = useRouter()
const localUserStore = LocalUserDetailStore()
const playStatus = useGlobalPlayStatusStore()

const loading = ref(false)
const scanning = ref(false)
const tracks = shallowRef<any[]>([])
const searchQuery = ref('')
const coverCache = ref<Record<string, string>>({})

// 目录管理
const scanDirs = ref<string[]>([])
const showDirModal = ref(false)

// 多选
const selectedIds = ref<Set<string>>(new Set())
const multiSelect = ref(false)
const showAddToPlaylist = ref(false)
const songsToAdd = ref<any[]>([])

// 右键菜单
const contextMenuVisible = ref(false)
const contextMenuPos = ref({ top: 0, left: 0 })
const contextMenuTrack = ref<any | null>(null)

// 当前播放
const currentSongId = computed(() => localUserStore.userInfo?.lastPlaySongId)

const displayTracks = computed(() => {
  const q = (searchQuery.value || '').trim().toLowerCase()
  if (!q) return tracks.value
  const includes = (s?: string) => !!s && s.toLowerCase().includes(q)
  return tracks.value.filter(t => includes(t.name) || includes(t.singer) || includes(t.albumName))
})

const hasSelection = computed(() => selectedIds.value.size > 0)
const isAllSelected = computed(() =>
  displayTracks.value.length > 0 && displayTracks.value.every(t => selectedIds.value.has(t.songmid))
)
const { list: virtualTracks, containerProps, wrapperProps } = useVirtualList(displayTracks, {
  itemHeight: 60,
  overscan: 8
})

watch(searchQuery, () => {
  containerProps.ref.value?.scrollTo({ top: 0 })
})

// 加载歌曲
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

// 加载封面
const loadCovers = async (ids: string[]) => {
  try {
    const res = await (window as any).api?.localMusic?.getCoversBase64?.(ids)
    if (res?.success && res.data) coverCache.value = { ...coverCache.value, ...res.data }
  } catch {}
}

// 目录管理
const fetchDirs = async () => {
  try {
    const res = await (window as any).api?.localMusic?.getDirs?.()
    if (res?.success && Array.isArray(res.data)) scanDirs.value = res.data
  } catch {}
}

const selectDirs = async () => {
  try {
    const res = await (window as any).api?.localMusic?.selectDirs?.()
    const dirs = res?.success ? (res.data || []) : (Array.isArray(res) ? res : [])
    if (Array.isArray(dirs) && dirs.length > 0) {
      scanDirs.value = Array.from(new Set([...scanDirs.value, ...dirs]))
    }
  } catch {}
}

const removeDir = (d: string) => {
  scanDirs.value = scanDirs.value.filter(x => x !== d)
}

const saveDirs = async () => {
  try {
    await (window as any).api?.localMusic?.setDirs?.(scanDirs.value)
    showDirModal.value = false
    MessagePlugin.success('目录已保存')
  } catch {
    MessagePlugin.error('保存目录失败')
  }
}

// 扫描
const scanLibrary = async () => {
  if (scanDirs.value.length === 0) {
    MessagePlugin.warning('请先选择扫描目录')
    return
  }
  scanning.value = true
  try {
    const scanRes = await (window as any).api?.localMusic?.scan?.(scanDirs.value)
    if (scanRes?.success) {
      const data = scanRes.data
      const parts = [`扫描完成: ${data.scanned} 个文件, ${data.added} 首新增`]
      if (data.updated) parts.push(`${data.updated} 首更新`)
      if (data.errors) parts.push(`${data.errors} 首失败`)
      MessagePlugin.success(parts.join('，'))
      await fetchTracks()
    }
  } catch (e) { console.error('扫描失败:', e); MessagePlugin.error('扫描失败') }
  finally { scanning.value = false }
}

// 清空
const clearScan = async () => {
  try {
    const res = await (window as any).api?.localMusic?.clearIndex?.()
    if (res?.success) {
      tracks.value = []
      MessagePlugin.success('已清空扫描索引')
    } else {
      MessagePlugin.error('清空失败')
    }
  } catch {
    MessagePlugin.error('清空失败')
  }
}

// 播放
const handlePlay = (track: any) => {
  const song = {
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: coverCache.value[track.songmid] || '',
    source: 'local', url: track.url || '', interval: track.interval,
    path: track.path, hasCover: !!track.hasCover
  }
  playStatus.updatePlayerInfo(song)
  playSong(song)
  localUserStore.addSongToFirst(song)
}

// 播放全部
const playAll = () => {
  if (displayTracks.value.length === 0) return
  const songList = displayTracks.value.map(track => ({
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: '',
    source: 'local', url: track.url || '', interval: track.interval,
    path: track.path, hasCover: !!track.hasCover
  }))
  localUserStore.replaceSongList(songList as any)
  playSong(songList[0] as any)
  playStatus.updatePlayerInfo(songList[0] as any)
  MessagePlugin.success(`正在播放 ${songList.length} 首本地歌曲`)
}

// 添加全部到播放列表
const addAllToPlaylist = () => {
  if (displayTracks.value.length === 0) return
  displayTracks.value.forEach(track => {
    const song = {
      songmid: track.songmid, name: track.name, singer: track.singer,
      albumName: track.albumName, img: '',
      source: 'local', url: track.url || '', interval: track.interval,
      hasCover: !!track.hasCover
    }
    localUserStore.addSong(song)
  })
  MessagePlugin.success('已将全部加入播放列表')
}

// 选择
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
    selectedIds.value = new Set(displayTracks.value.map(t => t.songmid))
  }
}

const clearSelection = () => { selectedIds.value = new Set() }

const batchPlay = () => {
  const selected = displayTracks.value.filter(t => selectedIds.value.has(t.songmid))
  if (selected.length === 0) return
  const songList = selected.map(track => ({
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: '',
    source: 'local', url: track.url || '', interval: track.interval,
    hasCover: !!track.hasCover
  }))
  localUserStore.replaceSongList(songList as any)
  playSong(songList[0] as any)
  playStatus.updatePlayerInfo(songList[0] as any)
}

const batchAddToPlaylist = () => {
  const selected = displayTracks.value.filter(t => selectedIds.value.has(t.songmid))
  if (selected.length === 0) return
  songsToAdd.value = selected.map(track => ({
    songmid: track.songmid, name: track.name, singer: track.singer,
    albumName: track.albumName, img: '',
    source: 'local', url: track.url || '', interval: track.interval,
    hasCover: !!track.hasCover
  }))
  showAddToPlaylist.value = true
}

// 右键菜单
const handleContextMenu = (e: MouseEvent, track: any) => {
  e.preventDefault()
  e.stopPropagation()
  contextMenuTrack.value = track
  contextMenuPos.value = { top: e.clientY, left: e.clientX }
  contextMenuVisible.value = true
}

const closeContextMenu = () => {
  contextMenuVisible.value = false
  contextMenuTrack.value = null
}

const handleMenuAction = (action: string) => {
  const track = contextMenuTrack.value
  if (!track) return
  closeContextMenu()
  if (action === 'play') handlePlay(track)
  else if (action === 'addToEnd') {
    const song = {
      songmid: track.songmid, name: track.name, singer: track.singer,
      albumName: track.albumName, img: '',
      source: 'local', url: track.url || '', interval: track.interval,
      hasCover: !!track.hasCover
    }
    localUserStore.addSong(song)
    MessagePlugin.success('已添加到播放列表')
  } else if (action === 'addToList') {
    songsToAdd.value = [{
      songmid: track.songmid, name: track.name, singer: track.singer,
      albumName: track.albumName, img: '',
      source: 'local', url: track.url || '', interval: track.interval,
      hasCover: !!track.hasCover
    }]
    showAddToPlaylist.value = true
  } else if (action === 'editTags') {
    router.push({ name: 'local-tag-editor', query: { songmid: track.songmid } })
  }
}

// 编辑标签
const openTagEditor = (track: any) => {
  router.push({ name: 'local-tag-editor', query: { songmid: track.songmid } })
}

// 格式化
const formatDuration = (sec: number) => {
  if (!sec || !isFinite(sec)) return '--:--'
  const m = Math.floor(sec / 60), s = Math.floor(sec % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

const formatSize = (bytes: number) => {
  if (!bytes) return ''
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / 1048576).toFixed(1) + ' MB'
}

// 更多操作下拉
const moreActions = [
  { content: '播放全部', value: 'playAll' },
  { content: '添加全部到播放列表', value: 'addAll' },
  { content: multiSelect.value ? '取消批量选择' : '批量选择', value: 'toggleMulti' },
  { content: '清空所有', value: 'clear' }
]

const handleMoreAction = (value: string) => {
  if (value === 'playAll') playAll()
  else if (value === 'addAll') addAllToPlaylist()
  else if (value === 'toggleMulti') { multiSelect.value = !multiSelect.value; if (!multiSelect.value) clearSelection() }
  else if (value === 'clear') clearScan()
}

const handleGlobalClick = () => { if (contextMenuVisible.value) closeContextMenu() }

// 生命周期
onMounted(async () => {
  await fetchDirs()
  await fetchTracks()
  document.addEventListener('click', handleGlobalClick)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleGlobalClick)
})
</script>

<template>
  <div class="local-container">
    <!-- 头部 -->
    <div class="local-header">
      <div class="left-container">
        <h2 class="title">
          本地音乐库<span style="font-size: 12px; color: var(--td-text-color-placeholder)">共 {{ tracks.length }} 首</span>
        </h2>
      </div>
      <div class="right-container">
        <t-button shape="round" theme="primary" variant="text" @click="showDirModal = true">
          <span style="display: flex; align-items: center">
            <span style="font-weight: bold">选择目录</span>
            <ChevronRightIcon :stroke-width="2.5" style="margin-left: 2px" />
          </span>
        </t-button>
      </div>
    </div>

    <!-- 控制栏 -->
    <div class="controls">
      <t-button theme="primary" class="local-btn play-all" @click="playAll" :disabled="tracks.length === 0">
        <template #icon><i class="iconfont icon-bofang"></i></template>
        播放全部
      </t-button>
      <t-button theme="default" class="local-btn scan" :loading="scanning" @click="scanLibrary">
        <template #icon><RefreshIcon :stroke-width="1.5" /></template>
      </t-button>
      <t-dropdown
        trigger="hover"
        :options="moreActions"
        placement="bottom-left"
        @click="(data: any) => handleMoreAction(data.value)"
      >
        <t-button theme="default" class="local-btn more">
          <template #icon><EllipsisIcon :stroke-width="1.5" /></template>
        </t-button>
      </t-dropdown>
      <div style="margin-left: auto; display: flex; align-items: center">
        <t-input
          v-model="searchQuery"
          clearable
          placeholder="搜索本地歌曲/歌手/专辑"
          style="width: 260px"
        >
          <template #prefix-icon><SearchIcon size="16px" /></template>
        </t-input>
      </div>
    </div>

    <!-- 批量操作栏 -->
    <div v-if="hasSelection" class="batch-toolbar">
      <span class="batch-info">已选择 {{ selectedIds.size }} 首</span>
      <t-button size="small" @click="batchPlay">播放选中</t-button>
      <t-button size="small" @click="batchAddToPlaylist">添加到歌单</t-button>
      <t-button size="small" variant="text" @click="clearSelection">取消选择</t-button>
    </div>

    <!-- 歌曲列表 -->
    <div v-if="loading" class="loading-container">
      <div class="loading-content">
        <div class="loading-spinner"></div>
        <p>加载中...</p>
      </div>
    </div>

    <div v-else-if="displayTracks.length > 0" class="list">
      <div class="list-header">
        <span class="col-check">
          <t-checkbox :checked="isAllSelected" @change="toggleSelectAll" />
        </span>
        <span class="col-cover"></span>
        <span class="col-name">歌曲</span>
        <span class="col-singer">歌手</span>
        <span class="col-album">专辑</span>
        <span class="col-duration">时长</span>
        <span class="col-size">大小</span>
        <span class="col-actions-header">操作</span>
      </div>
      <div v-bind="containerProps" class="list-body">
        <div v-bind="wrapperProps">
          <div
            v-for="{ data: track } in virtualTracks"
            :key="track.songmid"
            class="row"
            :class="{ 'is-selected': selectedIds.has(track.songmid) }"
            @click="multiSelect ? toggleSelect(track.songmid) : handlePlay(track)"
            @contextmenu="handleContextMenu($event, track)"
          >
            <div class="col-check" @click.stop>
              <t-checkbox :checked="selectedIds.has(track.songmid)" @change="toggleSelect(track.songmid)" />
            </div>
            <div class="col-cover">
              <img
                v-if="coverCache[track.songmid]"
                :src="coverCache[track.songmid]"
                class="track-cover"
                loading="lazy"
              />
              <img v-else src="/default-cover.png" class="track-cover" loading="lazy" />
            </div>
            <div class="col-name">
              <span class="name-text" :class="{ playing: track.songmid === currentSongId }">{{ track.name }}</span>
            </div>
            <span class="col-singer">{{ track.singer || '未知' }}</span>
            <span class="col-album">{{ track.albumName || '未知专辑' }}</span>
            <span class="col-duration">{{ formatDuration(track.duration || track.interval) }}</span>
            <span class="col-size">{{ formatSize(track.size) }}</span>
            <div class="col-actions" @click.stop>
              <t-button variant="text" size="small" @click="openTagEditor(track)">编辑</t-button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="empty">
      {{ searchQuery ? '没有匹配的音乐' : '暂无数据，点击选择目录后扫描' }}
    </div>

    <!-- 目录管理弹窗 -->
    <t-dialog
      v-model:visible="showDirModal"
      header="选择本地文件夹"
      placement="center"
      width="500px"
      :footer="false"
    >
      <div class="dir-modal-content">
        <div class="dir-hint">你可以添加常用目录，文件将即时索引。</div>
        <div v-for="d in scanDirs" :key="d" class="dir-row">
          <span class="dir-path">{{ d }}</span>
          <t-button size="small" variant="text" theme="danger" @click="removeDir(d)">删除</t-button>
        </div>
        <div class="dir-actions">
          <t-button block variant="outline" @click="selectDirs">添加文件夹</t-button>
          <t-button block theme="primary" @click="saveDirs">确认</t-button>
        </div>
      </div>
    </t-dialog>

    <!-- 右键菜单 -->
    <Teleport to="body">
      <div
        v-if="contextMenuVisible"
        class="context-menu"
        :style="{ top: contextMenuPos.top + 'px', left: contextMenuPos.left + 'px' }"
        @click.stop
      >
        <div class="menu-item" @click="handleMenuAction('play')">
          <PlayCircleIcon size="14px" /> 播放
        </div>
        <div class="menu-item" @click="handleMenuAction('addToEnd')">
          <i class="iconfont icon-zengjia" style="font-size:14px"></i> 加入播放列表
        </div>
        <div class="menu-item" @click="handleMenuAction('addToList')">
          <FolderIcon size="14px" /> 添加到本地歌单
        </div>
        <div class="menu-separator"></div>
        <div class="menu-item" @click="handleMenuAction('editTags')">
          <i class="iconfont icon-bianji" style="font-size:14px"></i> 编辑标签
        </div>
      </div>
    </Teleport>

    <AddToPlaylistDialog
      v-model:visible="showAddToPlaylist"
      :songs="songsToAdd"
    />
  </div>
</template>

<style scoped>
.local-container {
  padding: 0 2rem;
  padding-top: 1rem;
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 头部 */
.local-header {
  margin-bottom: 1rem;
  display: flex;
  flex-direction: row;
  justify-content: space-between;
  align-items: center;
}

.local-header .title {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  font-size: 28px;
  font-weight: 900;
  color: var(--td-text-color-primary);
  margin: 0;
}

.local-header .title span {
  padding-left: 8px;
  font-size: 18px;
}

/* 控制栏 */
.controls {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.local-btn {
  padding: 6px 9px;
  border-radius: 8px;
  height: 36px;
}

/* 批量操作栏 */
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

.batch-info {
  font-size: 13px;
  color: var(--td-brand-color);
  font-weight: 500;
  margin-right: 4px;
}

/* 加载状态 */
.loading-container {
  display: flex;
  justify-content: center;
  align-items: center;
  flex: 1;
}

.loading-content {
  text-align: center;
}

.loading-spinner {
  width: 40px;
  height: 40px;
  border: 4px solid var(--td-bg-color-component-hover);
  border-top: 4px solid var(--td-brand-color);
  border-radius: 50%;
  will-change: transform; animation: spin 1s linear infinite;
  margin: 0 auto 16px;
}

.loading-content p {
  font-size: 14px;
  color: var(--td-text-color-secondary);
  margin: 0;
}

/* 歌曲列表 */
.list {
  margin-top: 0;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 0;
  overflow: hidden;
}

.list-header {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  color: var(--td-text-color-secondary);
  border-bottom: 1px solid var(--td-border-level-1-color);
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--td-bg-color-container);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  flex-shrink: 0;
}

.list-body {
  flex: 1;
  overflow-y: auto;
}

.row {
  display: flex;
  align-items: center;
  min-height: 60px;
  padding: 10px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color var(--motion-duration-quick) var(--motion-ease-standard);
  box-sizing: border-box;
}

.row:hover {
  background: var(--td-bg-color-component-hover);
}

.row.is-selected {
  background: var(--td-brand-color-light);
}

/* 列宽 */
.col-check {
  width: 36px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.col-cover {
  width: 44px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.track-cover {
  width: 40px;
  height: 40px;
  border-radius: 4px;
  object-fit: cover;
}

.col-name {
  flex: 2;
  min-width: 0;
  font-size: 14px;
}

.name-text {
  color: var(--td-text-color-primary);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: block;
}

.name-text.playing {
  color: var(--td-brand-color);
}

.col-singer {
  flex: 1.5;
  font-size: 13px;
  color: var(--td-text-color-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.col-album {
  flex: 1.5;
  font-size: 13px;
  color: var(--td-text-color-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.col-duration {
  width: 60px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}

.col-size {
  width: 80px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}

.col-actions {
  width: 60px;
  flex-shrink: 0;
}

.col-actions-header {
  width: 60px;
  flex-shrink: 0;
}

/* 空状态 */
.empty {
  padding: 24px;
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--td-text-color-placeholder);
}

/* 目录弹窗 */
.dir-modal-content {
  padding: 0;
}

.dir-hint {
  margin-bottom: 10px;
  color: var(--td-text-color-secondary);
  font-size: 12px;
}

.dir-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  border-bottom: 1px solid var(--td-border-level-1-color);
}

.dir-path {
  font-size: 13px;
  color: var(--td-text-color-primary);
  word-break: break-all;
  flex: 1;
  margin-right: 12px;
}

.dir-actions {
  display: flex;
  gap: 12px;
  margin-top: 20px;
}

.dir-actions .t-button {
  flex: 1;
}

@keyframes spin { to { transform: rotate(360deg); } }

/* 响应式 */
@media (max-width: 768px) {
  .local-container {
    padding: var(--mobile-page-top-gutter) var(--mobile-page-gutter) 0;
    overflow: hidden;
  }

  .local-header {
    align-items: flex-start;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .local-header .title {
    border-left: none;
    padding-left: 0;
    font-size: clamp(2rem, 9vw, 2.6rem);
    line-height: 1.1;
    letter-spacing: -0.04em;
  }

  .local-header .title span {
    display: block;
    padding: 0.35rem 0 0;
    font-size: 1rem !important;
    font-weight: 500;
    letter-spacing: 0;
  }

  .right-container :deep(.t-button) {
    min-height: var(--mobile-touch-target);
  }

  .controls {
    flex-wrap: wrap;
    gap: 8px;
  }

  .controls > div {
    width: 100%;
    margin-left: 0 !important;
  }

  .controls :deep(.t-input) {
    width: 100% !important;
    min-height: var(--mobile-touch-target);
  }

  .local-btn {
    min-width: var(--mobile-touch-target);
    min-height: var(--mobile-touch-target);
    height: var(--mobile-touch-target);
    border-radius: var(--mobile-control-radius);
    touch-action: manipulation;
  }

  .list-body {
    -webkit-overflow-scrolling: touch;
  }

  .list-header {
    padding: 8px 4px;
  }

  .row {
    padding: 8px 4px;
    border-radius: var(--mobile-card-radius-small);
    touch-action: manipulation;
  }

  .col-check {
    width: var(--mobile-touch-target);
  }

  .col-cover {
    width: 48px;
  }

  .col-singer {
    flex: 1;
  }

  .col-album,
  .col-size {
    display: none;
  }

  .col-duration {
    width: 48px;
  }

  .col-actions,
  .col-actions-header {
    width: 52px;
  }

  .col-actions :deep(.t-button) {
    min-height: 36px;
  }

  .batch-toolbar {
    flex-wrap: wrap;
    border-radius: var(--mobile-card-radius-small);
  }

  .batch-toolbar :deep(.t-button) {
    min-height: 36px;
  }
}
</style>

<style>
/* 右键菜单 (unscoped) */
.context-menu {
  position: fixed;
  z-index: 9999;
  min-width: 156px;
  background: var(--td-bg-color-container);
  border-radius: 10px;
  box-shadow: var(--glass-shadow-control), 0 0 0 1px var(--td-border-level-1-color);
  padding: 4px;
  animation: menuIn var(--motion-duration-quick) var(--motion-ease-out);
}

.context-menu .menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  font-size: 13px;
  color: var(--td-text-color-primary);
  border-radius: 6px;
  cursor: pointer;
  transition: background-color var(--motion-duration-instant) var(--motion-ease-standard);
}

.context-menu .menu-item:hover {
  background: var(--td-bg-color-component-hover);
}

.context-menu .menu-separator {
  height: 1px;
  background: var(--td-border-level-1-color);
  margin: 4px 8px;
}

@keyframes menuIn {
  from { opacity: 0; transform: translateY(-4px) scale(0.96); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>

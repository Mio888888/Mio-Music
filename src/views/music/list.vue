<script setup lang="ts">
import { ref, onMounted, computed, onBeforeUnmount, onActivated, watch, nextTick } from 'vue'
import { useRoute } from 'vue-router'
import { musicSdk, type MusicItem } from '@/services/musicSdk'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { downloadSong } from '@/utils/downloadHelper'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { EllipsisIcon, SearchIcon, PlayCircleIcon } from 'tdesign-icons-vue-next'
import AddToPlaylistDialog from '@/components/Playlist/AddToPlaylistDialog.vue'
import SkeletonLoader from '@/components/SkeletonLoader.vue'

const route = useRoute()
const playStatus = useGlobalPlayStatusStore()
const localUserStore = LocalUserDetailStore()

// 路由参数
const isLocalPlaylist = computed(() => route.query.type === 'local')
const isLeaderboard = computed(() => route.query.isLeaderboard === 'true')
const playlistId = computed(() => route.params.id as string)
const playlistSource = computed(() => (route.query.source as string) || '')

// 歌单信息
const playlistInfo = ref({
  id: '',
  title: (route.query.title as string) || '歌单',
  author: (route.query.author as string) || '',
  cover: (route.query.cover as string) || '/default-cover.png',
  total: 0,
  source: (route.query.source as string) || '',
  desc: (route.query.description as string) || ''
})

const songs = ref<MusicItem[]>([])
const loading = ref(false)
const loadingMore = ref(false)
const hasMore = ref(true)
const currentPage = ref(1)
const totalCount = ref(0)
const playlistAuthor = ref('')

const currentSongId = computed(() => localUserStore.userInfo?.lastPlaySongId)
const hoveredIndex = ref(-1)
const activeMenuIndex = ref(-1)
const menuPos = ref({ top: 0, left: 0 })

// 搜索
const searchQuery = ref('')
const searchFocused = ref(false)
const displaySongs = computed(() => {
  const q = (searchQuery.value || '').trim().toLowerCase()
  if (!q) return songs.value
  const includes = (s?: string) => !!s && s.toLowerCase().includes(q)
  return songs.value.filter(s => includes(s.name) || includes(s.singer) || includes(s.albumName))
})

// 紧凑头部
const isHeaderCompact = ref(false)
const scrollContainer = ref<HTMLElement | null>(null)

// 封面编辑（本地歌单）
const fileInputRef = ref<HTMLInputElement | null>(null)

// 添加到歌单
const showAddToPlaylist = ref(false)
const songsToAdd = ref<any[]>([])

// 获取歌曲
const fetchSongs = async (reset = false) => {
  if (loading.value) return
  if (reset) { currentPage.value = 1; songs.value = []; hasMore.value = true }
  if (!hasMore.value) return

  loading.value = true
  try {
    // 本地歌单
    if (isLocalPlaylist.value) {
      playlistInfo.value.id = playlistId.value
      const rows = await localUserStore.getSongsForPlaylist(playlistId.value)
      const parsed = (rows || []).map(r => {
        try { return JSON.parse(r.data) } catch {
          return { songmid: r.songmid, name: r.name, singer: r.singer, albumName: r.albumName, img: r.img, source: 'local', interval: '' }
        }
      })
      songs.value = parsed
      totalCount.value = parsed.length
      playlistInfo.value.total = parsed.length
      const desc = (route.query.description as string) || ''
      if (desc) playlistInfo.value.desc = desc
      hasMore.value = false
      return
    }

    // 网络歌单
    if (loadingMore.value) return
    if (!reset && !hasMore.value) return
    if (reset) loadingMore.value = false
    else loadingMore.value = true

    const res = isLeaderboard.value
      ? await musicSdk.getLeaderboardDetail(playlistId.value, currentPage.value)
      : await musicSdk.getPlaylistDetail(playlistId.value, currentPage.value)
    const newSongs = res?.list || []
    songs.value = reset ? newSongs : [...songs.value, ...newSongs]
    totalCount.value = res?.total || 0
    if (res?.info) {
      playlistInfo.value.desc = res.info.desc || ''
      playlistAuthor.value = res.info.author || ''
    }
    hasMore.value = songs.value.length < (res?.total || 0)
    currentPage.value += 1
    loadingMore.value = false
  } catch (e) { console.error('获取歌单详情失败:', e) }
  finally { loading.value = false }
}

// 播放歌曲
const handlePlay = (song: MusicItem) => {
  playStatus.updatePlayerInfo(song as any)
  playSong(song as any)
}

// 播放全部
const handlePlayAll = () => {
  if (songs.value.length === 0) return
  const dialog = DialogPlugin.confirm({
    header: '播放歌单',
    body: `确定要用歌单"${playlistInfo.value.title}"中的 ${songs.value.length} 首歌曲替换当前播放列表吗？`,
    confirmBtn: '确定替换',
    cancelBtn: '取消',
    onConfirm: () => {
      dialog.destroy()
      const sourceSongs = displaySongs.value.length > 0 ? displaySongs.value : songs.value
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

// 随机播放
const handleShufflePlay = () => {
  if (songs.value.length === 0) return
  const dialog = DialogPlugin.confirm({
    header: '随机播放歌单',
    body: `确定要用歌单"${playlistInfo.value.title}"中的 ${songs.value.length} 首歌曲随机替换当前播放列表吗？`,
    confirmBtn: '确定替换',
    cancelBtn: '取消',
    onConfirm: () => {
      dialog.destroy()
      const sourceSongs = [...songs.value]
      for (let i = sourceSongs.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [sourceSongs[i], sourceSongs[j]] = [sourceSongs[j], sourceSongs[i]]
      }
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

// 下载全部
const handleDownloadAll = () => {
  if (songs.value.length === 0) return
  MessagePlugin.info(`开始下载 ${songs.value.length} 首歌曲`)
  songs.value.forEach(song => downloadSong(song))
}

// 收藏歌曲
const favoriteSong = async (song: MusicItem) => {
  try {
    let favId = await localUserStore.getFavoritesId()
    if (!favId) {
      const pl = await localUserStore.createPlaylist('我喜欢的音乐', '收藏的歌曲')
      if (!pl) { MessagePlugin.warning('创建收藏夹失败'); return }
      await localUserStore.setFavoritesId(pl.id)
      favId = pl.id
    }
    await localUserStore.addSongsToPlaylist(favId, [song as any])
    MessagePlugin.success('已收藏')
  } catch (e) {
    console.error('收藏失败:', e)
    MessagePlugin.error('收藏失败')
  }
}

// 从本地歌单移出
const handleRemoveFromPlaylist = async (song: MusicItem) => {
  if (!isLocalPlaylist.value) return
  try {
    const ok = await localUserStore.removeSongFromPlaylist(playlistInfo.value.id, String(song.songmid))
    if (ok) {
      songs.value = songs.value.filter(s => s.songmid !== song.songmid)
      playlistInfo.value.total = songs.value.length
      MessagePlugin.success(`已将"${song.name}"从歌单中移出`)
    } else {
      MessagePlugin.error('移出歌曲失败')
    }
  } catch {
    MessagePlugin.error('移出歌曲失败')
  }
}

// 菜单操作
const toggleMenu = (event: MouseEvent, index: number) => {
  event.stopPropagation()
  if (activeMenuIndex.value === index) {
    activeMenuIndex.value = -1
    return
  }
  const btn = event.currentTarget as HTMLElement
  const rect = btn.getBoundingClientRect()
  menuPos.value = {
    top: rect.bottom + 4,
    left: Math.max(8, rect.right - 156)
  }
  activeMenuIndex.value = index
}

const handleMenuAction = (action: string, index: number) => {
  const song = displaySongs.value[index]
  activeMenuIndex.value = -1
  if (action === 'play') handlePlay(song)
  else if (action === 'download') downloadSong(song)
  else if (action === 'favorite') favoriteSong(song)
  else if (action === 'add') { songsToAdd.value = [song as any]; showAddToPlaylist.value = true }
  else if (action === 'remove') handleRemoveFromPlaylist(song)
}

const closeMenu = () => { activeMenuIndex.value = -1 }

// 封面编辑
const handleCoverClick = () => {
  if (!isLocalPlaylist.value) return
  fileInputRef.value?.click()
}

const handleFileSelect = async (event: Event) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return
  if (!file.type.startsWith('image/')) {
    MessagePlugin.error('请选择图片文件')
    return
  }
  if (file.size > 5 * 1024 * 1024) {
    MessagePlugin.error('图片文件大小不能超过5MB')
    return
  }
  try {
    const reader = new FileReader()
    reader.onload = async (e) => {
      const base64Data = e.target?.result as string
      const ok = await localUserStore.updatePlaylistCover(playlistInfo.value.id, base64Data)
      if (ok) {
        playlistInfo.value.cover = base64Data
        MessagePlugin.success('封面更新成功')
      } else {
        MessagePlugin.error('封面更新失败')
      }
    }
    reader.readAsDataURL(file)
  } catch {
    MessagePlugin.error('处理图片文件失败')
  }
  target.value = ''
}

// 滚动处理
const handleScroll = (event: Event) => {
  const target = event.target as HTMLElement
  const { scrollTop, scrollHeight, clientHeight } = target
  if (activeMenuIndex.value >= 0) activeMenuIndex.value = -1

  // 紧凑头部
  isHeaderCompact.value = scrollTop > 100

  // 触底加载
  if (scrollHeight - scrollTop - clientHeight < 100 && !loading.value && !loadingMore.value && hasMore.value && !isLocalPlaylist.value) {
    fetchSongs()
  }
}

// 更多操作下拉
const moreDropdownVisible = ref(false)
const moreActions = computed(() => {
  const items: any[] = [
    { content: '下载全部', value: 'downloadAll' }
  ]
  return items
})

const handleMoreAction = (value: string) => {
  moreDropdownVisible.value = false
  if (value === 'downloadAll') handleDownloadAll()
}

watch(playlistId, () => fetchSongs(true))

onMounted(() => {
  fetchSongs(true)
  document.addEventListener('click', closeMenu)
})

onActivated(() => {
  if (songs.value.length === 0) fetchSongs(true)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', closeMenu)
})
</script>

<template>
  <div class="list-container">
    <!-- 隐藏的文件选择器 -->
    <input
      ref="fileInputRef"
      type="file"
      accept="image/*"
      style="display: none"
      @change="handleFileSelect"
    />

    <!-- 歌单头部 -->
    <div class="fixed-header" :class="{ compact: isHeaderCompact }">
      <div
        class="playlist-header"
        :class="{ compact: isHeaderCompact }"
        :style="{ '--header-cover': `url(${playlistInfo.cover})` }"
      >
        <!-- 封面 -->
        <div
          class="playlist-cover"
          :class="{ clickable: isLocalPlaylist }"
          @click="handleCoverClick"
        >
          <img :src="playlistInfo.cover" :alt="playlistInfo.title" />
          <div v-if="isLocalPlaylist" class="cover-overlay">
            <svg class="edit-icon" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
            </svg>
            <span>点击修改封面</span>
          </div>
        </div>

        <!-- 详情 -->
        <div class="playlist-details">
          <h1 class="playlist-title">{{ playlistInfo.title }}</h1>
          <div class="playlist-collapse" :class="{ hidden: isHeaderCompact }">
            <p class="playlist-desc">
              {{ playlistInfo.desc || 'By ' + playlistInfo.source }}
            </p>
            <p class="playlist-stats">{{ playlistInfo.total || songs.length }} 首歌曲</p>
          </div>
          <div class="playlist-actions">
            <t-button
              theme="primary"
              size="medium"
              :disabled="songs.length === 0 || loading"
              class="play-btn"
              @click="handlePlayAll"
            >
              <template #icon>
                <svg class="play-icon" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </template>
              播放全部
            </t-button>

            <t-button
              variant="outline"
              size="medium"
              :disabled="songs.length === 0 || loading"
              class="shuffle-btn"
              @click="handleShufflePlay"
            >
              <template #icon>
                <svg class="shuffle-icon" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z" />
                </svg>
              </template>
              随机播放
            </t-button>

            <t-dropdown
              :max-column-width="160"
              :options="moreActions"
              trigger="hover"
              @click="(data: any) => handleMoreAction(data.value)"
            >
              <t-button theme="default" class="action-btn-more">
                <template #icon>
                  <EllipsisIcon :stroke-width="1.5" />
                </template>
              </t-button>
            </t-dropdown>

            <div class="playlist-search" :class="{ focused: searchFocused || !!searchQuery }">
              <t-input
                v-model="searchQuery"
                :placeholder="searchFocused ? '搜索歌单内歌曲/歌手/专辑' : '搜索'"
                clearable
                @focus="searchFocused = true"
                @blur="searchFocused = !!searchQuery"
              >
                <template #prefix-icon>
                  <SearchIcon size="16px" />
                </template>
              </t-input>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 歌曲列表 -->
    <div ref="scrollContainer" class="scrollable-content" @scroll="handleScroll">
      <div v-if="loading" class="loading-container">
        <div class="loading-content">
          <div class="loading-spinner"></div>
          <p>加载中...</p>
        </div>
      </div>

      <div v-else class="song-list-wrapper">
        <div class="song-list-header">
          <span class="col-index">#</span>
          <span class="col-name">歌曲</span>
          <span class="col-singer">歌手</span>
          <span class="col-album">专辑</span>
          <span class="col-duration">时长</span>
        </div>

        <div v-if="displaySongs.length > 0" class="song-list">
          <div
            v-for="(song, index) in displaySongs"
            :key="song.songmid || index"
            class="song-row"
            :class="{ 'is-playing': song.songmid === currentSongId }"
            @click="handlePlay(song)"
            @mouseenter="hoveredIndex = index"
            @mouseleave="hoveredIndex = -1"
          >
            <span class="col-index">
              <i v-if="song.songmid === currentSongId" class="iconfont icon-bofang playing-icon"></i>
              <span v-else>{{ index + 1 }}</span>
            </span>
            <div class="col-name">
              <span class="name-text">{{ song.name }}</span>
            </div>
            <span class="col-singer">{{ song.singer }}</span>
            <span class="col-album">{{ song.albumName }}</span>
            <div class="col-actions">
              <template v-if="hoveredIndex === index">
                <button class="action-btn" @click.stop="handlePlay(song)" title="播放">
                  <i class="iconfont icon-bofang"></i>
                </button>
                <button class="action-btn" @click.stop="favoriteSong(song)" title="收藏">
                  <i class="iconfont icon-xinxi"></i>
                </button>
                <button class="action-btn" @click.stop="toggleMenu($event, index)" title="更多">
                  <i class="iconfont icon-gengduo"></i>
                </button>
              </template>
              <span v-else class="duration-text">{{ song.interval || '--:--' }}</span>
            </div>
          </div>
        </div>

        <div v-if="loadingMore" class="loading-more">
          <div class="loading-spinner small"></div>
        </div>

        <div v-if="!loading && displaySongs.length === 0" class="empty-state">
          <p>{{ searchQuery ? '没有匹配的歌曲' : '暂无歌曲' }}</p>
        </div>
      </div>
    </div>

    <!-- 更多操作菜单 -->
    <Teleport to="body">
      <div v-if="activeMenuIndex >= 0" class="more-menu" :style="{ top: menuPos.top + 'px', left: menuPos.left + 'px' }" @click.stop>
        <div class="menu-item" @click="handleMenuAction('play', activeMenuIndex)">
          <i class="iconfont icon-bofang"></i> 立即播放
        </div>
        <div class="menu-item" @click="handleMenuAction('favorite', activeMenuIndex)">
          <i class="iconfont icon-xinxi"></i> 收藏歌曲
        </div>
        <div class="menu-item" @click="handleMenuAction('add', activeMenuIndex)">
          <i class="iconfont icon-zengjia"></i> 添加到歌单
        </div>
        <div class="menu-item" @click="handleMenuAction('download', activeMenuIndex)">
          <i class="iconfont icon-xiazai"></i> 下载
        </div>
        <template v-if="isLocalPlaylist">
          <div class="menu-separator"></div>
          <div class="menu-item danger" @click="handleMenuAction('remove', activeMenuIndex)">
            <i class="iconfont icon-shanchu"></i> 从歌单移出
          </div>
        </template>
      </div>
    </Teleport>

    <AddToPlaylistDialog
      v-model:visible="showAddToPlaylist"
      :songs="songsToAdd"
    />
  </div>
</template>

<style scoped>
.list-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 20px;
  overflow: hidden;
}

/* 固定头部 */
.fixed-header {
  flex-shrink: 0;
  margin-bottom: 20px;
}

/* 歌单头部 */
.playlist-header {
  display: flex;
  align-items: center;
  gap: 1.5rem;
  padding: 1.5rem;
  height: 240px;
  background: var(--td-bg-color-container);
  border-radius: 0.75rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  position: relative;
  z-index: 1;
  overflow: hidden;
}

.playlist-header::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-image: var(--header-cover);
  background-size: cover;
  background-position: top center;
  background-repeat: no-repeat;
  z-index: -1;
  border-radius: inherit;
  filter: blur(10px);
  transform: scale(1.1);
  -webkit-mask-image: linear-gradient(to bottom, rgba(0, 0, 0, 0.6) 0%, rgba(0, 0, 0, 0.05) 60%, rgba(0, 0, 0, 0) 70%);
  mask-image: linear-gradient(to bottom, rgba(0, 0, 0, 0.6) 0%, rgba(0, 0, 0, 0.05) 60%, rgba(0, 0, 0, 0) 70%);
  opacity: 0.6;
}

@media (prefers-color-scheme: dark) {
  .playlist-header::before {
    filter: blur(10px) grayscale(0.5) brightness(0.6);
  }
}

.playlist-header.compact {
  height: 120px;
  padding: 1rem;
  gap: 1rem;
}

.playlist-header.compact .playlist-title {
  font-size: 25px;
}

/* 封面 */
.playlist-cover {
  height: 100%;
  aspect-ratio: 1 / 1;
  border-radius: 0.5rem;
  overflow: hidden;
  flex-shrink: 0;
  position: relative;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.playlist-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform 0.2s ease;
}

.playlist-cover.clickable {
  cursor: pointer;
}

.playlist-cover.clickable:hover .cover-overlay {
  opacity: 1;
}

.playlist-cover.clickable:hover img {
  transform: scale(1.05);
}

.cover-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s ease;
  color: white;
  font-size: 12px;
  text-align: center;
  padding: 8px;
}

.edit-icon {
  width: 24px;
  height: 24px;
  margin-bottom: 4px;
}

.cover-overlay span {
  font-weight: 500;
  line-height: 1.2;
}

/* 详情 */
.playlist-details {
  flex: 1;
  min-width: 0;
}

.playlist-title {
  line-height: 1em;
  font-size: 34px;
  font-weight: 800;
  color: var(--td-text-color-primary);
  margin: 0 0 0.5rem;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.playlist-collapse {
  transition: all 0.3s;
  overflow: hidden;
}

.playlist-collapse.hidden {
  opacity: 0;
  max-height: 0;
  margin: 0;
}

.playlist-desc {
  font-size: 1rem;
  color: var(--td-text-color-secondary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  text-overflow: ellipsis;
  overflow: hidden;
  margin: 0 0 0.5rem;
}

.playlist-stats {
  font-size: 0.875rem;
  color: var(--td-text-color-placeholder);
  margin: 0 0 0;
}

/* 操作按钮 */
.playlist-actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 1rem;
}

.play-btn,
.shuffle-btn {
  min-width: 120px;
}

.play-icon,
.shuffle-icon {
  width: 16px;
  height: 16px;
}

.action-btn-more {
  width: 36px;
  height: 36px;
  padding: 6px;
  border-radius: 8px;
}

/* 搜索 */
.playlist-search {
  margin-left: auto;
  width: 90px;
  transition: width 0.2s;
}

.playlist-search.focused {
  width: 250px;
}

/* 可滚动内容 */
.scrollable-content {
  background: var(--td-bg-color-container);
  border-radius: 8px;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

/* 加载状态 */
.loading-container {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
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
  animation: spin 1s linear infinite;
  margin: 0 auto 16px;
}

.loading-spinner.small {
  width: 24px;
  height: 24px;
  border-width: 2px;
}

.loading-content p {
  font-size: 14px;
  color: var(--td-text-color-secondary);
  margin: 0;
}

/* 歌曲列表 */
.song-list-wrapper {
  display: flex;
  flex-direction: column;
}

.song-list-header {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  border-bottom: 1px solid var(--td-border-level-1-color);
  position: sticky;
  top: 0;
  background: var(--td-bg-color-container);
  z-index: 3;
}

.song-list {
  flex: 1;
}

.song-row {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  cursor: pointer;
  transition: background 0.15s;
  border-radius: 8px;
}

.song-row:hover { background: var(--td-bg-color-component-hover); }
.song-row.is-playing { background: var(--td-brand-color-light); }
.song-row.is-playing .name-text { color: var(--td-brand-color); }
.song-row.is-playing .playing-icon { color: var(--td-brand-color); font-size: 14px; }

.col-index {
  width: 36px;
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  flex-shrink: 0;
}

.col-name { flex: 3; min-width: 0; }
.name-text {
  font-size: 14px;
  color: var(--td-text-color-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: block;
}

.col-singer {
  flex: 2;
  font-size: 13px;
  color: var(--td-text-color-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.col-album {
  flex: 2;
  font-size: 13px;
  color: var(--td-text-color-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.col-actions {
  width: 100px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.col-duration {
  width: 100px;
  flex-shrink: 0;
  text-align: right;
}

.duration-text {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
}

.action-btn {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--td-text-color-secondary);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s;
  font-size: 14px;
}

.action-btn:hover {
  color: var(--td-brand-color);
  background: var(--td-brand-color-light);
}

/* 加载更多 */
.loading-more { display: flex; justify-content: center; padding: 20px; }

/* 空状态 */
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  color: var(--td-text-color-placeholder);
}

@keyframes spin { to { transform: rotate(360deg); } }

/* 响应式 */
@media (max-width: 768px) {
  .list-container { padding: 15px; }

  .playlist-header {
    flex-direction: column;
    text-align: center;
    gap: 1rem;
    height: auto;
    padding: 1.5rem;
  }

  .playlist-cover {
    width: 100px;
    height: 100px;
  }

  .playlist-actions {
    flex-wrap: wrap;
    justify-content: center;
  }

  .play-btn, .shuffle-btn {
    min-width: auto;
  }

  .playlist-search {
    width: 100%;
    margin-left: 0;
  }

  .playlist-search.focused {
    width: 100%;
  }
}
</style>

<style>
/* 更多菜单 (unscoped, teleported to body) */
.more-menu {
  position: fixed;
  z-index: 9999;
  min-width: 156px;
  background: var(--td-bg-color-container);
  border-radius: 10px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.15), 0 0 0 1px var(--td-border-level-1-color);
  padding: 4px;
  animation: menuIn 0.15s ease;
}

.more-menu .menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  font-size: 13px;
  color: var(--td-text-color-primary);
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.12s;
}

.more-menu .menu-item:hover { background: var(--td-bg-color-component-hover); }
.more-menu .menu-item .iconfont { font-size: 14px; color: var(--td-text-color-secondary); }
.more-menu .menu-item:hover .iconfont { color: var(--td-brand-color); }

.more-menu .menu-item.danger { color: var(--td-error-color); }
.more-menu .menu-item.danger:hover { background: var(--td-error-color-light); }
.more-menu .menu-item.danger .iconfont { color: var(--td-error-color); }

.more-menu .menu-separator {
  height: 1px;
  background: var(--td-border-level-1-color);
  margin: 4px 8px;
}

@keyframes menuIn {
  from { opacity: 0; transform: translateY(-4px) scale(0.96); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>

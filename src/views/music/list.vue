<script setup lang="ts">
import { ref, onMounted, computed, onBeforeUnmount, onActivated, watch } from 'vue'
import { useRoute } from 'vue-router'
import { musicSdk, type MusicItem } from '@/services/musicSdk'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import SkeletonLoader from '@/components/SkeletonLoader.vue'

const route = useRoute()
const playStatus = useGlobalPlayStatusStore()
const localUserStore = LocalUserDetailStore()

const isLeaderboard = computed(() => route.query.isLeaderboard === 'true')
const playlistId = computed(() => route.params.id as string)
const playlistTitle = computed(() => (route.query.title as string) || '歌单')
const playlistCover = computed(() => (route.query.cover as string) || '/default-cover.png')
const playlistSource = computed(() => (route.query.source as string) || '')

const songs = ref<MusicItem[]>([])
const loading = ref(false)
const hasMore = ref(true)
const currentPage = ref(1)
const totalCount = ref(0)
const playlistDesc = ref('')
const playlistAuthor = ref('')

const currentSongId = computed(() => localUserStore.userInfo?.lastPlaySongId)
const hoveredIndex = ref(-1)
const activeMenuIndex = ref(-1)
const menuPos = ref({ top: 0, left: 0 })

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
    totalCount.value = res?.total || 0
    if (res?.info) {
      playlistDesc.value = res.info.desc || ''
      playlistAuthor.value = res.info.author || ''
    }
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
  if (activeMenuIndex.value >= 0) activeMenuIndex.value = -1
  if (scrollHeight - scrollTop - clientHeight < 100 && !loading.value && hasMore.value) fetchSongs()
}

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
  const song = songs.value[index]
  activeMenuIndex.value = -1
  if (action === 'play') handlePlay(song)
}

const closeMenu = () => { activeMenuIndex.value = -1 }

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
    <!-- 模糊背景 -->
    <div class="blur-bg" :style="{ backgroundImage: `url(${playlistCover})` }"></div>
    <div class="blur-overlay"></div>

    <!-- 歌单头部 -->
    <div class="list-header">
      <div class="header-cover">
        <img :src="playlistCover" :alt="playlistTitle" />
      </div>
      <div class="header-info">
        <h2 class="header-title">{{ playlistTitle }}</h2>
        <div class="header-meta">
          <span v-if="playlistAuthor">{{ playlistAuthor }}</span>
          <span v-if="playlistAuthor" class="meta-sep">·</span>
          <span v-if="totalCount">{{ totalCount }} 首</span>
          <span v-else>共 {{ songs.length }} 首</span>
        </div>
        <p v-if="playlistDesc" class="header-desc">{{ playlistDesc }}</p>
        <div class="header-actions">
          <t-button theme="primary" shape="round" @click="handlePlayAll" :disabled="songs.length === 0">
            <template #icon><i class="iconfont icon-bofang"></i></template>
            播放全部
          </t-button>
          <t-button variant="outline" shape="round">
            <template #icon><i class="iconfont icon-xiazai"></i></template>
            下载
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

      <SkeletonLoader v-if="loading && songs.length === 0" type="song-list" :rows="10" />

      <div v-if="songs.length > 0" class="song-list">
        <div
          v-for="(song, index) in songs"
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
              <button class="action-btn" title="收藏">
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

      <div v-if="loading && songs.length > 0" class="loading-more">
        <div class="loading-spinner"></div>
      </div>

      <div v-if="!loading && songs.length === 0" class="empty-state">
        <p>暂无歌曲</p>
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
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.list-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

/* 模糊背景 */
.blur-bg {
  position: absolute;
  top: -40px;
  left: -40px;
  right: -40px;
  height: 340px;
  background-size: cover;
  background-position: center;
  filter: blur(50px) saturate(1.8);
  opacity: 0.45;
  z-index: 0;
  pointer-events: none;
}

.blur-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 340px;
  background: linear-gradient(to bottom, transparent, var(--td-bg-color-container));
  z-index: 1;
  pointer-events: none;
}

/* 头部 */
.list-header {
  display: flex;
  gap: 24px;
  padding: 28px 24px 20px;
  flex-shrink: 0;
  position: relative;
  z-index: 2;
}

.header-cover {
  width: 180px;
  height: 180px;
  border-radius: 16px;
  overflow: hidden;
  flex-shrink: 0;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.25);
}

.header-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.header-info {
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  min-width: 0;
}

.header-title {
  font-size: 26px;
  font-weight: 700;
  color: var(--td-text-color-primary);
  margin: 0 0 8px;
  line-height: 1.3;
}

.header-meta {
  font-size: 13px;
  color: var(--td-text-color-secondary);
  margin-bottom: 8px;
}

.header-desc {
  font-size: 13px;
  color: var(--td-text-color-placeholder);
  margin: 0 0 16px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  line-height: 1.5;
}

.meta-sep { margin: 0 6px; }

.header-actions { display: flex; gap: 10px; }

/* 歌曲列表 */
.song-section {
  flex: 1;
  overflow-y: auto;
  padding: 0 24px 24px;
  position: relative;
  z-index: 2;
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

.song-list-header .col-duration {
  width: 100px;
  flex-shrink: 0;
  text-align: right;
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
.loading-spinner {
  width: 24px;
  height: 24px;
  border: 2px solid var(--td-bg-color-component);
  border-top-color: var(--td-brand-color);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

/* 空状态 */
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  color: var(--td-text-color-placeholder);
}

@keyframes spin { to { transform: rotate(360deg); } }
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

@keyframes menuIn {
  from { opacity: 0; transform: translateY(-4px) scale(0.96); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>

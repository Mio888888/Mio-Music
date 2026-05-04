<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, onActivated, onDeactivated } from 'vue'
import { useRouter } from 'vue-router'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { Edit2Icon, PlayCircleIcon, DeleteIcon, ViewListIcon, DownloadIcon } from 'tdesign-icons-vue-next'
import { LocalUserDetailStore, type PlaylistRow } from '@/store/LocalUserDetail'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { downloadSingleSong } from '@/utils/audio/download'
import type { SongList } from '@/types/audio'
import defaultCover from '/default-cover.png'

const router = useRouter()
const localUserStore = LocalUserDetailStore()
const playStatus = useGlobalPlayStatusStore()

const playlists = ref<PlaylistRow[]>([])
const loading = ref(false)
const favoritesId = ref<string | null>(null)
const scrollRef = ref<HTMLElement>()
const scrollTop = ref(0)

// 对话框状态
const showCreatePlaylistDialog = ref(false)
const showEditPlaylistDialog = ref(false)
const showImportDialog = ref(false)
const showNetworkImportDialog = ref(false)

// 创建表单
const newPlaylistForm = ref({ name: '', description: '' })

// 编辑表单
const editPlaylistForm = ref({ name: '', description: '' })
const currentEditingPlaylist = ref<PlaylistRow | null>(null)

// 网络导入
const networkPlaylistUrl = ref('')
const importPlatformType = ref('wy')
const songlistFileInputRef = ref<HTMLInputElement | null>(null)

// 加载歌单列表
const loadPlaylists = async () => {
  loading.value = true
  try {
    await localUserStore.loadPlaylists()
    playlists.value = [...localUserStore.playlists]

    // "我的喜欢"置顶
    try {
      const favId = await localUserStore.getFavoritesId()
      favoritesId.value = favId
      if (favId) {
        const idx = playlists.value.findIndex(p => p.id === favId)
        if (idx > 0) {
          const fav = playlists.value.splice(idx, 1)[0]
          playlists.value.unshift(fav)
        }
      }
    } catch {}
  } catch {
    MessagePlugin.error('加载歌单失败')
  } finally {
    loading.value = false
  }
}

// 创建歌单
const createPlaylist = async () => {
  if (!newPlaylistForm.value.name.trim()) {
    MessagePlugin.warning('歌单名称不能为空')
    return
  }
  try {
    const created = await localUserStore.createPlaylist(
      newPlaylistForm.value.name.trim(),
      newPlaylistForm.value.description.trim() || undefined,
      'local'
    )
    if (created) {
      MessagePlugin.success('歌单创建成功')
      showCreatePlaylistDialog.value = false
      newPlaylistForm.value = { name: '', description: '' }
    } else {
      MessagePlugin.error('创建歌单失败')
    }
  } catch {
    MessagePlugin.error('创建歌单失败')
  }
}

// 编辑歌单
const editPlaylist = (playlist: PlaylistRow) => {
  currentEditingPlaylist.value = playlist
  editPlaylistForm.value = {
    name: playlist.name,
    description: playlist.description || ''
  }
  showEditPlaylistDialog.value = true
}

const savePlaylistEdit = async () => {
  if (!currentEditingPlaylist.value) return
  if (!editPlaylistForm.value.name.trim()) {
    MessagePlugin.warning('歌单名称不能为空')
    return
  }
  try {
    const ok = await localUserStore.updatePlaylist(
      currentEditingPlaylist.value.id,
      editPlaylistForm.value.name.trim(),
      editPlaylistForm.value.description.trim()
    )
    if (ok) {
      MessagePlugin.success('歌单信息更新成功')
      showEditPlaylistDialog.value = false
      currentEditingPlaylist.value = null
      await loadPlaylists()
    } else {
      MessagePlugin.error('更新歌单信息失败')
    }
  } catch {
    MessagePlugin.error('更新歌单信息失败')
  }
}

// 删除歌单
const deletePlaylist = (playlist: PlaylistRow) => {
  const confirmDialog = DialogPlugin.confirm({
    header: '确认删除',
    body: `确定要删除歌单"${playlist.name}"吗？此操作不可撤销。`,
    confirmBtn: '删除',
    cancelBtn: '取消',
    theme: 'danger',
    onConfirm: async () => {
      try {
        const ok = await localUserStore.deletePlaylist(playlist.id)
        if (ok) {
          MessagePlugin.success('歌单删除成功')
          playlists.value = playlists.value.filter(p => p.id !== playlist.id)
        } else {
          MessagePlugin.error('删除歌单失败')
        }
      } catch {
        MessagePlugin.error('删除歌单失败')
      }
      confirmDialog.destroy()
    },
    onCancel: () => confirmDialog.destroy()
  })
}

// 查看歌单详情
const viewPlaylist = (playlist: PlaylistRow) => {
  router.push({
    name: 'list',
    params: { id: playlist.id },
    query: {
      title: playlist.name,
      cover: playlist.coverImgUrl || '',
      source: playlist.source,
      type: 'local',
      description: playlist.description || ''
    }
  })
}

// 播放歌单
const playPlaylist = async (playlist: PlaylistRow) => {
  try {
    const rows = await localUserStore.getSongsForPlaylist(playlist.id)
    if (!rows || rows.length === 0) {
      MessagePlugin.warning('歌单中没有歌曲')
      return
    }
    const songs: SongList[] = rows.map(r => {
      try {
        return JSON.parse(r.data)
      } catch {
        return {
          songmid: r.songmid, name: r.name, singer: r.singer,
          albumName: r.albumName, img: r.img, source: 'local', url: ''
        } as any
      }
    })
    localUserStore.replaceSongList(songs)
    playSong(songs[0] as any)
    playStatus.updatePlayerInfo(songs[0] as any)
    MessagePlugin.success(`正在播放歌单"${playlist.name}"`)
  } catch {
    MessagePlugin.error('播放歌单失败')
  }
}

// 下载歌单全部歌曲
const downloadPlaylist = async (playlist: PlaylistRow) => {
  try {
    const rows = await localUserStore.getSongsForPlaylist(playlist.id)
    if (!rows || rows.length === 0) {
      MessagePlugin.warning('歌单中没有歌曲')
      return
    }
    const songs: any[] = rows.map(r => {
      try { return JSON.parse(r.data) } catch {
        return { songmid: r.songmid, name: r.name, singer: r.singer, albumName: r.albumName, img: r.img, source: 'local' }
      }
    })
    MessagePlugin.info(`开始下载 ${songs.length} 首歌曲`)
    songs.forEach(s => downloadSingleSong(s as any))
  } catch {
    MessagePlugin.error('下载失败')
  }
}

// --- 导入功能 ---

// 从播放列表导入
const importFromPlaylist = async () => {
  showImportDialog.value = false
  const currentList = JSON.parse(JSON.stringify(localUserStore.list))
  if (!currentList || currentList.length === 0) {
    MessagePlugin.warning('当前播放列表为空，无法导入')
    return
  }
  try {
    const now = new Date()
    const playlistName = `播放列表 ${now.getFullYear()}-${(now.getMonth() + 1).toString().padStart(2, '0')}-${now.getDate().toString().padStart(2, '0')} ${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}`
    const created = await localUserStore.createPlaylist(playlistName, `从播放列表导入，共 ${currentList.length} 首歌曲`, 'local')
    if (created) {
      const added = await localUserStore.addSongsToPlaylist(created.id, currentList)
      MessagePlugin.success(`成功从播放列表导入 ${added} 首歌曲到歌单"${playlistName}"`)
      await loadPlaylists()
    } else {
      MessagePlugin.error('创建歌单失败')
    }
  } catch {
    MessagePlugin.error('从播放列表导入失败')
  }
}

// 从本地文件导入
const triggerSonglistFileInput = () => {
  showImportDialog.value = false
  songlistFileInputRef.value?.click()
}

const handleSonglistFileChange = async (e: Event) => {
  const input = e.target as HTMLInputElement
  if (!input.files?.length) return
  const file = input.files[0]
  try {
    const text = await file.text()
    let imported: any[]
    try {
      imported = JSON.parse(text)
    } catch {
      MessagePlugin.error('无法解析歌单文件')
      return
    }
    if (!Array.isArray(imported)) {
      MessagePlugin.error('歌单格式不正确')
      return
    }
    const rawName = file.name.replace(/\.(cmpl|cpl|json)$/i, '')
    const created = await localUserStore.createPlaylist(rawName, '从本地文件导入', 'local')
    if (created) {
      const added = await localUserStore.addSongsToPlaylist(created.id, imported)
      MessagePlugin.success(`成功导入 ${added} 首歌曲到歌单"${rawName}"`)
      await loadPlaylists()
    }
  } catch (err) {
    MessagePlugin.error(`导入失败: ${(err as Error).message}`)
  } finally {
    input.value = ''
  }
}

// 从网络歌单导入
const importFromNetwork = () => {
  showImportDialog.value = false
  showNetworkImportDialog.value = true
  networkPlaylistUrl.value = ''
  importPlatformType.value = 'wy'
}

const confirmNetworkImport = async () => {
  if (!networkPlaylistUrl.value.trim()) {
    MessagePlugin.warning('请输入有效的歌单链接')
    return
  }
  showNetworkImportDialog.value = false
  await handleNetworkPlaylistImport(networkPlaylistUrl.value.trim())
}

const cancelNetworkImport = () => {
  showNetworkImportDialog.value = false
  networkPlaylistUrl.value = ''
}

// 解析歌单 ID
const resolvePlaylistId = (input: string, platform: string): string | null => {
  const isNumeric = /^\d+$/.test(input)
  if (platform === 'wy') {
    const m = input.match(/(?:music\.163\.com\/.*[?&]id=|playlist\?id=|playlist\/|id=)(\d+)/i)
    return m?.[1] || (isNumeric ? input : null)
  }
  if (platform === 'tx') {
    const m = input.match(/(?:y\.qq\.com\/n\/ryqq\/playlist\/|music\.qq\.com\/.*[?&]id=|playlist[?&]id=|[?&]id=)(\d+)/i)
    return m?.[1] || (isNumeric ? input : null)
  }
  if (platform === 'kw') {
    const m = input.match(/(?:kuwo\.cn\/playlist_detail\/|kuwo\.cn\/.*[?&]pid=|[?&](?:pid|id)=)(\d+)/i)
    return m?.[1] || (isNumeric ? input : null)
  }
  if (platform === 'kg') {
    return input // 酷狗传完整链接
  }
  if (platform === 'mg') {
    const m = input.match(/[?&]id=(\d+)/i)
    return m?.[1] || (isNumeric ? input : null)
  }
  if (platform === 'bd') {
    const m = input.match(/[?&]playlistId=(\d+)/i)
    return m?.[1] || (isNumeric ? input : null)
  }
  return isNumeric ? input : null
}

const platformNames: Record<string, string> = {
  wy: '网易云音乐', tx: 'QQ音乐', kw: '酷我音乐',
  bd: '波点音乐', kg: '酷狗音乐', mg: '咪咕音乐'
}

const handleNetworkPlaylistImport = async (input: string) => {
  const platform = importPlatformType.value
  const pName = platformNames[platform] || '音乐平台'
  const playlistId = resolvePlaylistId(input, platform)

  if (!playlistId) {
    MessagePlugin.error(`无法识别的${pName}歌单链接或ID格式`)
    return
  }

  const loadMsg = MessagePlugin.loading('正在获取歌单信息...', 0)
  try {
    let allSongs: any[] = []
    let detailInfo: any = {}
    let page = 1
    let total = Infinity

    while (allSongs.length < total) {
      const detailResult = await (window as any).api?.music?.requestSdk?.('getPlaylistDetail', {
        source: platform,
        id: playlistId,
        page
      })
      if (!detailResult || detailResult.error) {
        MessagePlugin.error(`获取${pName}歌单详情失败` + (detailResult?.error ? `：${detailResult.error}` : ''))
        return
      }
      const list = detailResult.list || []
      if (detailResult.info) detailInfo = detailResult.info
      total = detailResult.total || list.length
      allSongs = allSongs.concat(list)
      page++
      if (list.length === 0) break
    }

    if (allSongs.length === 0) {
      MessagePlugin.warning('该歌单没有歌曲')
      return
    }

    // 获取封面
    const songsNeedPic = allSongs.filter(s => !s.img)
    if (songsNeedPic.length > 0 && songsNeedPic.length <= 50) {
      await Promise.all(songsNeedPic.map(async (song) => {
        try {
          const url = await (window as any).api?.music?.requestSdk?.('getPic', {
            source: platform,
            songInfo: song
          })
          if (typeof url === 'string') song.img = url
        } catch {}
      }))
    }

    // 创建本地歌单
    const coverImg = detailInfo.cover || detailInfo.img || ''
    const name = `导入自${pName}`
    const created = await localUserStore.createPlaylist(name, `从${pName}导入，共 ${allSongs.length} 首`, platform)
    if (created) {
      const added = await localUserStore.addSongsToPlaylist(created.id, allSongs)
      if (coverImg) await localUserStore.updatePlaylistCover(created.id, coverImg)
      MessagePlugin.success(`成功导入 ${added} 首歌曲到歌单"${name}"`)
      await loadPlaylists()
    }
  } catch (err) {
    MessagePlugin.error(`导入失败: ${(err as Error).message}`)
  } finally {
    loadMsg.then((inst: any) => inst.close())
  }
}

// 右键菜单
const contextMenuVisible = ref(false)
const contextMenuPos = ref({ top: 0, left: 0 })
const contextMenuPlaylist = ref<PlaylistRow | null>(null)

const handleContextMenu = (e: MouseEvent, playlist: PlaylistRow) => {
  e.preventDefault()
  e.stopPropagation()
  contextMenuPlaylist.value = playlist
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  contextMenuPos.value = { top: e.clientY, left: e.clientX }
  contextMenuVisible.value = true
}

const closeContextMenu = () => {
  contextMenuVisible.value = false
  contextMenuPlaylist.value = null
}

const handleMenuAction = async (action: string) => {
  const pl = contextMenuPlaylist.value
  if (!pl) return
  closeContextMenu()
  if (action === 'play') await playPlaylist(pl)
  else if (action === 'view') viewPlaylist(pl)
  else if (action === 'edit') editPlaylist(pl)
  else if (action === 'delete') deletePlaylist(pl)
  else if (action === 'download') await downloadPlaylist(pl)
}

// 格式化日期
const formatDate = (iso: string) => {
  if (!iso) return ''
  try {
    const d = new Date(iso)
    return `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, '0')}-${d.getDate().toString().padStart(2, '0')}`
  } catch { return '' }
}

const handleGlobalClick = () => { if (contextMenuVisible.value) closeContextMenu() }

// 生命周期
onMounted(() => {
  loadPlaylists()
  document.addEventListener('click', handleGlobalClick)
})
onBeforeUnmount(() => {
  document.removeEventListener('click', handleGlobalClick)
})
onActivated(() => { if (scrollRef.value) scrollRef.value.scrollTop = scrollTop.value })
onDeactivated(() => { if (scrollRef.value) scrollTop.value = scrollRef.value.scrollTop })
</script>

<template>
  <div ref="scrollRef" class="page">
    <input
      ref="songlistFileInputRef"
      accept=".cmpl,.cpl,.json"
      style="display: none"
      type="file"
      @change="handleSonglistFileChange"
    />
    <div class="local-container">
      <!-- 页面标题 -->
      <div class="page-header">
        <div class="header-left">
          <h2>本地歌单</h2>
        </div>
        <div class="header-actions">
          <t-button theme="primary" variant="outline" @click="showCreatePlaylistDialog = true">
            <i class="iconfont icon-zengjia"></i>
            新建歌单
          </t-button>
          <t-button theme="primary" @click="showImportDialog = true">
            <i class="iconfont icon-daoru"></i>
            导入
          </t-button>
        </div>
      </div>

      <!-- 歌单区域 -->
      <div class="playlists-section">
        <div class="section-header">
          <h3>我的歌单 ({{ playlists.length }})</h3>
          <div class="section-actions">
            <t-button :loading="loading" size="small" theme="primary" variant="text" @click="loadPlaylists">
              <i class="iconfont icon-shuaxin"></i>
              刷新
            </t-button>
          </div>
        </div>

        <!-- 加载状态 -->
        <div v-if="loading" class="loading-state">
          <t-loading size="large" text="加载中..." />
        </div>

        <!-- 歌单网格 -->
        <div v-else-if="playlists.length > 0" class="playlists-grid">
          <div
            v-for="playlist in playlists"
            :key="playlist.id"
            class="playlist-card"
            @contextmenu="handleContextMenu($event, playlist)"
          >
            <div class="playlist-cover" @click="viewPlaylist(playlist)">
              <img
                v-if="playlist.coverImgUrl && playlist.coverImgUrl !== 'default-cover'"
                :alt="playlist.name"
                :src="playlist.coverImgUrl"
                class="cover-image"
              />
              <img v-else :alt="playlist.name" :src="defaultCover" class="cover-image" />
              <div class="cover-overlay">
                <i class="iconfont icon-bofang"></i>
              </div>
            </div>
            <div class="playlist-info">
              <div class="playlist-name-row" @click="viewPlaylist(playlist)">
                <div :title="playlist.name" class="playlist-name-text">
                  {{ playlist.name }}
                </div>
                <div v-if="playlist.id === favoritesId" class="playlist-tags">
                  <t-tag size="small" theme="danger" variant="light-outline">我的喜欢</t-tag>
                </div>
              </div>
              <div :title="playlist.description" class="playlist-description">
                {{ playlist.description || '这个人很懒并没有留下任何描述...' }}
              </div>
              <div class="playlist-meta">
                <span class="source-tag">{{ playlist.source || 'local' }}</span>
                <span v-if="playlist.createTime">创建于 {{ formatDate(playlist.createTime) }}</span>
              </div>
            </div>
            <div class="playlist-actions">
              <t-tooltip content="播放歌单">
                <t-button shape="circle" size="small" theme="primary" variant="text" @click="playPlaylist(playlist)">
                  <i class="iconfont icon-bofang"></i>
                </t-button>
              </t-tooltip>
              <t-tooltip content="查看详情">
                <t-button shape="circle" size="small" theme="default" variant="text" @click="viewPlaylist(playlist)">
                  <ViewListIcon :stroke-width="1.5" />
                </t-button>
              </t-tooltip>
              <t-tooltip content="编辑歌单">
                <t-button shape="circle" size="small" theme="success" variant="text" @click="editPlaylist(playlist)">
                  <Edit2Icon />
                </t-button>
              </t-tooltip>
              <t-tooltip content="删除歌单">
                <t-button shape="circle" size="small" theme="danger" variant="text" @click="deletePlaylist(playlist)">
                  <i class="iconfont icon-shanchu"></i>
                </t-button>
              </t-tooltip>
            </div>
          </div>
        </div>

        <!-- 空状态 -->
        <div v-else class="empty-playlists">
          <div class="empty-icon">
            <i class="iconfont icon-gedan"></i>
          </div>
          <h4>暂无歌单</h4>
          <p>创建您的第一个歌单来管理音乐</p>
          <t-button theme="primary" @click="showCreatePlaylistDialog = true">
            <i class="iconfont icon-zengjia"></i>
            创建歌单
          </t-button>
        </div>
      </div>
    </div>

    <!-- 创建歌单对话框 -->
    <t-dialog
      v-model:visible="showCreatePlaylistDialog"
      :cancel-btn="{ content: '取消' }"
      :confirm-btn="{ content: '创建', theme: 'primary' }"
      header="创建新歌单"
      placement="center"
      width="500px"
      @confirm="createPlaylist"
    >
      <div class="create-form">
        <t-form :data="newPlaylistForm" layout="vertical">
          <t-form-item label="歌单名称" name="name" required>
            <t-input
              v-model="newPlaylistForm.name"
              clearable
              placeholder="请输入歌单名称"
              @keyup.enter="createPlaylist"
            />
          </t-form-item>
          <t-form-item label="歌单描述" name="description">
            <t-textarea
              v-model="newPlaylistForm.description"
              :autosize="{ minRows: 3, maxRows: 5 }"
              :maxlength="200"
              placeholder="请输入歌单描述（可选）"
            />
          </t-form-item>
        </t-form>
      </div>
    </t-dialog>

    <!-- 编辑歌单对话框 -->
    <t-dialog
      v-model:visible="showEditPlaylistDialog"
      :cancel-btn="{ content: '取消', variant: 'outline' }"
      :confirm-btn="{ content: '保存', theme: 'primary' }"
      header="编辑歌单信息"
      placement="center"
      width="500px"
      @confirm="savePlaylistEdit"
    >
      <div class="edit-playlist-content">
        <div class="form-item">
          <label class="form-label">歌单名称</label>
          <t-input
            v-model="editPlaylistForm.name"
            autofocus
            clearable
            maxlength="50"
            placeholder="请输入歌单名称"
            show-word-limit
          />
        </div>
        <div class="form-item">
          <label class="form-label">歌单描述</label>
          <t-textarea
            v-model="editPlaylistForm.description"
            :autosize="{ minRows: 3, maxRows: 6 }"
            maxlength="200"
            placeholder="请输入歌单描述（可选）"
            show-word-limit
          />
        </div>
      </div>
    </t-dialog>

    <!-- 导入选择对话框 -->
    <t-dialog
      v-model:visible="showImportDialog"
      :footer="false"
      header="选择导入方式"
      placement="center"
      width="400px"
    >
      <div class="import-options">
        <div class="import-option" @click="importFromPlaylist">
          <div class="option-icon">
            <i class="iconfont icon-liebiao"></i>
          </div>
          <div class="option-content">
            <h4>从播放列表</h4>
            <p>将当前播放列表保存为歌单</p>
          </div>
          <div class="option-arrow">
            <i class="iconfont icon-youjiantou"></i>
          </div>
        </div>
        <div class="import-option" @click="triggerSonglistFileInput">
          <div class="option-icon">
            <i class="iconfont icon-daoru"></i>
          </div>
          <div class="option-content">
            <h4>从本地歌单文件</h4>
            <p>导入歌单文件（.cmpl/.cpl/.json）</p>
          </div>
          <div class="option-arrow">
            <i class="iconfont icon-youjiantou"></i>
          </div>
        </div>
        <div class="import-option" @click="importFromNetwork">
          <div class="option-icon">
            <i class="iconfont icon-wangluo"></i>
          </div>
          <div class="option-content">
            <h4>从网络歌单</h4>
            <p>导入网易云音乐、QQ音乐等平台歌单</p>
            <span class="coming-soon">实验性功能</span>
          </div>
          <div class="option-arrow">
            <i class="iconfont icon-youjiantou"></i>
          </div>
        </div>
      </div>
    </t-dialog>

    <!-- 网络歌单导入对话框 -->
    <t-dialog
      v-model:visible="showNetworkImportDialog"
      :cancel-btn="{ content: '取消', variant: 'outline' }"
      :confirm-btn="{ content: '开始导入', theme: 'primary' }"
      :style="{ maxHeight: '80vh' }"
      header="导入网络歌单"
      placement="center"
      width="600px"
      @cancel="cancelNetworkImport"
      @confirm="confirmNetworkImport"
    >
      <div class="network-import-content">
        <div class="platform-selector">
          <label class="form-label">选择导入平台</label>
          <t-radio-group v-model="importPlatformType" variant="primary-filled">
            <t-radio-button value="wy">网易云音乐</t-radio-button>
            <t-radio-button value="tx">QQ音乐</t-radio-button>
            <t-radio-button value="kw">酷我音乐</t-radio-button>
            <t-radio-button value="bd">波点音乐</t-radio-button>
            <t-radio-button value="kg">酷狗音乐</t-radio-button>
            <t-radio-button value="mg">咪咕音乐</t-radio-button>
          </t-radio-group>
        </div>

        <div class="import-content-wrapper">
          <div :key="importPlatformType" class="import-content">
            <div style="margin-bottom: 1em">
              请输入{{ platformNames[importPlatformType] || '音乐平台' }}歌单链接或歌单ID，系统将自动识别格式并导入歌单中的所有歌曲到本地歌单。
            </div>
            <t-input
              v-model="networkPlaylistUrl"
              :placeholder="
                importPlatformType === 'wy' ? 'https://music.163.com/playlist?id=123456789 或 123456789'
                : importPlatformType === 'tx' ? 'https://y.qq.com/n/ryqq/playlist/123456789 或 123456789'
                : importPlatformType === 'kw' ? 'http://www.kuwo.cn/playlist_detail/123456789 或 123456789'
                : importPlatformType === 'bd' ? 'https://h5app.kuwo.cn/m/bodian/collection.html?playlistId=123456789 或 123456789'
                : importPlatformType === 'kg' ? 'https://www.kugou.com/yy/special/single/123456789 或 123456789'
                : importPlatformType === 'mg' ? 'https://music.migu.cn/v3/music/playlist/123456789 或 123456789'
                : '请输入歌单链接或ID'
              "
              autofocus
              clearable
              @enter="confirmNetworkImport"
            />
            <div class="import-tips">
              <p class="tip-title">{{ platformNames[importPlatformType] || '音乐平台' }}支持的输入格式：</p>
              <ul class="tip-list">
                <li v-if="importPlatformType === 'wy'">完整链接：https://music.163.com/playlist?id=123456789</li>
                <li v-if="importPlatformType === 'wy'">手机链接：https://music.163.com/m/playlist?id=123456789</li>
                <li v-if="importPlatformType === 'wy'">纯数字ID：123456789</li>
                <li v-if="importPlatformType === 'tx'">完整链接：https://y.qq.com/n/ryqq/playlist/123456789</li>
                <li v-if="importPlatformType === 'tx'">手机链接：https://i.y.qq.com/v8/playsquare/playlist.html?id=123456789</li>
                <li v-if="importPlatformType === 'tx'">纯数字ID：123456789</li>
                <li v-if="importPlatformType === 'kw'">完整链接：http://www.kuwo.cn/playlist_detail/123456789</li>
                <li v-if="importPlatformType === 'kw'">参数链接：http://www.kuwo.cn/playlist?pid=123456789</li>
                <li v-if="importPlatformType === 'kw'">纯数字ID：123456789</li>
                <li v-if="importPlatformType === 'bd'">手机链接：https://h5app.kuwo.cn/m/bodian/collection.html?playlistId=123456789</li>
                <li v-if="importPlatformType === 'bd'">纯数字ID：123456789</li>
                <li v-if="importPlatformType === 'kg'">完整链接：https://www.kugou.com/yy/special/single/123456789</li>
                <li v-if="importPlatformType === 'kg'">手机版链接：https://m.kugou.com/songlist/gcid_3z9vj0yqz4bz00b</li>
                <li v-if="importPlatformType === 'kg'">酷狗码：123456789</li>
                <li v-if="importPlatformType === 'mg'">完整链接：https://music.migu.cn/v3/music/playlist/123456789</li>
                <li v-if="importPlatformType === 'mg'">纯数字ID：123456789</li>
              </ul>
              <p class="tip-note">智能识别：系统会自动从输入中提取歌单ID</p>
            </div>
          </div>
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
          <i class="iconfont icon-bofang"></i> 播放歌单
        </div>
        <div class="menu-item" @click="handleMenuAction('view')">
          <ViewListIcon :stroke-width="1.5" style="width:14px;height:14px" /> 查看详情
        </div>
        <div class="menu-item" @click="handleMenuAction('download')">
          <DownloadIcon style="width:14px;height:14px" /> 全部下载
        </div>
        <div class="menu-separator"></div>
        <div class="menu-item" @click="handleMenuAction('edit')">
          <Edit2Icon style="width:14px;height:14px" /> 编辑歌单
        </div>
        <div class="menu-item danger" @click="handleMenuAction('delete')">
          <DeleteIcon style="width:14px;height:14px" /> 删除歌单
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.page {
  width: 100%;
  height: 100%;
  overflow-y: auto;
}

.local-container {
  padding: 0 2rem;
  padding-top: 1rem;
  width: 100%;
  color: var(--td-text-color-primary);
}

/* 页面标题 */
.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 2rem;
}

.page-header h2 {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  color: var(--td-text-color-primary);
  margin: 0;
  font-size: 1.875rem;
  font-weight: 600;
}

.header-actions {
  display: flex;
  gap: 0.75rem;
}

/* 歌单区域 */
.playlists-section {
  margin-bottom: 3rem;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
}

.section-header h3 {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
}

.section-actions {
  display: flex;
  gap: 0.5rem;
}

.loading-state {
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 4rem 2rem;
}

/* 歌单网格 */
.playlists-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 1.5rem;
}

.playlist-card {
  display: flex;
  flex-direction: column;
  background: var(--td-bg-color-container);
  border-radius: 0.75rem;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  transition: transform 0.2s ease, box-shadow 0.2s ease;
}

.playlist-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
}

.playlist-card:hover .cover-overlay {
  opacity: 1;
}

.playlist-cover {
  height: 180px;
  background: var(--td-bg-color-component-hover);
  position: relative;
  cursor: pointer;
  overflow: hidden;
}

.cover-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.cover-overlay {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s ease;
}

.cover-overlay .iconfont {
  font-size: 3rem;
  color: #fff;
}

.playlist-info {
  padding: 1rem;
  flex: 1;
  display: flex;
  flex-direction: column;
}

.playlist-name-row {
  display: flex;
  align-items: flex-start;
  margin-bottom: 0.5rem;
  cursor: pointer;
  gap: 6px;
}

.playlist-name-row:hover .playlist-name-text {
  color: var(--td-brand-color);
}

.playlist-name-text {
  font-weight: 600;
  color: var(--td-text-color-primary);
  font-size: 1rem;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
  word-break: break-all;
  line-height: 1.4;
}

.playlist-tags {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  height: 1.4em;
  margin-top: 1px;
}

.playlist-description {
  flex: 1;
  font-size: 0.78rem;
  color: var(--td-text-color-secondary);
  margin-bottom: 0.5rem;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
}

.playlist-meta {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: var(--td-text-color-placeholder);
}

.source-tag {
  text-transform: uppercase;
  font-weight: 500;
  color: var(--td-brand-color);
}

.playlist-actions {
  display: flex;
  justify-content: flex-end;
  padding: 0 1rem 1rem;
  gap: 0.5rem;
}

/* 空状态 */
.empty-playlists {
  text-align: center;
  padding: 4rem 2rem;
}

.empty-icon {
  margin-bottom: 1.5rem;
}

.empty-icon .iconfont {
  font-size: 4rem;
  color: var(--td-text-color-placeholder);
}

.empty-playlists h4 {
  color: var(--td-text-color-primary);
  margin-bottom: 0.5rem;
  font-size: 1.125rem;
  font-weight: 600;
}

.empty-playlists p {
  color: var(--td-text-color-secondary);
  margin-bottom: 2rem;
}

/* 创建/编辑表单 */
.create-form {
  padding: 1rem 0;
}

.edit-playlist-content .form-item {
  margin-bottom: 1.5rem;
}

.edit-playlist-content .form-item:last-child {
  margin-bottom: 0;
}

.edit-playlist-content .form-label {
  display: block;
  margin-bottom: 0.5rem;
  font-weight: 500;
  color: var(--td-text-color-primary);
  font-size: 14px;
}

/* 导入选择对话框 */
.import-options {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.import-option {
  display: flex;
  align-items: center;
  padding: 1rem;
  border: 1px solid var(--td-border-level-1-color);
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.2s ease;
  background: var(--td-bg-color-container);
}

.import-option:hover {
  border-color: var(--td-brand-color-4);
  background: var(--td-bg-color-component-hover);
}

.import-option .option-icon {
  margin-right: 1rem;
}

.import-option .option-icon .iconfont {
  font-size: 1.5rem;
  color: var(--td-brand-color-4);
}

.import-option .option-content {
  flex: 1;
}

.import-option .option-content h4 {
  font-size: 1rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
  margin-bottom: 0.25rem;
}

.import-option .option-content p {
  font-size: 0.875rem;
  color: var(--td-text-color-secondary);
  margin: 0;
}

.import-option .option-content .coming-soon {
  display: inline-block;
  background: var(--td-warning-color-light);
  color: var(--td-warning-color);
  padding: 0.125rem 0.5rem;
  border-radius: 0.25rem;
  font-size: 0.75rem;
  font-weight: 500;
  margin-top: 0.5rem;
}

.import-option .option-arrow .iconfont {
  font-size: 1rem;
  color: var(--td-text-color-placeholder);
}

/* 网络歌单导入 */
.network-import-content {
  max-height: 60vh;
  overflow-y: auto;
  padding: 0 10px;
}

.platform-selector {
  margin-bottom: 1.5rem;
  border-bottom: 1px solid var(--td-border-level-1-color);
  padding-bottom: 1rem;
}

.platform-selector .form-label {
  display: block;
  margin-bottom: 1rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
  font-size: 15px;
}

.network-import-content :deep(.t-radio-group) {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
}

.import-content {
  margin-top: 1rem;
}

.import-tips {
  background: var(--td-bg-color-component-hover);
  border-radius: 12px;
  padding: 1.25rem;
  border: 1px solid var(--td-border-level-1-color);
  margin-top: 1.5rem;
  border-left: 4px solid var(--td-brand-color-4);
}

.tip-title {
  margin: 0 0 0.75rem;
  font-weight: 600;
  color: var(--td-text-color-primary);
  font-size: 15px;
}

.tip-list {
  margin: 0 0 0.75rem;
  padding-left: 1.5rem;
}

.tip-list li {
  color: var(--td-text-color-secondary);
  font-size: 13px;
  margin-bottom: 0.5rem;
  font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
  padding: 0.25rem 0.5rem;
  background: var(--td-bg-color-container);
  border-radius: 4px;
}

.tip-note {
  margin: 0;
  color: var(--td-text-color-placeholder);
  font-size: 12px;
  font-style: italic;
}
</style>

<style>
/* 右键菜单 (unscoped, teleported to body) */
.context-menu {
  position: fixed;
  z-index: 9999;
  min-width: 156px;
  background: var(--td-bg-color-container);
  border-radius: 10px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.15), 0 0 0 1px var(--td-border-level-1-color);
  padding: 4px;
  animation: menuIn 0.15s ease;
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
  transition: background 0.12s;
}

.context-menu .menu-item:hover {
  background: var(--td-bg-color-component-hover);
}

.context-menu .menu-item.danger {
  color: var(--td-error-color);
}

.context-menu .menu-item.danger:hover {
  background: var(--td-error-color-light);
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

import { ref, toRaw } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { playSetting } from '@/store/playSetting'
import { PlayMode, type SongList } from '@/types/audio'
import { MessagePlugin } from 'tdesign-vue-next'
import { calculateBestQuality } from '@/utils/quality'
import PluginRunner from '@/utils/plugin/PluginRunner'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export const playMode = ref<PlayMode>(PlayMode.SEQUENCE)
export const isLoadingSong = ref(false)

let _playIndex = -1
let currentPlayRequestId = 0

// ===== 无缝换曲预加载状态 =====
let prefetchRequestId = 0
let preloadedSong: SongList | null = null
let preloadedReady = false
let unlistenPreload: UnlistenFn | null = null

const qualityMap: Record<string, string> = {
  '128k': '128kbps', '320k': '320kbps', flac: 'FLAC 无损',
  flac24bit: '24bit FLAC', hires: 'Hi-Res 高解析度', atmos: '杜比全景声',
  master: '母带音质'
}

/**
 * 通过音乐插件解析歌曲真实播放 URL
 */
export async function getSongRealUrl(song: SongList): Promise<string> {
  try {
    if (song.source === 'local') {
      const id = song.songmid
      const url = await (window as any).api.localMusic.getUrlById(id)
      if (typeof url === 'object' && url?.error) throw new Error(url.error)
      if (typeof url === 'string') return url
      throw new Error('本地歌曲URL获取失败')
    }
    if (song.url && typeof song.url === 'string') {
      return song.url
    }
    const localUserStore = LocalUserDetailStore()
    let quality =
      (localUserStore.userInfo.sourceQualityMap || {})[song.source || ''] ||
      (localUserStore.userSource.quality as string)
    quality = calculateBestQuality(song.types, quality) || '128k'

    console.log(`使用音质: ${quality} - ${qualityMap[quality] || quality}`)

    const pluginId = localUserStore.userSource.pluginId
    if (!pluginId) {
      throw new Error('未选择音源插件，请先在设置中选择插件')
    }

    let rawUrl: string | null = null

    try {
      rawUrl = await PluginRunner.getMusicUrl(
        pluginId, song.source || 'kw', toRaw(song) as any, quality
      )
    } catch (e: any) {
      throw new Error(`插件解析失败: ${e?.message || e}`)
    }

    if (!rawUrl || typeof rawUrl !== 'string') {
      throw new Error('无法获取播放链接')
    }

    return rawUrl
  } catch (error: any) {
    console.error('获取歌曲URL失败:', error)
    throw new Error('获取歌曲播放链接失败: ' + (error.message || ''))
  }
}

// ===== 自动换源 =====

interface FindMusicCandidate {
  songmid: string | number
  name: string
  singer: string
  albumName?: string
  source?: string
  interval?: string
  types?: any
  [key: string]: any
}

/**
 * 通过 Rust 后端跨源搜索匹配候选歌曲，逐个尝试获取 URL
 */
export async function autoSwitchSource(song: SongList): Promise<{ url: string; song: SongList } | null> {
  if (song.source === 'local') return null

  console.warn(`[自动换源] "${song.name} - ${song.singer}" 原源(${song.source})失败，正在跨源搜索...`)

  let candidates: FindMusicCandidate[] = []
  try {
    const result: any = await invoke('service_music_find_music', {
      name: song.name,
      singer: song.singer,
      albumName: song.albumName || '',
      interval: song.interval || '',
      source: song.source || '',
    })
    console.log('[自动换源] find_music 返回:', result)
    if (Array.isArray(result)) {
      candidates = result
    } else if (result?.data && Array.isArray(result.data)) {
      candidates = result.data
    }
  } catch (e) {
    console.warn('[自动换源] 跨源搜索失败:', e)
    return null
  }

  if (candidates.length === 0) {
    console.warn('[自动换源] 未找到其他源的匹配歌曲')
    return null
  }

  console.log(`[自动换源] 找到 ${candidates.length} 个候选，逐个尝试...`)

  for (const candidate of candidates) {
    const newSource = candidate.source || '未知'
    try {
      const candidateSong: SongList = {
        ...candidate,
        source: candidate.source,
      }
      const url = await getSongRealUrl(candidateSong)
      if (url && typeof url === 'string') {
        console.log(`[自动换源] 成功切换到 ${newSource} 源`)
        return { url, song: candidateSong }
      }
    } catch (e) {
      console.log(`[自动换源] ${newSource} 源获取 URL 失败:`, e)
    }
  }

  console.warn('[自动换源] 所有候选源均无法获取播放链接')
  return null
}

export function getPlayIndex(): number {
  return _playIndex
}

export function getCurrentSong(): SongList | null {
  const store = LocalUserDetailStore()
  if (_playIndex >= 0 && _playIndex < store.list.length) {
    return store.list[_playIndex]
  }
  return null
}

// ===== 计算下一首 =====

function computeNextSong(): SongList | null {
  const store = LocalUserDetailStore()
  if (store.list.length === 0) return null

  if (playMode.value === PlayMode.SINGLE) {
    return store.list[_playIndex] || null
  }
  if (playMode.value === PlayMode.RANDOM) {
    return store.list[Math.floor(Math.random() * store.list.length)]
  }
  // SEQUENCE
  const nextIdx = (_playIndex + 1) % store.list.length
  return store.list[nextIdx]
}

// ===== 预加载 =====

function syncSeamlessConfig() {
  const setting = playSetting()
  invoke('player__set_seamless_config', {
    mode: setting.seamlessMode,
    crossfadeDurationMs: setting.crossfadeDuration,
  })
}

async function ensurePreloadListener() {
  if (unlistenPreload) return
  unlistenPreload = await listen('player:preload_ready', () => {
    preloadedReady = true
  })
}

export async function scheduleNextPrefetch() {
  const setting = playSetting()
  if (!setting.isSeamlessTransition) return

  syncSeamlessConfig()
  await ensurePreloadListener()

  preloadedSong = null
  preloadedReady = false

  const nextSong = computeNextSong()
  if (!nextSong || nextSong.source === 'local') return

  const requestId = ++prefetchRequestId

  try {
    const url = await getSongRealUrl(toRaw(nextSong) as any)
    if (requestId !== prefetchRequestId) return

    await invoke('player__preload', { url })
    preloadedSong = nextSong
  } catch (e) {
    console.warn('预加载下一曲失败:', e)
  }
}

export function invalidatePrefetch() {
  prefetchRequestId++
  preloadedSong = null
  preloadedReady = false
  invoke('player__clear_secondary').catch(() => {})
}

// ===== 播放控制 =====

/**
 * 播放歌曲：解析 URL → Tauri Rust 原生播放
 */
export async function playSong(song: SongList) {
  // 手动切歌时清除预加载
  invalidatePrefetch()

  const requestId = Date.now()
  currentPlayRequestId = requestId

  await new Promise((resolve) => setTimeout(resolve, 200))
  if (currentPlayRequestId !== requestId) return

  const store = LocalUserDetailStore()
  const audio = ControlAudioStore()
  const globalPlayStatus = useGlobalPlayStatusStore()

  try {
    isLoadingSong.value = true

    const idx = store.list.findIndex((s) => s.songmid === song.songmid)
    if (idx === -1) {
      store.addSongToFirst(song)
      _playIndex = 0
    } else {
      _playIndex = idx
    }

    store.userInfo.lastPlaySongId = song.songmid
    globalPlayStatus.player.songInfo = toRaw(song) as any

    // 解析真实播放 URL（失败时自动换源）
    let url: string | undefined
    let actualSong = song
    try {
      url = await getSongRealUrl(toRaw(song) as any)
    } catch (originalError) {
      // 原源失败，尝试自动换源
      console.warn(`[playSong] 原源(${song.source})播放失败，触发自动换源`)
      const switched = await autoSwitchSource(toRaw(song) as any)
      if (switched) {
        url = switched.url
        actualSong = switched.song
        MessagePlugin.success(`已自动切换到 ${switched.song.source || '其他'} 源播放`)
      }
    }
    if (currentPlayRequestId !== requestId) return

    if (!url || typeof url !== 'string') {
      MessagePlugin.warning('无法获取播放链接')
      isLoadingSong.value = false
      return
    }

    // 调用 Rust 原生播放器
    const result: any = await invoke('player__play', { url })
    if (currentPlayRequestId !== requestId) return

    if (result && !result.success) {
      throw new Error(result.error || '播放器启动失败')
    }

    // 更新 macOS 系统媒体控制
    try {
      await invoke('player__update_now_playing', {
        title: song.name || '未知歌曲',
        artist: song.singer || '未知艺术家',
        album: song.albumName || '',
        duration: 0,
        coverUrl: song.img || null
      })
    } catch {}

    // 设置音量（恢复上次音量）
    await invoke('player__set_volume', { volume: audio.Audio.volume })

    isLoadingSong.value = false

    // 播放成功后调度预加载
    scheduleNextPrefetch()
  } catch (error: any) {
    if (currentPlayRequestId !== requestId) return
    console.error('播放失败:', error)
    isLoadingSong.value = false
    MessagePlugin.error(error.message || '播放失败')
  }
}

export function playNext(): SongList | null {
  const store = LocalUserDetailStore()
  if (store.list.length === 0) return null

  // 无缝换曲模式：尝试使用预加载的下一曲
  const setting = playSetting()
  if (setting.isSeamlessTransition && preloadedSong && preloadedReady) {
    seamlessNext()
    return preloadedSong
  }

  if (playMode.value === PlayMode.SINGLE) {
    const song = store.list[_playIndex]
    if (song) { playSong(song); return song }
  }
  if (playMode.value === PlayMode.RANDOM) {
    _playIndex = Math.floor(Math.random() * store.list.length)
  } else {
    _playIndex = (_playIndex + 1) % store.list.length
  }
  const song = store.list[_playIndex]
  if (song) {
    playSong(song)
  }
  return song
}

/**
 * 无缝换曲：尝试使用预加载的 secondary slot 切换
 * @returns true 表示无缝切换成功，false 表示需要回退到普通 playNext
 */
export async function seamlessNext(): Promise<boolean> {
  if (!preloadedSong || !preloadedReady) return false

  const setting = playSetting()

  if (setting.seamlessMode === 'crossfade') {
    // crossfade 模式：Rust poll 已在末尾自动触发渐变，此处只需 swap slot
    try {
      await invoke('player__swap_slot')
    } catch {
      return false
    }
  } else {
    // gapless 模式：即时切换
    try {
      const result: any = await invoke('player__gapless_swap')
      if (result && !result.success) return false
      if (result?.data === false) return false
    } catch {
      return false
    }
  }

  // 更新播放状态
  const store = LocalUserDetailStore()
  const globalPlayStatus = useGlobalPlayStatusStore()
  const audio = ControlAudioStore()
  const song = preloadedSong

  const idx = store.list.findIndex((s) => s.songmid === song.songmid)
  if (idx !== -1) {
    _playIndex = idx
  }
  store.userInfo.lastPlaySongId = song.songmid
  globalPlayStatus.player.songInfo = toRaw(song) as any

  // 更新 macOS 系统媒体控制
  try {
    await invoke('player__update_now_playing', {
      title: song.name || '未知歌曲',
      artist: song.singer || '未知艺术家',
      album: song.albumName || '',
      duration: 0,
      coverUrl: song.img || null
    })
  } catch {}

  await invoke('player__set_volume', { volume: audio.Audio.volume })

  // 重置预加载状态并调度下一曲预加载
  preloadedSong = null
  preloadedReady = false
  scheduleNextPrefetch()

  return true
}

export function playPrevious(): SongList | null {
  const store = LocalUserDetailStore()
  if (store.list.length === 0) return null
  if (playMode.value === PlayMode.RANDOM) {
    _playIndex = Math.floor(Math.random() * store.list.length)
  } else {
    _playIndex = (_playIndex - 1 + store.list.length) % store.list.length
  }
  const song = store.list[_playIndex]
  if (song) {
    playSong(song)
  }
  return song
}

export function updatePlayMode() {
  const modes = [PlayMode.SEQUENCE, PlayMode.RANDOM, PlayMode.SINGLE]
  const idx = modes.indexOf(playMode.value)
  playMode.value = modes[(idx + 1) % modes.length]
  // 播放模式变更后重新调度预加载
  invalidatePrefetch()
  scheduleNextPrefetch()
}

export function togglePlayPause() {
  const audio = ControlAudioStore()
  if (audio.Audio.isPlay) {
    invoke('player__pause')
    audio.Audio.isPlay = false
  } else {
    // 没有已加载的音频（重启后），重新播放上一首或列表第一首
    if (!audio.Audio.url) {
      const store = LocalUserDetailStore()
      if (store.list.length > 0) {
        const lastId = store.userInfo.lastPlaySongId
        const idx = lastId ? store.list.findIndex((s: SongList) => s.songmid === lastId) : -1
        const song = idx >= 0 ? store.list[idx] : store.list[0]
        playSong(song)
        return
      }
      return
    }
    invoke('player__resume')
    audio.Audio.isPlay = true
  }
}

export function setVolume(vol: number) {
  const audio = ControlAudioStore()
  audio.setVolume(vol)
  const store = LocalUserDetailStore()
  store.userInfo.volume = vol
}

export function seekTo(time: number) {
  const audio = ControlAudioStore()
  audio.setCurrentTime(time)
  invoke('player__seek', { position: time })
}

// Legacy object export for backward compatibility
export const globalPlaylist = {
  get playIndex() { return _playIndex },
  get playMode() { return playMode.value },
  getCurrentSong,
  playSong,
  playNext,
  playPrev: playPrevious,
  setPlayMode(mode: PlayMode) { playMode.value = mode },
  cyclePlayMode(): PlayMode {
    updatePlayMode()
    return playMode.value
  }
}

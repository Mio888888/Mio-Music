import { ref, toRaw } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { PlayMode, type SongList } from '@/types/audio'
import { MessagePlugin } from 'tdesign-vue-next'
import { calculateBestQuality } from '@/utils/quality'
import PluginRunner from '@/utils/plugin/PluginRunner'

export const playMode = ref<PlayMode>(PlayMode.SEQUENCE)
export const isLoadingSong = ref(false)

let _playIndex = -1
let currentPlayRequestId = 0

const qualityMap: Record<string, string> = {
  '128k': '标准', '192k': '高品', '320k': '超高', flac: '无损',
  flac24bit: '超高解析', hires: '高清臻音', atmos: '全景环绕',
  atmos_plus: '全景增强', master: '超清母带'
}

/**
 * 通过音乐插件解析歌曲真实播放 URL
 */
export async function getSongRealUrl(song: SongList): Promise<string> {
  try {
    // 本地歌曲
    if (song.source === 'local') {
      const id = song.songmid
      const url = await (window as any).api.localMusic.getUrlById(id)
      if (typeof url === 'object' && url?.error) throw new Error(url.error)
      if (typeof url === 'string') return url
      throw new Error('本地歌曲URL获取失败')
    }
    // 已有直链（如 navidrome 等服务插件歌曲）
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

    // data: URI 或本地路径直接返回
    if (rawUrl.startsWith('data:') || rawUrl.startsWith('file:')) {
      return rawUrl
    }

    // 通过 Rust 后端代理获取音频数据，避免 WebView CORS 302 重定向问题
    try {
      const dataUri = await (window as any).api.audioProxy(rawUrl)
      if (dataUri && typeof dataUri === 'string' && dataUri.startsWith('data:')) {
        const sizeKB = Math.round(dataUri.length / 1024)
        console.log(`音频已通过 Rust 代理获取 (data URI, 大小: ${sizeKB} KB, 前缀: ${dataUri.substring(0, 40)}...)`)
        return dataUri
      }
      console.warn('audioProxy 返回非 data URI:', typeof dataUri, String(dataUri).substring(0, 100))
    } catch (proxyErr: any) {
      console.warn('音频代理失败，尝试直链播放:', proxyErr?.message || proxyErr)
    }

    // 代理失败时回退到原始 URL
    return rawUrl
  } catch (error: any) {
    console.error('获取歌曲URL失败:', error)
    throw new Error('获取歌曲播放链接失败: ' + (error.message || ''))
  }
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

/**
 * 播放歌曲：解析 URL → 设置音频源 → 自动播放
 */
export async function playSong(song: SongList) {
  const requestId = Date.now()
  currentPlayRequestId = requestId

  // 防抖：连续快速点击时取消之前的请求
  await new Promise((resolve) => setTimeout(resolve, 200))
  if (currentPlayRequestId !== requestId) return

  const store = LocalUserDetailStore()
  const audio = ControlAudioStore()
  const globalPlayStatus = useGlobalPlayStatusStore()

  try {
    isLoadingSong.value = true

    // 更新播放列表索引
    const idx = store.list.findIndex((s) => s.songmid === song.songmid)
    if (idx === -1) {
      store.addSongToFirst(song)
      _playIndex = 0
    } else {
      _playIndex = idx
    }

    // 更新当前播放歌曲信息
    store.userInfo.lastPlaySongId = song.songmid
    globalPlayStatus.player.songInfo = toRaw(song) as any

    // 暂停当前播放
    if (audio.Audio.isPlay && audio.Audio.audio) {
      audio.Audio.isPlay = false
      audio.Audio.audio.pause()
    }

    // 解析真实播放 URL
    const url = await getSongRealUrl(toRaw(song) as any)
    if (currentPlayRequestId !== requestId) return

    if (!url || typeof url !== 'string') {
      MessagePlugin.warning('无法获取播放链接')
      isLoadingSong.value = false
      return
    }

    // 设置音频 URL 并播放
    audio.setUrl(url)
    await audio.start()
    if (currentPlayRequestId !== requestId) return

    isLoadingSong.value = false
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
}

export function togglePlayPause() {
  const audio = ControlAudioStore()
  if (audio.Audio.isPlay) {
    audio.stop()
  } else {
    audio.start().catch((error) => {
      console.warn('播放失败:', error.message || error)
    })
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
  if (audio.Audio.audio) {
    audio.Audio.audio.currentTime = time
  }
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

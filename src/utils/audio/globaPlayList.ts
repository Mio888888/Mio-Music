import { ref, toRaw } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { PlayMode, type SongList } from '@/types/audio'
import { MessagePlugin } from 'tdesign-vue-next'
import { calculateBestQuality } from '@/utils/quality'
import PluginRunner from '@/utils/plugin/PluginRunner'
import { invoke } from '@tauri-apps/api/core'

export const playMode = ref<PlayMode>(PlayMode.SEQUENCE)
export const isLoadingSong = ref(false)

let _playIndex = -1
let currentPlayRequestId = 0

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
 * 播放歌曲：解析 URL → Tauri Rust 原生播放
 */
export async function playSong(song: SongList) {
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

    // 解析真实播放 URL
    const url = await getSongRealUrl(toRaw(song) as any)
    if (currentPlayRequestId !== requestId) return

    if (!url || typeof url !== 'string') {
      MessagePlugin.warning('无法获取播放链接')
      isLoadingSong.value = false
      return
    }

    // 调用 Rust 原生播放器
    const result = await invoke('player__play', { url })
    if (currentPlayRequestId !== requestId) return

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
    invoke('player__pause')
    audio.Audio.isPlay = false
  } else {
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

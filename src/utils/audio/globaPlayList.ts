import { ref } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { PlayMode } from '@/types/audio'
import type { SongList } from '@/types/audio'

export const playMode = ref<PlayMode>(PlayMode.SEQUENCE)
export const isLoadingSong = ref(false)

let _playIndex = -1

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

export function playSong(song: SongList) {
  const store = LocalUserDetailStore()
  const idx = store.list.findIndex((s) => s.songmid === song.songmid)
  if (idx === -1) {
    store.addSongToFirst(song)
    _playIndex = 0
  } else {
    _playIndex = idx
  }
  const audio = ControlAudioStore()
  if (song.url) {
    isLoadingSong.value = true
    audio.setUrl(song.url)
  }
  store.userInfo.lastPlaySongId = song.songmid
}

export function playNext(): SongList | null {
  const store = LocalUserDetailStore()
  if (store.list.length === 0) return null
  if (playMode.value === PlayMode.SINGLE) {
    const song = store.list[_playIndex]
    if (song) return song
  }
  if (playMode.value === PlayMode.RANDOM) {
    _playIndex = Math.floor(Math.random() * store.list.length)
  } else {
    _playIndex = (_playIndex + 1) % store.list.length
  }
  const song = store.list[_playIndex]
  if (song) {
    store.userInfo.lastPlaySongId = song.songmid
    const audio = ControlAudioStore()
    if (song.url) audio.setUrl(song.url)
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
    store.userInfo.lastPlaySongId = song.songmid
    const audio = ControlAudioStore()
    if (song.url) audio.setUrl(song.url)
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
    audio.start()
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

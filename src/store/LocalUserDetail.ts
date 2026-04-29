import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { ControlAudioStore } from './ControlAudio'
import type { SongList } from '../types/audio'
import type { UserInfo } from '../types/userInfo'

export const LocalUserDetailStore = defineStore('Local', () => {
  const list = ref<SongList[]>([])
  const userInfo = ref<UserInfo>({})
  const initialization = ref(false)
  const isWatchStarted = ref(false)

  function init(): void {
    const UserInfoLocal = localStorage.getItem('userInfo')
    const ListLocal = localStorage.getItem('songList')
    if (UserInfoLocal) {
      userInfo.value = JSON.parse(UserInfoLocal) as UserInfo
      if (!userInfo.value.sourceQualityMap) userInfo.value.sourceQualityMap = {}
    } else {
      userInfo.value = {
        lastPlaySongId: null, topBarStyle: false, mainColor: '#00DAC0',
        volume: 80, currentTime: 0, selectSources: 'wy', sourceQualityMap: {}, hasGuide: false
      }
      localStorage.setItem('userInfo', JSON.stringify(userInfo.value))
    }
    if (ListLocal) {
      list.value = JSON.parse(ListLocal) as SongList[]
    } else {
      list.value = []
      localStorage.setItem('songList', JSON.stringify([]))
    }
    initialization.value = true
    const Audio = ControlAudioStore()
    startWatch()
    Audio.setVolume(userInfo.value.volume as number)
  }

  function startWatch() {
    if (isWatchStarted.value) return
    isWatchStarted.value = true
    watch(list, (newVal) => { localStorage.setItem('songList', JSON.stringify(newVal)) }, { deep: true })
    watch(userInfo, (newVal) => { localStorage.setItem('userInfo', JSON.stringify(newVal)) }, { deep: true })
  }

  function addSong(song: SongList) {
    if (!list.value.find((item) => item.songmid === song.songmid)) list.value.push(song)
    return list.value
  }

  function addSongToFirst(song: SongList) {
    const existingIndex = list.value.findIndex((item) => item.songmid === song.songmid)
    if (existingIndex !== -1) {
      const existingSong = list.value.splice(existingIndex, 1)[0]
      list.value.unshift(existingSong)
    } else {
      list.value.unshift(song)
    }
    return list.value
  }

  function removeSong(songId: number | string) {
    const index = list.value.findIndex((item) => item.songmid === songId)
    if (index !== -1) {
      const newList = [...list.value]
      newList.splice(index, 1)
      list.value = newList
    }
  }

  function clearList() { list.value = [] }

  function replaceSongList(songs: SongList[]) {
    const seen = new Set<string | number>()
    const deduped: SongList[] = []
    for (const s of songs) {
      const mid = (s as any).songmid
      if (!seen.has(mid)) { seen.add(mid); deduped.push(s) }
    }
    list.value = deduped
    return list.value
  }

  const userSource = computed(() => ({
    pluginId: userInfo.value.pluginId,
    source: userInfo.value.selectSources,
    quality: (userInfo.value.sourceQualityMap || {})[userInfo.value.selectSources as string] || userInfo.value.selectQuality
  }))

  return { list, userInfo, initialization, init, addSong, addSongToFirst, removeSong, clearList, replaceSongList, userSource }
}, { persist: false })

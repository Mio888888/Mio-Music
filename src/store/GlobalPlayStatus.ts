import { defineStore } from 'pinia'
import type { SongList } from '@/types/audio'
import { LocalUserDetailStore } from './LocalUserDetail'
import { playSetting } from './playSetting'
import { reactive, watch, toRaw } from 'vue'
import { analyzeImageColors, type Color } from '@/utils/color/colorExtractor'
import { parseLrc, type LyricLine } from '@/types/lyric'
import { convertLrcFormat } from '@/utils/lyrics/lrcParser'

export interface Comment {
  id: number | string
  text: string
  time: number
  timeStr: string
  location: string
  userName: string
  avatar: string
  userId: number | string
  likedCount: number
  images: string[]
  reply: Comment[]
}

interface CommentsState {
  hotList: Comment[]
  latestList: Comment[]
  hotTotal: number
  hotPage: number
  hotMaxPage: number
  latestTotal: number
  latestPage: number
  latestMaxPage: number
  limit: number
  type: 'hot' | 'latest'
  hotIsLoading: boolean
  latestIsLoading: boolean
}

interface PlayerState {
  songId?: string
  songInfo?: Omit<SongList, 'songmid'> & { songmid: null | number | string }
  cover?: string
  songName: string
  singer: string
  coverDetail: {
    ColorObject?: Color; mainColor?: string; lightMainColor?: string
    contrastColor?: string; textColor?: string; hoverColor?: string
    playBg?: string; playBgHover?: string; useBlackText?: boolean
  }
  lyrics: { lines: LyricLine[]; trans?: string; source?: string }
  isLoading: boolean
  comments: CommentsState
}

const DEFAULT_SONG_INFO = {
  songmid: null, hash: '', name: '欢迎使用澜音 Music', singer: '可以配置音源插件来播放你的歌曲',
  albumName: '', albumId: '0', source: '', interval: '00:00', img: '', lrc: null, types: [], _types: {}, typeUrl: {}
}

export const useGlobalPlayStatusStore = defineStore('globalPlayStatus', () => {
  const localUserStore = LocalUserDetailStore()
  const playSettingStore = playSetting()
  const player = reactive<PlayerState>({
    songId: void 0,
    songInfo: DEFAULT_SONG_INFO,
    cover: void 0,
    songName: '',
    singer: '',
    coverDetail: {
      ColorObject: void 0, mainColor: 'var(--td-brand-color-5)', lightMainColor: 'rgba(255,255,255,0.9)',
      contrastColor: 'var(--player-text-idle)', textColor: 'var(--player-text-idle)',
      hoverColor: 'var(--player-text-hover-idle)', playBg: 'var(--player-btn-bg-idle)',
      playBgHover: 'var(--player-btn-bg-hover-idle)', useBlackText: false
    },
    lyrics: { lines: [] },
    isLoading: false,
    comments: { hotList: [], latestList: [], hotTotal: 0, hotPage: 0, hotMaxPage: 0, latestTotal: 0, latestPage: 0, latestMaxPage: 0, limit: 20, type: 'hot', hotIsLoading: false, latestIsLoading: false }
  })

  watch(() => localUserStore.userInfo.lastPlaySongId, (newId) => {
    if (newId && newId !== player.songId) {
      player.songId = String(newId)
      const song = localUserStore.list.find((s: any) => s.songmid === newId)
      if (song) updatePlayerInfo(song)
    }
  }, { immediate: true })

  let currentBlobUrl: string | null = null

  watch(() => player.songInfo?.img, async (newImg) => {
    if (currentBlobUrl) { URL.revokeObjectURL(currentBlobUrl); currentBlobUrl = null }
    const info: any = player.songInfo
    if (!newImg && info?.source === 'local' && info?.hasCover && info?.songmid) {
      try {
        const data = await (window as any).api?.localMusic?.getCoverBase64?.(String(info.songmid))
        if (data && player.songInfo && String(player.songInfo.songmid) === String(info.songmid) && !player.songInfo.img) player.songInfo.img = data
      } catch {}
    }
    const coverUrl = newImg || '/default-cover.png'
    if (coverUrl.startsWith('http')) {
      try {
        const resp = await fetch(coverUrl); const blob = await resp.blob(); currentBlobUrl = URL.createObjectURL(blob); player.cover = currentBlobUrl
      } catch { player.cover = coverUrl }
    } else { player.cover = coverUrl }
  }, { immediate: true })

  watch(() => player.cover, async (newCover) => {
    if (!newCover) return
    try {
      const { dominantColor, useBlackText } = await analyzeImageColors(newCover)
      player.coverDetail.ColorObject = dominantColor
      player.coverDetail.mainColor = `rgba(${dominantColor.r},${dominantColor.g},${dominantColor.b},1)`
      const base = useBlackText ? '0,0,0' : '255,255,255'
      player.coverDetail.textColor = `rgba(${base},0.6)`; player.coverDetail.hoverColor = `rgba(${base},1)`
      player.coverDetail.contrastColor = player.coverDetail.textColor
      player.coverDetail.playBg = 'rgba(255,255,255,0.2)'; player.coverDetail.playBgHover = 'rgba(255,255,255,0.33)'
      let r = dominantColor.r, g = dominantColor.g, b = dominantColor.b
      r = Math.min(255, r + (255 - r) * 0.8); g = Math.min(255, g + (255 - g) * 0.8); b = Math.min(255, b + (255 - b) * 0.8)
      player.coverDetail.lightMainColor = `rgba(${Math.round(r)},${Math.round(g)},${Math.round(b)},0.9)`
      player.coverDetail.useBlackText = useBlackText
    } catch { resetColors() }
  })

  function resetColors() {
    player.coverDetail.mainColor = 'var(--td-brand-color-5)'; player.coverDetail.lightMainColor = 'rgba(255,255,255,0.9)'
    player.coverDetail.contrastColor = 'var(--player-text-idle)'; player.coverDetail.textColor = 'var(--player-text-idle)'
    player.coverDetail.hoverColor = 'var(--player-text-hover-idle)'; player.coverDetail.playBg = 'var(--player-btn-bg-idle)'
    player.coverDetail.playBgHover = 'var(--player-btn-bg-hover-idle)'
  }

  watch([() => player.songId, () => player.songInfo?.songmid], async ([newId], _old, onCleanup) => {
    if (!newId || !player.songInfo) { player.lyrics.lines = []; return }
    player.isLoading = true
    let active = true; const abort = new AbortController(); onCleanup(() => { active = false; abort.abort() })
    try {
      const source = (player.songInfo as any).source || 'kg'
      let parsedLyrics: any[] = []
      // Try to get lyrics via SDK
      try {
        const lyricData = await (window as any).api?.music?.requestSdk?.('getLyric', {
          source, songInfo: JSON.parse(JSON.stringify(toRaw(player.songInfo))),
          grepLyricInfo: playSettingStore.getIsGrepLyricInfo, useStrictMode: playSettingStore.getStrictGrep
        })
        if (!active) return
        if (lyricData?.lyric) {
          const converted = convertLrcFormat(lyricData.lyric)
          const parsed = parseLrc(converted)
          parsedLyrics = parsed.lines
        }
      } catch {}
      if (active) player.lyrics.lines = parsedLyrics
    } catch { if (active) player.lyrics.lines = [] }
    finally { if (active) player.isLoading = false }
  }, { immediate: true })

  function updatePlayerInfo(songInfo: SongList) {
    if (player.songInfo?.songmid === songInfo.songmid) return
    player.songInfo = songInfo
    player.songName = songInfo.name || ''
    player.singer = songInfo.singer || ''
    updateComments(songInfo)
  }

  async function fetchComments(page = 1, type: 'hot' | 'latest' = 'hot') {
    const currentSongInfo = toRaw(player.songInfo)
    if (!currentSongInfo || !currentSongInfo.songmid) return

    if (type === 'hot') {
      player.comments.hotIsLoading = true
    } else {
      player.comments.latestIsLoading = true
    }
    try {
      const method = type === 'hot' ? 'getHotComment' : 'getComment'
      const res = await (window as any).api?.music?.requestSdk?.(method, {
        source: (currentSongInfo as any).source || 'wy',
        songInfo: JSON.parse(JSON.stringify(currentSongInfo)),
        page,
        limit: player.comments.limit
      })
      if (type === 'hot') {
        if (page === 1) {
          player.comments.hotList = res?.comments || []
        } else {
          player.comments.hotList.push(...(res?.comments || []))
        }
        player.comments.hotTotal = res?.total || 0
        player.comments.hotPage = page
        player.comments.hotMaxPage = res?.maxPage || 1
      } else {
        if (page === 1) {
          player.comments.latestList = res?.comments || []
        } else {
          player.comments.latestList.push(...(res?.comments || []))
        }
        player.comments.latestTotal = res?.total || 0
        player.comments.latestPage = page
        player.comments.latestMaxPage = res?.maxPage || 1
      }
      player.comments.type = type
    } catch (err) {
      console.warn('[Comments] fetch failed:', err)
    } finally {
      if (type === 'hot') {
        player.comments.hotIsLoading = false
      } else {
        player.comments.latestIsLoading = false
      }
    }
  }

  function updateComments(songInfo: SongList) {
    const knownSources = ['wy', 'tx', 'mg', 'kg', 'kw', 'bd']
    if ((songInfo as any).source === 'local' || !knownSources.includes((songInfo as any).source)) return
    player.comments.hotList = []
    player.comments.latestList = []
    player.comments.hotPage = 0
    player.comments.hotTotal = 0
    player.comments.hotMaxPage = 0
    player.comments.latestPage = 0
    player.comments.latestTotal = 0
    player.comments.latestMaxPage = 0
    fetchComments(1, 'hot')
    fetchComments(1, 'latest')
  }

  return { player, updatePlayerInfo, fetchComments }
}, { persist: true })

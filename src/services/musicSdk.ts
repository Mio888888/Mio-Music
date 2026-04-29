import { invoke } from '@tauri-apps/api/core'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'

export interface MusicItem {
  songmid: string | number
  singer: string
  name: string
  albumName: string
  albumId: string | number
  source: string
  interval: string
  img: string
  lrc: string | null
  types?: string[]
  _types?: Record<string, any>
  typeUrl?: Record<string, any>
  hash?: string
}

export interface SearchResult {
  list: MusicItem[]
  allPage: number
  limit: number
  total: number
  source: string
}

export interface PlaylistItem {
  id: string | number
  name: string
  img: string
  source: string
  desc?: string
  playCount?: number
  author?: string
}

export interface PlaylistResult {
  list: PlaylistItem[]
  allPage: number
  limit: number
  total: number
  source: string
}

export interface PlaylistDetailResult {
  list: MusicItem[]
  info: any
  allPage: number
  limit: number
  total: number
  source: string
}

function getSource(): string {
  const store = LocalUserDetailStore()
  return store.userSource.source || 'kw'
}

export const musicSdk = {
  async request(method: string, args: Record<string, any> = {}): Promise<any> {
    const source = args.source || getSource()
    const result = await invoke('service_music_sdk_request', {
      method,
      args: { ...args, source }
    })
    return result
  },

  async search(keyword: string, page = 1, limit = 30): Promise<SearchResult> {
    return this.request('search', { keyword, page, limit })
  },

  async tipSearch(keyword: string): Promise<any> {
    const source = getSource()
    return invoke('service_music_tip_search', { source, keyword })
  },

  async getHotSonglist(): Promise<PlaylistResult> {
    return this.request('getHotSonglist')
  },

  async getPlaylistTags(): Promise<any> {
    return this.request('getPlaylistTags')
  },

  async getCategoryPlaylists(sortId?: string, tagId?: string, page = 1, limit = 30): Promise<PlaylistResult> {
    return this.request('getCategoryPlaylists', { sortId, tagId, page, limit })
  },

  async getPlaylistDetail(id: string | number, page = 1): Promise<PlaylistDetailResult> {
    return this.request('getPlaylistDetail', { id, page })
  },

  async getLeaderboards(): Promise<any> {
    return this.request('getLeaderboards')
  },

  async getLeaderboardDetail(id: string | number, page = 1): Promise<PlaylistDetailResult> {
    return this.request('getLeaderboardDetail', { id, page })
  },

  async getMusicUrl(songInfo: MusicItem, quality?: string): Promise<string> {
    const res = await this.request('getMusicUrl', { songInfo, quality })
    return res?.url || ''
  },

  async getPic(songInfo: MusicItem): Promise<string> {
    const res = await this.request('getPic', { songInfo })
    return res?.url || ''
  },

  async getLyric(songInfo: MusicItem): Promise<string> {
    const res = await this.request('getLyric', { songInfo })
    return res?.lrc || ''
  },

  async searchPlaylist(keyword: string, page = 1, limit = 30): Promise<PlaylistResult> {
    return this.request('searchPlaylist', { keyword, page, limit })
  },

  async getComment(songInfo: MusicItem, page = 1, limit = 30): Promise<any> {
    return this.request('getComment', { songInfo, page, limit })
  }
}

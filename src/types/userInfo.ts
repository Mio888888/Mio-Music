export interface MusicSource {
  name: string
  type: string
  qualitys: string[]
}

export interface UserInfo {
  lastPlaySongId?: string | number | null
  topBarStyle?: boolean
  mainColor?: string
  volume?: number
  currentTime?: number
  selectSources?: string
  selectQuality?: string
  pluginId?: string
  pluginName?: string
  supportedSources?: Record<string, MusicSource>
  sourceQualityMap?: Record<string, string>
  hasGuide?: boolean
  deepseekAPIkey?: string
  [key: string]: any
}

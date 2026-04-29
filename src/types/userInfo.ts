export interface UserInfo {
  lastPlaySongId?: string | number | null
  topBarStyle?: boolean
  mainColor?: string
  volume?: number
  currentTime?: number
  selectSources?: string
  selectQuality?: string
  pluginId?: string
  sourceQualityMap?: Record<string, string>
  hasGuide?: boolean
  deepseekAPIkey?: string
  [key: string]: any
}

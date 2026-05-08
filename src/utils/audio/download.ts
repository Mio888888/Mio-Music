import { NotifyPlugin, MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'
import { toRaw, h } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { musicSdk } from '@/services/musicSdk'
import PluginRunner from '@/utils/plugin/PluginRunner'
import {
  getQualityDisplayName,
  buildQualityFormats,
  compareQuality,
  calculateBestQuality,
  filterByPluginQualities
} from '@/utils/quality'
import i18n from '@/locales'

interface MusicItem {
  singer: string
  name: string
  albumName: string
  albumId: number
  source: string
  interval: string
  songmid: number
  img: string
  lrc: null | string
  types: Array<{ type: string; size: string }>
  _types: Record<string, any>
  typeUrl: Record<string, any>
}

function extractLyricText(result: any): string {
  if (!result) return ''
  if (typeof result === 'string') return result
  if (typeof result?.data === 'string') return result.data
  const obj = typeof result === 'object' ? result : typeof result?.data === 'object' ? result.data : null
  if (!obj) return ''
  return obj.lxlyric || obj.crlyric || obj.lyric || obj.lrc || obj.tlyric || obj.rlyric || ''
}

function getCleanSongInfo(songInfo: any) {
  return JSON.parse(JSON.stringify(toRaw(songInfo)))
}

function getLikelyServicePluginId(songInfo: any): string | undefined {
  const keys = ['_servicePluginId', 'servicePluginId']
  for (const key of keys) {
    const val = songInfo?.[key]
    if (typeof val === 'string' && val) return val
  }
  return undefined
}

async function fetchTtmlLyricText(source: string, songId: string | number): Promise<string> {
  const ttmlSource = source === 'wy' ? 'ncm' : source === 'tx' ? 'qq' : ''
  if (!ttmlSource) return ''

  const url = `https://amll-ttml-db.stevexmh.net/${ttmlSource}/${songId}`
  const proxyResponse = await (window as any).api?.httpProxy?.(url, { method: 'GET', timeout: 10000 })
  const statusCode = Number(proxyResponse?.statusCode || 0)
  if (statusCode >= 400) return ''
  const body = proxyResponse?.body
  return typeof body === 'string' && body.length >= 100 ? body : ''
}

async function fetchSdkLyricText(source: string, songInfo: MusicItem): Promise<string> {
  try {
    const result = await (window as any).api?.music?.requestSdk?.('getLyric', {
      source,
      songInfo: getCleanSongInfo(songInfo),
      grepLyricInfo: true,
      useStrictMode: false
    })
    return extractLyricText(result)
  } catch {
    return ''
  }
}

async function fetchServicePluginLyricText(pluginId: string | undefined, songInfo: MusicItem): Promise<string> {
  if (!pluginId) return ''
  try {
    const result = await PluginRunner.getLyric(pluginId, songInfo.source || 'kw', getCleanSongInfo(songInfo))
    return extractLyricText(result)
  } catch {
    return ''
  }
}

async function resolveDownloadLyricText(pluginId: string | undefined, songInfo: MusicItem): Promise<string> {
  const source = songInfo.source || 'kw'
  const songId = songInfo.songmid

  if (source === 'wy' || source === 'tx') {
    const ttml = await fetchTtmlLyricText(source, songId)
    if (ttml) return ttml
    return fetchSdkLyricText(source, songInfo)
  }

  const servicePluginId = getLikelyServicePluginId(songInfo) || pluginId
  const serviceLyric = await fetchServicePluginLyricText(servicePluginId, songInfo)
  if (serviceLyric) return serviceLyric

  return fetchSdkLyricText(source, songInfo)
}


export function createQualityDialog(
  songInfoOrTypes: MusicItem | Array<{ type: string; size?: string }>,
  userQuality: string,
  title: string = i18n.global.t('download.selectQuality'),
  pluginQualities?: string[]
): Promise<string | null> {
  return new Promise((resolve) => {
    let types: Array<{ type: string; size?: string }> = []
    if (Array.isArray(songInfoOrTypes)) {
      types = songInfoOrTypes
    } else {
      types = songInfoOrTypes.types || []
    }

    const availableQualities = filterByPluginQualities(buildQualityFormats(types), pluginQualities)
    const qualityOptions = [...availableQualities]

    qualityOptions.sort((a, b) => compareQuality(a.type, b.type))

    const dialog = DialogPlugin.confirm({
      header: title,
      width: 400,
      placement: 'center',
      body: () =>
        h(
          'div',
          {
            class: 'quality-selector'
          },
          [
            h(
              'div',
              {
                class: 'quality-list',
                style: {
                  maxHeight:
                    'max(calc(calc(70vh - 2 * var(--td-comp-paddingTB-xxl)) - 24px - 32px - 32px),100px)',
                  overflow: 'auto',
                  scrollbarWidth: 'none',
                  msOverflowStyle: 'none'
                }
              },
              qualityOptions.map((quality) => {
                const disabled = false
                return h(
                  'div',
                  {
                    key: quality.type,
                    class: 'quality-item',
                    title: undefined,
                    style: {
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: '12px 16px',
                      margin: '8px 0',
                      border:
                        '1px solid ' +
                        (disabled
                          ? 'var(--td-border-level-2-color)'
                          : 'var(--td-border-level-1-color)'),
                      borderRadius: '6px',
                      cursor: disabled ? 'not-allowed' : 'pointer',
                      transition: 'all 0.2s ease',
                      backgroundColor:
                        quality.type === userQuality
                          ? 'var(--td-brand-color-light)'
                          : 'var(--td-bg-color-container)',
                      opacity: disabled ? 0.55 : 1
                    },
                    onClick: () => {
                      if (disabled) return
                      dialog.destroy()
                      resolve(quality.type)
                    },
                    onMouseenter: (e: MouseEvent) => {
                      if (disabled) return
                      const target = e.target as HTMLElement
                      target.style.backgroundColor = 'var(--td-bg-color-secondarycontainer)'
                      target.style.borderColor = 'var(--td-brand-color)'
                    },
                    onMouseleave: (e: MouseEvent) => {
                      const target = e.target as HTMLElement
                      target.style.backgroundColor =
                        quality.type === userQuality
                          ? 'var(--td-brand-color-light)'
                          : 'var(--td-bg-color-container)'
                      target.style.borderColor = 'var(--td-border-level-1-color)'
                    }
                  },
                  [
                    h('div', { class: 'quality-info' }, [
                      h(
                        'div',
                        {
                          style: {
                            fontWeight: '500',
                            fontSize: '14px',
                            color:
                              quality.type === userQuality
                                ? 'var(--td-brand-color)'
                                : 'var(--td-text-color-primary)'
                          }
                        },
                        getQualityDisplayName(quality.type)
                      ),
                      h(
                        'div',
                        {
                          style: {
                            fontSize: '12px',
                            color: 'var(--td-text-color-secondary)',
                            marginTop: '2px'
                          }
                        },
                        quality.type.toUpperCase()
                      )
                    ]),
                    h(
                      'div',
                      {
                        class: 'quality-size',
                        style: {
                          fontSize: '12px',
                          color: 'var(--td-text-color-secondary)',
                          fontWeight: '500'
                        }
                      },
                      quality.size
                    )
                  ]
                )
              })
            )
          ]
        ),
      confirmBtn: null,
      cancelBtn: null,
      footer: false
    })
  })
}

async function downloadSingleSong(songInfo: MusicItem): Promise<void> {
  try {
    const localUserDetail = LocalUserDetailStore()
    const userQuality =
      (localUserDetail.userInfo.sourceQualityMap || {})[toRaw(songInfo).source as any] ||
      (localUserDetail.userSource.quality as string)

    const pluginQualities = (localUserDetail.userInfo.supportedSources || {})[toRaw(songInfo).source as string]?.qualitys
    const selectedQuality = await createQualityDialog(songInfo, userQuality, i18n.global.t('download.selectQuality'), pluginQualities)
    if (!selectedQuality) return

    let quality = selectedQuality as string
    const calculatedQuality = calculateBestQuality(songInfo.types, quality)
    if (calculatedQuality && calculatedQuality !== quality) {
      quality = calculatedQuality
      MessagePlugin.warning(i18n.global.t('download.qualityUnavailable', { name: getQualityDisplayName(quality) }))
    }

    const tip = MessagePlugin.success(i18n.global.t('download.gettingUrl') + songInfo.name)

    let rawUrl = ''
    if (songInfo.source === 'subsonic') {
      rawUrl = await musicSdk.getMusicUrl(toRaw(songInfo) as any, quality)
    } else {
      const pluginId = localUserDetail.userSource.pluginId
      if (!pluginId) {
        MessagePlugin.error(i18n.global.t('play.noSourcePlugin'))
        ;(await tip).close()
        return
      }
      rawUrl = await PluginRunner.getMusicUrl(
        pluginId,
        songInfo.source || 'kw',
        toRaw(songInfo) as any,
        quality
      )
    }

    ;(await tip).close()

    if (!rawUrl || typeof rawUrl !== 'string') {
      MessagePlugin.error(i18n.global.t('download.getUrlFailed'))
      return
    }

    const settingsStore = useSettingsStore()
    const template = settingsStore.settings.filenameTemplate || '%t - %s'
    const safeName = `${songInfo.name} - ${songInfo.singer}`.replace(/[\/\\:*?"<>|]/g, '_')

    const dirs = await invoke<{ cacheDir: string; downloadDir: string }>('get_directories')
    const downloadDir = dirs?.downloadDir || ''
    const urlExtension = rawUrl.match(/\.(mp3|flac|m4a|aac|ogg|opus|wav)(?:[?#/]|$)/i)?.[1]?.toLowerCase()
    const extension = urlExtension || (['flac', 'flac24bit', 'hires', 'master', 'atmos', 'atmos_plus'].includes(quality) ? 'flac' : 'mp3')
    const filePath = downloadDir ? `${downloadDir}/${safeName}.${extension}` : `${safeName}.${extension}`

    await (window as any).api?.download?.addTask(
      {
        name: songInfo.name,
        singer: songInfo.singer,
        songmid: songInfo.songmid,
        source: songInfo.source,
        img: songInfo.img,
        albumName: songInfo.albumName
      },
      rawUrl,
      filePath,
      null,
      quality,
      1
    )

    // 监听下载完成后写入标签
    const tagOptions = (settingsStore.settings.tagWriteOptions ?? {}) as Record<string, any>
    const needsTags = tagOptions.basicInfo || tagOptions.cover || tagOptions.lyrics || tagOptions.downloadLyrics
    if (needsTags) {
      const unlisten = await listen('download:task-completed', async (event: any) => {
        const task = event.payload
        if (task.file_path !== filePath && task.filePath !== filePath) return
        unlisten()

        try {
          const songInfoForTags = { ...toRaw(songInfo) }

          if (tagOptions.lyrics || tagOptions.downloadLyrics) {
            songInfoForTags.lrc = songInfo.source === 'subsonic'
              ? await musicSdk.getLyric(toRaw(songInfo) as any)
              : await resolveDownloadLyricText(localUserDetail.userSource.pluginId, toRaw(songInfo) as MusicItem)
          }

          await invoke('local_music__write_tags', {
            filePath,
            songInfo: songInfoForTags,
            tagWriteOptions: tagOptions
          })
        } catch (e) {
          console.warn('标签写入失败:', e)
        }
      })
    }

    await NotifyPlugin.success({
      title: i18n.global.t('download.addedToQueue'),
      content: `${songInfo.name} ${i18n.global.t('download.queueViewTip')}`
    })
  } catch (error: any) {
    console.error('下载失败:', error)
    const msg = error?.message || ''
    if (msg.includes('歌曲正在下载中')) {
      await NotifyPlugin.warning({ title: i18n.global.t('common.tip'), content: i18n.global.t('download.duplicateDownload') })
    } else if (msg.includes('歌曲已下载完成')) {
      await NotifyPlugin.info({ title: i18n.global.t('common.tip'), content: i18n.global.t('download.alreadyDownloaded') })
    } else {
      await NotifyPlugin.error({ title: i18n.global.t('download.downloadFailed'), content: msg || i18n.global.t('download.unknownError') })
    }
  }
}

export { downloadSingleSong }

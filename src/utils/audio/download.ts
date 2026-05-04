import { NotifyPlugin, MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'
import { toRaw, h } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import PluginRunner from '@/utils/plugin/PluginRunner'
import {
  getQualityDisplayName,
  buildQualityFormats,
  compareQuality,
  calculateBestQuality,
  filterByPluginQualities
} from '@/utils/quality'

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

export function createQualityDialog(
  songInfoOrTypes: MusicItem | Array<{ type: string; size?: string }>,
  userQuality: string,
  title: string = '选择下载音质(可滚动)',
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
    const selectedQuality = await createQualityDialog(songInfo, userQuality, '选择下载音质(可滚动)', pluginQualities)
    if (!selectedQuality) return

    let quality = selectedQuality as string
    const calculatedQuality = calculateBestQuality(songInfo.types, quality)
    if (calculatedQuality && calculatedQuality !== quality) {
      quality = calculatedQuality
      MessagePlugin.warning(`所选音质不可用，已自动调整为: ${getQualityDisplayName(quality)}`)
    }

    const pluginId = localUserDetail.userSource.pluginId
    if (!pluginId) {
      MessagePlugin.error('未选择音源插件，请先在设置中选择插件')
      return
    }

    const tip = MessagePlugin.success('正在获取下载地址：' + songInfo.name)

    const rawUrl = await PluginRunner.getMusicUrl(
      pluginId,
      songInfo.source || 'kw',
      toRaw(songInfo) as any,
      quality
    )

    ;(await tip).close()

    if (!rawUrl || typeof rawUrl !== 'string') {
      MessagePlugin.error('获取下载地址失败')
      return
    }

    const settingsStore = useSettingsStore()
    const template = settingsStore.settings.filenameTemplate || '%t - %s'
    const safeName = `${songInfo.name} - ${songInfo.singer}`.replace(/[\/\\:*?"<>|]/g, '_')

    const dirs = await invoke<{ cacheDir: string; downloadDir: string }>('get_directories')
    const downloadDir = dirs?.downloadDir || ''
    const filePath = downloadDir ? `${downloadDir}/${safeName}.flac` : `${safeName}.flac`

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
      pluginId?.toString(),
      quality,
      1
    )

    // 监听下载完成后写入标签
    const tagOptions = toRaw(settingsStore.settings.tagWriteOptions || {})
    const needsTags = tagOptions.basicInfo || tagOptions.cover || tagOptions.lyrics || tagOptions.downloadLyrics
    if (needsTags) {
      const unlisten = await listen('download:task-completed', async (event: any) => {
        const task = event.payload
        if (task.file_path !== filePath && task.filePath !== filePath) return
        unlisten()

        try {
          const songInfoForTags = { ...toRaw(songInfo) }

          if (tagOptions.lyrics || tagOptions.downloadLyrics) {
            try {
              const lrcResult = await PluginRunner.getLyric(
                pluginId, songInfo.source || 'kw', toRaw(songInfo) as any
              )
              songInfoForTags.lrc = typeof lrcResult === 'string' ? lrcResult : (lrcResult as any)?.lyric || ''
            } catch {}
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
      title: '已添加到下载队列',
      content: `${songInfo.name} 已添加，可在下载管理中查看进度`
    })
  } catch (error: any) {
    console.error('下载失败:', error)
    const msg = error?.message || ''
    if (msg.includes('歌曲正在下载中')) {
      await NotifyPlugin.warning({ title: '提示', content: '该歌曲正在下载中，请勿重复添加' })
    } else if (msg.includes('歌曲已下载完成')) {
      await NotifyPlugin.info({ title: '提示', content: '该歌曲已下载完成' })
    } else {
      await NotifyPlugin.error({ title: '下载失败', content: msg || '未知错误' })
    }
  }
}

export { downloadSingleSong }

import { NotifyPlugin, MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'
import { toRaw, h } from 'vue'
import {
  getQualityDisplayName,
  buildQualityFormats,
  compareQuality,
  calculateBestQuality
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
  title: string = '选择下载音质(可滚动)'
): Promise<string | null> {
  return new Promise((resolve) => {
    let types: Array<{ type: string; size?: string }> = []
    if (Array.isArray(songInfoOrTypes)) {
      types = songInfoOrTypes
    } else {
      types = songInfoOrTypes.types || []
    }

    const availableQualities = buildQualityFormats(types)
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
    console.log('开始下载', toRaw(songInfo))
    const LocalUserDetail = LocalUserDetailStore()
    const userQuality =
      (LocalUserDetail.userInfo.sourceQualityMap || {})[toRaw(songInfo.source) as any] ||
      (LocalUserDetail.userSource.quality as string)
    const settingsStore = useSettingsStore()

    const selectedQuality = await createQualityDialog(songInfo, userQuality)

    if (!selectedQuality) {
      return
    }

    let quality = selectedQuality as string

    const calculatedQuality = calculateBestQuality(songInfo.types, quality)
    if (calculatedQuality && calculatedQuality !== quality) {
      quality = calculatedQuality
      MessagePlugin.warning(`所选音质不可用，已自动调整为: ${getQualityDisplayName(quality)}`)
    }

    console.log(`使用音质下载: ${quality} - ${getQualityDisplayName(quality)}`)
    const tip = MessagePlugin.success('开始下载歌曲：' + songInfo.name)

    const songInfoWithTemplate = {
      ...toRaw(songInfo),
      template: settingsStore.settings.filenameTemplate || '%t - %s'
    }

    const hasDirectUrl = !!(songInfo as any).url && typeof (songInfo as any).url === 'string'

    const result = await window.api.music.requestSdk('downloadSingleSong', {
      pluginId: LocalUserDetail.userSource.pluginId?.toString() || '',
      source: songInfo.source,
      quality,
      songInfo: hasDirectUrl
        ? { ...songInfoWithTemplate, typeUrl: { [quality]: (songInfo as any).url } }
        : (songInfoWithTemplate as any),
      tagWriteOptions: toRaw(settingsStore.settings.tagWriteOptions),
      isCache: true,
      lazy: hasDirectUrl
    })

    ;(await tip).close()

    const savePath = result.filePath || result.path

    if (!savePath) {
      MessagePlugin.info(result.message || '未知状态')
    } else {
      await NotifyPlugin.success({
        title: '已添加到下载队列',
        content: `歌曲已添加，可在下载管理中查看进度`
      })
    }
  } catch (error: any) {
    console.error('下载失败:', error)
    const msg = error.message || ''
    if (msg.includes('歌曲正在下载中')) {
      await NotifyPlugin.warning({
        title: '提示',
        content: '该歌曲正在下载中，请勿重复添加'
      })
    } else if (msg.includes('歌曲已下载完成')) {
      await NotifyPlugin.info({
        title: '提示',
        content: '该歌曲已下载完成'
      })
    } else {
      await NotifyPlugin.error({
        title: '下载失败',
        content: msg || '未知错误'
      })
    }
  }
}

export { downloadSingleSong }

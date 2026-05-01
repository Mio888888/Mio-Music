import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { transitionVolume } from '@/utils/audio/volume'
import { LocalUserDetailStore } from './LocalUserDetail'
import { playSetting as usePlaySettingStore } from './playSetting'

import type {
  AudioEventCallback,
  AudioEventType,
  AudioSlot,
  AudioSubscriber,
  UnsubscribeFunction,
  ControlAudioState
} from '../types/audio'

let userInfo: any
export const ControlAudioStore = defineStore('controlAudio', () => {
  const Audio = reactive<ControlAudioState>({
    audio: null,
    audioA: null,
    audioB: null,
    primarySlot: 'A',
    srcA: '',
    srcB: '',
    secondaryUrl: '',
    isPlay: false,
    currentTime: 0,
    duration: 0,
    volume: 80,
    url: '',
    eventInit: false
  })

  const subscribers = reactive<Record<AudioEventType, AudioSubscriber[]>>({
    ended: [], seeked: [], timeupdate: [], play: [], pause: [], error: [], canplay: [], slotSwap: []
  })

  const generateId = (): string => `${Date.now()}_${Math.random().toString(36).substr(2, 9)}`

  const subscribe = (eventType: AudioEventType, callback: AudioEventCallback): UnsubscribeFunction => {
    const id = generateId()
    subscribers[eventType].push({ id, callback })
    return () => {
      const index = subscribers[eventType].findIndex((sub) => sub.id === id)
      if (index > -1) subscribers[eventType].splice(index, 1)
    }
  }

  const publish = (eventType: AudioEventType): void => {
    subscribers[eventType].forEach((subscriber) => {
      try { subscriber.callback() } catch (error) { console.error(`音频事件回调执行错误 [${eventType}]:`, error) }
    })
  }

  const clearAllSubscribers = (): void => {
    Object.keys(subscribers).forEach((eventType) => { subscribers[eventType as AudioEventType] = [] })
  }

  const clearEventSubscribers = (eventType: AudioEventType): void => { subscribers[eventType] = [] }

  const init = (elA: HTMLAudioElement | null, elB: HTMLAudioElement | null) => {
    userInfo = LocalUserDetailStore()
    console.log(elA, elB, '全局音频双槽挂载初始化success')
    Audio.audioA = elA
    Audio.audioB = elB
    Audio.audio = Audio.primarySlot === 'A' ? elA : elB
  }

  const getPrimaryEl = (): HTMLAudioElement | null => Audio.primarySlot === 'A' ? Audio.audioA : Audio.audioB
  const getSecondaryEl = (): HTMLAudioElement | null => Audio.primarySlot === 'A' ? Audio.audioB : Audio.audioA

  const swapPrimarySlot = () => {
    Audio.primarySlot = Audio.primarySlot === 'A' ? 'B' : 'A'
    Audio.audio = Audio.primarySlot === 'A' ? Audio.audioA : Audio.audioB
    Audio.url = Audio.primarySlot === 'A' ? Audio.srcA : Audio.srcB
    Audio.secondaryUrl = ''
    publish('slotSwap')
  }

  const setCurrentTime = (time: number) => {
    if (typeof time === 'number') {
      Audio.currentTime = time
      return
    }
    throw new Error('时间必须是数字类型')
  }

  const setDuration = (duration: number) => {
    if (typeof duration === 'number') {
      Audio.duration = duration
      return
    }
    throw new Error('时间必须是数字类型')
  }

  const setVolume = (volume: number, transition: boolean = false) => {
    const syncSecondary = (target: number) => {
      const sec = getSecondaryEl()
      if (sec) { try { sec.volume = Number(target.toFixed(2)) } catch {} }
    }
    if (typeof volume === 'number' && volume >= 0 && volume <= 100) {
      if (Audio.audio) {
        const v = volume / 100
        if (Audio.isPlay && transition) {
          transitionVolume(Audio.audio, v, Audio.volume <= volume)
        } else {
          Audio.audio.volume = Number(v.toFixed(2))
        }
        syncSecondary(v)
        Audio.volume = volume
        userInfo.userInfo.volume = volume
      }
    } else {
      if (typeof volume === 'number' && Audio.audio) {
        if (volume <= 0) {
          Audio.volume = 0
          Audio.audio.volume = 0
          syncSecondary(0)
          userInfo.userInfo.volume = 0
        } else {
          Audio.volume = 100
          Audio.audio.volume = 100
          syncSecondary(1)
          userInfo.userInfo.volume = 100
        }
      } else {
        throw new Error('音量必须是0-100之间的数字')
      }
    }
  }

  const setUrl = (url: string) => {
    if (typeof url !== 'string' || url.trim() === '') {
      throw new Error('音频URL不能为空')
    }
    if (Audio.isPlay) stop()
    const trimmed = url.trim()
    if (Audio.primarySlot === 'A') Audio.srcA = trimmed
    else Audio.srcB = trimmed
    Audio.url = trimmed
    console.log('音频URL已设置(slot', Audio.primarySlot, '):', Audio.url)
  }

  const setSecondaryUrl = (url: string) => {
    if (typeof url !== 'string' || url.trim() === '') {
      throw new Error('次要音频URL不能为空')
    }
    const trimmed = url.trim()
    if (Audio.primarySlot === 'A') Audio.srcB = trimmed
    else Audio.srcA = trimmed
    Audio.secondaryUrl = trimmed
    console.log('次要音频URL已设置(slot', Audio.primarySlot === 'A' ? 'B' : 'A', '):', trimmed)
  }

  const clearSecondarySrc = () => {
    if (Audio.primarySlot === 'A') Audio.srcB = ''
    else Audio.srcA = ''
    Audio.secondaryUrl = ''
  }

  /** 等待 audio 元素就绪（HAVE_CURRENT_DATA 以上） */
  const waitForReady = (el: HTMLAudioElement, timeoutMs = 5000): Promise<void> => {
    if (el.readyState >= 2) return Promise.resolve() // HAVE_CURRENT_DATA
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        el.removeEventListener('canplay', onReady)
        el.removeEventListener('error', onError)
        reject(new Error('音频加载超时'))
      }, timeoutMs)
      const onReady = () => { clearTimeout(timer); el.removeEventListener('canplay', onReady); el.removeEventListener('error', onError); resolve() }
      const onError = () => { clearTimeout(timer); el.removeEventListener('canplay', onReady); el.removeEventListener('error', onError); reject(new Error('音频加载失败')) }
      el.addEventListener('canplay', onReady, { once: true })
      el.addEventListener('error', onError, { once: true })
    })
  }

  const start = async () => {
    const playSetting = usePlaySettingStore()
    const volume = Audio.volume
    if (Audio.audio) {
      // 等待音频数据就绪再播放，避免 NotSupportedError
      try {
        await waitForReady(Audio.audio)
      } catch (e) {
        console.warn('音频未就绪:', (e as Error).message)
        return false
      }
      if (!playSetting.getIsPauseTransition) {
        try {
          Audio.audio.volume = volume / 100
          await Audio.audio.play()
          Audio.isPlay = true
          return Promise.resolve()
        } catch (error) {
          console.error('音频播放失败:', error)
          Audio.isPlay = false
          throw new Error('音频播放失败，请检查音频URL是否有效')
        }
      }

      Audio.audio.volume = 0
      try {
        await Audio.audio.play()
        Audio.isPlay = true
        return transitionVolume(Audio.audio, volume / 100, true, true)
      } catch (error) {
        Audio.audio.volume = volume / 100
        console.error('音频播放失败:', error)
        Audio.isPlay = false
        throw new Error('音频播放失败，请检查音频URL是否有效')
      }
    }
    return false
  }

  const stop = () => {
    const playSetting = usePlaySettingStore()
    if (Audio.audio) {
      Audio.isPlay = false
      if (!playSetting.getIsPauseTransition) {
        Audio.audio.pause()
        return Promise.resolve()
      }
      return transitionVolume(Audio.audio, Audio.volume / 100, false, true).then(() => {
        Audio.audio?.pause()
      })
    }
    return false
  }

  return {
    Audio, init, setCurrentTime, setVolume, setUrl, setSecondaryUrl, clearSecondarySrc,
    getPrimaryEl, getSecondaryEl, swapPrimarySlot, start, stop,
    subscribe, publish, clearAllSubscribers, clearEventSubscribers, setDuration
  }
}, { persist: false })

export type { AudioSlot }

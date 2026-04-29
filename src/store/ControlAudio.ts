import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { LocalUserDetailStore } from './LocalUserDetail'

import type { AudioEventCallback, AudioEventType, AudioSlot, AudioSubscriber, UnsubscribeFunction, ControlAudioState } from '../types/audio'

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

  const setCurrentTime = (time: number) => { if (typeof time === 'number') Audio.currentTime = time }
  const setDuration = (duration: number) => { if (typeof duration === 'number') Audio.duration = duration }

  const setVolume = (volume: number, transition: boolean = false) => {
    const syncSecondary = (target: number) => {
      const sec = getSecondaryEl()
      if (sec) { try { sec.volume = Number(target.toFixed(2)) } catch {} }
    }
    if (typeof volume === 'number' && volume >= 0 && volume <= 100) {
      if (Audio.audio) {
        const v = volume / 100
        Audio.audio.volume = Number(v.toFixed(2))
        syncSecondary(v)
        Audio.volume = volume
        userInfo.userInfo.volume = volume
      }
    }
  }

  const setUrl = (url: string) => {
    if (typeof url !== 'string' || url.trim() === '') return
    if (Audio.isPlay) stop()
    const trimmed = url.trim()
    if (Audio.primarySlot === 'A') Audio.srcA = trimmed
    else Audio.srcB = trimmed
    Audio.url = trimmed
  }

  const setSecondaryUrl = (url: string) => {
    const trimmed = url.trim()
    if (Audio.primarySlot === 'A') Audio.srcB = trimmed
    else Audio.srcA = trimmed
    Audio.secondaryUrl = trimmed
  }

  const clearSecondarySrc = () => {
    if (Audio.primarySlot === 'A') Audio.srcB = ''
    else Audio.srcA = ''
    Audio.secondaryUrl = ''
  }

  const start = async () => {
    if (Audio.audio) {
      try {
        Audio.audio.volume = Audio.volume / 100
        await Audio.audio.play()
        Audio.isPlay = true
        return Promise.resolve()
      } catch (error) {
        Audio.isPlay = false
        throw new Error('音频播放失败')
      }
    }
    return false
  }

  const stop = () => {
    if (Audio.audio) {
      Audio.isPlay = false
      Audio.audio.pause()
      return Promise.resolve()
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

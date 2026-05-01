import type playList from './playList'

export type AudioEventCallback = () => void
export type AudioEventType = 'ended' | 'seeked' | 'timeupdate' | 'play' | 'pause' | 'error' | 'canplay' | 'slotSwap'
export type AudioSlot = 'A' | 'B'

export interface AudioSubscriber {
  id: string
  callback: AudioEventCallback
}

export type UnsubscribeFunction = () => void

export type AudioSubscribeMethod = (
  eventType: AudioEventType,
  callback: AudioEventCallback
) => UnsubscribeFunction

export enum PlayMode {
  SEQUENCE = 'sequence',
  RANDOM = 'random',
  SINGLE = 'single'
}

export type ControlAudioState = {
  primarySlot: AudioSlot
  secondaryUrl: string
  isPlay: boolean
  currentTime: number
  duration: number
  volume: number
  url: string
  eventInit: boolean
}

export type SongList = playList

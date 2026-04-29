import { ControlAudioStore } from '@/store/ControlAudio'
import mediaSessionController from './useSmtc'

let installed = false
let smtcTimer: any = null
let globalKeyDownHandler: ((e: KeyboardEvent) => void) | null = null
let removeMusicCtrlListener: (() => void) | null = null

function dispatch(name: string, val?: any) {
  window.dispatchEvent(new CustomEvent('global-music-control', { detail: { name, val } }))
}

export function installGlobalMusicControls() {
  if (installed) return
  installed = true

  const controlAudio = ControlAudioStore()

  const tryInitSmtc = () => {
    const el = controlAudio.Audio.audio
    if (!el) return
    mediaSessionController.init(el, {
      play: () => dispatch('play'), pause: () => dispatch('pause'),
      playPrevious: () => dispatch('playPrev'), playNext: () => dispatch('playNext')
    })
    mediaSessionController.updatePlaybackState(el.paused ? 'paused' : 'playing')
  }

  tryInitSmtc()
  let smtcTries = 0
  smtcTimer = setInterval(() => {
    if (smtcTries > 20) { clearInterval(smtcTimer); smtcTimer = null; return }
    smtcTries++
    if (controlAudio.Audio.audio) { tryInitSmtc(); clearInterval(smtcTimer); smtcTimer = null }
  }, 150)

  let keyThrottle = false
  const throttle = (cb: () => void, delay: number) => {
    if (keyThrottle) return
    keyThrottle = true
    setTimeout(() => { try { cb() } finally { keyThrottle = false } }, delay)
  }

  globalKeyDownHandler = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement | null
    const tag = (target?.tagName || '').toLowerCase()
    const isEditable = target?.hasAttribute('contenteditable') || ['input','textarea'].includes(tag)
    throttle(() => {
      if (isEditable) return
      if (e.code === 'Space') { e.preventDefault(); dispatch('toggle') }
      else if (e.code === 'ArrowUp') { e.preventDefault(); dispatch('volumeDelta', 5) }
      else if (e.code === 'ArrowDown') { e.preventDefault(); dispatch('volumeDelta', -5) }
      else if (e.code === 'ArrowLeft') dispatch('seekDelta', -5)
      else if (e.code === 'ArrowRight') dispatch('seekDelta', 5)
      else if (e.code === 'KeyF') { e.preventDefault(); dispatch('toggleFullPlay') }
    }, 100)
  }
  document.addEventListener('keydown', globalKeyDownHandler)

  try { removeMusicCtrlListener = (window as any).api?.onMusicCtrl?.(() => dispatch('toggle')) } catch {}
}

export function uninstallGlobalMusicControls() {
  if (!installed) return
  installed = false
  if (smtcTimer) { clearInterval(smtcTimer); smtcTimer = null }
  if (globalKeyDownHandler) { document.removeEventListener('keydown', globalKeyDownHandler); globalKeyDownHandler = null }
  if (removeMusicCtrlListener) { removeMusicCtrlListener(); removeMusicCtrlListener = null }
  mediaSessionController.cleanup()
}

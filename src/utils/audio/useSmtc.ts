interface MediaSessionCallbacks { play: () => void; pause: () => void; playPrevious: () => void; playNext: () => void }
interface TrackMetadata { title: string; artist: string; album: string; artworkUrl: string }

class MediaSessionController {
  private audioElement: HTMLAudioElement | null = null
  private callbacks: MediaSessionCallbacks | null = null
  private eventListeners: Array<{ element: HTMLAudioElement; event: string; handler: EventListener }> = []

  private get isSupported(): boolean { return 'mediaSession' in navigator }

  updateMetadata(metadata: TrackMetadata): void {
    if (!this.isSupported) return
    try {
      navigator.mediaSession.metadata = new MediaMetadata({
        title: metadata.title || '未知歌曲', artist: metadata.artist || '未知艺术家',
        album: metadata.album || '未知专辑',
        artwork: metadata.artworkUrl ? ['96x96','128x128','192x192','256x256','384x384','512x512'].map(s => ({ src: metadata.artworkUrl, sizes: s, type: 'image/png' })) : []
      })
      if (this.audioElement) navigator.mediaSession.playbackState = this.audioElement.paused ? 'paused' : 'playing'
    } catch {}
  }

  init(audioElement: HTMLAudioElement, callbacks: MediaSessionCallbacks): void {
    if (!this.isSupported) return
    this.cleanup()
    this.audioElement = audioElement
    this.callbacks = callbacks
    const actionHandlers: Array<[MediaSessionAction, () => void]> = [
      ['play', callbacks.play], ['pause', callbacks.pause],
      ['previoustrack', callbacks.playPrevious], ['nexttrack', callbacks.playNext]
    ]
    actionHandlers.forEach(([action, handler]) => navigator.mediaSession.setActionHandler(action, handler))
    navigator.mediaSession.setActionHandler('seekto', (details) => {
      if (!this.audioElement || !details.seekTime) return
      try { this.audioElement.currentTime = details.seekTime } catch {}
    })
    try { if (this.audioElement) navigator.mediaSession.playbackState = this.audioElement.paused ? 'paused' : 'playing' } catch {}
  }

  updatePlaybackState(state: MediaSessionPlaybackState): void {
    if (!this.isSupported) return
    try { navigator.mediaSession.playbackState = state } catch {}
  }

  cleanup(): void {
    this.eventListeners.forEach(({ element, event, handler }) => element.removeEventListener(event, handler))
    this.eventListeners = []
    if (this.isSupported) {
      (['play','pause','previoustrack','nexttrack','seekto'] as MediaSessionAction[]).forEach(a => navigator.mediaSession.setActionHandler(a, null))
    }
    this.audioElement = null; this.callbacks = null
  }
}

export default new MediaSessionController()

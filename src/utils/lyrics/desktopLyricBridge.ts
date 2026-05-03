import { storeToRefs } from 'pinia'
import { emit } from '@tauri-apps/api/event'
import { ControlAudioStore } from '@/store/ControlAudio'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { findCurrentLine, getLineText } from '@/types/lyric'

let rafId: number | null = null
let isRunning = false

export function startDesktopLyricSync() {
  if (isRunning) return
  isRunning = true

  const audioStore = ControlAudioStore()
  const playStatus = useGlobalPlayStatusStore()
  const { Audio } = storeToRefs(audioStore)
  const { player } = storeToRefs(playStatus)

  let lastIdx = -1
  let lastPlayState = false

  const tick = () => {
    if (!isRunning) return

    const lines = player.value.lyrics?.lines || []
    const currentTimeMs = (Audio.value.currentTime || 0) * 1000
    const currentIdx = findCurrentLine(lines, currentTimeMs)
    const isPlaying = Audio.value.isPlay

    // 只在歌词行变化或播放状态变化时发送 IPC，从 60fps 降至按需发送
    if (currentIdx !== lastIdx || isPlaying !== lastPlayState) {
      lastIdx = currentIdx
      lastPlayState = isPlaying

      const currentLine = currentIdx >= 0 ? lines[currentIdx] : null
      const nextLine = currentIdx + 1 < lines.length ? lines[currentIdx + 1] : null

      emit('desktop-lyric-update', {
        currentLine: currentLine ? {
          text: getLineText(currentLine),
          translation: currentLine.translatedLyric,
          time: currentLine.startTime,
          duration: currentLine.endTime - currentLine.startTime
        } : null,
        nextLine: nextLine ? { text: getLineText(nextLine), translation: nextLine.translatedLyric } : null,
        currentIndex: currentIdx,
        totalLines: lines.length,
        currentTime: currentTimeMs,
        isPlaying,
        songName: player.value.songInfo?.name || '',
        songSinger: player.value.songInfo?.singer || ''
      }).catch(() => {})
    }

    rafId = requestAnimationFrame(tick)
  }

  rafId = requestAnimationFrame(tick)
}

export function stopDesktopLyricSync() {
  isRunning = false
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
}

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, type ComputedRef } from 'vue'
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event'
import type { LyricLine, LyricWord } from '@/types/lyric'

// ---------- Types ----------

interface RenderLine {
  line: LyricLine
  index: number
  key: string
  active: boolean
}

// ---------- State ----------

const lyricLines = ref<LyricLine[]>([])
const currentLineIndex = ref(-1)
const songInfo = ref({ name: '', singer: '' })
const isPlaying = ref(false)
const isLocked = ref(false)
const isHovering = ref(false)

// Local RAF clock
let baseMs = 0
let anchorTick = 0
const playSeekMs = ref(0)
let rafId: number | null = null

// Unlisteners
const unlisteners: UnlistenFn[] = []

// Style options (from DesktopLyricStyle settings)
interface StyleOption {
  fontSize?: number
  mainColor?: string
  shadowColor?: string
  fontWeight?: number
  fontFamily?: string
}

const styleOptions = ref<StyleOption>({})
const playedColor = computed(() => styleOptions.value.mainColor || '#73BCFC')
const unplayedColor = 'rgba(255,255,255,0.5)'
const shadowColor = computed(() => styleOptions.value.shadowColor || 'rgba(255,255,255,0.5)')
const fontSize = computed(() => styleOptions.value.fontSize || 30)
const fontWeight = computed(() => styleOptions.value.fontWeight || 700)
const fontFamily = computed(() => styleOptions.value.fontFamily || '')

// Refs for scroll measurement
const lineRefMap = new Map<string, HTMLElement>()
const contentRefMap = new Map<string, HTMLElement>()

// ---------- RAF Clock ----------

function startRafLoop() {
  if (rafId !== null) return
  const tick = () => {
    if (isPlaying.value) {
      playSeekMs.value = baseMs + (performance.now() - anchorTick)
    } else {
      playSeekMs.value = baseMs
    }
    rafId = requestAnimationFrame(tick)
  }
  rafId = requestAnimationFrame(tick)
}

function stopRafLoop() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
}

// ---------- IPC event handlers ----------

function handleLyricChange(lines: LyricLine[]) {
  lyricLines.value = lines || []
}

function handleSongChange(data: { name: string; singer: string }) {
  songInfo.value = {
    name: data?.name || '',
    singer: data?.singer || ''
  }
}

function handleIndexChange(index: number) {
  currentLineIndex.value = index
}

function handleProgress(data: { currentMs: number; timestamp: number }) {
  if (typeof data?.currentMs === 'number') {
    const newBase = Math.floor(data.currentMs)
    const drift = Math.abs(newBase - playSeekMs.value)
    if (drift > 300) {
      baseMs = newBase
      anchorTick = performance.now()
    }
  }
}

function handlePlayState(playing: boolean) {
  const wasPlaying = isPlaying.value
  isPlaying.value = !!playing
  if (isPlaying.value && !wasPlaying) {
    baseMs = playSeekMs.value
    anchorTick = performance.now()
  } else if (!isPlaying.value && wasPlaying) {
    baseMs = playSeekMs.value
    anchorTick = performance.now()
  }
}

// ---------- Compute render lines ----------

function getSafeEndTime(lyrics: LyricLine[], idx: number): number {
  const cur = lyrics[idx]
  const next = lyrics[idx + 1]
  const curEnd = Number(cur?.endTime)
  const curStart = Number(cur?.startTime)
  if (Number.isFinite(curEnd) && curEnd > curStart) return curEnd
  const nextStart = Number(next?.startTime)
  if (Number.isFinite(nextStart) && nextStart > curStart) return nextStart
  return 0
}

function placeholder(text: string): RenderLine[] {
  return [
    {
      line: {
        startTime: 0,
        endTime: 0,
        words: [{ word: text, startTime: 0, endTime: 0 }] as LyricWord[],
        translatedLyric: '',
        romanLyric: '',
        isBG: false,
        isDuet: false
      },
      index: -1,
      key: 'placeholder',
      active: true
    }
  ]
}

const renderLyricLines = computed<RenderLine[]>(() => {
  const lyrics = lyricLines.value
  if (!songInfo.value.name && !lyrics?.length) {
    return placeholder('Ceru Desktop Lyric')
  }
  if (!lyrics?.length) return placeholder('纯音乐，请欣赏')

  const idx = currentLineIndex.value
  if (idx < 0) {
    const playTitle = `${songInfo.value.name} - ${songInfo.value.singer}`
    return placeholder(playTitle)
  }

  const current = lyrics[idx]
  const next = lyrics[idx + 1]
  if (!current) return []

  const safeEnd = getSafeEndTime(lyrics, idx)
  const lines: RenderLine[] = []

  lines.push({
    line: { ...current, endTime: safeEnd },
    index: idx,
    key: `${idx}-orig`,
    active: true
  })

  if (next) {
    lines.push({
      line: next,
      index: idx + 1,
      key: `${idx + 1}-orig`,
      active: false
    })
  }

  return lines
})

// ---------- Word-by-word style ----------

const LYRIC_LOOKAHEAD = 300

function getWordStyle(word: LyricWord, lineIndex: number, isActive: boolean) {
  if (!isActive) {
    const hasPlayed = playSeekMs.value >= (word.endTime || 0)
    return { backgroundPositionX: hasPlayed ? '0%' : '100%' }
  }

  const duration = Math.max((word.endTime || 0) - (word.startTime || 0), 0.001)
  const progress = Math.max(
    Math.min((playSeekMs.value + LYRIC_LOOKAHEAD - (word.startTime || 0)) / duration, 1),
    0
  )
  return {
    backgroundPositionX: `${100 - progress * 100}%`
  }
}

function getPlainText(words: LyricWord[]): string {
  if (!Array.isArray(words)) return ''
  return words.map((w) => w?.word || '').join('')
}

function isYrcLine(line: LyricLine): boolean {
  return Array.isArray(line?.words) && line.words.length > 1
}

// ---------- Horizontal scroll ----------

const scrollStartAtProgress = 0.3
const END_MARGIN_SEC = 2

function getScrollStyle(line: RenderLine) {
  const container = lineRefMap.get(line.key)
  const content = contentRefMap.get(line.key)
  if (!container || !content || !line?.line) return {}
  const containerStyle = window.getComputedStyle(container)
  const contentStyle = window.getComputedStyle(content)
  const padL = parseFloat(containerStyle.paddingLeft) || 0
  const padR = parseFloat(containerStyle.paddingRight) || 0
  const marginL = parseFloat(contentStyle.marginLeft) || 0
  const marginR = parseFloat(contentStyle.marginRight) || 0
  const borderL = parseFloat(contentStyle.borderLeftWidth) || 0
  const borderR = parseFloat(contentStyle.borderRightWidth) || 0
  const visibleWidth = Math.max(0, container.clientWidth - padL - padR)
  const contentFullWidth = Math.max(0, content.scrollWidth + marginL + marginR + borderL + borderR)
  const overflow = Math.max(0, contentFullWidth - visibleWidth)
  if (overflow <= 0) return { transform: 'translateX(0px)' }
  const seekMs = playSeekMs.value
  const start = Number(line.line.startTime ?? 0)
  const endRaw = Number(line.line.endTime)
  const hasSafeEnd = Number.isFinite(endRaw) && endRaw > 0 && endRaw > start
  if (!hasSafeEnd) return { transform: 'translateX(0px)' }
  const end = Math.max(start + 0.001, endRaw - END_MARGIN_SEC)
  const duration = Math.max(end - start, 0.001)
  const progress = Math.max(Math.min((seekMs - start) / duration, 1), 0)
  if (progress <= scrollStartAtProgress) return { transform: 'translateX(0px)' }
  const ratio = (progress - scrollStartAtProgress) / (1 - scrollStartAtProgress)
  const offset = Math.round(overflow * ratio)
  return {
    transform: `translateX(-${offset}px)`,
    willChange: 'transform'
  }
}

function setLineRef(el: Element | null, key: string) {
  if (el) lineRefMap.set(key, el as HTMLElement)
  else lineRefMap.delete(key)
}

function setContentRef(el: Element | null, key: string) {
  if (el) contentRefMap.set(key, el as HTMLElement)
  else contentRefMap.delete(key)
}

// ---------- Control events ----------

function emitControl(name: string, value?: boolean) {
  emit('desktop-lyric-control', { name, value }).catch(() => {})
}

function onPrev() {
  emitControl('playPrev')
}

function onToggle() {
  emitControl('toggle')
}

function onNext() {
  emitControl('playNext')
}

function onToggleLock() {
  const next = !isLocked.value
  isLocked.value = next
  emitControl('lock', next)
}

function onClose() {
  emitControl('close')
}

// ---------- Line positioning ----------

function getLineTop(index: number): string {
  if (index === 0) return '0px'
  return `${fontSize.value * 1.9}px`
}

// ---------- Lifecycle ----------

onMounted(async () => {
  // Load saved style options
  try {
    const saved = await (window as any).electron?.ipcRenderer?.invoke('get-desktop-lyric-option')
    if (saved) styleOptions.value = saved
  } catch {}

  // Listen for real-time style changes from settings UI
  unlisteners.push(
    await listen<StyleOption>('desktop-lyric-style-change', (event) => {
      styleOptions.value = event.payload
    })
  )

  // Listen for IPC events from bridge
  unlisteners.push(
    await listen<LyricLine[]>('desktop-lyric-change', (event) => {
      handleLyricChange(event.payload)
    })
  )

  unlisteners.push(
    await listen<{ name: string; singer: string }>('desktop-song-change', (event) => {
      handleSongChange(event.payload)
    })
  )

  unlisteners.push(
    await listen<number>('desktop-lyric-index', (event) => {
      handleIndexChange(event.payload)
    })
  )

  unlisteners.push(
    await listen<{ currentMs: number; timestamp: number }>('desktop-lyric-progress', (event) => {
      handleProgress(event.payload)
    })
  )

  unlisteners.push(
    await listen<boolean>('desktop-lyric-play-state', (event) => {
      handlePlayState(event.payload)
    })
  )

  // Start RAF clock
  startRafLoop()

  // Notify bridge that we're ready to receive data
  emit('desktop-lyric-ready', {}).catch(() => {})
})

onBeforeUnmount(() => {
  stopRafLoop()
  unlisteners.forEach((fn) => {
    try { fn() } catch {}
  })
  unlisteners.length = 0
  lineRefMap.clear()
  contentRefMap.clear()
})
</script>

<template>
  <div
    :class="['desktop-lyric', { locked: isLocked, hovered: isHovering }]"
    @mouseenter="isHovering = true"
    @mouseleave="isHovering = false"
  >
    <!-- Lyric display area -->
    <TransitionGroup
      tag="div"
      name="lyric-slide"
      :style="{
        fontSize: fontSize + 'px',
        textShadow: `0 0 4px ${shadowColor}`,
        fontFamily: fontFamily || undefined,
        fontWeight: fontWeight
      }"
      class="lyric-container"
    >
      <div
        v-for="(renderLine, renderIndex) in renderLyricLines"
        :key="renderLine.key"
        :ref="(el) => setLineRef(el as Element | null, renderLine.key)"
        :class="[
          'lyric-line',
          {
            active: renderLine.active,
            'is-yrc': renderLine.active && isYrcLine(renderLine.line),
            'is-next': !renderLine.active
          }
        ]"
        :style="{
          top: getLineTop(renderIndex),
          fontSize: renderIndex > 0 ? '0.8em' : '1em'
        }"
      >
        <!-- Word-by-word rendering for YRC lines -->
        <template v-if="renderLine.active && isYrcLine(renderLine.line)">
          <span
            :ref="(el) => setContentRef(el as Element | null, renderLine.key)"
            class="scroll-content"
            :style="getScrollStyle(renderLine)"
          >
            <span class="content">
              <span
                v-for="(word, wordIndex) in renderLine.line.words"
                :key="wordIndex"
                :class="{
                  'content-text': true,
                  'end-with-space': word.word.endsWith(' ') || word.startTime === 0
                }"
              >
                <span
                  class="word"
                  :style="[
                    {
                      backgroundImage: `linear-gradient(to right, ${playedColor} 50%, ${unplayedColor} 50%)`,
                      backgroundSize: '200% 100%',
                      backgroundClip: 'text',
                      WebkitBackgroundClip: 'text',
                      color: 'transparent'
                    },
                    getWordStyle(word, renderLine.index, renderLine.active)
                  ]"
                >
                  {{ word.word }}
                </span>
              </span>
            </span>
          </span>
        </template>
        <!-- Plain text for non-YRC lines or next line -->
        <template v-else>
          <span
            :ref="(el) => setContentRef(el as Element | null, renderLine.key)"
            class="scroll-content"
            :style="[
              getScrollStyle(renderLine),
              {
                color: renderLine.active ? playedColor : unplayedColor
              }
            ]"
          >
            {{ getPlainText(renderLine.line?.words || []) }}
          </span>
        </template>
      </div>

      <!-- Placeholder when no lines -->
      <span v-if="renderLyricLines.length === 0" key="placeholder" class="lyric-line placeholder">
        &nbsp;
      </span>
    </TransitionGroup>

    <!-- Control bar (shown on hover, not when locked) -->
    <Transition name="controls">
      <div v-if="isHovering && !isLocked" class="controls-bar" @pointerdown.stop>
        <div class="song-info" v-if="songInfo.name">
          {{ songInfo.name }} - {{ songInfo.singer }}
        </div>
        <div class="control-buttons">
          <!-- Previous -->
          <button class="ctrl-btn" title="上一首" @click="onPrev">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
            </svg>
          </button>
          <!-- Play/Pause -->
          <button class="ctrl-btn" :title="isPlaying ? '暂停' : '播放'" @click="onToggle">
            <svg v-if="isPlaying" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 4h4v16H6zM14 4h4v16h-4z"/>
            </svg>
            <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z"/>
            </svg>
          </button>
          <!-- Next -->
          <button class="ctrl-btn" title="下一首" @click="onNext">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/>
            </svg>
          </button>
          <!-- Lock -->
          <button class="ctrl-btn" :title="isLocked ? '解锁' : '锁定'" @click="onToggleLock">
            <svg v-if="isLocked" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
            <svg v-else width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 9.9-1"/>
            </svg>
          </button>
          <!-- Close -->
          <button class="ctrl-btn" title="关闭" @click="onClose">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.desktop-lyric {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: transparent;
  user-select: none;
  position: relative;
  color: #fff;
  font-family: -apple-system, BlinkMacSystemFont, 'PingFangSC-Semibold', 'Segoe UI', Roboto,
    sans-serif;
  font-weight: v-bind(fontWeight);

  &.locked {
    pointer-events: none;
  }
}

.lyric-container {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 0 8px;

  .lyric-line {
    position: absolute;
    width: 100%;
    left: 0;
    line-height: normal;
    padding: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition:
      top 0.6s cubic-bezier(0.55, 0, 0.1, 1),
      font-size 0.6s cubic-bezier(0.55, 0, 0.1, 1),
      opacity 0.6s cubic-bezier(0.55, 0, 0.1, 1),
      transform 0.6s cubic-bezier(0.55, 0, 0.1, 1);
    will-change: top, font-size, transform;
    transform-origin: left center;

    &.placeholder {
      visibility: hidden;
    }

    .scroll-content {
      display: inline-block;
      white-space: nowrap;
      will-change: transform;
    }

    &.is-yrc {
      .content {
        display: inline-flex;
        flex-wrap: nowrap;
        width: auto;
        overflow-wrap: normal;
        word-break: normal;
        white-space: nowrap;
      }

      .content-text {
        position: relative;
        display: inline-block;

        .word {
          display: inline-block;
          background-clip: text;
          -webkit-background-clip: text;
          color: transparent;
          background-size: 200% 100%;
          background-repeat: no-repeat;
          background-position-x: 100%;
          will-change: background-position-x;
        }

        &.end-with-space {
          margin-right: 5vh;

          &:last-child {
            margin-right: 0;
          }
        }
      }
    }

    &.is-next {
      color: v-bind('unplayedColor');
    }
  }
}

/* TransitionGroup slide animation */
.lyric-slide-move,
.lyric-slide-enter-active,
.lyric-slide-leave-active {
  transition:
    transform 0.6s cubic-bezier(0.55, 0, 0.1, 1),
    opacity 0.6s cubic-bezier(0.55, 0, 0.1, 1);
  will-change: transform, opacity;
}

.lyric-slide-enter-from {
  opacity: 0;
  transform: translateY(100%);
}

.lyric-slide-leave-to {
  opacity: 0;
  transform: translateY(-100%);
}

.lyric-slide-leave-active {
  position: absolute;
}

/* Controls bar */
.controls-bar {
  position: absolute;
  bottom: 10px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(10px);
  padding: 8px 16px;
  border-radius: 20px;
}

.song-info {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.8);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 300px;
}

.control-buttons {
  display: flex;
  gap: 8px;
  align-items: center;
}

.ctrl-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.8);
  cursor: pointer;
  padding: 4px;
  border-radius: 50%;
  transition: all 0.2s;

  &:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.2);
  }

  &:active {
    transform: scale(0.95);
  }
}

/* Controls transition */
.controls-enter-active,
.controls-leave-active {
  transition: all 0.2s ease;
}

.controls-enter-from,
.controls-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}
</style>

<style>
body {
  background-color: transparent !important;
}
</style>

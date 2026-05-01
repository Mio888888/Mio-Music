<template>
  <div class="lyric" @mouseleave="lrcAllLeave">
    <div class="lyric-content">
      <div
        ref="lyricScrollContainer"
        class="lyric-scroll-container"
        tabindex="-1"
        @wheel="handleUserScroll"
      >
        <div id="lrc-placeholder" class="placeholder" />
        <div
          v-for="(line, index) in lyricLines"
          :id="`lrc-${index}`"
          :key="index"
          :class="getLyricLineClass(line, index)"
          :style="getLyricLineStyle(index)"
          @click="emitLineClick(line)"
        >
          <template v-if="line.words.length > 1">
            <div class="content">
              <div
                v-for="(word, tIndex) in line.words"
                :key="tIndex"
                :class="{
                  'content-text': true,
                  'end-with-space': word.word.endsWith(' ')
                }"
                :style="getYrcVars(word, index)"
              >
                <span class="yrc-word">
                  {{ word.word }}
                </span>
              </div>
            </div>
          </template>
          <template v-else>
            <span class="content">
              {{ line.words.map(w => w.word).join('') }}
            </span>
          </template>
          <span v-if="line.translatedLyric" class="tran">{{ line.translatedLyric }}</span>
          <span v-if="line.romanLyric" class="roma">{{ line.romanLyric }}</span>
        </div>
        <div class="placeholder" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from 'vue'
import type { LyricLine, LyricWord } from '@/types/lyric'

const props = defineProps({
  lyricLines: {
    type: Array as () => LyricLine[],
    default: () => []
  },
  currentTime: {
    type: Number,
    default: 0
  },
  playing: {
    type: Boolean,
    default: false
  },
  alignPosition: {
    type: Number,
    default: 0.5
  },
  enableBlur: {
    type: Boolean,
    default: false
  },
  enableSpring: {
    type: Boolean,
    default: false
  },
  enableScale: {
    type: Boolean,
    default: false
  },
  textAlign: {
    type: String,
    default: 'left'
  }
})

const emit = defineEmits(['line-click'])

const lyricScrollContainer = ref<HTMLElement | null>(null)

const activeLineIndices = computed<number[]>(() => {
  const lyrics = props.lyricLines
  const currentSeek = props.currentTime
  const hasMultiWord = lyrics.some((l) => l.words.length > 1)
  if (hasMultiWord) {
    const indices: number[] = []
    for (let i = 0; i < lyrics.length; i++) {
      const start = lyrics[i].startTime || 0
      const end = lyrics[i].endTime || start + Infinity
      if (currentSeek >= start && currentSeek < end) {
        indices.push(i)
      }
    }
    if (indices.length === 0 && currentSeek > 0) {
      const next = lyrics.findIndex((v) => (v.startTime || 0) > currentSeek)
      if (next === -1) return [lyrics.length - 1]
      if (next > 0) return [next - 1]
    }
    return indices.length > 3 ? indices.slice(-3) : indices
  } else {
    const playSeek = currentSeek + 300
    const idx = lyrics.findIndex((v) => (v.startTime || 0) > playSeek)
    if (idx === -1) return [lyrics.length - 1]
    if (idx > 0) return [idx - 1]
    return []
  }
})

const scrollTargetIndex = computed<number>(() => {
  const lyrics = props.lyricLines
  const currentSeek = props.currentTime
  const hasMultiWord = lyrics.some((l) => l.words.length > 1)
  if (hasMultiWord) {
    for (let i = 0; i < lyrics.length; i++) {
      const start = lyrics[i].startTime || 0
      const end = lyrics[i].endTime || start + Infinity
      if (currentSeek >= start && currentSeek < end) return i
    }
    if (currentSeek > 0) {
      const next = lyrics.findIndex((v) => (v.startTime || 0) > currentSeek)
      if (next === -1) return lyrics.length - 1
      if (next > 0) return next - 1
    }
    return -1
  } else {
    const playSeek = currentSeek + 300
    const idx = lyrics.findIndex((v) => (v.startTime || 0) > playSeek)
    if (idx === -1) return lyrics.length - 1
    if (idx > 0) return idx - 1
    return -1
  }
})

const firstActiveIndex = computed(() => activeLineIndices.value[0] ?? -1)

let scrollAnimationId: number | null = null
const userScrolling = ref(false)
let userScrollTimeoutId: ReturnType<typeof setTimeout> | null = null
const USER_SCROLL_TIMEOUT = 3000

const handleUserScroll = () => {
  userScrolling.value = true
  if (userScrollTimeoutId !== null) clearTimeout(userScrollTimeoutId)
  userScrollTimeoutId = setTimeout(() => {
    userScrolling.value = false
    userScrollTimeoutId = null
    lyricsScroll(scrollTargetIndex.value)
  }, USER_SCROLL_TIMEOUT)
}

const smoothScrollTo = (container: HTMLElement, targetY: number, duration = 300) => {
  if (scrollAnimationId !== null) {
    cancelAnimationFrame(scrollAnimationId)
    scrollAnimationId = null
  }
  const startY = container.scrollTop
  const diff = targetY - startY
  if (Math.abs(diff) < 0.5) {
    container.scrollTop = targetY
    return
  }
  const startTime = performance.now()
  const step = (currentTime: number) => {
    const elapsed = currentTime - startTime
    const progress = Math.min(elapsed / duration, 1)
    const easedProgress =
      progress < 0.5 ? 2 * progress * progress : 1 - Math.pow(-2 * progress + 2, 2) / 2
    container.scrollTop = startY + diff * easedProgress
    if (progress < 1) {
      scrollAnimationId = requestAnimationFrame(step)
    } else {
      scrollAnimationId = null
    }
  }
  scrollAnimationId = requestAnimationFrame(step)
}

const lyricsScroll = (index: number) => {
  const container = lyricScrollContainer.value
  if (!container) return
  if (userScrolling.value) return
  const lrcItemDom = document.getElementById(index >= 0 ? `lrc-${index}` : 'lrc-placeholder')
  if (!lrcItemDom) return
  const containerHeight = container.clientHeight
  const elementTop = lrcItemDom.offsetTop
  const elementHeight = lrcItemDom.offsetHeight
  let targetY = elementTop - (containerHeight - elementHeight) * props.alignPosition
  targetY = Math.max(0, Math.min(targetY, container.scrollHeight - container.clientHeight))
  smoothScrollTo(container, targetY, 500)
}

const lrcAllLeave = () => {
  userScrolling.value = false
  if (userScrollTimeoutId !== null) {
    clearTimeout(userScrollTimeoutId)
    userScrollTimeoutId = null
  }
  lyricsScroll(scrollTargetIndex.value)
}

const YRC_DIM_ALPHA = 0.3
const YRC_LINE_FADE_MS = 250
const yrcFadingLineIndex = ref<number | null>(null)
const yrcFadingUntilAt = ref<number>(0)

const isLineActive = (index: number): boolean => activeLineIndices.value.includes(index)

const getYrcFadeFactor = (index: number): number => {
  if (yrcFadingLineIndex.value !== index) return 1
  const now = Date.now()
  if (now >= yrcFadingUntilAt.value) return 1
  const remain = yrcFadingUntilAt.value - now
  return Math.min(Math.max(remain / YRC_LINE_FADE_MS, 0), 1)
}

type CssVars = Record<`--${string}`, string>

const getYrcVars = (wordData: LyricWord, lyricIndex: number): CssVars => {
  const currentSeek = props.currentTime
  const fadeFactor = getYrcFadeFactor(lyricIndex)
  const startTime = wordData.startTime
  const endTime = wordData.endTime
  const duration = endTime - startTime
  const safeDuration = Math.max(duration, 1)
  const rawProgress = (currentSeek - startTime) / safeDuration
  const progress = Math.min(Math.max(rawProgress, 0), 1)
  const maskX = `${(1 - progress) * 100}%`
  const hasStarted = currentSeek >= startTime
  const brightAlpha = hasStarted ? YRC_DIM_ALPHA + (1 - YRC_DIM_ALPHA) * fadeFactor : YRC_DIM_ALPHA
  const darkAlpha = YRC_DIM_ALPHA

  let translateY = '0.16em'
  let scale = '1'
  let transitionDuration = '0.4s'
  let timingFunction = 'cubic-bezier(0.25, 0.46, 0.45, 0.94)'

  if (hasStarted && fadeFactor >= 1) {
    const clampedDuration = Math.max(100, Math.min(duration, 800))
    const factor = (clampedDuration - 100) / 700
    const maxTranslateEm = factor * 0.01
    const maxTranslate = `${maxTranslateEm}em`
    const maxScale = 1.02 + factor * 0.13

    if (progress < 1) {
      translateY = `-${maxTranslate}`
      scale = `${maxScale}`
      const riseDuration = Math.max(duration * 0.8, 1100) / 1000
      transitionDuration = `${riseDuration}s`
      timingFunction = 'ease-out'
    } else {
      translateY = '0px'
      scale = '1'
      transitionDuration = '2s'
      timingFunction = 'cubic-bezier(0.34, 1.3, 0.64, 1)'
    }
  }

  return {
    '--yrc-mask-x': maskX,
    '--yrc-opacity': '1',
    '--yrc-bright-alpha': `${brightAlpha}`,
    '--yrc-dark-alpha': `${darkAlpha}`,
    '--yrc-translate-y': translateY,
    '--yrc-scale': scale,
    '--yrc-anim-duration': transitionDuration,
    '--yrc-anim-ease': timingFunction
  }
}

const getLyricLineClass = (line: LyricLine, index: number) => {
  const isOn = isLineActive(index)
  const isYrc = line.words.length > 1
  return [
    'lrc-line',
    isYrc ? 'is-yrc' : 'is-lrc',
    { on: isOn }
  ]
}

const getLyricLineStyle = (index: number) => {
  if (!props.enableBlur || userScrolling.value) return { filter: 'blur(0)' }
  const activeIdx = firstActiveIndex.value
  const isOn = isLineActive(index)
  const dist = Math.abs(activeIdx - index)
  const blurPx = dist === 0 ? 0 : Math.min(1.2 + Math.pow(dist, 0.7) * 1.5, 8)
  return {
    filter: isOn ? 'blur(0)' : `blur(${blurPx}px)`
  }
}

const emitLineClick = (line: LyricLine) => {
  emit('line-click', { time: line.startTime, line })
}

watch(scrollTargetIndex, (val, oldVal) => {
  lyricsScroll(val)
  if (typeof oldVal === 'number' && oldVal >= 0 && oldVal !== val) {
    yrcFadingLineIndex.value = oldVal
    yrcFadingUntilAt.value = Date.now() + YRC_LINE_FADE_MS
  }
})

watch(
  () => props.textAlign,
  () => {
    nextTick(() => setTimeout(() => lyricsScroll(scrollTargetIndex.value), 600))
    nextTick(() => setTimeout(() => lyricsScroll(scrollTargetIndex.value), 800))
  }
)

onMounted(() => {
  nextTick().then(() => lyricsScroll(scrollTargetIndex.value))
})

onBeforeUnmount(() => {
  if (scrollAnimationId !== null) {
    cancelAnimationFrame(scrollAnimationId)
    scrollAnimationId = null
  }
  if (userScrollTimeoutId !== null) {
    clearTimeout(userScrollTimeoutId)
    userScrollTimeoutId = null
  }
})
</script>

<style lang="scss" scoped>
.lyric {
  position: relative;
  width: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: flex-start;
  height: 100%;
  overflow: hidden;

  .lyric-scroll-container {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    padding-left: 10px;
    padding-right: v-bind("props.textAlign === 'center' ? '10px' : '80px'");
    box-sizing: border-box;
    scrollbar-width: none;
    -ms-overflow-style: none;
    &::-webkit-scrollbar {
      display: none;
    }
    @media (max-width: 990px) {
      padding-right: 20px;
    }
  }

  .placeholder {
    width: 100%;
    &:first-child {
      height: 300px;
      display: flex;
      align-items: flex-end;
    }
    &:last-child {
      height: 0;
      padding-top: 100%;
    }
  }

  .lyric-content {
    width: 100%;
    height: 100%;
  }

  .lrc-line {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: v-bind("props.textAlign === 'center' ? 'center' : 'flex-start'");
    margin: clamp(4px, 1.2vh, 12px) 0;
    padding: clamp(8px, 2vh, 16px) 16px;
    transform: scale(0.9);
    transform-origin: v-bind("props.textAlign === 'center' ? 'center' : 'left center'");
    will-change: filter, opacity, transform;
    transition:
      filter 0.5s,
      opacity 0.5s,
      transform 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94);
    cursor: pointer;
    width: 100%;
    opacity: 0.25;
    text-align: v-bind(textAlign);

    .content {
      display: block;
      font-size: var(--amll-lyric-player-font-size, 22px);
      font-weight: var(--amll-lyric-player-font-weight, 600);
      color: var(--amll-lyric-view-color, rgba(255, 255, 255, 1));
      width: 100%;
      overflow-wrap: anywhere;
      word-break: break-word;
      white-space: normal;
      hyphens: auto;
      text-align: v-bind(textAlign);

      .content-text {
        position: relative;
        display: inline-block;
        overflow: visible;
        overflow-wrap: anywhere;
        word-break: break-word;
        white-space: normal;
        font-weight: var(--amll-lyric-player-font-weight, 600);
        text-align: v-bind(textAlign);

        .yrc-word {
          font-weight: var(--amll-lyric-player-font-weight, 600);
          display: inline-block;
          box-sizing: border-box;
          padding-block: 0.2em;
          margin-block: -0.2em;
          opacity: var(--yrc-opacity, 0.3);
        }

        &.end-with-space {
          margin-right: 12px;
          &:last-child {
            margin-right: 0;
          }
        }
      }
    }

    .tran {
      margin-top: 8px;
      opacity: 0.6;
      font-size: calc(var(--amll-lyric-player-font-size, 22px) * 0.52);
      color: var(--amll-lyric-view-color, rgba(255, 255, 255, 1));
      transition: opacity 0.35s;
      width: 100%;
      overflow-wrap: anywhere;
      word-break: break-word;
      white-space: normal;
      hyphens: auto;
      text-align: v-bind(textAlign);
    }

    .roma {
      margin-top: 4px;
      opacity: 0.5;
      font-size: calc(var(--amll-lyric-player-font-size, 22px) * 0.73);
      color: var(--amll-lyric-view-color, rgba(255, 255, 255, 1));
      transition: opacity 0.35s;
      width: 100%;
      overflow-wrap: anywhere;
      word-break: break-word;
      white-space: normal;
      hyphens: auto;
      text-align: v-bind(textAlign);
    }

    &.is-yrc {
      .content {
        display: flex;
        flex-wrap: wrap;
        width: 100%;
        overflow-wrap: anywhere;
        word-break: break-word;
        white-space: normal;
        justify-content: v-bind("props.textAlign === 'center' ? 'center' : 'flex-start'");
      }
      .tran,
      .roma {
        opacity: 0.3;
      }
    }

    &.on {
      opacity: 1 !important;
      transform: scale(1.05);
      .tran,
      .roma {
        opacity: 0.6;
      }
    }

    &::before {
      content: '';
      display: block;
      position: absolute;
      left: 0;
      top: 0;
      width: 100%;
      height: 100%;
      border-radius: 8px;
      background-color: rgba(255, 255, 255, 0.14);
      opacity: 0;
      z-index: 0;
      transform: scale(1.05);
      transition:
        transform 0.35s ease,
        opacity 0.35s ease;
      pointer-events: none;
    }

    @media (hover: hover) and (pointer: fine) {
      .lyric &:hover {
        opacity: 1;
        &::before {
          transform: scale(1);
          opacity: 1;
        }
      }
      .lyric &:active {
        &::before {
          transform: scale(0.95);
        }
      }
    }
  }

  .lrc-line.is-yrc.on {
    .content-text {
      .yrc-word {
        will-change: -webkit-mask-position-x, transform;
        mask-image: linear-gradient(
          to right,
          rgba(0, 0, 0, var(--yrc-bright-alpha, 1)) 45.4545454545%,
          rgba(0, 0, 0, var(--yrc-dark-alpha, 0.3)) 54.5454545455%
        );
        mask-size: 220% 100%;
        mask-repeat: no-repeat;
        -webkit-mask-image: linear-gradient(
          to right,
          rgba(0, 0, 0, var(--yrc-bright-alpha, 1)) 45.4545454545%,
          rgba(0, 0, 0, var(--yrc-dark-alpha, 0.3)) 54.5454545455%
        );
        -webkit-mask-size: 220% 100%;
        -webkit-mask-repeat: no-repeat;
        -webkit-mask-position-x: var(--yrc-mask-x, 0%);
        transform: translateY(var(--yrc-translate-y, 1.5px)) scale(var(--yrc-scale, 1));
        transform-origin: center bottom;
        transition: transform var(--yrc-anim-duration, 0.4s)
          var(--yrc-anim-ease, cubic-bezier(0.25, 0.46, 0.45, 0.94));
      }
    }
  }
}
</style>

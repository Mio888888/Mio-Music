<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { storeToRefs } from 'pinia'
import { ControlAudioStore } from '@/store/ControlAudio'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { findCurrentLine, type LyricLine } from '@/types/lyric'

const audioStore = ControlAudioStore()
const playStatus = useGlobalPlayStatusStore()
const { Audio } = storeToRefs(audioStore)
const { player } = storeToRefs(playStatus)

const containerRef = ref<HTMLElement>()
const currentLineIndex = ref(-1)
const isUserScrolling = ref(false)
let scrollTimer: ReturnType<typeof setTimeout> | null = null

const lyrics = computed(() => player.value.lyrics?.lines || [])

// Find current line based on playback time
watch(
  () => Audio.value.currentTime,
  (time) => {
    if (!lyrics.value.length) return
    const timeMs = time * 1000
    const idx = findCurrentLine(lyrics.value, timeMs)
    if (idx !== currentLineIndex.value) {
      currentLineIndex.value = idx
      if (!isUserScrolling.value) scrollToLine(idx)
    }
  }
)

const scrollToLine = (index: number) => {
  if (index < 0 || !containerRef.value) return
  nextTick(() => {
    const container = containerRef.value
    if (!container) return
    const lineEl = container.children[index] as HTMLElement
    if (!lineEl) return
    const containerHeight = container.clientHeight
    const lineTop = lineEl.offsetTop
    const lineHeight = lineEl.clientHeight
    const scrollTarget = lineTop - containerHeight / 2 + lineHeight / 2
    container.scrollTo({ top: scrollTarget, behavior: 'smooth' })
  })
}

const handleScroll = () => {
  isUserScrolling.value = true
  if (scrollTimer) clearTimeout(scrollTimer)
  scrollTimer = setTimeout(() => {
    isUserScrolling.value = false
  }, 3000)
}

const getLineStyle = (index: number) => {
  const isActive = index === currentLineIndex.value
  const distance = Math.abs(index - currentLineIndex.value)
  return {
    opacity: isActive ? 1 : Math.max(0.3, 1 - distance * 0.15),
    transform: isActive ? 'scale(1.05)' : 'scale(1)',
    fontWeight: isActive ? '700' : '400',
    color: isActive ? 'var(--td-brand-color)' : 'var(--td-text-color-primary)',
    transition: 'all 0.3s ease',
  }
}
</script>

<template>
  <div class="lyric-display">
    <div v-if="lyrics.length === 0" class="lyric-empty">
      <p>暂无歌词</p>
    </div>
    <div v-else ref="containerRef" class="lyric-scroll" @scroll="handleScroll">
      <div class="lyric-padding"></div>
      <div
        v-for="(line, index) in lyrics"
        :key="index"
        class="lyric-line"
        :style="getLineStyle(index)"
      >
        <div class="lyric-text">{{ line.text }}</div>
        <div v-if="line.translation" class="lyric-translation">{{ line.translation }}</div>
      </div>
      <div class="lyric-padding"></div>
    </div>
  </div>
</template>

<style scoped>
.lyric-display {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.lyric-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}

.lyric-empty p {
  color: var(--td-text-color-secondary);
  font-size: 14px;
}

.lyric-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 0 20px;
  scroll-behavior: smooth;
  mask-image: linear-gradient(
    to bottom,
    transparent 0%,
    black 15%,
    black 85%,
    transparent 100%
  );
}

.lyric-padding {
  height: 40%;
}

.lyric-line {
  padding: 8px 0;
  text-align: center;
  cursor: pointer;
  line-height: 1.6;
}

.lyric-text {
  font-size: 16px;
  white-space: pre-wrap;
}

.lyric-translation {
  font-size: 13px;
  margin-top: 4px;
  opacity: 0.7;
  white-space: pre-wrap;
}
</style>

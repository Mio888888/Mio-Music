<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface LyricData {
  text: string
  translation?: string
  time: number
  duration: number
}

interface UpdatePayload {
  currentLine: LyricData | null
  nextLine: LyricData | null
  currentIndex: number
  totalLines: number
  currentTime: number
  isPlaying: boolean
  songName: string
  songSinger: string
}

const currentLine = ref<LyricData | null>(null)
const nextLine = ref<LyricData | null>(null)
const songInfo = ref({ name: '', singer: '' })
const isPlaying = ref(false)
const isLocked = ref(false)
const isHovering = ref(false)

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<UpdatePayload>('desktop-lyric-update', (event) => {
    const data = event.payload
    currentLine.value = data.currentLine
    nextLine.value = data.nextLine
    isPlaying.value = data.isPlaying
    if (data.songName) songInfo.value.name = data.songName
    if (data.songSinger) songInfo.value.singer = data.songSinger
  })
})

onUnmounted(() => {
  if (unlisten) unlisten()
})

const displayText = computed(() => currentLine.value?.text || '')
const displayTranslation = computed(() => currentLine.value?.translation || '')
const nextText = computed(() => nextLine.value?.text || '')

const containerClass = computed(() => ({
  'desktop-lyric': true,
  'locked': isLocked.value,
  'hovering': isHovering.value,
  'no-song': !displayText.value
}))
</script>

<template>
  <div
    :class="containerClass"
    @mouseenter="isHovering = true"
    @mouseleave="isHovering = false"
  >
    <div class="lyric-content">
      <div class="current-line">
        <span class="lyric-text">{{ displayText || '等待播放...' }}</span>
      </div>
      <div v-if="displayTranslation" class="translation-line">
        {{ displayTranslation }}
      </div>
      <div v-else-if="nextText" class="next-line">
        {{ nextText }}
      </div>
    </div>

    <Transition name="controls">
      <div v-if="isHovering && !isLocked" class="controls-bar">
        <div class="song-info" v-if="songInfo.name">
          {{ songInfo.name }} - {{ songInfo.singer }}
        </div>
        <div class="control-buttons">
          <button class="ctrl-btn" @click="isLocked = !isLocked">
            {{ isLocked ? '解锁' : '锁定' }}
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
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
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.locked {
  pointer-events: none;
}

.lyric-content {
  text-align: center;
  padding: 0 20px;
  max-width: 80%;
}

.current-line {
  margin-bottom: 8px;
}

.lyric-text {
  font-size: 28px;
  font-weight: 700;
  color: #fff;
  text-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
  line-height: 1.4;
}

.translation-line, .next-line {
  font-size: 16px;
  color: rgba(255, 255, 255, 0.7);
  text-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  line-height: 1.4;
}

.no-song .lyric-text {
  font-size: 18px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.5);
}

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
}

.ctrl-btn {
  background: none;
  border: 1px solid rgba(255, 255, 255, 0.3);
  color: #fff;
  font-size: 12px;
  padding: 4px 12px;
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.ctrl-btn:hover {
  background: rgba(255, 255, 255, 0.2);
}

.controls-enter-active, .controls-leave-active {
  transition: all 0.2s ease;
}
.controls-enter-from, .controls-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}
</style>

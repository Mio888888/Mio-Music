<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { playSetting } from '@/store/playSetting'

const canvas = ref<HTMLCanvasElement | null>(null)
let analyser: AnalyserNode | null = null
let audioCtx: AudioContext | null = null
let animFrame: number | null = null
let sourceConnected = false

const playSettingStore = playSetting()

function initAudio() {
  if (audioCtx) return
  audioCtx = new AudioContext()
  analyser = audioCtx.createAnalyser()
  analyser.fftSize = 256
  analyser.smoothingTimeConstant = 0.8

  // Connect to the first audio element on the page
  const audio = document.querySelector('audio') as HTMLAudioElement | null
  if (audio && !sourceConnected) {
    try {
      const source = audioCtx.createMediaElementSource(audio)
      source.connect(analyser)
      analyser.connect(audioCtx.destination)
      sourceConnected = true
    } catch { /* already connected */ }
  }
}

function draw() {
  if (!canvas.value || !analyser) return
  const ctx = canvas.value.getContext('2d')
  if (!ctx) return

  const WIDTH = canvas.value.width
  const HEIGHT = canvas.value.height
  const bufferLength = analyser.frequencyBinCount
  const dataArray = new Uint8Array(bufferLength)
  analyser.getByteFrequencyData(dataArray)

  ctx.clearRect(0, 0, WIDTH, HEIGHT)

  const barCount = 64
  const barWidth = WIDTH / barCount - 1
  const step = Math.floor(bufferLength / barCount)

  for (let i = 0; i < barCount; i++) {
    const val = dataArray[i * step] / 255
    const barHeight = val * HEIGHT * 0.9

    const hue = 160 + (i / barCount) * 60
    ctx.fillStyle = `hsla(${hue}, 80%, 60%, ${0.4 + val * 0.6})`
    const x = i * (barWidth + 1)
    const y = HEIGHT - barHeight
    ctx.beginPath()
    ctx.roundRect(x, y, barWidth, barHeight, 2)
    ctx.fill()
  }

  animFrame = requestAnimationFrame(draw)
}

function startVisualizer() {
  if (!playSettingStore.isAudioVisualizer) return
  initAudio()
  if (analyser) draw()
}

function stopVisualizer() {
  if (animFrame) { cancelAnimationFrame(animFrame); animFrame = null }
}

function resizeCanvas() {
  if (!canvas.value) return
  const rect = canvas.value.getBoundingClientRect()
  canvas.value.width = rect.width * window.devicePixelRatio
  canvas.value.height = rect.height * window.devicePixelRatio
  const ctx = canvas.value.getContext('2d')
  if (ctx) ctx.scale(window.devicePixelRatio, window.devicePixelRatio)
}

onMounted(() => {
  resizeCanvas()
  window.addEventListener('resize', resizeCanvas)
  if (playSettingStore.isAudioVisualizer) startVisualizer()
})

onBeforeUnmount(() => {
  stopVisualizer()
  window.removeEventListener('resize', resizeCanvas)
})

watch(() => playSettingStore.isAudioVisualizer, (v) => {
  if (v) startVisualizer(); else stopVisualizer()
})
</script>

<template>
  <div v-if="playSettingStore.isAudioVisualizer" class="audio-visualizer">
    <canvas ref="canvas" class="visualizer-canvas" />
  </div>
</template>

<style scoped>
.audio-visualizer {
  width: 100%; height: 48px; overflow: hidden; opacity: 0.8;
}
.visualizer-canvas {
  width: 100%; height: 100%; display: block;
}
</style>

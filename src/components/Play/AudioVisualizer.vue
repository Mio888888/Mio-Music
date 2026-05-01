<script lang="ts" setup>
import { ref, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { storeToRefs } from 'pinia'
import audioManager from '@/utils/audio/AudioManager'

interface Props {
  show?: boolean
  height?: number
  barCount?: number
  color?: string
  backgroundColor?: string
}

const props = withDefaults(defineProps<Props>(), {
  show: true,
  height: 80,
  barCount: 64,
  color: 'rgba(255, 255, 255, 0.8)',
  backgroundColor: 'transparent'
})

const emit = defineEmits<{
  lowFreqUpdate: [volume: number]
}>()

const canvasRef = ref<HTMLCanvasElement>()
const animationId = ref<number>()
const analyser = ref<AnalyserNode>()
const lastFrameTime = ref(0)
const dataArray = ref<Uint8Array>()
const resizeObserver = ref<ResizeObserver>()
const componentId = ref<string>(`visualizer-${Date.now()}-${Math.random()}`)

const controlAudio = ControlAudioStore()
const { Audio } = storeToRefs(controlAudio)

const initAudioAnalyser = () => {
  if (!Audio.value.audio) return
  try {
    const minSize = props.barCount * 2
    let fftSize = 32
    while (fftSize < minSize) fftSize *= 2
    fftSize = Math.min(fftSize, 2048)

    const createdAnalyser = audioManager.createAnalyser(
      Audio.value.audio,
      componentId.value,
      fftSize
    )
    analyser.value = createdAnalyser || undefined

    if (analyser.value) {
      const bufferLength = analyser.value.frequencyBinCount
      dataArray.value = new Uint8Array(new ArrayBuffer(bufferLength))
    } else {
      const bufferLength = fftSize / 2
      dataArray.value = new Uint8Array(new ArrayBuffer(bufferLength))
    }
  } catch (error) {
    console.error('音频分析器初始化失败:', error)
    dataArray.value = new Uint8Array(new ArrayBuffer(256))
  }
}

const draw = (ts?: number) => {
  if (!canvasRef.value || !analyser.value || !dataArray.value) return

  const now = ts ?? performance.now()
  if (now - lastFrameTime.value < 33) {
    animationId.value = requestAnimationFrame(draw)
    return
  }
  lastFrameTime.value = now

  const canvas = canvasRef.value
  const ctx = canvas.getContext('2d')
  if (!ctx) {
    animationId.value = requestAnimationFrame(draw)
    return
  }

  if (analyser.value && dataArray.value) {
    analyser.value.getByteFrequencyData(dataArray.value as Uint8Array)
  } else {
    const time = now * 0.001
    for (let i = 0; i < dataArray.value.length; i++) {
      const frequency = i / dataArray.value.length
      const amplitude = Math.sin(time * 2 + frequency * 10) * 0.5 + 0.5
      const bass = Math.sin(time * 4) * 0.3 + 0.7
      dataArray.value[i] = Math.floor(amplitude * bass * 255 * (1 - frequency * 0.7))
    }
  }

  // Low frequency event
  let lowFreqSum = 0
  const lowBins = Math.min(3, dataArray.value.length)
  for (let i = 0; i < lowBins; i++) lowFreqSum += dataArray.value[i]
  emit('lowFreqUpdate', lowFreqSum / lowBins / 255)

  ctx.clearRect(0, 0, canvas.width, canvas.height)

  if (props.backgroundColor !== 'transparent') {
    ctx.fillStyle = props.backgroundColor
    ctx.fillRect(0, 0, canvas.width, canvas.height)
  }

  const container = canvas.parentElement
  if (!container) {
    animationId.value = requestAnimationFrame(draw)
    return
  }
  const containerRect = container.getBoundingClientRect()
  const canvasWidth = containerRect.width
  const canvasHeight = props.height

  const halfBarCount = Math.floor(props.barCount / 2)
  const barWidth = canvasWidth / 2 / halfBarCount
  const maxBarHeight = canvasHeight * 0.9
  const centerX = canvasWidth / 2

  const gradient = ctx.createLinearGradient(0, canvasHeight, 0, 0)
  gradient.addColorStop(0, props.color)
  gradient.addColorStop(1, props.color.replace(/[\d\.]+\)$/g, '0.3)'))
  ctx.fillStyle = gradient

  // Symmetric spectrum
  for (let i = 0; i < halfBarCount; i++) {
    let barHeight = (dataArray.value[i] / 255) * maxBarHeight
    barHeight = Math.pow(barHeight / maxBarHeight, 0.6) * maxBarHeight
    const y = canvasHeight - barHeight

    const leftX = centerX - (i + 1) * barWidth
    ctx.fillRect(leftX, y, barWidth, barHeight)

    const rightX = centerX + i * barWidth
    ctx.fillRect(rightX, y, barWidth, barHeight)
  }

  if (props.show && Audio.value.isPlay) {
    animationId.value = requestAnimationFrame(draw)
  }
}

const startVisualization = () => {
  if (!props.show || !Audio.value.isPlay) return
  if (!analyser.value) initAudioAnalyser()
  draw()
}

const stopVisualization = () => {
  try {
    if (animationId.value) {
      cancelAnimationFrame(animationId.value)
      animationId.value = undefined
    }
  } catch (error) {
    console.warn('停止动画帧时出错:', error)
  }
}

watch(() => Audio.value.isPlay, (isPlaying) => {
  if (isPlaying && props.show) startVisualization()
  else stopVisualization()
})

watch(() => props.show, (show) => {
  if (show && Audio.value.isPlay) startVisualization()
  else stopVisualization()
})

// Watch for active audio element change (crossfade slot swap)
watch(() => Audio.value.audio, (newEl, oldEl) => {
  if (!newEl || newEl === oldEl) return
  try {
    stopVisualization()
    try { audioManager.removeAnalyser(componentId.value) } catch {}
    analyser.value = undefined
    initAudioAnalyser()
    if (props.show && Audio.value.isPlay) startVisualization()
  } catch (e) {
    console.warn('AudioVisualizer: 切换活跃 audio 元素失败:', e)
  }
})

const resizeCanvas = () => {
  if (!canvasRef.value) return
  const canvas = canvasRef.value
  const container = canvas.parentElement
  if (!container) return

  const containerRect = container.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  canvas.width = containerRect.width * dpr
  canvas.height = props.height * dpr
  canvas.style.width = containerRect.width + 'px'
  canvas.style.height = props.height + 'px'

  const ctx = canvas.getContext('2d')
  if (ctx) {
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.scale(dpr, dpr)
  }
}

onMounted(() => {
  if (canvasRef.value) {
    resizeCanvas()
    resizeObserver.value = new ResizeObserver(() => {
      nextTick(() => resizeCanvas())
    })
    const container = canvasRef.value.parentElement
    if (container) resizeObserver.value.observe(container)
  }
  if (Audio.value.audio && props.show && Audio.value.isPlay) {
    initAudioAnalyser()
    startVisualization()
  }
})

onBeforeUnmount(() => {
  stopVisualization()
  try {
    if (analyser.value) {
      analyser.value.disconnect()
      analyser.value = undefined
    }
    try { audioManager.removeAnalyser(componentId.value) } catch {}
  } catch (error) {
    console.warn('清理音频资源时出错:', error)
  }
  try {
    if (resizeObserver.value) {
      resizeObserver.value.disconnect()
      resizeObserver.value = undefined
    }
  } catch (error) {
    console.warn('断开 ResizeObserver 时出错:', error)
  }
  dataArray.value = undefined
})
</script>

<template>
  <div v-if="show" class="audio-visualizer" :style="{ height: `${height}px` }">
    <canvas ref="canvasRef" class="visualizer-canvas" :style="{ height: `${height}px` }" />
  </div>
</template>

<style lang="scss" scoped>
.audio-visualizer {
  width: 100%;
  position: relative;
  overflow: hidden;

  .visualizer-canvas {
    width: 100%;
    display: block;
    border-radius: 4px;
  }
}
</style>

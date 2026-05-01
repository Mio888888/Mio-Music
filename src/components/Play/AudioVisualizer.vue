<script lang="ts" setup>
import { ref, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { storeToRefs } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

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
const resizeObserver = ref<ResizeObserver>()

const controlAudio = ControlAudioStore()
const { Audio } = storeToRefs(controlAudio)

let unlisten: UnlistenFn | null = null
let animationId: number | undefined

// ---- 非 reactive 数据，避免 Vue 响应式追踪开销 ----
const BAND_COUNT = 64
const SMOOTHING = 0.7 // 越大越平滑（0 = 无平滑，1 = 极度平滑）
const targetBands = new Float64Array(BAND_COUNT)   // Rust 最新数据
const smoothedBands = new Float64Array(BAND_COUNT) // 插值平滑后的数据

// 预计算的绘制参数（只在 resize 时更新）
let drawWidth = 0
let drawHeight = 0
let barWidth = 0
let halfBars = 0
let centerX = 0
let maxBarH = 0

// dB → 归一化幅度（0~1），带感知加权
function dbToNorm(db: number): number {
  // 范围约 -80dB ~ +10dB，映射到 0~1
  const clamped = Math.max(-80, Math.min(10, db))
  const norm = (clamped + 80) / 90
  // gamma 校正让低幅度更明显
  return Math.pow(norm, 0.55)
}

const setupSpectrumListener = async () => {
  unlisten = await listen('player:spectrum', (event: any) => {
    const { bands } = event.payload
    if (!bands || !Array.isArray(bands)) return
    const len = Math.min(bands.length, BAND_COUNT)
    for (let i = 0; i < len; i++) {
      targetBands[i] = bands[i]
    }
  })
}

function updateSmoothed() {
  const inv = 1 - SMOOTHING
  for (let i = 0; i < BAND_COUNT; i++) {
    smoothedBands[i] = smoothedBands[i] * SMOOTHING + targetBands[i] * inv
  }
}

let cachedGradient: CanvasGradient | null = null
let lastColor = ''

function getGradient(ctx: CanvasRenderingContext2D): CanvasGradient | CanvasPattern {
  if (lastColor === props.color && cachedGradient) return cachedGradient
  const g = ctx.createLinearGradient(0, drawHeight, 0, 0)
  g.addColorStop(0, props.color)
  g.addColorStop(1, props.color.replace(/[\d.]+\)$/, '0.2)'))
  cachedGradient = g
  lastColor = props.color
  return g
}

function draw() {
  if (!canvasRef.value) return
  const canvas = canvasRef.value
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  updateSmoothed()

  // 清屏
  if (props.backgroundColor === 'transparent') {
    ctx.clearRect(0, 0, canvas.width, canvas.height)
  } else {
    ctx.fillStyle = props.backgroundColor
    ctx.fillRect(0, 0, canvas.width, canvas.height)
  }

  if (drawWidth === 0) {
    animationId = requestAnimationFrame(draw)
    return
  }

  ctx.fillStyle = getGradient(ctx)

  // 绘制频谱条
  for (let i = 0; i < halfBars; i++) {
    const norm = dbToNorm(smoothedBands[i])
    const barH = norm * maxBarH
    const y = drawHeight - barH

    // 左侧（镜像）
    const lx = centerX - (i + 1) * barWidth
    ctx.fillRect(lx, y, barWidth - 0.5, barH)

    // 右侧
    const rx = centerX + i * barWidth
    ctx.fillRect(rx, y, barWidth - 0.5, barH)
  }

  // 低频事件
  let lowSum = 0
  for (let i = 0; i < 3 && i < BAND_COUNT; i++) lowSum += dbToNorm(smoothedBands[i])
  emit('lowFreqUpdate', lowSum / 3)

  if (props.show && Audio.value.isPlay) {
    animationId = requestAnimationFrame(draw)
  }
}

function startVisualization() {
  if (!props.show || !Audio.value.isPlay) return
  if (animationId !== undefined) cancelAnimationFrame(animationId)
  animationId = requestAnimationFrame(draw)
}

function stopVisualization() {
  if (animationId !== undefined) {
    cancelAnimationFrame(animationId)
    animationId = undefined
  }
  // 清屏
  if (canvasRef.value) {
    const ctx = canvasRef.value.getContext('2d')
    if (ctx) ctx.clearRect(0, 0, canvasRef.value.width, canvasRef.value.height)
  }
}

watch(() => Audio.value.isPlay, (playing) => {
  if (playing && props.show) startVisualization()
  else stopVisualization()
})

watch(() => props.show, (show) => {
  if (show && Audio.value.isPlay) startVisualization()
  else stopVisualization()
})

watch(() => props.color, () => { cachedGradient = null })

function resizeCanvas() {
  if (!canvasRef.value) return
  const canvas = canvasRef.value
  const container = canvas.parentElement
  if (!container) return

  const rect = container.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  canvas.width = Math.round(rect.width * dpr)
  canvas.height = Math.round(props.height * dpr)
  canvas.style.width = rect.width + 'px'
  canvas.style.height = props.height + 'px'

  const ctx = canvas.getContext('2d')
  if (ctx) {
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.scale(dpr, dpr)
  }

  // 更新绘制参数缓存
  drawWidth = rect.width
  drawHeight = props.height
  halfBars = Math.floor(props.barCount / 2)
  barWidth = drawWidth / 2 / halfBars
  centerX = drawWidth / 2
  maxBarH = drawHeight * 0.92
  cachedGradient = null
}

onMounted(async () => {
  await setupSpectrumListener()

  if (canvasRef.value) {
    resizeCanvas()
    resizeObserver.value = new ResizeObserver(() => {
      nextTick(resizeCanvas)
    })
    const container = canvasRef.value.parentElement
    if (container) resizeObserver.value.observe(container)
  }

  if (props.show && Audio.value.isPlay) {
    startVisualization()
  }
})

onBeforeUnmount(() => {
  stopVisualization()
  if (unlisten) { unlisten(); unlisten = null }
  if (resizeObserver.value) {
    resizeObserver.value.disconnect()
    resizeObserver.value = undefined
  }
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

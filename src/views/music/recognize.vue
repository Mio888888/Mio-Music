<script setup lang="ts">
import { ref, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ControlAudioStore } from '@/store/ControlAudio'
import { MessagePlugin } from 'tdesign-vue-next'
import {
  MicrophoneIcon,
  StopCircleIcon,
  UploadIcon,
  PlayCircleIcon,
  SearchIcon,
  RefreshIcon,
  PlayIcon
} from 'tdesign-icons-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { playSong } from '@/utils/audio/globaPlayList'
import { searchValue } from '@/store/search'
import { useRouter } from 'vue-router'

const audioStore = ControlAudioStore()
const localUserStore = LocalUserDetailStore()
const searchStore = searchValue()
const router = useRouter()

const MAX_DURATION = 15
const SLICE_DURATION = 3000

const running = ref(false)
const status = ref('')
const currentDuration = ref(0)
const recognizedSongs = ref<any[]>([])
const wasPlaying = ref(false)

let unlistenChunk: UnlistenFn | null = null
let unlistenLevel: UnlistenFn | null = null
let timer: any = null

// --- Canvas visualizer ---
const canvasRef = ref<HTMLCanvasElement | null>(null)
let animFrame = 0
let audioLevel = 0
let smoothLevel = 0

const BAR_COUNT = 64
const barPhases = Float32Array.from({ length: BAR_COUNT }, () => Math.random() * Math.PI * 2)

function drawVisualizer() {
  const canvas = canvasRef.value
  if (!canvas) return

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const dpr = window.devicePixelRatio || 1
  const size = 280
  canvas.width = size * dpr
  canvas.height = size * dpr
  canvas.style.width = `${size}px`
  canvas.style.height = `${size}px`
  ctx.scale(dpr, dpr)

  const cx = size / 2
  const cy = size / 2
  const innerR = 52
  const maxBarH = 50

  ctx.clearRect(0, 0, size, size)

  // Smooth level interpolation
  smoothLevel += (audioLevel - smoothLevel) * 0.25

  const now = performance.now() / 1000

  // Draw circular spectrum bars
  for (let i = 0; i < BAR_COUNT; i++) {
    const angle = (i / BAR_COUNT) * Math.PI * 2 - Math.PI / 2
    const phase = barPhases[i]

    // Simulate spectrum: mix base pattern with audio level
    const baseH = 0.15 + 0.25 * Math.abs(Math.sin(phase + now * 1.2))
    const levelH = smoothLevel * (0.3 + 0.7 * Math.abs(Math.sin(phase * 2.3 + now * 0.8)))
    const h = baseH + levelH
    const barH = Math.max(2, h * maxBarH)

    const x1 = cx + Math.cos(angle) * (innerR + 4)
    const y1 = cy + Math.sin(angle) * (innerR + 4)
    const x2 = cx + Math.cos(angle) * (innerR + 4 + barH)
    const y2 = cy + Math.sin(angle) * (innerR + 4 + barH)

    const alpha = 0.3 + h * 0.7
    ctx.beginPath()
    ctx.moveTo(x1, y1)
    ctx.lineTo(x2, y2)
    ctx.strokeStyle = `rgba(var(--td-brand-color-rgb, 0, 82, 204), ${alpha})`
    ctx.lineWidth = 2.2
    ctx.lineCap = 'round'
    ctx.stroke()
  }

  // Inner glow ring
  if (smoothLevel > 0.05) {
    const glowAlpha = smoothLevel * 0.5
    ctx.beginPath()
    ctx.arc(cx, cy, innerR + 2, 0, Math.PI * 2)
    ctx.strokeStyle = `rgba(var(--td-brand-color-rgb, 0, 82, 204), ${glowAlpha})`
    ctx.lineWidth = 2
    ctx.shadowColor = `rgba(var(--td-brand-color-rgb, 0, 82, 204), 0.6)`
    ctx.shadowBlur = 15 * smoothLevel
    ctx.stroke()
    ctx.shadowBlur = 0
  }

  if (running.value) {
    animFrame = requestAnimationFrame(drawVisualizer)
  }
}

function startVisualizerLoop() {
  stopVisualizerLoop()
  animFrame = requestAnimationFrame(drawVisualizer)
}

function stopVisualizerLoop() {
  if (animFrame) {
    cancelAnimationFrame(animFrame)
    animFrame = 0
  }
}

// --- Script utilities ---
function loadScript(src: string) {
  return new Promise<void>((resolve, reject) => {
    const s = document.createElement('script')
    s.src = src
    s.onload = () => resolve()
    s.onerror = (e) => reject(e)
    document.head.appendChild(s)
  })
}

async function ensureAFP() {
  const g = window as any
  if (typeof g.GenerateFP === 'function') return
  if (!g.__afp_wasm_loaded__) {
    try {
      await loadScript('afp.wasm.js')
      g.__afp_wasm_loaded__ = true
    } catch {}
  }
  if (!g.__afp_runtime_loaded__) {
    try {
      await loadScript('afp.js')
      g.__afp_runtime_loaded__ = true
    } catch {}
  }
}

async function resampleTo8kMono(audioBuffer: AudioBuffer): Promise<Float32Array> {
  const ctx = new (window.OfflineAudioContext || (window as any).webkitOfflineAudioContext)(
    1,
    Math.floor(audioBuffer.duration * 8000),
    8000
  )
  const source = ctx.createBufferSource()
  source.buffer = audioBuffer
  source.connect(ctx.destination)
  source.start()
  const renderedBuffer = await ctx.startRendering()
  return renderedBuffer.getChannelData(0)
}

async function queryNetease(fp: string, duration: number) {
  const params = new URLSearchParams({
    sessionId: crypto.randomUUID?.() || Math.random().toString(36).slice(2),
    algorithmCode: 'shazam_v2',
    duration: String(duration),
    rawdata: fp,
    times: '1',
    decrypt: '1'
  })
  const url = `https://interface.music.163.com/api/music/audio/match?${params.toString()}`
  const resp = await fetch(url, { method: 'POST' })
  return resp.json()
}

function parseResult(resp: any): any[] {
  const list = resp?.data?.result
  if (!Array.isArray(list) || list.length === 0) return []
  return list.map((item: any) => ({
    songmid: String(item?.song?.id || ''),
    name: String(item?.song?.name || ''),
    singer: (item?.song?.artists || []).map((a: any) => a.name).join(' / '),
    albumName: String(item?.song?.album?.name || ''),
    img: (item?.song?.album?.cover || '').replace(/^http:/, 'https:') + '?param=100y100',
    startTime: item?.startTime || 0
  }))
}

async function start() {
  if (running.value) return

  try {
    status.value = 'initializing'
    await ensureAFP()

    if (audioStore.Audio.isPlay) {
      wasPlaying.value = true
      await audioStore.stop()
    } else {
      wasPlaying.value = false
    }

    running.value = true
    status.value = 'recording'
    currentDuration.value = 0
    recognizedSongs.value = []

    // Listen for audio level updates (~15fps)
    unlistenLevel = await listen<{ level: number }>('audio-capture:level', (event) => {
      audioLevel = event.payload.level
    })

    // Listen for PCM chunks
    unlistenChunk = await listen<{ data: number[]; sampleRate: number }>(
      'audio-capture:chunk',
      async (event) => {
        const pcm8k = new Float32Array(event.payload.data)
        await tryRecognizePCM(pcm8k)
      }
    )

    await invoke('audio_capture_start', { chunkDurationMs: SLICE_DURATION })

    await nextTick()
    startVisualizerLoop()

    const startTime = Date.now()
    timer = setInterval(() => {
      const elapsed = (Date.now() - startTime) / 1000
      currentDuration.value = Math.floor(elapsed)
      if (elapsed >= MAX_DURATION) {
        stopRecording(false)
      }
    }, 1000)
  } catch (err) {
    console.error('启动录音失败', err)
    MessagePlugin.error('启动录音失败，请检查麦克风权限')
    reset()
  }
}

async function tryRecognizePCM(pcm8k: Float32Array) {
  if (!running.value) return

  try {
    let hasSound = false
    for (let i = 0; i < pcm8k.length; i += 100) {
      if (Math.abs(pcm8k[i]) > 0.01) {
        hasSound = true
        break
      }
    }
    if (!hasSound) return

    const gen = (window as any).GenerateFP
    if (typeof gen !== 'function') return

    const fp = await gen(pcm8k)
    const duration = pcm8k.length / 8000
    const resp = await queryNetease(fp, duration)
    const result = parseResult(resp)

    if (result.length > 0) {
      recognizedSongs.value = result
      status.value = 'success'
      stopRecording(true)
    }
  } catch (e) {
    console.error('Recognition attempt failed', e)
  }
}

async function stopRecording(success: boolean = false) {
  if (!running.value) return

  try {
    await invoke('audio_capture_stop')
  } catch {}

  stopVisualizerLoop()
  audioLevel = 0
  smoothLevel = 0

  if (unlistenLevel) {
    unlistenLevel()
    unlistenLevel = null
  }
  if (unlistenChunk) {
    unlistenChunk()
    unlistenChunk = null
  }
  if (timer) {
    clearInterval(timer)
    timer = null
  }

  running.value = false

  if (!success) {
    status.value = 'failed'
  }

  if (wasPlaying.value) {
    setTimeout(() => {
      audioStore.start()
    }, 500)
    wasPlaying.value = false
  }
}

const fileInput = ref<HTMLInputElement | null>(null)

function triggerUpload() {
  fileInput.value?.click()
}

async function onFilePicked(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (running.value) return

  status.value = 'processing'
  running.value = true
  recognizedSongs.value = []

  try {
    await ensureAFP()
    const arrayBuffer = await file.arrayBuffer()
    const ctx = new (window.AudioContext || (window as any).webkitAudioContext)()

    try {
      const audioBuffer = await ctx.decodeAudioData(arrayBuffer)
      const pcm8k = await resampleTo8kMono(audioBuffer)
      const targetLength = MAX_DURATION * 8000
      const slice = new Float32Array(Math.min(pcm8k.length, targetLength))
      slice.set(pcm8k.subarray(0, slice.length))

      const gen = (window as any).GenerateFP
      if (typeof gen === 'function') {
        const fp = await gen(slice)
        const resp = await queryNetease(fp, slice.length / 8000)
        const result = parseResult(resp)
        if (result.length > 0) {
          recognizedSongs.value = result
          status.value = 'success'
        } else {
          status.value = 'failed'
          MessagePlugin.warning('未识别到歌曲')
        }
      }
    } finally {
      ctx.close()
    }
  } catch (e) {
    console.error('File recognition failed', e)
    status.value = 'failed'
    MessagePlugin.error('识别失败')
  } finally {
    running.value = false
  }
}

function reset() {
  running.value = false
  status.value = ''
  currentDuration.value = 0
  audioLevel = 0
  smoothLevel = 0

  stopVisualizerLoop()

  try {
    invoke('audio_capture_stop')
  } catch {}

  if (unlistenLevel) {
    unlistenLevel()
    unlistenLevel = null
  }
  if (unlistenChunk) {
    unlistenChunk()
    unlistenChunk = null
  }
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

function backToInitial() {
  reset()
  recognizedSongs.value = []
}

async function handlePlayResult(song: any) {
  if (!song) return
  localUserStore.addSongToFirst(song)
  await playSong(song)

  if (song.startTime && song.startTime > 0) {
    const seconds = song.startTime / 1000
    setTimeout(async () => {
      audioStore.setCurrentTime(seconds)
      try {
        await invoke('player__seek', { position: seconds })
      } catch {}
      MessagePlugin.success(`已跳转至识别片段: ${formatTime(seconds)}`)
    }, 500)
  }
}

function formatTime(seconds: number) {
  const m = Math.floor(seconds / 60)
  const s = Math.floor(seconds % 60)
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

function handleSearchResult(song: any) {
  if (!song) return
  searchStore.setValue(song.name)
  router.push({ name: 'search' })
}

onUnmounted(() => {
  reset()
})
</script>

<template>
  <div class="recognize-page">
    <div class="recognize-container">
      <div class="header">
        <h2>听歌识曲</h2>
        <p class="subtitle">识别电脑正在播放的声音或上传音频文件</p>
      </div>

      <div v-if="!recognizedSongs.length || running" class="visualizer-section">
        <div class="viz-wrapper" :class="{ 'is-active': running }">
          <canvas ref="canvasRef" class="viz-canvas"></canvas>
          <div class="icon-container">
            <MicrophoneIcon size="48px" />
          </div>
          <!-- Outer ripple waves -->
          <div class="wave w1"></div>
          <div class="wave w2"></div>
          <div class="wave w3"></div>
          <div class="wave w4"></div>
        </div>
      </div>

      <div class="status-display">
        <template v-if="running">
          <p class="status-text recording-text">
            <span class="recording-dot"></span>
            正在识别中... {{ currentDuration }}s / {{ MAX_DURATION }}s
          </p>
          <t-progress
            :percentage="(currentDuration / MAX_DURATION) * 100"
            size="small"
            :label="false"
            :color="'var(--td-brand-color)'"
          />
        </template>
        <template v-else-if="recognizedSongs.length > 0">
          <div class="result-list">
            <div
              v-for="(song, idx) in recognizedSongs"
              :key="song.songmid"
              class="result-item"
              :style="{ animationDelay: `${idx * 0.1}s` }"
            >
              <div class="result-cover-wrapper">
                <img :src="song.img" class="result-cover" />
              </div>
              <div class="result-content">
                <h3>{{ song.name }}</h3>
                <p>{{ song.singer }}</p>
                <div v-if="song.startTime > 0" class="result-meta">
                  <span>识别片段: {{ formatTime(song.startTime / 1000) }}</span>
                </div>
              </div>
              <div class="result-actions">
                <t-button theme="primary" shape="circle" @click="handlePlayResult(song)">
                  <template #icon><PlayCircleIcon /></template>
                </t-button>
                <t-button
                  theme="default"
                  variant="outline"
                  shape="circle"
                  @click="handleSearchResult(song)"
                >
                  <template #icon><SearchIcon /></template>
                </t-button>
              </div>
            </div>
          </div>
          <div class="result-footer">
            <t-button theme="primary" variant="text" @click="backToInitial">
              <template #icon><RefreshIcon /></template>
              继续识别
            </t-button>
          </div>
        </template>
        <template v-else-if="status === 'failed'">
          <div class="failed-section">
            <p class="status-text error">未能识别到歌曲</p>
            <t-button theme="default" variant="outline" size="small" @click="start">
              <template #icon><RefreshIcon /></template>
              重试
            </t-button>
          </div>
        </template>
        <template v-else>
          <p class="status-text">点击下方按钮开始</p>
        </template>
      </div>

      <div v-if="recognizedSongs.length === 0" class="actions">
        <t-button
          shape="circle"
          size="large"
          theme="primary"
          class="main-btn"
          :class="{ 'is-recording': running }"
          :loading="status === 'initializing' || status === 'processing'"
          @click="running ? stopRecording(false) : start()"
        >
          <template #icon>
            <StopCircleIcon v-if="running" size="36px" />
            <PlayIcon v-else size="36px" />
          </template>
        </t-button>

        <div class="sub-actions">
          <t-button variant="text" theme="default" :disabled="running" @click="triggerUpload">
            <template #icon><UploadIcon /></template>
            上传文件
          </t-button>
        </div>
        <input ref="fileInput" type="file" accept="audio/*" hidden @change="onFilePicked" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.recognize-page {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  overflow: hidden;
}

.recognize-container {
  width: 100%;
  height: 100%;
  padding: 24px;
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 20px;
  overflow: hidden;
  align-items: center;
}

.header h2 {
  font-size: 28px;
  font-weight: 700;
  margin-bottom: 6px;
  color: var(--td-text-color-primary);
  letter-spacing: -0.5px;
}

.subtitle {
  font-size: 14px;
  color: var(--td-text-color-secondary);
}

/* --- Visualizer --- */
.visualizer-section {
  display: flex;
  justify-content: center;
  padding: 8px 0;
  flex-shrink: 0;
}

.viz-wrapper {
  position: relative;
  width: 200px;
  height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.viz-canvas {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: 1;
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.5s ease;
}

.is-active .viz-canvas {
  opacity: 1;
}

.icon-container {
  position: relative;
  z-index: 10;
  width: 88px;
  height: 88px;
  background: var(--td-brand-color);
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: white;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.15);
  transition: all 0.6s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}

.is-active .icon-container {
  transform: scale(1.08);
  box-shadow:
    0 0 0 3px rgba(var(--td-brand-color-rgb, 0, 82, 204), 0.15),
    0 6px 30px rgba(var(--td-brand-color-rgb, 0, 82, 204), 0.35);
}

/* Ripple waves */
.wave {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 88px;
  height: 88px;
  border-radius: 50%;
  border: 1.5px solid var(--td-brand-color);
  opacity: 0;
  z-index: 0;
  pointer-events: none;
}

.is-active .wave {
  animation: ripple-out 3s ease-out infinite;
}
.is-active .w2 { animation-delay: 0.75s; }
.is-active .w3 { animation-delay: 1.5s; }
.is-active .w4 { animation-delay: 2.25s; }

@keyframes ripple-out {
  0% {
    width: 88px;
    height: 88px;
    opacity: 0.35;
  }
  100% {
    width: 200px;
    height: 200px;
    opacity: 0;
  }
}

/* --- Status --- */
.status-display {
  width: 100%;
  max-width: 600px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  gap: 12px;
  flex: 1;
  overflow-y: auto;
  padding: 0 4px;
}

.status-display::-webkit-scrollbar {
  width: 6px;
}
.status-display::-webkit-scrollbar-thumb {
  background-color: var(--td-scrollbar-color);
  border-radius: 3px;
}
.status-display::-webkit-scrollbar-track {
  background: transparent;
}

.status-text {
  font-size: 15px;
  color: var(--td-text-color-secondary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.recording-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--td-error-color);
  animation: pulse-dot 1.2s ease-in-out infinite;
}

@keyframes pulse-dot {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(0.8); }
}

.status-text.error {
  color: var(--td-error-color);
}

.failed-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  animation: fadeIn 0.4s ease;
}

/* --- Result list --- */
.result-list {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.result-item {
  display: flex;
  align-items: center;
  background: var(--td-bg-color-secondary);
  padding: 12px;
  border-radius: 12px;
  gap: 16px;
  transition: all 0.2s ease;
  animation: slideUp 0.45s cubic-bezier(0.22, 1, 0.36, 1) both;
}

.result-item:hover {
  background: var(--td-bg-color-component-hover);
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.06);
}

.result-cover-wrapper {
  width: 60px;
  height: 60px;
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;
}

.result-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.result-content {
  flex: 1;
  text-align: left;
  min-width: 0;
}

.result-content h3 {
  font-size: 16px;
  font-weight: 600;
  color: var(--td-text-color-primary);
  margin: 0 0 4px 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.result-content p {
  font-size: 14px;
  color: var(--td-text-color-secondary);
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.result-meta {
  margin-top: 4px;
  font-size: 12px;
  color: var(--td-brand-color);
  background: var(--td-brand-color-light);
  display: inline-block;
  padding: 2px 8px;
  border-radius: 4px;
}

.result-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(16px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

/* --- Actions --- */
.actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 20px;
  margin-top: auto;
  padding-bottom: 20px;
}

.main-btn {
  width: 72px;
  height: 72px;
  font-size: 36px;
  transition: all 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}

.main-btn.is-recording {
  animation: btn-breathe 2s ease-in-out infinite;
}

.main-btn:active {
  transform: scale(0.92);
}

@keyframes btn-breathe {
  0%, 100% { box-shadow: 0 0 0 0 rgba(var(--td-brand-color-rgb, 0, 82, 204), 0.3); }
  50% { box-shadow: 0 0 0 10px rgba(var(--td-brand-color-rgb, 0, 82, 204), 0); }
}

.sub-actions {
  display: flex;
  gap: 16px;
}

.result-footer {
  margin-top: 16px;
  display: flex;
  justify-content: center;
}
</style>

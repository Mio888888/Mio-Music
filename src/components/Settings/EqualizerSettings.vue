<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { useEqualizerStore } from '@/store/Equalizer'
import { ControlAudioStore } from '@/store/ControlAudio'
import AudioManager from '@/utils/audio/AudioManager'

const eqStore = useEqualizerStore()
const audioStore = ControlAudioStore()

const enabled = ref(eqStore.enabled)
const currentPreset = ref(eqStore.currentPreset)
const gains = ref<number[]>([...eqStore.gains])
const showSaveDialog = ref(false)
const newPresetName = ref('')

const builtInPresets = [
  { name: 'Flat', label: '原声', gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
  { name: 'Pop', label: '流行', gains: [-1, 4, 6, 7, 5, 0, -2, -2, -1, -1] },
  { name: 'Rock', label: '摇滚', gains: [5, 4, 3, 1, -1, -1, 0, 2, 3, 4] },
  { name: 'Jazz', label: '爵士', gains: [4, 3, 1, 2, -2, -2, 0, 1, 3, 4] },
  { name: 'Classical', label: '古典', gains: [5, 4, 3, 2, -1, -1, 0, 2, 3, 5] },
  { name: 'Bass Boost', label: '低音增强', gains: [8, 6, 4, 2, 0, 0, 0, 0, 0, 0] },
  { name: 'Vocal Boost', label: '人声增强', gains: [-2, -1, 0, 3, 6, 6, 3, 0, -1, -2] },
  { name: 'Treble Boost', label: '高音增强', gains: [0, 0, 0, 0, 0, 0, 2, 4, 6, 8] }
]

const allPresets = ref([...builtInPresets])
const frequencies = ['32', '64', '125', '250', '500', '1K', '2K', '4K', '8K', '16K']

const canvasRef = ref<HTMLCanvasElement>()
let animationId: number | null = null

const applyGains = () => {
  try {
    const audio = audioStore.Audio?.audio
    if (audio) {
      gains.value.forEach((gain, index) => {
        AudioManager.setEqualizerBand(audio, index, gain)
      })
    }
  } catch {}
}

const handlePresetChange = (val: any) => {
  const name = String(val)
  currentPreset.value = name
  const preset = allPresets.value.find(p => p.name === name)
  if (preset) {
    gains.value = [...preset.gains]
    eqStore.setGains([...gains.value])
    eqStore.setCurrentPreset(name)
    applyGains()
  }
}

const onGainChange = (index: number, val: number) => {
  gains.value[index] = val
  eqStore.setGains([...gains.value])
  applyGains()
}

const resetToCurrentPreset = () => {
  const preset = allPresets.value.find(p => p.name === currentPreset.value)
  if (preset) {
    gains.value = [...preset.gains]
    eqStore.setGains([...gains.value])
    applyGains()
  }
}

const saveNewPreset = () => {
  const name = newPresetName.value.trim()
  if (!name) return
  if (allPresets.value.some(p => p.name === name)) return
  allPresets.value.push({ name, label: name, gains: [...gains.value] })
  currentPreset.value = name
  eqStore.setCurrentPreset(name)
  showSaveDialog.value = false
  newPresetName.value = ''
}

const deleteCurrentPreset = () => {
  const builtInNames = builtInPresets.map(p => p.name)
  if (builtInNames.includes(currentPreset.value)) return
  const idx = allPresets.value.findIndex(p => p.name === currentPreset.value)
  if (idx !== -1) {
    allPresets.value.splice(idx, 1)
    currentPreset.value = 'Flat'
    gains.value = [...builtInPresets[0].gains]
    eqStore.setCurrentPreset('Flat')
    eqStore.setGains([...gains.value])
    applyGains()
  }
}

const exportConfig = () => {
  const config = { preset: currentPreset.value, gains: gains.value, customPresets: allPresets.value.filter(p => !builtInPresets.some(b => b.name === p.name)) }
  const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = `eq-preset-${currentPreset.value}.json`; a.click()
  URL.revokeObjectURL(url)
}

const handleFileImport = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] })
    if (selected) {
      const filePath = typeof selected === 'string' ? selected : (selected as any).path
      const { readFile } = await import('@tauri-apps/plugin-fs')
      const content = await readFile(filePath)
      const config = JSON.parse(new TextDecoder().decode(content))
      if (config.gains && Array.isArray(config.gains)) {
        gains.value = config.gains
        currentPreset.value = config.preset || 'Custom'
        eqStore.setGains([...gains.value])
        eqStore.setCurrentPreset(currentPreset.value)
        applyGains()
      }
    }
  } catch (e) { console.error('导入失败:', e) }
}

const startVisualization = () => {
  if (!canvasRef.value) return
  const ctx = canvasRef.value.getContext('2d')
  if (!ctx) return
  const draw = () => {
    if (!canvasRef.value) return
    const width = canvasRef.value.width
    const height = canvasRef.value.height
    ctx.clearRect(0, 0, width, height)
    const barCount = 32
    const barWidth = width / barCount - 2
    for (let i = 0; i < barCount; i++) {
      const value = Math.random() * 0.3 + 0.1
      const barHeight = value * height
      const x = i * (barWidth + 2)
      const gradient = ctx.createLinearGradient(x, height, x, height - barHeight)
      gradient.addColorStop(0, '#00DAC0')
      gradient.addColorStop(1, '#57b4ff')
      ctx.fillStyle = gradient
      ctx.fillRect(x, height - barHeight, barWidth, barHeight)
    }
    animationId = requestAnimationFrame(draw)
  }
  draw()
}

watch(enabled, (val) => { eqStore.setEnabled(val); if (val) applyGains() })

onMounted(() => { startVisualization() })
onUnmounted(() => { if (animationId) cancelAnimationFrame(animationId) })
</script>

<template>
  <div class="equalizer-settings">
    <div class="eq-header">
      <div class="eq-enable"><t-switch v-model="enabled" /><span>{{ enabled ? '均衡器已启用' : '均衡器已关闭' }}</span></div>
    </div>
    <div v-if="enabled" class="eq-content">
      <div class="visualization"><canvas ref="canvasRef" width="600" height="100" /></div>
      <div class="preset-section">
        <t-select :value="currentPreset" style="width: 200px;" @change="handlePresetChange">
          <t-option v-for="p in allPresets" :key="p.name" :value="p.name" :label="p.label" />
        </t-select>
        <div class="preset-actions">
          <t-button size="small" @click="resetToCurrentPreset">重置</t-button>
          <t-button size="small" theme="primary" @click="showSaveDialog = true">保存预设</t-button>
          <t-button size="small" variant="outline" @click="exportConfig">导出</t-button>
          <t-button size="small" variant="outline" @click="handleFileImport">导入</t-button>
          <t-button v-if="!builtInPresets.some(p => p.name === currentPreset)" size="small" theme="danger" variant="outline" @click="deleteCurrentPreset">删除</t-button>
        </div>
      </div>
      <div class="sliders-container">
        <div v-for="(freq, index) in frequencies" :key="index" class="slider-wrapper">
          <div class="gain-label">{{ gains[index] > 0 ? '+' : '' }}{{ gains[index].toFixed(1) }}</div>
          <input type="range" class="vertical-slider" :value="gains[index]" min="-12" max="12" step="0.1" orient="vertical" @input="(e) => onGainChange(index, Number((e.target as HTMLInputElement).value))" />
          <div class="freq-label">{{ freq }}</div>
        </div>
      </div>
    </div>
    <t-dialog v-model:visible="showSaveDialog" header="保存新预设" @confirm="saveNewPreset">
      <t-input v-model="newPresetName" placeholder="请输入预设名称" />
    </t-dialog>
  </div>
</template>

<style scoped>
.equalizer-settings { display: flex; flex-direction: column; gap: 1rem; }
.eq-header { display: flex; align-items: center; justify-content: space-between; }
.eq-enable { display: flex; align-items: center; gap: 0.5rem; }
.eq-content { display: flex; flex-direction: column; gap: 1rem; }
.visualization { background: var(--td-bg-color-page); border-radius: 0.5rem; overflow: hidden; border: 1px solid var(--td-border-level-1-color); canvas { width: 100%; height: 100px; display: block; } }
.preset-section { display: flex; align-items: center; gap: 1rem; flex-wrap: wrap; }
.preset-actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }
.sliders-container { display: flex; justify-content: space-around; align-items: flex-end; height: 250px; padding: 1rem 0; }
.slider-wrapper { display: flex; flex-direction: column; align-items: center; gap: 0.5rem; }
.gain-label { font-size: 0.75rem; color: var(--td-text-color-secondary); font-weight: 500; min-width: 3em; text-align: center; }
.freq-label { font-size: 0.75rem; color: var(--td-text-color-secondary); }
.vertical-slider { writing-mode: vertical-lr; direction: rtl; -webkit-appearance: slider-vertical; appearance: slider-vertical; width: 20px; height: 180px; }
</style>

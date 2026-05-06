<template>
  <div class="equalizer-settings">
    <t-card title="全局均衡器 (Professional EQ)" :bordered="false">
      <template #actions>
        <t-space>
          <t-switch v-model="enabled" :label="['开启', '关闭']" />
          <t-button theme="default" variant="text" @click="resetToCurrentPreset">重置</t-button>
        </t-space>
      </template>

      <div class="eq-content">
        <!-- Visualizer -->
        <div class="visualizer-container">
          <canvas ref="canvasRef" height="200"></canvas>
        </div>

        <!-- Preset Selector -->
        <div class="controls-row">
          <div class="preset-controls">
            <t-select
              v-model="currentPresetName"
              placeholder="选择预设"
              class="preset-select"
              @change="(val) => handlePresetChange(val as string)"
            >
              <t-option
                v-for="preset in presets"
                :key="preset.name"
                :label="preset.name"
                :value="preset.name"
              />
            </t-select>

            <t-button
              v-if="canDeleteCurrentPreset"
              theme="primary"
              variant="text"
              size="small"
              @click="saveCurrentToPreset"
            >
              <template #icon><SaveIcon /></template>
              保存
            </t-button>

            <t-button
              v-if="canDeleteCurrentPreset"
              theme="danger"
              variant="text"
              size="small"
              @click="confirmDeletePreset"
            >
              <template #icon><DeleteIcon /></template>
              删除
            </t-button>
          </div>

          <div class="action-buttons">
            <t-button theme="primary" variant="outline" @click="savePresetDialogVisible = true"
              >保存预设</t-button
            >
            <t-button theme="default" variant="outline" @click="exportConfig">导出配置</t-button>
            <t-button theme="default" variant="outline" @click="triggerImport">导入配置</t-button>
            <input
              ref="fileInputRef"
              type="file"
              accept=".json"
              style="display: none"
              @change="handleFileImport"
            />
          </div>
        </div>

        <!-- Sliders -->
        <div class="sliders-container">
          <div v-for="(freq, index) in frequencies" :key="index" class="slider-group">
            <div class="slider-wrapper">
              <t-slider
                v-model="gains[index]"
                :min="-12"
                :max="12"
                :step="0.1"
                layout="vertical"
                :show-tooltip="true"
                :disabled="!enabled"
                @change="(val) => onGainChange(index, val as number)"
              />
            </div>
            <span class="freq-label">{{ formatFreq(freq) }}</span>
            <span class="gain-label">{{ gains[index].toFixed(1) }}dB</span>
          </div>
        </div>
      </div>
    </t-card>

    <t-dialog
      v-model:visible="savePresetDialogVisible"
      header="保存为新预设"
      @confirm="saveNewPreset"
    >
      <t-input v-model="newPresetName" placeholder="输入预设名称" />
    </t-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useEqualizerStore } from '@/store/Equalizer'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { DeleteIcon, SaveIcon } from 'tdesign-icons-vue-next'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

const BUILTIN_PRESETS = [
  'Flat(原声)', 'Pop(流行)', 'Rock(摇滚)', 'Jazz(爵士)',
  'Classical(古典)', 'Bass Boost(低音增强)', 'Vocal Boost(人声增强)', 'Treble Boost(高音增强)'
]

const eqStore = useEqualizerStore()
const { enabled, currentPreset, gains, presets } = storeToRefs(eqStore)

const frequencies = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
const canvasRef = ref<HTMLCanvasElement | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)
const savePresetDialogVisible = ref(false)
const newPresetName = ref('')

let animationId: number
let unlisten: UnlistenFn | null = null
let spectrumData = new Array(64).fill(-80)

const formatFreq = (freq: number) => freq >= 1000 ? `${freq / 1000}k` : `${freq}`

const currentPresetName = computed({
  get: () => currentPreset.value,
  set: (val) => { currentPreset.value = val }
})

const canDeleteCurrentPreset = computed(() => {
  const reversedPresets = [...presets.value].reverse()
  const preset = reversedPresets.find((p) => p.name === currentPreset.value)
  return preset && preset.originalGains !== undefined
})

const confirmDeletePreset = () => {
  if (!canDeleteCurrentPreset.value) {
    MessagePlugin.warning('内置预设不能删除')
    return
  }
  const dialog = DialogPlugin.confirm({
    header: '删除预设',
    body: `确定要删除预设 "${currentPreset.value}" 吗？`,
    confirmBtn: { theme: 'danger', content: '删除' },
    onConfirm: () => { deleteCurrentPreset(); dialog.destroy() }
  })
}

const deleteCurrentPreset = () => {
  const presetName = currentPreset.value
  let index = -1
  for (let i = presets.value.length - 1; i >= 0; i--) {
    if (presets.value[i].name === presetName) { index = i; break }
  }
  if (index === -1) { MessagePlugin.error('预设不存在'); return }
  if (presets.value[index].originalGains === undefined) { MessagePlugin.warning('内置预设不能删除'); return }
  presets.value.splice(index, 1)
  currentPreset.value = 'Flat'
  handlePresetChange('Flat')
  MessagePlugin.success(`预设 "${presetName}" 已删除`)
  eqStore.addLog(`Deleted preset: ${presetName}`)
}

const saveCurrentToPreset = () => {
  const presetName = currentPreset.value
  if (BUILTIN_PRESETS.includes(presetName)) { MessagePlugin.warning('内置预设不能修改，请创建新预设'); return }
  const preset = presets.value.find((p) => p.name === presetName)
  if (!preset) { MessagePlugin.error('预设不存在'); return }
  preset.gains = [...gains.value]
  MessagePlugin.success(`已保存当前值到预设 "${presetName}"`)
  eqStore.addLog(`Updated preset "${presetName}" with current gains: ${gains.value.map((g) => g.toFixed(1)).join(', ')}`)
}

// 同步 EQ 到 Rust 后端
const applyGains = () => {
  const targetGains = enabled.value ? gains.value : new Array(10).fill(0)
  targetGains.forEach((gain, index) => {
    invoke('player__set_eq_band', { index, gain })
  })
}

watch([gains, enabled], () => { applyGains() }, { deep: true })

const handlePresetChange = (val: string) => {
  const preset = presets.value.find((p) => p.name === val)
  if (preset) {
    gains.value = [...preset.gains]
    eqStore.addLog(`Applied preset: ${val}`)
  }
}

const onGainChange = (index: number, val: number) => {
  eqStore.addLog(`Adjusted band ${frequencies[index]}Hz to ${val}dB`)
}

const resetToCurrentPreset = () => {
  const presetName = currentPreset.value
  if (BUILTIN_PRESETS.includes(presetName)) {
    const preset = presets.value.find((p) => p.name === presetName)
    if (preset) {
      gains.value = [...preset.gains]
      MessagePlugin.success(`已重置到 "${presetName}" 预设的原始值`)
      eqStore.addLog(`Reset to preset original values: ${presetName}`)
    }
  } else {
    const preset = presets.value.find((p) => p.name === presetName)
    if (preset && preset.originalGains) {
      gains.value = [...preset.originalGains]
      MessagePlugin.success(`已重置到 "${presetName}" 的初始值`)
      eqStore.addLog(`Reset custom preset "${presetName}" to original values`)
    } else {
      handlePresetChange('Flat')
      MessagePlugin.success('已重置到 Flat')
      eqStore.addLog(`Reset custom preset "${presetName}" to Flat`)
    }
  }
}

const saveNewPreset = () => {
  if (!newPresetName.value) return
  if (BUILTIN_PRESETS.includes(newPresetName.value)) { MessagePlugin.warning(`"${newPresetName.value}" 是内置预设名称，请使用其他名称`); return }
  if (presets.value.some((p) => p.name === newPresetName.value)) { MessagePlugin.warning(`预设 "${newPresetName.value}" 已存在`); return }
  const currentGains = [...gains.value]
  presets.value.push({ name: newPresetName.value, gains: currentGains, originalGains: currentGains })
  currentPreset.value = newPresetName.value
  savePresetDialogVisible.value = false
  newPresetName.value = ''
  MessagePlugin.success('预设保存成功')
  eqStore.addLog(`Saved new preset with gains: ${currentGains.map((g) => g.toFixed(1)).join(', ')}`)
}

const exportConfig = () => {
  const data = JSON.stringify({ presets: presets.value, currentPreset: currentPreset.value, gains: gains.value, enabled: enabled.value }, null, 2)
  const blob = new Blob([data], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = 'ceru-music-eq-config.json'; a.click()
  URL.revokeObjectURL(url)
  eqStore.addLog('Exported configuration')
}

const triggerImport = () => { fileInputRef.value?.click() }

const handleFileImport = async (event: Event) => {
  const input = event.target as HTMLInputElement
  if (!input.files?.length) return
  const file = input.files[0]
  try {
    const text = await file.text()
    const data = JSON.parse(text)
    if (data.presets) {
      presets.value = data.presets.map((preset: any) => {
        if (preset.basePreset && !preset.originalGains) {
          const basePreset = data.presets.find((p: any) => p.name === preset.basePreset)
          if (basePreset) return { ...preset, originalGains: [...basePreset.gains] }
        }
        return preset
      })
    }
    if (data.enabled !== undefined) enabled.value = data.enabled
    if (data.gains) gains.value = data.gains
    if (data.currentPreset) currentPreset.value = data.currentPreset
    MessagePlugin.success('配置导入成功')
    eqStore.addLog('Imported configuration')
  } catch (e) {
    MessagePlugin.error('导入失败，文件格式错误')
  }
  input.value = ''
}

const readThemeValue = (name: string, fallback: string): string => {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

// 可视化 — 使用 Rust 频谱事件
const resizeCanvas = () => {
  if (!canvasRef.value) return
  const container = canvasRef.value.parentElement
  if (!container) return
  const rect = container.getBoundingClientRect()
  canvasRef.value.width = Math.floor(rect.width)
}

const setupVisualizer = async () => {
  if (!canvasRef.value) return
  resizeCanvas()
  unlisten = await listen('player:spectrum', (event: any) => {
    const { bands } = event.payload
    if (bands && Array.isArray(bands)) spectrumData = bands
  })
  const ctx = canvasRef.value.getContext('2d')
  if (!ctx) return

  const draw = () => {
    if (!ctx || !canvasRef.value) return
    animationId = requestAnimationFrame(draw)

    const width = canvasRef.value.width
    const height = canvasRef.value.height

    ctx.fillStyle = readThemeValue('--settings-eq-visualizer-trail', 'rgba(0, 0, 0, 0.18)')
    ctx.fillRect(0, 0, width, height)

    const barStartColor = readThemeValue('--settings-eq-visualizer-bar-start', readThemeValue('--td-brand-color-5', '#00a74d'))
    const barEndColor = readThemeValue('--settings-eq-visualizer-bar-end', readThemeValue('--td-brand-color-7', '#03de6d'))

    const barCount = Math.min(spectrumData.length, 128)
    const barWidth = (width / barCount) * 2.5
    let x = 0

    for (let i = 0; i < barCount; i++) {
      const normalized = Math.max(0, Math.min(1, (spectrumData[i] + 80) / 80))
      const barHeight = Math.pow(normalized, 0.6) * (height / 2)

      const gradient = ctx.createLinearGradient(0, height, 0, 0)
      gradient.addColorStop(0, barStartColor)
      gradient.addColorStop(1, barEndColor)
      ctx.fillStyle = gradient
      ctx.fillRect(x, height - barHeight, barWidth, barHeight)
      x += barWidth + 1
    }
  }
  draw()
}

onMounted(() => {
  setupVisualizer()
  applyGains()
  window.addEventListener('resize', resizeCanvas)
})

onUnmounted(() => {
  if (animationId) cancelAnimationFrame(animationId)
  if (unlisten) { unlisten(); unlisten = null }
  window.removeEventListener('resize', resizeCanvas)
})
</script>

<style scoped>
.equalizer-settings {
  padding: 20px;
  color: var(--td-text-color-primary);
}
.eq-content {
  display: flex;
  flex-direction: column;
  gap: 20px;
}
.visualizer-container {
  width: 100%;
  background: var(--settings-eq-visualizer-bg, var(--td-bg-color-page));
  border-radius: 8px;
  overflow: hidden;
}
.visualizer-container canvas {
  width: 100%;
  height: auto;
  display: block;
}
.controls-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}
.preset-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.preset-select {
  width: 160px;
}
.action-buttons {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.sliders-container {
  display: flex;
  justify-content: space-between;
  height: 250px;
  padding: 20px 0;
}
.slider-group {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
}
.slider-wrapper {
  height: 180px;
  display: flex;
  justify-content: center;
}
.freq-label {
  margin-top: 10px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
}
.gain-label {
  margin-top: 4px;
  font-size: 10px;
  color: var(--td-text-color-secondary);
}
</style>

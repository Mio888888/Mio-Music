<template>
  <div class="equalizer-settings">
    <t-card :title="t('settings.equalizer.title')" :bordered="false">
      <template #actions>
        <t-space>
          <t-switch v-model="enabled" :label="[t('settings.equalizer.on'), t('settings.equalizer.off')]" />
          <t-button theme="default" variant="text" @click="resetToCurrentPreset">{{ t('settings.equalizer.reset') }}</t-button>
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
              :placeholder="t('settings.equalizer.selectPreset')"
              class="preset-select"
              @change="(val: unknown) => handlePresetChange(val as string)"
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
              {{ t('settings.equalizer.savePreset') }}
            </t-button>

            <t-button
              v-if="canDeleteCurrentPreset"
              theme="danger"
              variant="text"
              size="small"
              @click="confirmDeletePreset"
            >
              <template #icon><DeleteIcon /></template>
              {{ t('settings.equalizer.deletePreset') }}
            </t-button>
          </div>

          <div class="action-buttons">
            <t-button theme="primary" variant="outline" @click="savePresetDialogVisible = true"
              >{{ t('settings.equalizer.saveAsPreset') }}</t-button
            >
            <t-button theme="default" variant="outline" @click="exportConfig">{{ t('settings.equalizer.exportConfig') }}</t-button>
            <t-button theme="default" variant="outline" @click="triggerImport">{{ t('settings.equalizer.importConfig') }}</t-button>
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
          <div v-for="(freq, index) in frequencies" :key="freq" class="slider-group">
            <div class="slider-wrapper">
              <t-slider
                v-model="gains[index]"
                :min="-12"
                :max="12"
                :step="0.1"
                layout="vertical"
                :show-tooltip="true"
                :disabled="!enabled"
                @change="(val: number | number[]) => onGainChange(index, val as number)"
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
      :header="t('settings.equalizer.saveNewPresetTitle')"
      @confirm="saveNewPreset"
    >
      <t-input v-model="newPresetName" :placeholder="t('settings.equalizer.presetNamePlaceholder')" />
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
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const BUILTIN_PRESETS = computed(() => [
  t('settings.equalizer.presetFlat'), t('settings.equalizer.presetPop'), t('settings.equalizer.presetRock'), t('settings.equalizer.presetJazz'),
  t('settings.equalizer.presetClassical'), t('settings.equalizer.presetBassBoost'), t('settings.equalizer.presetVocalBoost'), t('settings.equalizer.presetTrebleBoost')
])

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
    MessagePlugin.warning(t('settings.equalizer.builtinNoDelete'))
    return
  }
  const dialog = DialogPlugin.confirm({
    header: t('settings.equalizer.deletePresetTitle'),
    body: t('settings.equalizer.deletePresetBody', { name: currentPreset.value }),
    confirmBtn: { theme: 'danger', content: t('settings.equalizer.deletePreset') },
    onConfirm: () => { deleteCurrentPreset(); dialog.destroy() }
  })
}

const deleteCurrentPreset = () => {
  const presetName = currentPreset.value
  let index = -1
  for (let i = presets.value.length - 1; i >= 0; i--) {
    if (presets.value[i].name === presetName) { index = i; break }
  }
  if (index === -1) { MessagePlugin.error(t('settings.equalizer.presetNotExist')); return }
  if (presets.value[index].originalGains === undefined) { MessagePlugin.warning(t('settings.equalizer.builtinNoDelete')); return }
  presets.value.splice(index, 1)
  currentPreset.value = 'Flat'
  handlePresetChange('Flat')
  MessagePlugin.success(t('settings.equalizer.presetDeleted', { name: presetName }))
  eqStore.addLog(`Deleted preset: ${presetName}`)
}

const saveCurrentToPreset = () => {
  const presetName = currentPreset.value
  if (BUILTIN_PRESETS.value.includes(presetName)) { MessagePlugin.warning(t('settings.equalizer.builtinNoModify')); return }
  const preset = presets.value.find((p) => p.name === presetName)
  if (!preset) { MessagePlugin.error(t('settings.equalizer.presetNotExist')); return }
  preset.gains = [...gains.value]
  MessagePlugin.success(t('settings.equalizer.savedToPreset', { name: presetName }))
  eqStore.addLog(`Updated preset "${presetName}" with current gains: ${gains.value.map((g) => g.toFixed(1)).join(', ')}`)
}

// 同步 EQ 到 Rust 后端
const applyGains = () => {
  const targetGains = enabled.value ? gains.value : new Array(10).fill(0)
  targetGains.forEach((gain, index) => {
    invoke('player__set_eq_band', { index, gain })
  })
}

watch([() => [...gains.value], enabled], () => { applyGains() })

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
  if (BUILTIN_PRESETS.value.includes(presetName)) {
    const preset = presets.value.find((p) => p.name === presetName)
    if (preset) {
      gains.value = [...preset.gains]
      MessagePlugin.success(t('settings.equalizer.resetToPreset', { name: presetName }))
      eqStore.addLog(`Reset to preset original values: ${presetName}`)
    }
  } else {
    const preset = presets.value.find((p) => p.name === presetName)
    if (preset && preset.originalGains) {
      gains.value = [...preset.originalGains]
      MessagePlugin.success(t('settings.equalizer.resetToInitial', { name: presetName }))
      eqStore.addLog(`Reset custom preset "${presetName}" to original values`)
    } else {
      handlePresetChange('Flat')
      MessagePlugin.success(t('settings.equalizer.resetToFlat'))
      eqStore.addLog(`Reset custom preset "${presetName}" to Flat`)
    }
  }
}

const saveNewPreset = () => {
  if (!newPresetName.value) return
  if (BUILTIN_PRESETS.value.includes(newPresetName.value)) { MessagePlugin.warning(t('settings.equalizer.builtinNameConflict', { name: newPresetName.value })); return }
  if (presets.value.some((p) => p.name === newPresetName.value)) { MessagePlugin.warning(t('settings.equalizer.presetExists', { name: newPresetName.value })); return }
  const currentGains = [...gains.value]
  presets.value.push({ name: newPresetName.value, gains: currentGains, originalGains: currentGains })
  currentPreset.value = newPresetName.value
  savePresetDialogVisible.value = false
  newPresetName.value = ''
  MessagePlugin.success(t('settings.equalizer.presetSaveSuccess'))
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
    MessagePlugin.success(t('settings.equalizer.importSuccess'))
    eqStore.addLog('Imported configuration')
  } catch (e) {
    MessagePlugin.error(t('settings.equalizer.importFailed'))
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

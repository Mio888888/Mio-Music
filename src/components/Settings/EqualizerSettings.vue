<template>
  <div class="equalizer-settings">
    <t-card :title="t('settings.equalizer.title')" :bordered="false">
      <template #actions>
        <t-space>
          <t-switch
            v-model="enabled"
            :label="[t('settings.equalizer.on'), t('settings.equalizer.off')]"
            @change="(val: unknown) => handleEnabledChange(Boolean(val))"
          />
          <t-button theme="default" variant="text" @click="resetToCurrentPreset">
            {{ t('settings.equalizer.reset') }}
          </t-button>
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
              v-model="selectedPresetId"
              :placeholder="t('settings.equalizer.selectPreset')"
              class="preset-select"
              @change="(val: unknown) => handlePresetChange(val as string)"
            >
              <t-option
                v-for="preset in presets"
                :key="preset.id"
                :label="displayPresetName(preset)"
                :value="preset.id"
              />
            </t-select>

            <t-button
              v-if="canModifyCurrentPreset"
              theme="primary"
              variant="text"
              size="small"
              @click="saveCurrentToPreset"
            >
              <template #icon><SaveIcon /></template>
              {{ t('settings.equalizer.savePreset') }}
            </t-button>

            <t-button
              v-if="canModifyCurrentPreset"
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
            <t-button theme="primary" variant="outline" @click="savePresetDialogVisible = true">
              {{ t('settings.equalizer.saveAsPreset') }}
            </t-button>
            <t-button theme="default" variant="outline" @click="exportConfig">
              {{ t('settings.equalizer.exportConfig') }}
            </t-button>
            <t-button theme="default" variant="outline" @click="triggerImport">
              {{ t('settings.equalizer.importConfig') }}
            </t-button>
            <input
              ref="fileInputRef"
              type="file"
              accept=".json"
              style="display: none"
              @change="handleFileImport"
            />
          </div>
        </div>

        <!-- Global Gain -->
        <div class="global-gain-control">
          <div class="global-gain-header">
            <span class="global-gain-title">{{ t('settings.equalizer.globalGain') }}</span>
            <span class="global-gain-value">{{ gains.global.toFixed(1) }}dB</span>
          </div>
          <t-slider
            v-model="gains.global"
            :min="EQ_GAIN_MIN"
            :max="EQ_GAIN_MAX"
            :step="0.1"
            :show-tooltip="true"
            :disabled="!enabled"
            @change="(val: number | number[]) => onGlobalGainChange(val as number)"
          />
        </div>

        <!-- Sliders -->
        <div class="sliders-container">
          <div v-for="(freq, index) in EQ_FREQUENCIES" :key="freq" class="slider-group">
            <div class="slider-wrapper">
              <t-slider
                v-model="gains.bands[index]"
                :min="EQ_GAIN_MIN"
                :max="EQ_GAIN_MAX"
                :step="0.1"
                layout="vertical"
                :show-tooltip="true"
                :disabled="!enabled"
                @change="(val: number | number[]) => onBandGainChange(index, val as number)"
              />
            </div>
            <span class="freq-label">{{ formatFreq(freq) }}</span>
            <span class="gain-label">{{ gains.bands[index].toFixed(1) }}dB</span>
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
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { DialogPlugin, MessagePlugin } from 'tdesign-vue-next'
import { DeleteIcon, SaveIcon } from 'tdesign-icons-vue-next'
import {
  EQ_FLAT_PRESET_ID,
  EQ_FREQUENCIES,
  EQ_GAIN_MAX,
  EQ_GAIN_MIN,
  type EqualizerPreset,
  useEqualizerStore
} from '@/store/Equalizer'

const { t } = useI18n()
const eqStore = useEqualizerStore()
const { enabled, currentPresetId, gains, presets, currentPresetDetail } = storeToRefs(eqStore)

const PRESET_LABEL_KEYS: Record<string, string> = {
  flat: 'settings.equalizer.presetFlat',
  pop: 'settings.equalizer.presetPop',
  rock: 'settings.equalizer.presetRock',
  jazz: 'settings.equalizer.presetJazz',
  classical: 'settings.equalizer.presetClassical',
  bass_boost: 'settings.equalizer.presetBassBoost',
  vocal_boost: 'settings.equalizer.presetVocalBoost',
  treble_boost: 'settings.equalizer.presetTrebleBoost'
}

const SPECTRUM_BAND_COUNT = 128
const SILENCE_DB = -80

interface SpectrumPayload {
  bands?: unknown
}

const canvasRef = ref<HTMLCanvasElement | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)
const savePresetDialogVisible = ref(false)
const newPresetName = ref('')

let animationId: number
let unlisten: UnlistenFn | null = null
let spectrumData = new Array<number>(SPECTRUM_BAND_COUNT).fill(SILENCE_DB)

const formatFreq = (freq: number) => freq >= 1000 ? `${freq / 1000}k` : `${freq}`

const displayPresetName = (preset: EqualizerPreset) => {
  const key = PRESET_LABEL_KEYS[preset.id]
  return preset.isDefault && key ? t(key) : preset.name
}

const currentPresetDisplayName = computed(() => {
  const preset = currentPresetDetail.value
  return preset ? displayPresetName(preset) : t(PRESET_LABEL_KEYS[EQ_FLAT_PRESET_ID])
})

const selectedPresetId = computed({
  get: () => currentPresetId.value,
  set: (val: string) => {
    currentPresetId.value = val
  }
})

const canModifyCurrentPreset = computed(() => {
  const preset = currentPresetDetail.value
  return Boolean(preset && !preset.isDefault)
})

const localizedDefaultPresetNames = computed(() => new Set(
  presets.value
    .filter((preset) => preset.isDefault)
    .map((preset) => displayPresetName(preset).trim().toLocaleLowerCase())
))

const handleEnabledChange = (val: boolean) => {
  eqStore.setEnabled(val)
}

const handlePresetChange = (val: string) => {
  if (!eqStore.applyPreset(val)) {
    MessagePlugin.error(t('settings.equalizer.presetNotExist'))
  }
}

const confirmDeletePreset = () => {
  if (!canModifyCurrentPreset.value) {
    MessagePlugin.warning(t('settings.equalizer.builtinNoDelete'))
    return
  }

  const dialog = DialogPlugin.confirm({
    header: t('settings.equalizer.deletePresetTitle'),
    body: t('settings.equalizer.deletePresetBody', { name: currentPresetDisplayName.value }),
    confirmBtn: { theme: 'danger', content: t('settings.equalizer.deletePreset') },
    onConfirm: () => {
      deleteCurrentPreset()
      dialog.destroy()
    }
  })
}

const deleteCurrentPreset = () => {
  const presetName = currentPresetDisplayName.value
  const result = eqStore.deleteUserPreset(currentPresetId.value)
  if (!result.success) {
    MessagePlugin.warning(result.error || t('settings.equalizer.builtinNoDelete'))
    return
  }
  MessagePlugin.success(t('settings.equalizer.presetDeleted', { name: presetName }))
}

const saveCurrentToPreset = () => {
  const presetName = currentPresetDisplayName.value
  const result = eqStore.updateUserPreset(currentPresetId.value)
  if (!result.success) {
    MessagePlugin.warning(result.error || t('settings.equalizer.builtinNoModify'))
    return
  }
  MessagePlugin.success(t('settings.equalizer.savedToPreset', { name: presetName }))
}

const onGlobalGainChange = (val: number) => {
  eqStore.setGlobalGain(val)
}

const onBandGainChange = (index: number, val: number) => {
  eqStore.setBandGain(index, val)
}

const resetToCurrentPreset = () => {
  if (eqStore.resetToCurrentPreset()) {
    MessagePlugin.success(t('settings.equalizer.resetToPreset', { name: currentPresetDisplayName.value }))
    return
  }

  eqStore.applyPreset(EQ_FLAT_PRESET_ID)
  MessagePlugin.success(t('settings.equalizer.resetToFlat'))
}

const saveNewPreset = () => {
  const name = newPresetName.value.trim()
  if (!name) return

  if (localizedDefaultPresetNames.value.has(name.toLocaleLowerCase())) {
    MessagePlugin.warning(t('settings.equalizer.builtinNameConflict', { name }))
    return
  }

  const result = eqStore.createUserPreset(name)
  if (!result.success) {
    MessagePlugin.warning(result.error || t('settings.equalizer.presetExists', { name }))
    return
  }

  savePresetDialogVisible.value = false
  newPresetName.value = ''
  MessagePlugin.success(t('settings.equalizer.presetSaveSuccess'))
}

const exportConfig = () => {
  const data = JSON.stringify(eqStore.exportConfig(), null, 2)
  const blob = new Blob([data], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'mio-eq-config.json'
  a.click()
  URL.revokeObjectURL(url)
  eqStore.addLog('Exported configuration')
}

const triggerImport = () => {
  fileInputRef.value?.click()
}

const handleFileImport = async (event: Event) => {
  const input = event.target as HTMLInputElement
  if (!input.files?.length) return

  const file = input.files[0]
  try {
    const text = await file.text()
    const data = JSON.parse(text)
    const result = eqStore.importConfig(data)
    if (!result.success) {
      MessagePlugin.error(result.error || t('settings.equalizer.importFailed'))
      return
    }
    MessagePlugin.success(t('settings.equalizer.importSuccess'))
  } catch (error) {
    MessagePlugin.error(t('settings.equalizer.importFailed'))
  } finally {
    input.value = ''
  }
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
  unlisten = await listen<SpectrumPayload>('player:spectrum', (event) => {
    const { bands } = event.payload
    if (!Array.isArray(bands)) return

    const nextSpectrumData = new Array<number>(SPECTRUM_BAND_COUNT).fill(SILENCE_DB)
    const len = Math.min(bands.length, SPECTRUM_BAND_COUNT)
    for (let i = 0; i < len; i++) {
      const band = bands[i]
      nextSpectrumData[i] = typeof band === 'number' && Number.isFinite(band) ? band : SILENCE_DB
    }
    spectrumData = nextSpectrumData
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

    const barCount = SPECTRUM_BAND_COUNT
    const barWidth = (width / barCount) * 2.5
    let x = 0

    for (let i = 0; i < barCount; i++) {
      const normalized = Math.max(0, Math.min(1, (spectrumData[i] - SILENCE_DB) / 80))
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
  window.addEventListener('resize', resizeCanvas)
})

onUnmounted(() => {
  if (animationId) cancelAnimationFrame(animationId)
  if (unlisten) {
    unlisten()
    unlisten = null
  }
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
  width: 180px;
}
.action-buttons {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.global-gain-control {
  padding: 12px 16px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--td-bg-color-container), var(--td-brand-color) 5%);
  border: 1px solid var(--td-border-level-1-color);
}
.global-gain-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
}
.global-gain-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--td-text-color-primary);
}
.global-gain-value {
  font-size: 12px;
  color: var(--td-text-color-secondary);
}
.sliders-container {
  display: flex;
  justify-content: space-between;
  height: 280px;
  padding: 20px 0;
}
.slider-group {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
}
.slider-wrapper {
  height: 210px;
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

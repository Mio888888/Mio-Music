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
        <!-- Frequency Response Curve Canvas -->
        <div class="visualizer-container" ref="canvasContainerRef">
          <canvas ref="canvasRef"></canvas>
        </div>

        <!-- Band Gain Sliders -->
        <div class="sliders-container">
          <div
            v-for="(band, index) in gains.bands"
            :key="index"
            class="slider-group"
            :class="{ 'slider-group--selected': selectedBandIndex === index }"
            @click="selectedBandIndex = index"
          >
            <div class="slider-wrapper">
              <t-slider
                :value="band.gain"
                :min="EQ_GAIN_MIN"
                :max="EQ_GAIN_MAX"
                :step="0.1"
                layout="vertical"
                :show-tooltip="true"
                :disabled="!enabled || isGainDisabled(band.type)"
                @change="(val: number | number[]) => onBandGainChange(index, val as number)"
              />
            </div>
            <span class="freq-label">{{ formatFreq(band.frequency) }}</span>
            <span class="gain-label">{{ isGainDisabled(band.type) ? '--' : `${band.gain.toFixed(1)}dB` }}</span>
          </div>
        </div>

        <!-- Selected Band Parameter Panel -->
        <div v-if="selectedBandIndex >= 0 && selectedBandIndex < gains.bands.length" class="band-params-panel">
          <div class="band-params-header">
            <span class="band-params-title">{{ t('settings.equalizer.bandParams', { index: selectedBandIndex + 1 }) }}</span>
            <span class="band-params-freq">{{ formatFreq(gains.bands[selectedBandIndex].frequency) }}</span>
          </div>
          <div class="band-params-body">
            <div class="param-row">
              <label class="param-label">{{ t('settings.equalizer.frequency') }}</label>
              <div class="param-control">
                <input
                  type="range"
                  class="param-slider"
                  :min="0"
                  :max="1"
                  :step="0.001"
                  :value="freqToSliderValue(gains.bands[selectedBandIndex].frequency)"
                  :disabled="!enabled"
                  @input="onFreqSliderChange"
                />
              </div>
              <span class="param-value">{{ formatFreq(gains.bands[selectedBandIndex].frequency) }}</span>
            </div>
            <div class="param-row">
              <label class="param-label">{{ t('settings.equalizer.qFactor') }}</label>
              <div class="param-control">
                <input
                  type="range"
                  class="param-slider"
                  :min="EQ_Q_MIN"
                  :max="EQ_Q_MAX"
                  :step="0.01"
                  :value="gains.bands[selectedBandIndex].q"
                  :disabled="!enabled"
                  @input="onQSliderChange"
                />
              </div>
              <span class="param-value">{{ gains.bands[selectedBandIndex].q.toFixed(2) }}</span>
            </div>
            <div class="param-row">
              <label class="param-label">{{ t('settings.equalizer.filterType') }}</label>
              <div class="param-control">
                <t-select
                  :value="gains.bands[selectedBandIndex].type"
                  :disabled="!enabled"
                  size="small"
                  @change="(val: unknown) => onTypeChange(val as FilterType)"
                >
                  <t-option
                    v-for="ft in ALL_FILTER_TYPES"
                    :key="ft"
                    :value="ft"
                    :label="filterTypeLabel(ft)"
                  />
                </t-select>
              </div>
            </div>
          </div>
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
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { DialogPlugin, MessagePlugin } from 'tdesign-vue-next'
import { DeleteIcon, SaveIcon } from 'tdesign-icons-vue-next'
import {
  ALL_FILTER_TYPES,
  EQ_FLAT_PRESET_ID,
  EQ_FREQ_MAX,
  EQ_FREQ_MIN,
  EQ_GAIN_MAX,
  EQ_GAIN_MIN,
  EQ_Q_MAX,
  EQ_Q_MIN,
  type EqualizerPreset,
  type FilterType,
  useEqualizerStore
} from '@/store/Equalizer'

const { t } = useI18n()
const eqStore = useEqualizerStore()
const { enabled, currentPresetId, gains, presets, currentPresetDetail } = storeToRefs(eqStore)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SAMPLE_RATE = 44100
const CANVAS_HEIGHT = 250
const SPECTRUM_BAND_COUNT = 128
const SILENCE_DB = -80
const RESPONSE_POINTS = 300

const GRID_FREQS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]
const GRID_DB = [-24, -18, -12, -6, 0, 6, 12, 18, 24]

const FILTER_TYPE_I18N_KEYS: Record<FilterType, string> = {
  peak: 'settings.equalizer.typePeak',
  lowshelf: 'settings.equalizer.typeLowShelf',
  highshelf: 'settings.equalizer.typeHighShelf',
  lowpass: 'settings.equalizer.typeLowPass',
  highpass: 'settings.equalizer.typeHighPass',
  notch: 'settings.equalizer.typeNotch',
}

const GAIN_DISABLED_TYPES = new Set<string>(['lowpass', 'highpass', 'notch'])

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

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

const canvasRef = ref<HTMLCanvasElement | null>(null)
const canvasContainerRef = ref<HTMLDivElement | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)
const savePresetDialogVisible = ref(false)
const newPresetName = ref('')
const selectedBandIndex = ref(-1)

let animationId = 0
let unlisten: UnlistenFn | null = null
let spectrumData = new Array<number>(SPECTRUM_BAND_COUNT).fill(SILENCE_DB)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const formatFreq = (freq: number) => freq >= 1000 ? `${freq / 1000}k` : `${freq}`

const isGainDisabled = (type: FilterType) => GAIN_DISABLED_TYPES.has(type)

const filterTypeLabel = (type: FilterType) => t(FILTER_TYPE_I18N_KEYS[type])

const freqToSliderValue = (freq: number): number => {
  return Math.log10(freq / EQ_FREQ_MIN) / Math.log10(EQ_FREQ_MAX / EQ_FREQ_MIN)
}

const sliderValueToFreq = (val: number): number => {
  return EQ_FREQ_MIN * Math.pow(EQ_FREQ_MAX / EQ_FREQ_MIN, val)
}

// ---------------------------------------------------------------------------
// Frequency / dB mapping
// ---------------------------------------------------------------------------

const freqToX = (freq: number, width: number): number => {
  const minLog = Math.log10(20)
  const maxLog = Math.log10(20000)
  return ((Math.log10(freq) - minLog) / (maxLog - minLog)) * width
}

const dbToY = (db: number, height: number): number => {
  return ((24 - db) / 48) * height
}

// ---------------------------------------------------------------------------
// Biquad math
// ---------------------------------------------------------------------------

interface BiquadCoeffs {
  b0: number
  b1: number
  b2: number
  a0: number
  a1: number
  a2: number
}

function calcBiquadCoeffs(
  type: FilterType,
  freq: number,
  gainDb: number,
  q: number,
  sampleRate: number
): BiquadCoeffs {
  const A = Math.pow(10, gainDb / 40)
  const omega = 2 * Math.PI * freq / sampleRate
  const cosW = Math.cos(omega)
  const sinW = Math.sin(omega)
  const alpha = sinW / (2 * q)

  switch (type) {
    case 'peak': {
      const b0 = 1 + alpha * A
      const b1 = -2 * cosW
      const b2 = 1 - alpha * A
      const a0 = 1 + alpha / A
      const a1 = -2 * cosW
      const a2 = 1 - alpha / A
      return { b0, b1, b2, a0, a1, a2 }
    }
    case 'lowshelf': {
      const sqrtA = Math.sqrt(A)
      const b0 = A * ((A + 1) - (A - 1) * cosW + 2 * sqrtA * alpha)
      const b1 = 2 * A * ((A - 1) - (A + 1) * cosW)
      const b2 = A * ((A + 1) - (A - 1) * cosW - 2 * sqrtA * alpha)
      const a0 = (A + 1) + (A - 1) * cosW + 2 * sqrtA * alpha
      const a1 = -2 * ((A - 1) + (A + 1) * cosW)
      const a2 = (A + 1) + (A - 1) * cosW - 2 * sqrtA * alpha
      return { b0, b1, b2, a0, a1, a2 }
    }
    case 'highshelf': {
      const sqrtA = Math.sqrt(A)
      const b0 = A * ((A + 1) + (A - 1) * cosW + 2 * sqrtA * alpha)
      const b1 = -2 * A * ((A - 1) + (A + 1) * cosW)
      const b2 = A * ((A + 1) + (A - 1) * cosW - 2 * sqrtA * alpha)
      const a0 = (A + 1) - (A - 1) * cosW + 2 * sqrtA * alpha
      const a1 = 2 * ((A - 1) - (A + 1) * cosW)
      const a2 = (A + 1) - (A - 1) * cosW - 2 * sqrtA * alpha
      return { b0, b1, b2, a0, a1, a2 }
    }
    case 'lowpass': {
      const b0 = (1 - cosW) / 2
      const b1 = 1 - cosW
      const b2 = (1 - cosW) / 2
      const a0 = 1 + alpha
      const a1 = -2 * cosW
      const a2 = 1 - alpha
      return { b0, b1, b2, a0, a1, a2 }
    }
    case 'highpass': {
      const b0 = (1 + cosW) / 2
      const b1 = -(1 + cosW)
      const b2 = (1 + cosW) / 2
      const a0 = 1 + alpha
      const a1 = -2 * cosW
      const a2 = 1 - alpha
      return { b0, b1, b2, a0, a1, a2 }
    }
    case 'notch': {
      const b0 = 1
      const b1 = -2 * cosW
      const b2 = 1
      const a0 = 1 + alpha
      const a1 = -2 * cosW
      const a2 = 1 - alpha
      return { b0, b1, b2, a0, a1, a2 }
    }
  }
}

function calcFilterResponseDb(
  coeffs: BiquadCoeffs,
  freq: number,
  sampleRate: number
): number {
  const omega = 2 * Math.PI * freq / sampleRate
  const cosW = Math.cos(omega)
  const sinW = Math.sin(omega)
  const { b0, b1, b2, a0, a1, a2 } = coeffs
  const cos2W = 2 * cosW * cosW - 1
  const realNum = b0 + b1 * cosW + b2 * cos2W
  const imagNum = -(b1 * sinW + b2 * 2 * sinW * cosW)
  const realDen = a0 + a1 * cosW + a2 * cos2W
  const imagDen = -(a1 * sinW + a2 * 2 * sinW * cosW)
  const magNum = Math.sqrt(realNum * realNum + imagNum * imagNum)
  const magDen = Math.sqrt(realDen * realDen + imagDen * imagDen)
  return 20 * Math.log10(magNum / Math.max(magDen, 1e-10))
}

// Pre-compute log-spaced frequency points (20Hz - 20kHz)
const freqPoints = (() => {
  const points: number[] = []
  const minLog = Math.log10(20)
  const maxLog = Math.log10(20000)
  for (let i = 0; i < RESPONSE_POINTS; i++) {
    const logFreq = minLog + (i / (RESPONSE_POINTS - 1)) * (maxLog - minLog)
    points.push(Math.pow(10, logFreq))
  }
  return points
})()

function calcTotalResponseDb(bands: { frequency: number; gain: number; q: number; type: FilterType; enabled: boolean }[], globalGain: number): number[] {
  return freqPoints.map(freq => {
    let totalDb = 0
    for (const band of bands) {
      if (!band.enabled) continue
      const coeffs = calcBiquadCoeffs(band.type, band.frequency, band.gain, band.q, SAMPLE_RATE)
      totalDb += calcFilterResponseDb(coeffs, freq, SAMPLE_RATE)
    }
    return totalDb + globalGain
  })
}

// ---------------------------------------------------------------------------
// Theme helpers
// ---------------------------------------------------------------------------

const readThemeValue = (name: string, fallback: string): string => {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

// ---------------------------------------------------------------------------
// Preset / display
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Event handlers
// ---------------------------------------------------------------------------

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

const onFreqSliderChange = (event: Event) => {
  const input = event.target as HTMLInputElement
  const freq = sliderValueToFreq(parseFloat(input.value))
  eqStore.setBandFrequency(selectedBandIndex.value, freq)
}

const onQSliderChange = (event: Event) => {
  const input = event.target as HTMLInputElement
  eqStore.setBandQ(selectedBandIndex.value, parseFloat(input.value))
}

const onTypeChange = (type: FilterType) => {
  eqStore.setBandType(selectedBandIndex.value, type)
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

// ---------------------------------------------------------------------------
// Spectrum listener
// ---------------------------------------------------------------------------

interface SpectrumPayload {
  bands?: unknown
}

// ---------------------------------------------------------------------------
// Canvas rendering
// ---------------------------------------------------------------------------

const resizeCanvas = () => {
  if (!canvasRef.value || !canvasContainerRef.value) return
  const rect = canvasContainerRef.value.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  canvasRef.value.width = Math.floor(rect.width * dpr)
  canvasRef.value.height = Math.floor(CANVAS_HEIGHT * dpr)
  canvasRef.value.style.width = `${rect.width}px`
  canvasRef.value.style.height = `${CANVAS_HEIGHT}px`
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

    const dpr = window.devicePixelRatio || 1
    const width = canvasRef.value.width / dpr
    const height = canvasRef.value.height / dpr

    ctx.save()
    ctx.scale(dpr, dpr)

    // --- Layer 0: Background + Grid ---
    drawBackground(ctx, width, height)

    // --- Layer 1: Spectrum Analyzer ---
    drawSpectrum(ctx, width, height)

    // --- Layer 2: EQ Response Curve ---
    drawEQCurve(ctx, width, height)

    // --- Layer 3: Band Markers ---
    drawBandMarkers(ctx, width, height)

    ctx.restore()
  }
  draw()
}

function drawBackground(ctx: CanvasRenderingContext2D, width: number, height: number) {
  // Background
  const bgColor = readThemeValue('--settings-eq-visualizer-bg', readThemeValue('--td-bg-color-page', '#1a1a1a'))
  ctx.fillStyle = bgColor
  ctx.fillRect(0, 0, width, height)

  const gridColor = 'rgba(255,255,255,0.06)'
  const zeroLineColor = 'rgba(255,255,255,0.12)'
  const labelColor = 'rgba(255,255,255,0.35)'

  // Vertical grid lines (frequency)
  for (const freq of GRID_FREQS) {
    const x = freqToX(freq, width)
    ctx.beginPath()
    ctx.moveTo(x, 0)
    ctx.lineTo(x, height)
    ctx.strokeStyle = gridColor
    ctx.lineWidth = 1
    ctx.stroke()

    // Frequency label at bottom
    ctx.fillStyle = labelColor
    ctx.font = '10px system-ui, sans-serif'
    ctx.textAlign = 'center'
    ctx.fillText(formatFreq(freq), x, height - 4)
  }

  // Horizontal grid lines (dB)
  for (const db of GRID_DB) {
    const y = dbToY(db, height)
    ctx.beginPath()
    ctx.moveTo(0, y)
    ctx.lineTo(width, y)
    ctx.strokeStyle = db === 0 ? zeroLineColor : gridColor
    ctx.lineWidth = db === 0 ? 1 : 0.5
    ctx.stroke()

    // dB label on left
    if (db % 6 === 0) {
      ctx.fillStyle = labelColor
      ctx.font = '9px system-ui, sans-serif'
      ctx.textAlign = 'left'
      ctx.fillText(`${db}`, 4, y - 2)
    }
  }
}

function drawSpectrum(ctx: CanvasRenderingContext2D, width: number, height: number) {
  const barCount = SPECTRUM_BAND_COUNT
  const barWidth = width / barCount

  const barStartColor = readThemeValue('--settings-eq-visualizer-bar-start', readThemeValue('--td-brand-color-5', 'rgba(0,167,77,0.15)'))
  const barEndColor = readThemeValue('--settings-eq-visualizer-bar-end', readThemeValue('--td-brand-color-7', 'rgba(3,222,109,0.25)'))

  for (let i = 0; i < barCount; i++) {
    const normalized = Math.max(0, Math.min(1, (spectrumData[i] - SILENCE_DB) / 80))
    const barHeight = Math.pow(normalized, 0.6) * (height / 2)
    if (barHeight < 1) continue

    const x = i * barWidth
    const gradient = ctx.createLinearGradient(0, height, 0, height - barHeight)
    gradient.addColorStop(0, barStartColor)
    gradient.addColorStop(1, barEndColor)
    ctx.fillStyle = gradient
    ctx.fillRect(x, height - barHeight, barWidth - 1, barHeight)
  }
}

function drawEQCurve(ctx: CanvasRenderingContext2D, width: number, height: number) {
  const response = calcTotalResponseDb(gains.value.bands, gains.value.global)

  // Curve color
  const curveColor = readThemeValue('--td-brand-color', '#00a74d')

  // Build path
  ctx.beginPath()
  let started = false
  for (let i = 0; i < freqPoints.length; i++) {
    const x = freqToX(freqPoints[i], width)
    const clampedDb = Math.max(-24, Math.min(24, response[i]))
    const y = dbToY(clampedDb, height)
    if (!started) {
      ctx.moveTo(x, y)
      started = true
    } else {
      ctx.lineTo(x, y)
    }
  }

  // Stroke
  ctx.strokeStyle = curveColor
  ctx.lineWidth = 2
  ctx.stroke()

  // Fill area (gradient from curve to bottom)
  const lastX = freqToX(freqPoints[freqPoints.length - 1], width)
  const firstX = freqToX(freqPoints[0], width)
  ctx.lineTo(lastX, dbToY(0, height))
  ctx.lineTo(firstX, dbToY(0, height))
  ctx.closePath()

  const fillGradient = ctx.createLinearGradient(0, 0, 0, height)
  // Parse curveColor to create semi-transparent version
  fillGradient.addColorStop(0, curveColor + '33')
  fillGradient.addColorStop(0.5, curveColor + '18')
  fillGradient.addColorStop(1, curveColor + '00')
  ctx.fillStyle = fillGradient
  ctx.fill()
}

function drawBandMarkers(ctx: CanvasRenderingContext2D, width: number, height: number) {
  const curveColor = readThemeValue('--td-brand-color', '#00a74d')
  const bands = gains.value.bands

  for (let i = 0; i < bands.length; i++) {
    const band = bands[i]
    const x = freqToX(band.frequency, width)

    // For LP/HP/Notch, show marker at 0dB line
    const effectiveGain = GAIN_DISABLED_TYPES.has(band.type) ? 0 : band.gain
    const clampedDb = Math.max(-24, Math.min(24, effectiveGain))
    const y = dbToY(clampedDb, height)

    const isSelected = i === selectedBandIndex.value

    if (isSelected) {
      // Outer glow
      ctx.beginPath()
      ctx.arc(x, y, 10, 0, Math.PI * 2)
      ctx.fillStyle = curveColor + '33'
      ctx.fill()

      // Selected dot
      ctx.beginPath()
      ctx.arc(x, y, 6, 0, Math.PI * 2)
      ctx.fillStyle = curveColor
      ctx.fill()
      ctx.strokeStyle = '#fff'
      ctx.lineWidth = 2
      ctx.stroke()
    } else {
      // Normal dot
      ctx.beginPath()
      ctx.arc(x, y, 4, 0, Math.PI * 2)
      ctx.fillStyle = curveColor + 'AA'
      ctx.fill()
      ctx.strokeStyle = curveColor
      ctx.lineWidth = 1
      ctx.stroke()
    }
  }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

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

/* --- Canvas visualizer --- */
.visualizer-container {
  width: 100%;
  background: var(--settings-eq-visualizer-bg, var(--td-bg-color-page));
  border-radius: 8px;
  overflow: hidden;
}
.visualizer-container canvas {
  width: 100%;
  height: 250px;
  display: block;
}

/* --- Band sliders --- */
.sliders-container {
  display: flex;
  justify-content: space-between;
  height: 280px;
  padding: 20px 0;
  gap: 4px;
}
.slider-group {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
  padding: 8px 2px;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 0.2s;
}
.slider-group:hover {
  background: color-mix(in srgb, var(--td-bg-color-container), var(--td-brand-color) 8%);
}
.slider-group--selected {
  background: color-mix(in srgb, var(--td-bg-color-container), var(--td-brand-color) 12%);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--td-brand-color), transparent 60%);
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

/* --- Band params panel --- */
.band-params-panel {
  padding: 14px 18px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--td-bg-color-container), var(--td-brand-color) 5%);
  border: 1px solid var(--td-border-level-1-color);
}
.band-params-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.band-params-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--td-text-color-primary);
}
.band-params-freq {
  font-size: 12px;
  color: var(--td-text-color-secondary);
}
.band-params-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.param-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.param-label {
  width: 64px;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--td-text-color-secondary);
}
.param-control {
  flex: 1;
  min-width: 0;
}
.param-value {
  width: 56px;
  flex-shrink: 0;
  font-size: 12px;
  text-align: right;
  color: var(--td-text-color-primary);
  font-variant-numeric: tabular-nums;
}
.param-slider {
  width: 100%;
  height: 4px;
  -webkit-appearance: none;
  appearance: none;
  background: var(--td-bg-color-component);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
}
.param-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--td-brand-color);
  cursor: pointer;
  border: 2px solid var(--td-bg-color-container);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}
.param-slider::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--td-brand-color);
  cursor: pointer;
  border: 2px solid var(--td-bg-color-container);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}
.param-slider:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* --- Controls row --- */
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

/* --- Global gain --- */
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
</style>

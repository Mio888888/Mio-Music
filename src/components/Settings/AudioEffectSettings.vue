<template>
  <div class="audio-effects-settings">
    <t-card :title="t('settings.audioEffect.title')" :bordered="false">
      <template #actions>
        <t-button theme="default" variant="text" @click="resetAll">{{ t('settings.audioEffect.resetAll') }}</t-button>
      </template>

      <div class="effects-grid">
        <!-- 1. Bass Boost -->
        <div class="effect-card">
          <div class="card-header">
            <div class="title">{{ t('settings.audioEffect.bassBoost') }}</div>
            <t-switch v-model="bassBoost.enabled" />
          </div>
          <div class="card-content">
            <div class="control-group">
              <label>{{ t('settings.audioEffect.bassGain') }}</label>
              <t-slider
                v-model="bassBoost.gain"
                :min="-12"
                :max="12"
                :step="0.5"
                :disabled="!bassBoost.enabled"
                label="${value}dB"
              />
            </div>
            <div class="presets">
              <t-radio-group
                v-model="bassPreset"
                class="effect-radio-group"
                :class="{ 'is-effect-disabled': !bassBoost.enabled }"
                variant="default-filled"
                :disabled="!bassBoost.enabled"
                @change="(val: string | number | boolean) => applyBassPreset(val as string)"
              >
                <t-radio-button value="light">{{ t('settings.audioEffect.bassLight') }}</t-radio-button>
                <t-radio-button value="medium">{{ t('settings.audioEffect.bassMedium') }}</t-radio-button>
                <t-radio-button value="heavy">{{ t('settings.audioEffect.bassHeavy') }}</t-radio-button>
              </t-radio-group>
            </div>
          </div>
        </div>

        <!-- 2. Surround Sound -->
        <div class="effect-card">
          <div class="card-header">
            <div class="title">{{ t('settings.audioEffect.surround') }}</div>
            <t-switch v-model="surround.enabled" />
          </div>
          <div class="card-content">
            <div class="control-group">
              <label>{{ t('settings.audioEffect.surroundSimulation') }}</label>
              <t-radio-group
                v-model="surround.mode"
                class="effect-radio-group"
                :class="{ 'is-effect-disabled': !surround.enabled }"
                variant="default-filled"
                :disabled="!surround.enabled"
              >
                <t-radio-button value="off">{{ t('settings.audioEffect.surroundOff') }}</t-radio-button>
                <t-radio-button value="small">{{ t('settings.audioEffect.surroundSmall') }}</t-radio-button>
                <t-radio-button value="medium">{{ t('settings.audioEffect.surroundMedium') }}</t-radio-button>
                <t-radio-button value="large">{{ t('settings.audioEffect.surroundLarge') }}</t-radio-button>
              </t-radio-group>
            </div>
            <div class="info-text">{{ t('settings.audioEffect.surroundInfo') }}</div>
          </div>
        </div>

        <!-- 3. Channel Balance -->
        <div class="effect-card">
          <div class="card-header">
            <div class="title">{{ t('settings.audioEffect.balance') }}</div>
            <t-switch v-model="balance.enabled" />
          </div>
          <div class="card-content">
            <div class="control-group">
              <div class="balance-labels">
                <span>Left</span>
                <span>Right</span>
              </div>
              <t-slider
                v-model="balance.value"
                :min="-1"
                :max="1"
                :step="0.05"
                :disabled="!balance.enabled"
                :tooltip-format="(val: number) => formatBalance(val)"
              />
            </div>
            <div class="visual-balance">
              <div class="speaker left" :style="{ opacity: 1 - Math.max(0, balance.value) }">
                🔊 L
              </div>
              <div class="listener">😐</div>
              <div class="speaker right" :style="{ opacity: 1 - Math.max(0, -balance.value) }">
                🔊 R
              </div>
            </div>
            <div style="text-align: center; margin-top: 10px">
              <t-button
                size="small"
                variant="outline"
                :disabled="!balance.enabled"
                @click="balance.value = 0"
                >{{ t('settings.audioEffect.centerCalibration') }}</t-button
              >
            </div>
          </div>
        </div>
      </div>
    </t-card>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { useAudioEffectsStore } from '@/store/AudioEffects'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const store = useAudioEffectsStore()
const { bassBoost, surround, balance } = storeToRefs(store)

const bassPreset = ref('light')

const applyEffects = () => {
  invoke('player__set_bass_boost', { gain: bassBoost.value.enabled ? bassBoost.value.gain : 0 })
  invoke('player__set_balance', { value: balance.value.enabled ? balance.value.value : 0 })
}

watch([bassBoost, surround, balance], () => { applyEffects() }, { deep: true })

const applyBassPreset = (val: string) => {
  if (!bassBoost.value.enabled) return
  switch (val) {
    case 'light': bassBoost.value.gain = 3; break
    case 'medium': bassBoost.value.gain = 6; break
    case 'heavy': bassBoost.value.gain = 9; break
  }
}

const resetAll = () => { store.resetEffects() }

const formatBalance = (val: number) => {
  if (val === 0) return t('settings.audioEffect.balanceCenter')
  return val < 0
    ? t('settings.audioEffect.balanceLeft', { percent: Math.abs(val * 100).toFixed(0) })
    : t('settings.audioEffect.balanceRight', { percent: Math.abs(val * 100).toFixed(0) })
}

onMounted(() => { applyEffects() })
</script>

<style scoped>
.audio-effects-settings {
  padding: 20px 0;
  color: var(--td-text-color-primary);
}
.effects-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 20px;
  margin-top: 20px;
}
.effect-card {
  background: var(--td-bg-color-container);
  border: 1px solid var(--td-component-border);
  border-radius: 8px;
  padding: 16px;
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
}
.effect-card:hover {
  border-color: var(--td-brand-color);
  box-shadow: var(--theme-shadow-light);
}
.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--td-component-stroke);
  padding-bottom: 8px;
}
.title {
  font-weight: 600;
  font-size: 16px;
  color: var(--td-text-color-primary);
}
.card-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.control-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.control-group label {
  font-size: 14px;
  color: var(--td-text-color-secondary);
}
.presets {
  display: flex;
  justify-content: center;
  margin-top: 8px;
}
.presets :deep(.t-radio-group),
.control-group :deep(.t-radio-group) {
  background: var(--td-bg-color-secondarycontainer);
  border: 1px solid var(--td-component-border);
}
.presets :deep(.effect-radio-group),
.control-group :deep(.effect-radio-group) {
  overflow: hidden;
  border-radius: 6px;
}
.presets :deep(.t-radio-group--filled .t-radio-button),
.control-group :deep(.t-radio-group--filled .t-radio-button) {
  color: var(--td-text-color-secondary);
}
.presets :deep(.t-radio-group--filled .t-radio-button:hover),
.control-group :deep(.t-radio-group--filled .t-radio-button:hover),
.presets :deep(.t-radio-group--filled .t-radio-button.t-is-checked),
.control-group :deep(.t-radio-group--filled .t-radio-button.t-is-checked),
.presets :deep(.t-radio-group--filled .t-radio-button.t-is-checked .t-radio-button__label),
.control-group :deep(.t-radio-group--filled .t-radio-button.t-is-checked .t-radio-button__label),
.presets :deep(.t-radio-group--filled .t-radio-button--checked),
.control-group :deep(.t-radio-group--filled .t-radio-button--checked),
.presets :deep(.t-radio-group--filled .t-radio-button--checked .t-radio-button__label),
.control-group :deep(.t-radio-group--filled .t-radio-button--checked .t-radio-button__label) {
  color: var(--settings-nav-label-active, var(--td-text-color-primary));
}
.presets :deep(.t-radio-group--filled .t-radio-group__bg-block),
.control-group :deep(.t-radio-group--filled .t-radio-group__bg-block) {
  background: var(--settings-nav-active-bg, var(--td-bg-color-component-active));
  border: 1px solid var(--settings-nav-active-border, var(--td-brand-color));
  box-shadow: var(--settings-nav-active-shadow, none);
}
.presets :deep(.effect-radio-group.is-effect-disabled),
.control-group :deep(.effect-radio-group.is-effect-disabled) {
  background: var(--td-bg-color-secondarycontainer);
  border-color: var(--td-component-border);
}
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button),
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button__label),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button__label) {
  color: var(--td-text-color-secondary) !important;
  opacity: 0.72;
}
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-group__bg-block),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-group__bg-block) {
  background: var(--td-bg-color-component) !important;
  border: 1px solid var(--td-component-border) !important;
  box-shadow: none !important;
}
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked),
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button--checked),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button--checked),
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked .t-radio-button__label),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked .t-radio-button__label),
.presets :deep(.effect-radio-group.is-effect-disabled .t-radio-button--checked .t-radio-button__label),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button--checked .t-radio-button__label) {
  color: var(--td-text-color-primary) !important;
  opacity: 0.76;
}
.info-text {
  font-size: 12px;
  color: var(--td-text-color-secondary);
  text-align: center;
}
.balance-labels {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--td-text-color-secondary);
}
.visual-balance {
  display: flex;
  justify-content: space-around;
  align-items: center;
  margin-top: 10px;
  font-size: 20px;
  background: var(--td-bg-color-secondarycontainer);
  padding: 10px;
  border-radius: 8px;
}
</style>

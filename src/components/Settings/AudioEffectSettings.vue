<template>
  <div class="audio-effects-settings">
    <div class="settings-inline-header">
      <t-button theme="default" variant="text" @click="resetAll">{{ t('settings.audioEffect.resetAll') }}</t-button>
    </div>

    <div class="effects-grid">
        <!-- Surround Sound -->
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

        <!-- Channel Balance -->
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
              <div class="channel-badge left" :class="{ active: balance.value <= 0 }">
                L
              </div>
              <div class="balance-center">Center</div>
              <div class="channel-badge right" :class="{ active: balance.value >= 0 }">
                R
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
  </div>
</template>

<script setup lang="ts">
import { watch, onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { useAudioEffectsStore } from '@/store/AudioEffects'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const store = useAudioEffectsStore()
const { surround, balance } = storeToRefs(store)

const applyEffects = () => {
  invoke('player__set_balance', { value: balance.value.enabled ? balance.value.value : 0 })
}

watch(
  [() => ({ ...surround.value }), () => ({ ...balance.value })],
  () => { applyEffects() }
)

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
  color: var(--td-text-color-primary);
}
.settings-inline-header {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 14px;
}
.effects-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  max-width: 900px;
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
.control-group :deep(.t-radio-group) {
  background: var(--td-bg-color-secondarycontainer);
  border: 1px solid var(--td-component-border);
}
.control-group :deep(.effect-radio-group) {
  overflow: hidden;
  border-radius: 6px;
}
.control-group :deep(.t-radio-group--filled .t-radio-button) {
  color: var(--td-text-color-secondary);
}
.control-group :deep(.t-radio-group--filled .t-radio-button:hover),
.control-group :deep(.t-radio-group--filled .t-radio-button.t-is-checked),
.control-group :deep(.t-radio-group--filled .t-radio-button.t-is-checked .t-radio-button__label),
.control-group :deep(.t-radio-group--filled .t-radio-button--checked),
.control-group :deep(.t-radio-group--filled .t-radio-button--checked .t-radio-button__label) {
  color: var(--settings-nav-label-active, var(--td-text-color-primary));
}
.control-group :deep(.t-radio-group--filled .t-radio-group__bg-block) {
  background: var(--settings-nav-active-bg, var(--td-bg-color-component-active));
  border: 1px solid var(--settings-nav-active-border, var(--td-brand-color));
  box-shadow: var(--settings-nav-active-shadow, none);
}
.control-group :deep(.effect-radio-group.is-effect-disabled) {
  background: var(--td-bg-color-secondarycontainer);
  border-color: var(--td-component-border);
}
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button__label) {
  color: var(--td-text-color-secondary) !important;
  opacity: 0.72;
}
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-group__bg-block) {
  background: var(--td-bg-color-component) !important;
  border: 1px solid var(--td-component-border) !important;
  box-shadow: none !important;
}
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button--checked),
.control-group :deep(.effect-radio-group.is-effect-disabled .t-radio-button.t-is-checked .t-radio-button__label),
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
  background: var(--td-bg-color-secondarycontainer);
  padding: 12px 16px;
  border-radius: 8px;
}
.channel-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  font-size: 13px;
  font-weight: 600;
  color: var(--td-text-color-secondary);
  background: var(--td-bg-color-container);
  border: 1px solid var(--td-component-border);
  transition: opacity 0.3s ease, color 0.3s ease, border-color 0.3s ease;
  opacity: 0.4;
}
.channel-badge.active {
  opacity: 1;
  color: var(--td-brand-color);
  border-color: var(--td-brand-color);
}
.balance-center {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  letter-spacing: 0.5px;
}
</style>

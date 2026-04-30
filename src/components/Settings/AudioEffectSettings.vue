<script setup lang="ts">
import { watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useAudioEffectsStore } from '@/store/AudioEffects'
import { ControlAudioStore } from '@/store/ControlAudio'
import AudioManager from '@/utils/audio/AudioManager'

const effectsStore = useAudioEffectsStore()
const { bassBoost, surround, balance } = storeToRefs(effectsStore)
const audioStore = ControlAudioStore()

const bassPresets = [
  { label: '轻度', gain: 3 },
  { label: '中度', gain: 6 },
  { label: '重度', gain: 9 }
]

const surroundModes = [
  { label: '关闭', value: 'off' },
  { label: '小空间', value: 'small' },
  { label: '中空间', value: 'medium' },
  { label: '大空间', value: 'large' }
]

const applyEffects = () => {
  try {
    const audio = audioStore.Audio?.audio
    if (!audio) return
    AudioManager.setBassBoost(audio, bassBoost.value.gain)
    AudioManager.setSurroundMode(audio, surround.value.mode)
    AudioManager.setBalance(audio, balance.value.value)
  } catch {}
}

watch([bassBoost, surround, balance], () => { applyEffects() }, { deep: true })
</script>

<template>
  <div class="audio-effect-settings">
    <div class="effect-section">
      <div class="effect-header">
        <div class="effect-info">
          <div class="effect-title">低音增强 (Bass Boost)</div>
          <div class="effect-desc">增强低频表现力</div>
        </div>
        <t-switch v-model="bassBoost.enabled" />
      </div>
      <template v-if="bassBoost.enabled">
        <div class="effect-control">
          <div class="preset-buttons">
            <t-button v-for="preset in bassPresets" :key="preset.label" size="small" :theme="bassBoost.gain === preset.gain ? 'primary' : 'default'" @click="bassBoost.gain = preset.gain">{{ preset.label }}</t-button>
          </div>
          <div class="gain-slider">
            <t-slider v-model="bassBoost.gain" :min="-12" :max="12" :step="0.5" />
            <span class="gain-value">{{ bassBoost.gain > 0 ? '+' : '' }}{{ bassBoost.gain }}dB</span>
          </div>
        </div>
      </template>
    </div>

    <div class="effect-section">
      <div class="effect-header">
        <div class="effect-info">
          <div class="effect-title">环绕音效 (Surround)</div>
          <div class="effect-desc">模拟空间环绕效果</div>
        </div>
        <t-switch v-model="surround.enabled" />
      </div>
      <template v-if="surround.enabled">
        <div class="effect-control">
          <t-radio-group v-model="surround.mode" variant="default-filled">
            <t-radio-button v-for="mode in surroundModes" :key="mode.value" :value="mode.value">{{ mode.label }}</t-radio-button>
          </t-radio-group>
        </div>
      </template>
    </div>

    <div class="effect-section">
      <div class="effect-header">
        <div class="effect-info">
          <div class="effect-title">声道平衡 (Balance)</div>
          <div class="effect-desc">调节左右声道平衡</div>
        </div>
        <t-switch v-model="balance.enabled" />
      </div>
      <template v-if="balance.enabled">
        <div class="effect-control">
          <div class="balance-visual">
            <span class="channel-label">L</span>
            <t-slider v-model="balance.value" :min="-1" :max="1" :step="0.1" style="flex:1" />
            <span class="channel-label">R</span>
          </div>
        </div>
      </template>
    </div>

    <div class="reset-action">
      <t-button theme="danger" variant="outline" @click="effectsStore.resetEffects()">重置全部音效</t-button>
    </div>
  </div>
</template>

<style scoped>
.audio-effect-settings { display: flex; flex-direction: column; gap: 1.5rem; }
.effect-section { padding: 1rem; background: var(--td-bg-color-page); border-radius: 0.5rem; border: 1px solid var(--td-border-level-1-color); }
.effect-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 0.75rem; }
.effect-info { .effect-title { font-weight: 600; color: var(--td-text-color-primary); font-size: 0.95rem; } .effect-desc { font-size: 0.8rem; color: var(--td-text-color-secondary); margin-top: 0.125rem; } }
.effect-control { padding-top: 0.75rem; border-top: 1px solid var(--td-border-level-1-color); }
.preset-buttons { display: flex; gap: 0.5rem; margin-bottom: 0.75rem; }
.gain-slider { display: flex; align-items: center; gap: 0.5rem; }
.gain-value { font-size: 0.8rem; color: var(--td-text-color-secondary); min-width: 4em; }
.balance-visual { display: flex; align-items: center; gap: 0.5rem; }
.channel-label { font-weight: 600; color: var(--td-text-color-secondary); font-size: 0.875rem; }
.reset-action { display: flex; justify-content: flex-end; }
</style>

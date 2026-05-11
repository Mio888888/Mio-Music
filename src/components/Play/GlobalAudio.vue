<script setup lang="ts">
import { onMounted, onUnmounted, provide, watch } from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { useEqualizerStore } from '@/store/Equalizer'
import { useAudioEffectsStore } from '@/store/AudioEffects'
import { invoke } from '@tauri-apps/api/core'
import { installShortDurationGuard, uninstallShortDurationGuard } from '@/utils/audio/globaPlayList'
import createLogger from '@/utils/logger'

const log = createLogger('GlobalAudio')

const audioStore = ControlAudioStore()
const eqStore = useEqualizerStore()
const effectStore = useAudioEffectsStore()

provide('audioSubscribe', audioStore.subscribe)

// EQ 同步集中在 Equalizer store；此处仅在全局播放器初始化后恢复一次快照。
const applyGlobalEQ = () => {
  void eqStore.syncToBackend()
}

// 音效变化时同步到 Rust 后端
const applyGlobalEffects = () => {
  const { balance } = effectStore
  invoke('player__set_balance', { value: balance.enabled ? balance.value : 0 })
}

onMounted(async () => {
  await audioStore.init()
  installShortDurationGuard()
  log.debug('Rust 原生音频引擎初始化完成')

  // 恢复 EQ 和音效设置到 Rust 后端
  applyGlobalEQ()
  applyGlobalEffects()
})

watch(
  [() => effectStore.surround, () => effectStore.balance],
  () => { applyGlobalEffects() },
  { deep: true }
)

onUnmounted(() => {
  uninstallShortDurationGuard()
  audioStore.destroy()
})
</script>

<template>
  <div />
</template>

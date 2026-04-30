<script setup lang="ts">
import { onMounted } from 'vue'
import { useAudioOutputStore } from '@/store/audioOutput'

const store = useAudioOutputStore()

onMounted(() => { store.scanDevices() })
</script>

<template>
  <div class="audio-output-settings">
    <div class="device-list">
      <div v-if="store.devices.length === 0" class="empty-hint">
        <p>未检测到音频输出设备</p>
        <t-button theme="primary" variant="outline" size="small" @click="store.scanDevices">刷新设备列表</t-button>
      </div>
      <div v-for="device in store.devices" :key="device.deviceId" class="device-item" :class="{ active: store.currentDeviceId === device.deviceId }" @click="store.setDevice(device.deviceId)">
        <div class="device-info">
          <div class="device-name">{{ device.label || device.deviceId }}</div>
          <div class="device-kind">{{ device.kind }}</div>
        </div>
        <div class="device-actions">
          <t-button v-if="store.currentDeviceId === device.deviceId" size="small" variant="text" theme="primary">当前</t-button>
        </div>
      </div>
    </div>
    <div class="refresh-action">
      <t-button theme="default" variant="outline" size="small" @click="store.scanDevices">刷新设备列表</t-button>
    </div>
  </div>
</template>

<style scoped>
.audio-output-settings { display: flex; flex-direction: column; gap: 0.75rem; }
.device-item {
  display: flex; align-items: center; justify-content: space-between;
  padding: 0.75rem 1rem; border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-page); border-radius: 0.5rem;
  cursor: pointer; transition: all 0.2s;
  &:hover { border-color: var(--td-brand-color-3); }
  &.active { border-color: var(--td-brand-color); background: var(--td-brand-color-1); }
}
.device-info { .device-name { font-weight: 500; font-size: 0.875rem; color: var(--td-text-color-primary); } .device-kind { font-size: 0.75rem; color: var(--td-text-color-secondary); } }
.empty-hint { text-align: center; padding: 2rem; color: var(--td-text-color-secondary); }
.refresh-action { display: flex; justify-content: flex-end; }
</style>

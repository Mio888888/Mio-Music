<template>
  <div class="dlna-device-settings">
    <div class="section-header">
      <span class="section-title">DLNA 投屏设备</span>
      <t-button
        variant="text"
        shape="circle"
        :loading="dlnaStore.isSearching"
        @click="dlnaStore.startSearch"
      >
        <template #icon><RefreshIcon /></template>
      </t-button>
    </div>

    <div class="device-list dlna-device-list">
      <t-radio-group
        v-model="dlnaStore.currentDevice"
        class="device-radio-group"
        @change="handleDlnaDeviceChange"
      >
        <div
          v-for="device in dlnaStore.devices"
          :key="device.usn"
          class="device-item"
          :class="{ active: dlnaStore.currentDevice?.usn === device.usn }"
        >
          <t-radio :value="device" class="device-radio">
            <div class="device-info">
              <span class="device-name">{{ device.name }}</span>
              <span class="device-address">{{ device.address }}</span>
            </div>
          </t-radio>
          <div v-if="dlnaStore.currentDevice?.usn === device.usn" class="device-meta">
            <t-tooltip content="停止投屏">
              <t-button variant="text" shape="circle" size="large" @click.stop="stopDlna">
                <template #icon><PoweroffIcon /></template>
              </t-button>
            </t-tooltip>
          </div>
        </div>
      </t-radio-group>
      <div v-if="dlnaStore.devices.length === 0 && !dlnaStore.isSearching" class="empty-msg">
        未检测到 DLNA 投屏设备
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { RefreshIcon, PoweroffIcon } from 'tdesign-icons-vue-next'
import { useDlnaStore } from '@/store/dlna'
import { ControlAudioStore } from '@/store/ControlAudio'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { MessagePlugin } from 'tdesign-vue-next'

const dlnaStore = useDlnaStore()
const controlAudio = ControlAudioStore()
const globalPlayStatus = useGlobalPlayStatusStore()

const handleDlnaDeviceChange = (val: any) => {
  if (val) {
    MessagePlugin.success(`已连接投屏设备: ${val.name}`)

    if (controlAudio.Audio?.url) {
      const url = controlAudio.Audio.url
      const title = globalPlayStatus.player?.songInfo?.name || 'CeruMusic'
      dlnaStore.play(url, title).then(() => {
        if (controlAudio.Audio.isPlay) {
          dlnaStore.resume()
        }
      })
    }
  }
}

const stopDlna = () => {
  dlnaStore.stop()
  dlnaStore.currentDevice = null
  MessagePlugin.success('已断开投屏设备')
}
</script>

<style scoped>
.dlna-device-settings {
  padding: 0;
  color: var(--td-text-color-primary);
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.section-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--td-text-color-primary);
}

.device-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 200px;
  overflow-y: auto;
  padding-right: 8px;
}

.device-list::-webkit-scrollbar {
  width: 6px;
}
.device-list::-webkit-scrollbar-thumb {
  background-color: var(--td-scrollbar-color);
  border-radius: 3px;
}

.device-radio-group {
  width: 100%;
}

.device-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px;
  border-radius: 6px;
  border: 1px solid transparent;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
  width: 100%;
  min-height: 50px;
}

.device-item:hover {
  background-color: var(--td-bg-color-secondarycontainer);
}

.device-item.active {
  background-color: var(--td-brand-color-light);
  border-color: var(--td-brand-color);
}

.device-radio {
  width: 100%;
}

.device-info {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
}

.device-name {
  font-weight: 500;
  flex: 1;
  color: var(--td-text-color-primary);
}

.device-item.active .device-name {
  color: var(--td-text-color-primary);
}

.device-address {
  font-size: 12px;
  color: var(--td-text-color-secondary);
  margin-left: 8px;
}

.device-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  align-items: flex-end;
  min-width: 120px;
}

.empty-msg {
  text-align: center;
  color: var(--td-text-color-secondary);
  padding: 20px;
}
</style>

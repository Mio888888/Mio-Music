<script setup lang="ts">
import { ref, onMounted } from 'vue'

interface LyricOption {
  fontSize: number
  mainColor: string
  shadowColor: string
  fontWeight: number
  position: 'left' | 'center' | 'right' | 'both'
  alwaysShowPlayInfo: boolean
  animation: boolean
  showYrc: boolean
  showTran: boolean
  isDoubleLine: boolean
  textBackgroundMask: boolean
  backgroundMaskColor: string
  fontFamily: string
}

const defaultOptions: LyricOption = {
  fontSize: 36,
  mainColor: '#ffffff',
  shadowColor: '#000000',
  fontWeight: 700,
  position: 'center',
  alwaysShowPlayInfo: false,
  animation: true,
  showYrc: true,
  showTran: true,
  isDoubleLine: false,
  textBackgroundMask: false,
  backgroundMaskColor: 'rgba(0,0,0,0.5)',
  fontFamily: 'PingFangSC-Semibold'
}

const options = ref<LyricOption>({ ...defaultOptions })
const isDesktopLyricOpen = ref(false)

const loadOptions = async () => {
  try {
    if ((window as any).electron?.ipcRenderer) {
      const saved = await (window as any).electron.ipcRenderer.invoke('get-desktop-lyric-option')
      if (saved) Object.assign(options.value, saved)
      const state = await (window as any).electron.ipcRenderer.invoke('get-lyric-open-state')
      isDesktopLyricOpen.value = !!state
    }
  } catch {
    // Silently ignore errors in non-Electron environment
  }
}

const applyOptions = () => {
  try {
    if ((window as any).electron?.ipcRenderer) {
      (window as any).electron.ipcRenderer.send('set-desktop-lyric-option', options.value, true)
    }
  } catch {
    // Silently ignore errors in non-Electron environment
  }
}

const toggleDesktopLyric = async (val: any) => {
  isDesktopLyricOpen.value = Boolean(val)
  try {
    if ((window as any).electron?.ipcRenderer) {
      (window as any).electron.ipcRenderer.send('change-desktop-lyric', val)
    }
  } catch {
    // Silently ignore errors in non-Electron environment
  }
}

const updateOption = <K extends keyof LyricOption>(key: K, value: LyricOption[K]) => {
  options.value[key] = value
  applyOptions()
}

onMounted(() => {
  loadOptions()
})
</script>

<template>
  <t-card title="桌面歌词样式" hover-shadow>
    <div class="desktop-lyric-style">
      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">桌面歌词开关</div>
          <div class="setting-desc">开启/关闭桌面歌词窗口</div>
        </div>
        <t-switch :value="isDesktopLyricOpen" @change="toggleDesktopLyric" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">字体大小</div>
          <div class="setting-desc">{{ options.fontSize }}px</div>
        </div>
        <t-input-number v-model="options.fontSize" :min="12" :max="96" :step="1" style="width: 120px;" @change="applyOptions" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">字体粗细</div>
        </div>
        <t-select :value="options.fontWeight" style="width: 120px;" @change="(v: any) => updateOption('fontWeight', Number(v))">
          <t-option :value="400" label="细体 (400)" />
          <t-option :value="500" label="中等 (500)" />
          <t-option :value="600" label="半粗 (600)" />
          <t-option :value="700" label="粗体 (700)" />
          <t-option :value="800" label="特粗 (800)" />
        </t-select>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">歌词颜色</div>
        </div>
        <t-color-picker :value="options.mainColor" @change="(v: string) => updateOption('mainColor', v)" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">阴影颜色</div>
        </div>
        <t-color-picker :value="options.shadowColor" @change="(v: string) => updateOption('shadowColor', v)" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">歌词位置</div>
        </div>
        <t-select :value="options.position" style="width: 120px;" @change="(v: any) => updateOption('position', v)">
          <t-option value="left" label="居左" />
          <t-option value="center" label="居中" />
          <t-option value="right" label="居右" />
          <t-option value="both" label="双行" />
        </t-select>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">显示逐字歌词</div>
          <div class="setting-desc">开启后显示逐字滚动效果</div>
        </div>
        <t-switch :value="options.showYrc" @change="(v: any) => updateOption('showYrc', Boolean(v))" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">显示翻译</div>
        </div>
        <t-switch :value="options.showTran" @change="(v: any) => updateOption('showTran', Boolean(v))" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">背景遮罩</div>
          <div class="setting-desc">在歌词文字下方显示半透明背景</div>
        </div>
        <t-switch :value="options.textBackgroundMask" @change="(v: any) => updateOption('textBackgroundMask', Boolean(v))" />
      </div>
    </div>
  </t-card>
</template>

<style scoped>
.desktop-lyric-style {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.875rem 1rem;
  border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-page);
  border-radius: 0.5rem;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.setting-title { font-weight: 600; color: var(--td-text-color-primary); font-size: 0.95rem; }
.setting-desc { color: var(--td-text-color-secondary); font-size: 0.8rem; }
</style>

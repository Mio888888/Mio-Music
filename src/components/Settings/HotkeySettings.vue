<template>
  <div>
    <div class="section">
      <div class="setting-item" style="margin-top: 0">
        <div class="item-info">
          <div class="item-title">启用全局快捷键</div>
          <div class="item-desc">关闭后将不会注册系统级快捷键</div>
        </div>
        <t-switch v-model="enabled" :loading="saving" @change="save" />
      </div>

      <div class="hotkey-list" :class="{ disabled: !enabled }">
        <div class="hotkey-toolbar">
          <t-button size="small" theme="default" variant="outline" :loading="saving" @click="resetToDefault">
            恢复默认
          </t-button>
        </div>
        <div v-for="it in hotkeyActions" :key="it.id" class="hotkey-row">
          <div class="hotkey-meta">
            <div class="hotkey-title">{{ it.title }}</div>
            <div class="hotkey-desc">{{ it.desc }}</div>
          </div>
          <div class="hotkey-actions">
            <t-tag v-if="failedActions.has(it.id)" theme="danger" variant="light">
              {{ displayBinding(it.id) }}
            </t-tag>
            <t-tag v-else theme="primary" variant="light">
              {{ displayBinding(it.id) }}
            </t-tag>
            <t-button size="small" theme="primary" variant="outline" :disabled="!enabled" @click="beginRecord(it.id)">
              设置
            </t-button>
            <t-button size="small" theme="default" variant="text" :disabled="!enabled" @click="clearHotkey(it.id)">
              清除
            </t-button>
          </div>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section-title">局内快捷键</div>
      <div class="section-desc">仅在主窗口聚焦且不在输入框时生效。</div>
      <div class="hotkey-list">
        <div v-for="it in inAppShortcuts" :key="it.keys + it.title" class="hotkey-row">
          <div class="hotkey-meta">
            <div class="hotkey-title">{{ it.title }}</div>
            <div class="hotkey-desc">{{ it.desc }}</div>
          </div>
          <div class="hotkey-actions">
            <t-tag theme="default" variant="light">{{ it.keys }}</t-tag>
          </div>
        </div>
      </div>
    </div>

    <t-dialog
      v-model:visible="recording.visible"
      :header="recordTitle"
      :close-btn="true"
      :close-on-esc-keydown="false"
      :close-on-overlay-click="false"
    >
      <div class="recording">
        <div class="preview">{{ recordPreview }}</div>
        <div class="preview sub">
          {{ recording.captured ? acceleratorToDisplay(recording.captured) : '等待输入...' }}
        </div>
        <div class="tips">
          <div>按 Esc 取消</div>
          <div>按 Backspace/Delete 清空</div>
        </div>
      </div>
      <template #footer>
        <t-button theme="default" @click="cancelRecord">取消</t-button>
        <t-button theme="primary" :loading="saving" :disabled="!recordCanSave" @click="confirmRecord">
          保存
        </t-button>
      </template>
    </t-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, toRaw, watch } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import type { HotkeyAction, HotkeyConfig } from '@/types/hotkeys'
import { defaultHotkeyConfig } from '@/types/hotkeys'
import { acceleratorToDisplay, createHotkeyRecorder, isCompleteAccelerator } from '@/utils/hotkeys/recording'

const hotkeyActions: Array<{ id: HotkeyAction; title: string; desc: string }> = [
  { id: 'toggle', title: '播放/暂停', desc: '全局切换播放状态' },
  { id: 'playPrev', title: '上一首', desc: '全局切换到上一首' },
  { id: 'playNext', title: '下一首', desc: '全局切换到下一首' },
  { id: 'seekBackward', title: '快退', desc: '全局快退 5 秒' },
  { id: 'seekForward', title: '快进', desc: '全局快进 5 秒' },
  { id: 'toggleDesktopLyric', title: '桌面歌词', desc: '全局切换桌面歌词显示/隐藏' },
  { id: 'volumeDown', title: '音量 -', desc: '全局音量降低 5%' },
  { id: 'volumeUp', title: '音量 +', desc: '全局音量提升 5%' },
  { id: 'setPlayModeSequence', title: '顺序播放', desc: '切换为顺序播放' },
  { id: 'setPlayModeRandom', title: '随机播放', desc: '切换为随机播放' },
  { id: 'togglePlayModeSingle', title: '单曲循环切换', desc: '切换单曲循环/顺序播放' }
]

const inAppShortcuts: Array<{ keys: string; title: string; desc: string }> = [
  { keys: 'Space', title: '播放/暂停', desc: '切换播放状态' },
  { keys: '↑', title: '音量 +', desc: '音量提升 5%' },
  { keys: '↓', title: '音量 -', desc: '音量降低 5%' },
  { keys: '←', title: '快退', desc: '快退 5 秒' },
  { keys: '→', title: '快进', desc: '快进 5 秒' },
  { keys: 'F', title: '全屏播放', desc: '切换全屏播放面板' }
]

const loading = ref(false)
const saving = ref(false)
const enabled = ref(true)
const bindings = ref<HotkeyConfig['bindings']>({})
const failedActions = ref<Set<HotkeyAction>>(new Set())

const displayBinding = (action: HotkeyAction) => {
  return acceleratorToDisplay(bindings.value[action] || '')
}

const load = async () => {
  loading.value = true
  try {
    const res = await (window as any).api.hotkeys.get()
    const cfg = res?.data as HotkeyConfig | undefined
    if (!cfg) return
    enabled.value = !!cfg.enabled
    bindings.value = { ...(cfg.bindings || {}) }
    const status = res?.status as any
    failedActions.value = new Set(status?.failedActions || [])
  } catch {
    // ignore
  } finally {
    loading.value = false
  }
}

const save = async () => {
  saving.value = true
  try {
    const res = await (window as any).api.hotkeys.set({
      enabled: enabled.value,
      bindings: { ...(toRaw(bindings.value) || {}) }
    })
    if (!res?.success) {
      MessagePlugin.warning(res?.error || '保存失败')
      return
    }
    const cfg = res?.data as HotkeyConfig | undefined
    if (cfg) {
      enabled.value = !!cfg.enabled
      bindings.value = { ...(cfg.bindings || {}) }
    }
    const status = res?.status as any
    failedActions.value = new Set(status?.failedActions || [])
    MessagePlugin.success('已保存')
  } catch {
    MessagePlugin.error('保存失败')
  } finally {
    saving.value = false
  }
}

const resetToDefault = async () => {
  enabled.value = !!defaultHotkeyConfig.enabled
  bindings.value = { ...(defaultHotkeyConfig.bindings || {}) }
  await save()
}

const clearHotkey = async (action: HotkeyAction) => {
  bindings.value[action] = ''
  await save()
}

const recording = ref<{ visible: boolean; action: HotkeyAction | null; preview: string; captured: string }>({
  visible: false, action: null, preview: '', captured: ''
})
let recorder: ReturnType<typeof createHotkeyRecorder> | null = null

const beginRecord = (action: HotkeyAction) => {
  recording.value = { visible: true, action, preview: '', captured: bindings.value[action] || '' }
  recorder?.unmount()
  recorder = createHotkeyRecorder({
    onPreviewChange: (preview) => { recording.value.preview = preview },
    onCapture: (acc) => {
      if (recording.value.action) {
        const conflict = Object.entries(bindings.value).find(
          ([k, v]) => k !== recording.value.action && (v || '').toLowerCase() === (acc || '').toLowerCase() && !!acc
        )
        if (conflict) {
          MessagePlugin.warning('快捷键已被其它功能占用')
          return
        }
      }
      recording.value.captured = acc
    },
    onCancel: cancelRecord
  })
  recorder.mount()
}

const cancelRecord = () => {
  recording.value = { visible: false, action: null, preview: '', captured: '' }
  recorder?.unmount()
  recorder = null
}

const confirmRecord = async () => {
  const action = recording.value.action
  if (!action) return
  const acc = recording.value.captured || ''
  if (acc && !isCompleteAccelerator(acc)) return
  bindings.value[action] = acc
  cancelRecord()
  await save()
}

const recordTitle = computed(() => {
  if (!recording.value.action) return '录入快捷键'
  const meta = hotkeyActions.find(a => a.id === recording.value.action)
  return meta ? `设置：${meta.title}` : '录入快捷键'
})

const recordPreview = computed(() => {
  if (!recording.value.preview) return '请按下组合键'
  return acceleratorToDisplay(recording.value.preview)
})

const recordCanSave = computed(() => {
  if (!recording.value.visible || !recording.value.action) return false
  if (!recording.value.captured) return true
  return isCompleteAccelerator(recording.value.captured)
})

onMounted(load)

watch(() => recording.value.visible, (v) => {
  if (!v) { recorder?.unmount(); recorder = null }
})

onBeforeUnmount(() => { recorder?.unmount(); recorder = null })
</script>

<style scoped>
.section { display: flex; flex-direction: column; gap: 8px; margin-bottom: 16px; }
.section-title { font-weight: 700; color: var(--td-text-color-primary); }
.section-desc { font-size: 12px; color: var(--td-text-color-secondary); }
.setting-item {
  display: flex; align-items: center; justify-content: space-between;
  padding: 0.875rem 1rem; border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-container); border-radius: 0.5rem; margin-top: 0.75rem;
}
.setting-item .item-info { display: flex; flex-direction: column; gap: 0.25rem; }
.setting-item .item-title { font-weight: 600; font-size: 0.95rem; }
.setting-item .item-desc { color: var(--td-text-color-secondary); font-size: 0.8rem; }
.hotkey-list { margin-top: 12px; display: flex; flex-direction: column; gap: 10px; }
.hotkey-list.disabled { opacity: 0.6; }
.hotkey-toolbar { display: flex; justify-content: flex-end; }
.hotkey-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 12px; border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-container); border-radius: 10px; gap: 12px;
}
.hotkey-meta { display: flex; flex-direction: column; gap: 4px; }
.hotkey-title { font-weight: 600; }
.hotkey-desc { font-size: 12px; color: var(--td-text-color-secondary); }
.hotkey-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
.recording { display: flex; flex-direction: column; gap: 12px; }
.preview { padding: 12px; border: 1px dashed var(--td-border-level-1-color); border-radius: 8px; font-weight: 700; text-align: center; }
.preview.sub { font-weight: 600; }
.tips { display: flex; justify-content: space-between; color: var(--td-text-color-secondary); font-size: 12px; }
@media (max-width: 768px) {
  .section {
    gap: 10px;
    margin-bottom: 12px;
  }

  .setting-item,
  .hotkey-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
  }

  .setting-item .item-info,
  .hotkey-meta,
  .hotkey-actions {
    width: 100%;
  }

  .hotkey-toolbar {
    justify-content: flex-start;
  }

  .hotkey-actions {
    flex-wrap: wrap;
    gap: 8px;
  }

  .hotkey-actions .t-tag {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tips {
    flex-direction: column;
    gap: 4px;
    text-align: center;
  }
}
</style>

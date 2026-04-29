<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'

const router = useRouter()

const handleMinimize = async () => {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().minimize()
}

const handleMaximize = async () => {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().toggleMaximize()
}

const handleClose = async () => {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().close()
}

const handleSettings = () => {
  router.push('/settings')
}
</script>

<template>
  <div class="window-controls">
    <t-button
      shape="circle"
      theme="default"
      variant="text"
      class="control-btn settings-btn"
      title="设置"
      @click="handleSettings"
    >
      <i class="iconfont icon-shezhi"></i>
    </t-button>

    <div class="separator"></div>

    <t-button
      shape="circle"
      theme="default"
      variant="text"
      class="control-btn minimize-btn"
      title="最小化"
      @click="handleMinimize"
    >
      <i class="iconfont icon-zuixiaohua"></i>
    </t-button>

    <t-button
      shape="circle"
      theme="default"
      variant="text"
      class="control-btn maximize-btn"
      title="最大化"
      @click="handleMaximize"
    >
      <i class="iconfont icon-a-tingzhiwukuang"></i>
    </t-button>

    <t-button
      shape="circle"
      theme="default"
      variant="text"
      class="control-btn close-btn"
      title="关闭"
      @click="handleClose"
    >
      <i class="iconfont icon-a-quxiaoguanbi"></i>
    </t-button>
  </div>
</template>

<style scoped>
.window-controls {
  display: flex;
  align-items: center;
  gap: 0.125rem;
  flex-shrink: 0;
  -webkit-app-region: no-drag;
}

.control-btn {
  width: 2.25rem;
  height: 2.25rem;
  min-width: 2.25rem;
  padding: 0;
  border: none;
  background: transparent;
}

.control-btn .iconfont {
  font-size: 1.125rem;
  color: var(--td-text-color-secondary);
}

.control-btn:hover .iconfont {
  color: var(--td-text-color-primary);
}

.separator {
  width: 1px;
  height: 1rem;
  background: var(--td-border-level-1-color);
  margin: 0 3px;
  border-radius: 2px;
}

.settings-btn:hover {
  background-color: var(--td-bg-color-component-hover);
}

.minimize-btn:hover,
.maximize-btn:hover {
  background-color: var(--td-bg-color-component-hover);
}

.close-btn:hover {
  background-color: #e81123;
}

.close-btn:hover .iconfont {
  color: #fff !important;
}
</style>

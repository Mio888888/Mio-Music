<template>
  <t-dialog
    v-model:visible="dialogVisible"
    :header="dialogTitle"
    :width="dialogWidth"
    :close-btn="true"
    :close-on-overlay-click="false"
    :destroy-on-close="true"
    placement="center"
    @close="handleClose"
  >
    <template #body>
      <div class="plugin-update-notice-content">
        <div class="notice-message">
          <p v-if="currentNotice?.content" class="message-text">{{ currentNotice.content }}</p>

          <div class="update-info">
            <div v-if="currentVersion" class="version-info">
              <span class="version-label">当前版本:</span>
              <span class="version-value">{{ currentVersion }}</span>
            </div>
            <div v-if="currentNotice?.newVersion" class="version-info">
              <span class="version-label">新版本:</span>
              <span class="version-value new-version">{{ currentNotice.newVersion }}</span>
            </div>
            <div v-if="currentNotice?.pluginType" class="plugin-type">
              <span class="type-label">插件类型:</span>
              <t-tag :theme="currentNotice.pluginType === 'cr' ? 'primary' : 'success'" size="small">
                {{ currentNotice.pluginType === 'cr' ? 'CeruMusic' : 'LX Music' }}
              </t-tag>
            </div>
          </div>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="dialog-actions">
        <t-button theme="default" :disabled="updating" @click="handleClose">稍后更新</t-button>
        <t-button
          v-if="currentNotice?.updateUrl"
          theme="primary"
          :loading="updating"
          @click="handleUpdate"
        >
          立即更新
        </t-button>
      </div>
    </template>
  </t-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted, onBeforeUnmount } from 'vue'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { usePluginUpdateNotice, type PluginUpdateNotice } from '@/composables/usePluginUpdateNotice'
import { usePluginStore } from '@/store/plugin'
import PluginRunner from '@/utils/plugin/PluginRunner'

const { notices, removeNotice, clearNotices } = usePluginUpdateNotice()
const pluginStore = usePluginStore()

const dialogVisible = ref(false)
const updating = ref(false)
const currentIndex = ref(-1)
let nextTimer: ReturnType<typeof setTimeout> | null = null

const currentNotice = computed<PluginUpdateNotice | null>(() => {
  if (currentIndex.value >= 0 && currentIndex.value < notices.value.length) {
    return notices.value[currentIndex.value]
  }
  return null
})

const currentVersion = computed(() => {
  if (!currentNotice.value) return ''
  const plugin = pluginStore.plugins.find(
    (p) => p.plugin_info.name === currentNotice.value!.pluginName
  )
  return plugin?.plugin_info?.version || ''
})

const dialogTitle = computed(() => {
  if (!currentNotice.value) return '插件更新'
  const base = currentNotice.value.pluginName
    ? `${currentNotice.value.pluginName} 有新版本可用`
    : '插件更新可用'
  const remaining = notices.value.length - (currentIndex.value + 1)
  if (remaining > 0) {
    return `${base} (还有 ${remaining} 个更新)`
  }
  return base
})

const dialogWidth = computed(() => '480px')

// Watch for new notices and auto-show
watch(
  () => notices.value.length,
  (newLen) => {
    if (newLen > 0 && !dialogVisible.value) {
      currentIndex.value = 0
      dialogVisible.value = true
    }
  },
  { immediate: true }
)

function handleClose() {
  dialogVisible.value = false
  if (currentIndex.value >= 0 && currentIndex.value < notices.value.length) {
    removeNotice(currentIndex.value)
  }
  currentIndex.value = -1
  updating.value = false

  // Show next notice after a short delay
  nextTimer = setTimeout(() => {
    nextTimer = null
    if (notices.value.length > 0) {
      currentIndex.value = 0
      dialogVisible.value = true
    }
  }, 300)
}

async function handleUpdate() {
  if (!currentNotice.value?.updateUrl) return
  updating.value = true

  try {
    const notice = currentNotice.value
    const pluginType = notice.pluginType || 'cr'

    // Find the matching plugin to get targetPluginId for replacement
    const existingPlugin = pluginStore.plugins.find(
      (p) => p.plugin_info.name === notice.pluginName
    )
    const targetPluginId = existingPlugin?.plugin_id

    await pluginStore.downloadAndAdd(notice.updateUrl, pluginType, targetPluginId)
    PluginRunner.clearCache(targetPluginId)

    MessagePlugin.success(`插件 "${notice.pluginName || '未知插件'}" 更新成功！`)
    handleClose()
  } catch (e: any) {
    console.error('[PluginUpdateNotice] 更新失败:', e)
    const notice = currentNotice.value
    DialogPlugin.confirm({
      header: '自动更新失败',
      body: `插件更新失败（${e.message || '未知错误'}），是否在浏览器中打开下载页面？`,
      confirmBtn: '打开浏览器',
      cancelBtn: '取消',
      onConfirm: () => {
        if (notice?.updateUrl) {
          window.open(notice.updateUrl)
        }
        handleClose()
      },
      onCancel: () => {
        // Stay on current dialog
      },
    })
  } finally {
    updating.value = false
  }
}

onBeforeUnmount(() => {
  if (nextTimer) {
    clearTimeout(nextTimer)
    nextTimer = null
  }
})

onUnmounted(() => {
  clearNotices()
})
</script>

<style scoped lang="scss">
.plugin-update-notice-content {
  .notice-message {
    .message-text {
      margin: 0 0 16px 0;
      font-size: 14px;
      line-height: 1.5;
      color: var(--td-text-color-primary);
    }

    .update-info {
      background: var(--td-bg-color-container);
      border-radius: 6px;
      padding: 16px;
      border: 1px solid var(--td-border-level-1-color);

      .version-info {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;

        &:last-child {
          margin-bottom: 0;
        }

        .version-label {
          font-size: 12px;
          color: var(--td-text-color-secondary);
        }

        .version-value {
          font-size: 12px;
          font-weight: 500;
          color: var(--td-text-color-primary);

          &.new-version {
            color: var(--td-brand-color);
          }
        }
      }

      .plugin-type {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-top: 8px;
        padding-top: 8px;
        border-top: 1px solid var(--td-border-level-2-color);

        .type-label {
          font-size: 12px;
          color: var(--td-text-color-secondary);
        }
      }
    }
  }
}

.dialog-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

@media (max-width: 768px) {
  .dialog-actions {
    flex-direction: column-reverse;

    :deep(.t-button) {
      width: 100%;
    }
  }
}

:deep(.t-dialog) {
  .t-dialog__header {
    border-bottom: 1px solid var(--td-border-level-1-color);
  }

  .t-dialog__footer {
    border-top: 1px solid var(--td-border-level-1-color);
  }
}
</style>

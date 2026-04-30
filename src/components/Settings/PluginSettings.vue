<template>
  <div class="plugin-section">
    <div class="section-header">
      <h2>插件管理</h2>
      <div class="header-actions">
        <t-button theme="primary" @click="showAddDialog = true">添加插件</t-button>
        <t-button variant="outline" @click="store.refresh()">刷新</t-button>
      </div>
    </div>

    <div v-if="store.loading && store.plugins.length === 0" class="loading">
      <t-loading size="small" />
    </div>

    <div v-else-if="store.plugins.length === 0" class="empty-state">
      <p>暂无插件</p>
      <p class="hint">点击"添加插件"导入 CeruMusic 或洛雪音乐的插件</p>
    </div>

    <div v-else class="plugin-list">
      <div v-for="plugin in store.plugins" :key="plugin.plugin_id" class="plugin-card">
        <div class="plugin-card-left">
          <div class="plugin-header">
            <span class="plugin-name">{{ plugin.plugin_info.name }}</span>
            <span class="plugin-version">v{{ plugin.plugin_info.version }}</span>
            <t-tag v-if="currentPluginId === plugin.plugin_id" theme="success" size="small">当前插件</t-tag>
          </div>
          <div class="plugin-author">作者：{{ plugin.plugin_info.author }}</div>
          <div v-if="plugin.plugin_info.description" class="plugin-desc">
            {{ plugin.plugin_info.description }}
          </div>
          <div v-if="plugin.supported_sources.length > 0" class="plugin-sources">
            <t-tag
              v-for="source in plugin.supported_sources"
              :key="source.name"
              size="small"
              variant="light"
              theme="primary"
            >
              {{ source.name }}
            </t-tag>
          </div>
        </div>
        <div class="plugin-card-right">
          <t-button
            v-if="currentPluginId !== plugin.plugin_id"
            theme="primary"
            variant="outline"
            size="small"
            @click="selectPlugin(plugin)"
          >
            使用
          </t-button>
          <t-button
            theme="danger"
            variant="outline"
            size="small"
            @click="confirmUninstall(plugin)"
          >
            卸载
          </t-button>
        </div>
      </div>
    </div>

    <!-- Add Plugin Dialog -->
    <t-dialog
      v-model:visible="showAddDialog"
      header="添加插件"
      width="500px"
      :footer="false"
    >
      <div v-if="addStep === 1" class="add-step-1">
        <p class="dialog-hint">选择插件类型</p>
        <t-radio-group v-model="pluginType" class="type-group">
          <t-radio value="cr">澜音插件</t-radio>
          <t-radio value="lx">洛雪插件</t-radio>
        </t-radio-group>

        <p class="dialog-hint" style="margin-top: 16px;">选择导入方式</p>
        <div class="import-methods">
          <div class="method-card" @click="importMethod = 'file'">
            <span class="method-icon">📄</span>
            <span class="method-label">本地文件</span>
            <span class="method-desc">导入本地插件文件</span>
          </div>
          <div class="method-card" @click="importMethod = 'url'">
            <span class="method-icon">🌐</span>
            <span class="method-label">在线导入</span>
            <span class="method-desc">通过链接下载插件</span>
          </div>
        </div>

        <div class="dialog-footer" style="margin-top: 20px;">
          <t-button @click="showAddDialog = false">取消</t-button>
          <t-button
            theme="primary"
            :disabled="!importMethod"
            @click="addStep = 2"
          >
            下一步
          </t-button>
        </div>
      </div>

      <div v-else class="add-step-2">
        <template v-if="importMethod === 'url'">
          <p class="dialog-hint">输入插件链接</p>
          <t-input
            v-model="pluginUrl"
            placeholder="https://example.com/plugin.js"
            clearable
          />
        </template>
        <template v-else>
          <p class="dialog-hint">选择本地插件文件</p>
          <t-button variant="outline" @click="selectLocalFile">
            选择文件
          </t-button>
          <div v-if="selectedFileName" class="file-name">{{ selectedFileName }}</div>
          <input
            ref="fileInputRef"
            type="file"
            accept=".js,.txt"
            style="display: none"
            @change="onFileSelected"
          />
        </template>

        <div class="dialog-footer" style="margin-top: 20px;">
          <t-button @click="addStep = 1">上一步</t-button>
          <t-button theme="primary" :loading="adding" @click="doAddPlugin">
            添加
          </t-button>
        </div>
      </div>
    </t-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { usePluginStore } from '@/store/plugin'
import { DialogPlugin, MessagePlugin } from 'tdesign-vue-next'
import type { LoadedPlugin } from '@/store/plugin'

const store = usePluginStore()

const currentPluginId = ref(localStorage.getItem('pluginId') || '')
const showAddDialog = ref(false)
const addStep = ref(1)
const pluginType = ref<'cr' | 'lx'>('cr')
const importMethod = ref<'file' | 'url'>()
const pluginUrl = ref('')
const selectedFileName = ref('')
const selectedFileContent = ref('')
const adding = ref(false)
const fileInputRef = ref<HTMLInputElement>()

onMounted(() => {
  store.initialize()
})

function selectPlugin(plugin: LoadedPlugin) {
  currentPluginId.value = plugin.plugin_id
  localStorage.setItem('pluginId', plugin.plugin_id)
  localStorage.setItem('pluginName', plugin.plugin_info.name)
  MessagePlugin.success(`已切换到插件: ${plugin.plugin_info.name}`)
}

function confirmUninstall(plugin: LoadedPlugin) {
  const dialog = DialogPlugin.confirm({
    header: '确认卸载',
    body: `确定要卸载插件 "${plugin.plugin_info.name}" 吗？`,
    confirmBtn: { content: '卸载', theme: 'danger' },
    cancelBtn: { content: '取消' },
    onConfirm: async () => {
      try {
        await store.uninstallPlugin(plugin.plugin_id)
        if (currentPluginId.value === plugin.plugin_id) {
          currentPluginId.value = ''
          localStorage.removeItem('pluginId')
          localStorage.removeItem('pluginName')
        }
        MessagePlugin.success('插件已卸载')
      } catch (e: any) {
        MessagePlugin.error(e.message || '卸载失败')
      }
    }
  })
}

function selectLocalFile() {
  fileInputRef.value?.click()
}

function onFileSelected(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  selectedFileName.value = file.name
  const reader = new FileReader()
  reader.onload = () => {
    selectedFileContent.value = reader.result as string
  }
  reader.readAsText(file)
}

async function doAddPlugin() {
  adding.value = true
  try {
    if (importMethod.value === 'url') {
      if (!pluginUrl.value.trim()) {
        MessagePlugin.warning('请输入插件链接')
        return
      }
      await store.downloadAndAdd(pluginUrl.value.trim(), pluginType.value)
    } else {
      if (!selectedFileContent.value) {
        MessagePlugin.warning('请选择插件文件')
        return
      }
      const name = selectedFileName.value.replace(/\.\w+$/, '') || '未命名插件'
      await store.addPlugin(selectedFileContent.value, name)
    }
    MessagePlugin.success('插件添加成功')
    showAddDialog.value = false
    resetAddDialog()
  } catch (e: any) {
    MessagePlugin.error(e.message || '添加失败')
  } finally {
    adding.value = false
  }
}

function resetAddDialog() {
  addStep.value = 1
  pluginType.value = 'cr'
  importMethod.value = undefined
  pluginUrl.value = ''
  selectedFileName.value = ''
  selectedFileContent.value = ''
}
</script>

<style scoped>
.plugin-section { height: 100%; }
.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}
.section-header h2 {
  font-size: 1.5rem;
  font-weight: 600;
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  margin: 0;
}
.header-actions { display: flex; gap: 8px; }

.loading { display: flex; justify-content: center; padding: 80px 0; }

.empty-state { text-align: center; padding: 80px 0; color: var(--td-text-color-secondary); }
.empty-state .hint { font-size: 13px; margin-top: 8px; }

.plugin-list { display: flex; flex-direction: column; gap: 12px; }
.plugin-card {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  background: var(--td-bg-color-container);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.04);
}
.plugin-card-left { flex: 1; min-width: 0; }
.plugin-card-right { display: flex; flex-direction: column; gap: 8px; flex-shrink: 0; margin-left: 16px; }
.plugin-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
.plugin-name { font-weight: 600; font-size: 16px; }
.plugin-version { font-size: 12px; color: var(--td-text-color-secondary); }
.plugin-author { font-size: 13px; color: var(--td-text-color-secondary); margin-bottom: 4px; }
.plugin-desc { font-size: 13px; color: var(--td-text-color-placeholder); margin-bottom: 8px; }
.plugin-sources { display: flex; gap: 6px; flex-wrap: wrap; }

.dialog-hint { font-size: 14px; color: var(--td-text-color-secondary); margin-bottom: 12px; }
.type-group { margin-bottom: 4px; }

.import-methods { display: flex; gap: 12px; }
.method-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 20px 12px;
  border: 2px solid var(--td-border-level-1-color);
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.15s;
}
.method-card:hover { border-color: var(--td-brand-color); background: var(--td-brand-color-1); }
.method-icon { font-size: 28px; }
.method-label { font-weight: 500; font-size: 14px; }
.method-desc { font-size: 12px; color: var(--td-text-color-secondary); }

.file-name {
  margin-top: 8px;
  padding: 8px 12px;
  background: var(--td-bg-color-secondarycontainer);
  border-radius: 6px;
  font-size: 13px;
  color: var(--td-text-color-secondary);
}

.dialog-footer { display: flex; justify-content: flex-end; gap: 8px; }
</style>

<template>
  <div class="plugin-section">
    <div class="section-header">
      <h2>插件管理</h2>
      <div class="header-actions">
        <t-button theme="primary" @click="showTypeDialog = true">
          <template #icon><t-icon name="add" /></template> 添加插件
        </t-button>
        <t-button theme="default" @click="doRefresh">
          <template #icon><t-icon name="refresh" /></template> 刷新
        </t-button>
      </div>
    </div>

    <!-- Loading -->
    <div v-if="store.loading && store.plugins.length === 0" class="state-block loading-state">
      <t-loading size="medium" />
      <span>加载中...</span>
    </div>

    <!-- Error -->
    <div v-else-if="error" class="state-block error-state">
      <t-icon name="error-circle" style="font-size: 48px; color: var(--td-error-color)" />
      <p>加载插件时出错</p>
      <p class="error-msg">{{ error }}</p>
      <t-button theme="default" size="small" @click="doRefresh">
        <template #icon><t-icon name="refresh" /></template> 重试
      </t-button>
    </div>

    <!-- Empty -->
    <div v-else-if="store.plugins.length === 0" class="state-block empty-state">
      <t-icon name="app" style="font-size: 48px" />
      <p>暂无已安装的插件</p>
      <p class="hint">点击"添加插件"按钮来安装新插件</p>
    </div>

    <!-- Plugin List -->
    <div v-else class="plugin-list">
      <div
        v-for="plugin in store.plugins"
        :key="plugin.plugin_id"
        class="plugin-card"
        :class="{ selected: store.isSelected(plugin.plugin_id) }"
      >
        <div class="plugin-info">
          <h3>
            {{ plugin.plugin_info.name }}
            <span class="version">v{{ plugin.plugin_info.version }}</span>
            <span v-if="store.isSelected(plugin.plugin_id)" class="tag tag-current">当前使用</span>
            <span v-if="store.isServicePlugin(plugin)" class="tag tag-service">服务插件</span>
          </h3>
          <p class="author">作者: {{ plugin.plugin_info.author }}</p>
          <p class="description">{{ plugin.plugin_info.description || '无描述' }}</p>
          <div v-if="plugin.supported_sources.length > 0" class="sources">
            <span class="source-label">支持的音源:</span>
            <span v-for="source in plugin.supported_sources" :key="source.name" class="source-tag">
              {{ source.name }}
            </span>
          </div>
        </div>
        <div class="plugin-actions">
          <t-button
            theme="default"
            size="small"
            @click="viewLogs(plugin)"
          >
            <template #icon><t-icon name="view-list" /></template> 日志
          </t-button>
          <t-button
            v-if="store.isServicePlugin(plugin)"
            theme="default"
            size="small"
            @click="openConfig(plugin)"
          >
            <template #icon><t-icon name="setting" /></template> 配置
          </t-button>
          <t-button
            v-if="store.isServicePlugin(plugin)"
            theme="primary"
            size="small"
            @click="openImport(plugin)"
          >
            <template #icon><t-icon name="download" /></template> 导入歌单
          </t-button>
          <t-button
            v-if="!store.isSelected(plugin.plugin_id) && !store.isServicePlugin(plugin)"
            theme="primary"
            size="small"
            @click="doSelect(plugin)"
          >
            <template #icon><t-icon name="check" /></template> 使用
          </t-button>
          <t-button
            theme="danger"
            size="small"
            variant="outline"
            @click="confirmUninstall(plugin)"
          >
            <template #icon><t-icon name="delete" /></template> 卸载
          </t-button>
        </div>
      </div>
    </div>

    <!-- Step 1: Select Plugin Type -->
    <t-dialog
      v-model:visible="showTypeDialog"
      header="选择插件类别"
      :confirm-btn="{ content: '下一步' }"
      :cancel-btn="{ content: '取消' }"
      @confirm="goToImportMethod"
    >
      <p class="dialog-tip">Tips: 如果插件提供者有提供澜音插件格式，建议使用澜音格式插件导入</p>
      <t-radio-group v-model="addPluginType" variant="primary-filled">
        <t-radio-button value="cr">澜音插件</t-radio-button>
        <t-radio-button value="lx">洛雪插件</t-radio-button>
      </t-radio-group>
    </t-dialog>

    <!-- Step 2: Select Import Method -->
    <t-dialog
      v-model:visible="showImportDialog"
      header="选择导入方式"
      :confirm-btn="{ content: '确定' }"
      :cancel-btn="{ content: '返回' }"
      @confirm="doImport"
      @cancel="backToTypeSelection"
    >
      <div class="import-method-container">
        <t-radio-group v-model="importMethod" variant="primary-filled">
          <t-radio-button value="local">本地导入</t-radio-button>
          <t-radio-button value="online">在线导入</t-radio-button>
        </t-radio-group>
        <div v-if="importMethod === 'online'" class="online-section">
          <t-input
            v-model="onlineUrl"
            placeholder="请输入插件下载地址"
            size="large"
          />
          <p class="dialog-tip">支持 HTTP/HTTPS 链接，插件文件应为 .js 格式</p>
        </div>
        <div v-else class="dialog-tip">
          Tips: 点击"确定"将从本地文件选择插件文件进行导入
        </div>
      </div>
    </t-dialog>

    <!-- Log Dialog -->
    <t-dialog
      v-model:visible="logDialogVisible"
      :close-btn="false"
      :footer="false"
      attach="body"
      width="80%"
      :style="{ maxWidth: '900px' }"
      class="log-dialog"
    >
      <template #header>
        <div class="log-dialog-header">
          <div class="log-title">
            <t-icon name="view-list" />
            {{ currentLogName }} - 插件日志
          </div>
          <div class="log-actions">
            <t-button size="small" variant="outline" :loading="logsLoading" @click="refreshLogs">
              刷新
            </t-button>
          </div>
          <div class="mac-controls">
            <div class="mac-btn close" @click="logDialogVisible = false" />
            <div class="mac-btn minimize" />
            <div class="mac-btn maximize" />
          </div>
        </div>
      </template>
      <div class="console-container">
        <div class="console-header">
          <div class="console-info">
            <span class="console-prompt">$</span>
            <span class="console-path">~/plugins/{{ currentLogName }}</span>
            <span class="console-time">{{ formatTime(new Date()) }}</span>
          </div>
        </div>
        <div ref="logContentRef" class="console-content">
          <div v-if="logsLoading" class="console-loading">
            <t-loading size="small" />
            <span>正在加载日志...</span>
          </div>
          <div v-else-if="logsError" class="console-error">
            <span>加载日志失败: {{ logsError }}</span>
          </div>
          <div v-else-if="logs.length === 0" class="console-empty">
            <span>暂无日志记录</span>
          </div>
          <div v-else class="log-entries">
            <div
              v-for="(log, i) in logs"
              :key="i"
              class="log-entry"
              :class="getLogLevel(log)"
            >
              <span class="log-ts">{{ formatLogTime(i) }}</span>
              <span class="log-content">{{ log }}</span>
            </div>
          </div>
        </div>
      </div>
    </t-dialog>

    <!-- Config Dialog -->
    <t-dialog
      v-model:visible="configDialogVisible"
      :header="`${configPluginName} - 配置`"
      :confirm-btn="{ content: '保存' }"
      :cancel-btn="{ content: '取消' }"
      @confirm="doSaveConfig"
    >
      <div class="config-form">
        <div v-for="field in configSchema" :key="field.key" class="config-field">
          <label class="config-label">
            {{ field.label }}
            <span v-if="field.required" class="required">*</span>
          </label>
          <t-input
            v-if="field.type === 'text'"
            v-model="configValues[field.key]"
            :placeholder="field.placeholder || ''"
          />
          <t-input
            v-else-if="field.type === 'password'"
            v-model="configValues[field.key]"
            type="password"
            :placeholder="field.placeholder || ''"
          />
          <t-input-number
            v-else-if="field.type === 'number'"
            v-model="configValues[field.key]"
            :placeholder="field.placeholder || ''"
            style="width: 100%"
          />
          <t-select
            v-else-if="field.type === 'select'"
            v-model="configValues[field.key]"
            :placeholder="field.placeholder || '请选择'"
          >
            <t-option
              v-for="opt in field.options"
              :key="opt.value"
              :value="opt.value"
              :label="opt.label"
            />
          </t-select>
        </div>
        <div class="config-test">
          <t-button
            theme="default"
            size="small"
            :loading="configTesting"
            @click="doTestConnection"
          >
            测试连接
          </t-button>
          <span
            v-if="configTestResult"
            class="test-result"
            :class="{ success: configTestResult.success, fail: !configTestResult.success }"
          >
            {{ configTestResult.message }}
          </span>
        </div>
      </div>
    </t-dialog>

    <!-- Import Playlist -->
    <ImportPlaylist
      v-model:visible="importDialogVisible"
      :plugin-id="importPluginId"
      :plugin-name="importPluginName"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'
import { usePluginStore } from '@/store/plugin'
import type { LoadedPlugin, PluginConfigField } from '@/store/plugin'
import ImportPlaylist from '@/components/ServicePlugin/ImportPlaylist.vue'

const store = usePluginStore()
const error = ref<string | null>(null)

// Add plugin flow
const showTypeDialog = ref(false)
const showImportDialog = ref(false)
const addPluginType = ref<'cr' | 'lx'>('cr')
const importMethod = ref<'local' | 'online'>('local')
const onlineUrl = ref('')

// Log dialog
const logDialogVisible = ref(false)
const currentLogId = ref('')
const currentLogName = ref('')
const logs = ref<string[]>([])
const logsLoading = ref(false)
const logsError = ref<string | null>(null)
const logContentRef = ref<HTMLElement>()

// Config dialog
const configDialogVisible = ref(false)
const configPluginId = ref('')
const configPluginName = ref('')
const configSchema = ref<PluginConfigField[]>([])
const configValues = ref<Record<string, any>>({})
const configTesting = ref(false)
const configTestResult = ref<{ success: boolean; message: string } | null>(null)

// Import dialog
const importDialogVisible = ref(false)
const importPluginId = ref('')
const importPluginName = ref('')

onMounted(() => {
  doRefresh()
})

async function doRefresh() {
  error.value = null
  try {
    await store.initialize()
  } catch (e: any) {
    error.value = e.message || '未知错误'
  }
}

// ==================== Add Plugin ====================

function goToImportMethod() {
  showTypeDialog.value = false
  showImportDialog.value = true
}

function backToTypeSelection() {
  showImportDialog.value = false
  showTypeDialog.value = true
  onlineUrl.value = ''
}

async function doImport() {
  try {
    showImportDialog.value = false
    if (importMethod.value === 'local') {
      const result = await store.selectAndAdd(addPluginType.value)
      if (!result) return // canceled
      MessagePlugin.success(`插件 "${result.plugin_info.name}" 安装成功！`)
    } else {
      if (!onlineUrl.value.trim()) {
        MessagePlugin.warning('请输入插件下载地址')
        showImportDialog.value = true
        return
      }
      try { new URL(onlineUrl.value) } catch {
        MessagePlugin.warning('请输入有效的URL地址')
        showImportDialog.value = true
        return
      }
      const result = await store.downloadAndAdd(onlineUrl.value.trim(), addPluginType.value)
      MessagePlugin.success(`插件 "${result.plugin_info.name}" 安装成功！`)
    }
    onlineUrl.value = ''
  } catch (e: any) {
    const msg = typeof e === 'string' ? e : (e.message || '未知错误')
    MessagePlugin.error(`安装插件失败: ${msg}`)
  }
}

// ==================== Select / Uninstall ====================

function doSelect(plugin: LoadedPlugin) {
  store.selectPlugin(plugin)
  MessagePlugin.success(`已选择插件: ${plugin.plugin_info.name}`)
}

function confirmUninstall(plugin: LoadedPlugin) {
  DialogPlugin.confirm({
    header: '确认卸载',
    body: `确定要卸载插件 "${plugin.plugin_info.name}" 吗？`,
    confirmBtn: { content: '确认卸载', theme: 'danger' },
    cancelBtn: { content: '取消' },
    onConfirm: async () => {
      try {
        await store.uninstallPlugin(plugin.plugin_id)
        MessagePlugin.success(`插件 "${plugin.plugin_info.name}" 卸载成功！`)
      } catch (e: any) {
        MessagePlugin.error(`卸载失败: ${e.message}`)
      }
    }
  })
}

// ==================== Logs ====================

async function viewLogs(plugin: LoadedPlugin) {
  currentLogId.value = plugin.plugin_id
  currentLogName.value = plugin.plugin_info.name
  logDialogVisible.value = true
  await loadLogs()
}

async function loadLogs() {
  if (!currentLogId.value) return
  logsLoading.value = true
  logsError.value = null
  try {
    logs.value = await store.getPluginLog(currentLogId.value)
    await nextTick()
    if (logContentRef.value) {
      logContentRef.value.scrollTop = logContentRef.value.scrollHeight
    }
  } catch (e: any) {
    logsError.value = e.message
    logs.value = []
  } finally {
    logsLoading.value = false
  }
}

function refreshLogs() { loadLogs() }

function getLogLevel(log: string): string {
  const l = log.toLowerCase()
  if (l.includes('error') || l.includes('错误')) return 'log-error'
  if (l.includes('warn') || l.includes('警告')) return 'log-warn'
  if (l.includes('info') || l.includes('信息')) return 'log-info'
  if (l.includes('debug') || l.includes('调试')) return 'log-debug'
  return 'log-default'
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString('zh-CN', { hour12: false })
}

function formatLogTime(index: number): string {
  const now = new Date()
  const t = new Date(now.getTime() - (logs.value.length - index - 1) * 1000)
  return t.toLocaleTimeString('zh-CN', { hour12: false })
}

// ==================== Config ====================

async function openConfig(plugin: LoadedPlugin) {
  configPluginId.value = plugin.plugin_id
  configPluginName.value = plugin.plugin_info.name
  configTestResult.value = null
  try {
    configSchema.value = await store.getConfigSchema(plugin.plugin_id)
    const saved = await store.getConfig(plugin.plugin_id)
    const values: Record<string, any> = {}
    for (const field of configSchema.value) {
      values[field.key] = saved[field.key] ?? field.default ?? ''
    }
    configValues.value = values
    configDialogVisible.value = true
  } catch (e: any) {
    MessagePlugin.error(`获取插件配置失败: ${e.message}`)
  }
}

async function doSaveConfig() {
  try {
    for (const field of configSchema.value) {
      if (field.required && !configValues.value[field.key]) {
        MessagePlugin.warning(`请填写 ${field.label}`)
        return
      }
    }
    await store.saveConfig(configPluginId.value, JSON.parse(JSON.stringify(configValues.value)))
    MessagePlugin.success('配置已保存')
    configDialogVisible.value = false
  } catch (e: any) {
    MessagePlugin.error(`保存配置失败: ${e.message}`)
  }
}

async function doTestConnection() {
  configTesting.value = true
  configTestResult.value = null
  try {
    await store.saveConfig(configPluginId.value, JSON.parse(JSON.stringify(configValues.value)))
    configTestResult.value = await store.testConnection(configPluginId.value)
    if (configTestResult.value.success) {
      MessagePlugin.success(configTestResult.value.message)
    } else {
      MessagePlugin.error(configTestResult.value.message)
    }
  } catch (e: any) {
    configTestResult.value = { success: false, message: e.message }
    MessagePlugin.error(`测试连接失败: ${e.message}`)
  } finally {
    configTesting.value = false
  }
}

// ==================== Import Playlist ====================

function openImport(plugin: LoadedPlugin) {
  importPluginId.value = plugin.plugin_id
  importPluginName.value = plugin.plugin_info.name
  importDialogVisible.value = true
}
</script>

<style scoped lang="scss">
.plugin-section {
  height: 100%;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
  flex-shrink: 0;

  h2 {
    font-size: 24px;
    font-weight: 600;
    border-left: 8px solid var(--td-brand-color-3);
    padding-left: 12px;
    border-radius: 8px;
    line-height: 1.5em;
    margin: 0;
  }
}

.header-actions {
  display: flex;
  gap: 12px;
}

// State blocks
.state-block {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 0;
  gap: 12px;
  background: var(--td-bg-color-container);
  border-radius: 12px;
  color: var(--td-text-color-secondary);
}

.error-msg {
  color: var(--td-error-color);
  font-size: 14px;
  max-width: 80%;
  text-align: center;
}

.hint {
  font-size: 14px;
  color: var(--td-text-color-placeholder);
}

// Plugin list
.plugin-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.plugin-card {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 20px;
  border-radius: 12px;
  background: var(--td-bg-color-container);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  transition: all 0.3s ease;
  border: 2px solid transparent;

  &.selected {
    border-color: var(--td-brand-color);
    background: var(--td-brand-color-1);
  }
}

.plugin-info {
  flex: 1;
  margin-right: 20px;

  h3 {
    margin: 0 0 8px 0;
    font-size: 18px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 10px;
    line-height: 1.4;
  }
}

.version {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  font-weight: 500;
  background: var(--td-bg-color-secondarycontainer);
  padding: 2px 8px;
  border-radius: 6px;
}

.tag {
  padding: 3px 10px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 500;
  color: white;

  &.tag-current {
    background: linear-gradient(135deg, var(--td-brand-color-5), var(--td-brand-color-6));
    box-shadow: 0 2px 4px rgba(0, 167, 77, 0.2);
  }

  &.tag-service {
    background: linear-gradient(135deg, #5b8def, #3a6ed8);
    box-shadow: 0 2px 4px rgba(58, 110, 216, 0.2);
  }
}

.author {
  margin: 0 0 4px 0;
  font-size: 14px;
  color: var(--td-text-color-secondary);
}

.description {
  margin: 0 0 8px 0;
  font-size: 14px;
  color: var(--td-text-color-placeholder);
  line-height: 1.5;
}

.sources {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.source-label {
  font-size: 13px;
  color: var(--td-text-color-placeholder);
  font-weight: 500;
}

.source-tag {
  background: linear-gradient(135deg, var(--td-brand-color-4), var(--td-brand-color-5));
  color: white;
  padding: 3px 10px;
  border-radius: 10px;
  font-size: 12px;
  font-weight: 500;
}

.plugin-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 120px;
}

// Dialog tips
.dialog-tip {
  font-size: 13px;
  color: var(--td-text-color-placeholder);
  margin-bottom: 16px;
  line-height: 1.5;
}

.import-method-container {
  padding: 8px 0;
}

.online-section {
  margin-top: 16px;
}

// Config form
.config-form {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 8px 0;
}

.config-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.config-label {
  font-size: 14px;
  font-weight: 500;

  .required {
    color: var(--td-error-color);
    margin-left: 2px;
  }
}

.config-test {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--td-border-level-1-color);

  .test-result {
    font-size: 13px;
    &.success { color: var(--td-success-color); }
    &.fail { color: var(--td-error-color); }
  }
}

// Log dialog
.log-dialog-header {
  display: flex;
  align-items: center;
  padding: 12px 20px;
  background: var(--td-bg-color-secondarycontainer);
  width: 100%;

  .log-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 14px;
    flex: 1;
  }

  .log-actions {
    display: flex;
    gap: 8px;
    margin-right: 12px;
  }

  .mac-controls {
    display: flex;
    gap: 8px;

    .mac-btn {
      width: 12px;
      height: 12px;
      border-radius: 50%;
      cursor: pointer;

      &.close { background: #ff5f57; &:hover { background: #ff3b30; } }
      &.minimize { background: #ffbd2e; &:hover { background: #ff9500; } }
      &.maximize { background: #28c840; &:hover { background: #30d158; } }
    }
  }
}

.console-container {
  background: #1e1e1e;
  color: #d4d4d4;
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', 'Consolas', monospace;
  font-size: 13px;
  line-height: 1.4;
  min-height: 300px;
  display: flex;
  flex-direction: column;
  border-radius: 0 0 12px 12px;
}

.console-header {
  background: #2d2d2d;
  border-bottom: 1px solid #3e3e3e;
  padding: 8px 16px;

  .console-info {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;

    .console-prompt { color: #4ec9b0; font-weight: bold; }
    .console-path { color: #9cdcfe; }
    .console-time { color: #6a9955; margin-left: auto; }
  }
}

.console-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  max-height: 50vh;

  &::-webkit-scrollbar { width: 8px; }
  &::-webkit-scrollbar-track { background: #1e1e1e; }
  &::-webkit-scrollbar-thumb { background: #424242; border-radius: 4px; &:hover { background: #555; } }
}

.console-loading,
.console-error,
.console-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 40px 0;
  color: #808080;
}

.console-error { color: #f44747; }

.log-entries {
  .log-entry {
    display: flex;
    margin-bottom: 4px;
    padding: 2px 0;
    border-radius: 3px;

    &:hover { background: rgba(255, 255, 255, 0.05); }

    .log-ts {
      color: #6a9955;
      font-size: 11px;
      width: 80px;
      text-align: center;
      flex-shrink: 0;
      margin-right: 12px;
      font-weight: 500;
    }

    .log-content {
      flex: 1;
      word-break: break-all;
      white-space: pre-wrap;
      user-select: text;
    }

    &.log-error .log-content { color: #f44747; }
    &.log-warn .log-content { color: #cca700; }
    &.log-info .log-content { color: #4fc1ff; }
    &.log-debug .log-content { color: #608b4e; }
    &.log-default .log-content { color: #d4d4d4; }
  }
}

// Responsive
@media (max-width: 768px) {
  .plugin-card {
    flex-direction: column;
    gap: 16px;

    .plugin-info { margin-right: 0; }
    .plugin-actions { flex-direction: row; justify-content: flex-end; min-width: auto; }
  }
}
</style>

<style lang="scss">
.log-dialog {
  .t-dialog {
    border-radius: 12px;
    overflow: hidden;
  }
  .t-dialog__header {
    padding: 0;
    background: var(--td-bg-color-secondarycontainer);
    border-bottom: 1px solid var(--td-border-level-1-color);
  }
  .t-dialog__body {
    padding: 0;
  }
}
</style>

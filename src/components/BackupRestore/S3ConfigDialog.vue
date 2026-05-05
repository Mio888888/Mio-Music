<template>
  <Teleport to="body">
    <Transition name="glass-fade">
      <div v-if="visible" class="liquid-glass-overlay" @click.self="handleClose">
        <div class="overlay-drag-bar" data-tauri-drag-region />
        <div class="liquid-glass-panel" @click.stop>
          <!-- Animated refraction border -->
          <div class="glass-border-glow" />
          <!-- Light sweep -->
          <div class="glass-light-sweep" />
          <!-- Ambient glow -->
          <div class="glass-ambient" />

          <!-- Header -->
          <div class="glass-header" data-tauri-drag-region>
            <div class="glass-title-group">
              <div class="glass-icon">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
                </svg>
              </div>
              <div class="glass-title-text">
                <h2 class="glass-title">备份 & 恢复</h2>
                <div class="glass-status">
                  <span :class="['status-indicator', { connected: store.isConnected }]" />
                  <span class="status-label">{{ store.statusText }}</span>
                </div>
              </div>
            </div>
            <button class="glass-close-btn" @click="handleClose">
              <i class="iconfont icon-a-quxiaoguanbi" />
            </button>
          </div>

          <!-- Tabs -->
          <div class="glass-tabs">
            <button
              :class="['glass-tab', { active: activeTab === 'recommend' }]"
              @click="activeTab = 'recommend'"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
              </svg>
              <span>推荐</span>
            </button>
            <button
              :class="['glass-tab', { active: activeTab === 'config' }]"
              @click="activeTab = 'config'"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
              <span>连接配置</span>
            </button>
            <button
              :class="['glass-tab', { active: activeTab === 'ops' }]"
              :disabled="!store.isConnected"
              @click="activeTab = 'ops'"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" />
              </svg>
              <span>备份操作</span>
            </button>
          </div>

          <!-- Tab: Recommend -->
          <div v-show="activeTab === 'recommend'" class="glass-content">
            <div class="recommend-intro">
              选择一个免费 S3 兼容存储服务，注册后填入连接配置即可使用。
            </div>

            <div class="recommend-list">
              <div class="recommend-card" @click="openLink('https://www.hi168.com')">
                <div class="recommend-card-header">
                  <div class="recommend-badge free">免费</div>
                  <div class="recommend-card-info">
                    <h3>Hi168 对象存储</h3>
                    <span class="recommend-url">hi168.com</span>
                  </div>
                </div>
                <div class="recommend-features">
                  <div class="feature-tag"><b>500 GB</b> 存储空间</div>
                  <div class="feature-tag"><b>10 GB</b> /月流量</div>
                  <div class="feature-tag">S3 兼容</div>
                </div>
                <p class="recommend-desc">提供大容量免费对象存储，适合歌单备份，S3 协议完全兼容。</p>
              </div>

              <div class="recommend-card" @click="openLink('https://data.cstcloud.cn/')">
                <div class="recommend-card-header">
                  <div class="recommend-badge free">免费</div>
                  <div class="recommend-card-info">
                    <h3>数据胶囊 · 中科院</h3>
                    <span class="recommend-url">data.cstcloud.cn</span>
                  </div>
                </div>
                <div class="recommend-features">
                  <div class="feature-tag"><b>20 GB</b> 存储空间</div>
                  <div class="feature-tag">S3 兼容</div>
                  <div class="feature-tag">科研用途</div>
                </div>
                <p class="recommend-desc">中国科学院计算机网络信息中心提供的云存储服务，安全可靠。</p>
              </div>
            </div>

            <div class="glass-hint">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
              <span>点击卡片可前往服务商官网注册，注册后在「连接配置」中填入密钥信息。</span>
            </div>
          </div>

          <!-- Tab: Config -->
          <div v-show="activeTab === 'config'" class="glass-content">
            <div class="form-grid">
              <div class="form-item full">
                <label>服务地址</label>
                <input v-model="store.config.endpoint" type="text" placeholder="https://s3.amazonaws.com" spellcheck="false" />
              </div>
              <div class="form-item">
                <label>区域</label>
                <input v-model="store.config.region" type="text" placeholder="auto" />
              </div>
              <div class="form-item">
                <label>存储桶</label>
                <input v-model="store.config.bucket" type="text" placeholder="my-bucket" />
              </div>
              <div class="form-item">
                <label>访问密钥 ID</label>
                <input v-model="store.config.accessKeyId" type="password" placeholder="AKIA..." />
              </div>
              <div class="form-item">
                <label>访问密钥</label>
                <input v-model="store.config.secretAccessKey" type="password" placeholder="••••••••" />
              </div>
            </div>

            <div v-if="store.errorMessage" class="glass-error">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10" /><line x1="15" y1="9" x2="9" y2="15" /><line x1="9" y1="9" x2="15" y2="15" />
              </svg>
              {{ store.errorMessage }}
            </div>

            <button
              :class="['glass-btn', 'primary', { loading: store.isConnecting }]"
              :disabled="store.isConnecting || !isConfigValid"
              @click="handleTestConnection"
            >
              <template v-if="store.isConnecting">
                <span class="glass-spinner" />
                连接中...
              </template>
              <template v-else-if="store.isConnected">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                已连接 — 点击重新测试
              </template>
              <template v-else>
                测试连接
              </template>
            </button>
          </div>

          <!-- Tab: Ops -->
          <div v-show="activeTab === 'ops'" class="glass-content">
            <!-- Password & Settings -->
            <div class="glass-field-group">
              <div class="form-item">
                <label>备份密码</label>
                <div class="input-wrapper">
                  <input
                    v-model="store.backupPassword"
                    :type="showBackupPwd ? 'text' : 'password'"
                    placeholder="加密备份所需密码"
                    autocomplete="new-password"
                  />
                  <button class="input-toggle" @click="showBackupPwd = !showBackupPwd">
                    <svg v-if="showBackupPwd" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
                    <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
                  </button>
                </div>
              </div>
              <div class="form-item">
                <label>最大保存份数</label>
                <input
                  :value="store.maxBackups"
                  type="number"
                  min="1"
                  max="100"
                  placeholder="10"
                  @input="store.maxBackups = Math.max(1, Math.min(100, parseInt(($event.target as HTMLInputElement)?.value, 10) || 10))"
                  @change="store.saveConfig()"
                />
              </div>
            </div>

            <div class="ops-card">
              <div class="ops-card-header">
                <div class="ops-icon backup">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="17 8 12 3 7 8" /><line x1="12" y1="3" x2="12" y2="15" />
                  </svg>
                </div>
                <div class="ops-card-text">
                  <h3>备份到云端</h3>
                  <p>加密备份歌单、设置和音源插件</p>
                  <span v-if="store.lastBackupTime" class="last-time">
                    上次备份：{{ formatTime(store.lastBackupTime) }}
                  </span>
                </div>
              </div>
              <button
                :class="['glass-btn', 'accent', { loading: store.isBackingUp }]"
                :disabled="store.isBackingUp || !store.backupPassword"
                @click="handleBackup"
              >
                <template v-if="store.isBackingUp">
                  <span class="glass-spinner" />
                  备份中...
                </template>
                <template v-else>
                  加密备份
                </template>
              </button>
            </div>

            <div class="ops-card">
              <div class="ops-card-header">
                <div class="ops-icon restore">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                </div>
                <div class="ops-card-text">
                  <h3>从云端恢复</h3>
                  <p>解密恢复歌单、设置和音源插件</p>
                </div>
              </div>
              <div class="restore-field">
                <div class="input-wrapper">
                  <input
                    v-model="restorePassword"
                    :type="showRestorePwd ? 'text' : 'password'"
                    placeholder="输入恢复密码"
                    autocomplete="current-password"
                  />
                  <button class="input-toggle" @click="showRestorePwd = !showRestorePwd">
                    <svg v-if="showRestorePwd" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
                    <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
                  </button>
                </div>
              </div>
              <div class="restore-actions">
                <button
                  :class="['glass-btn', 'outline', { loading: store.isRestoring }]"
                  :disabled="store.isRestoring || !restorePassword"
                  @click="handleRestore('merge')"
                >
                  <template v-if="store.isRestoring">
                    <span class="glass-spinner" />
                    恢复中...
                  </template>
                  <template v-else>
                    合并恢复
                  </template>
                </button>
                <button
                  :class="['glass-btn', 'danger', { loading: store.isRestoring }]"
                  :disabled="store.isRestoring || !restorePassword"
                  @click="handleRestore('overwrite')"
                >
                  覆盖恢复
                </button>
              </div>
            </div>

            <div v-if="store.errorMessage" class="glass-error">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10" /><line x1="15" y1="9" x2="9" y2="15" /><line x1="9" y1="9" x2="15" y2="15" />
              </svg>
              {{ store.errorMessage }}
            </div>

            <div class="glass-hint">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
              <span>备份采用 AES-256-GCM 加密，包含歌单、设置和音源插件。<b>合并</b> 保留本地数据 &nbsp;|&nbsp; <b>覆盖</b> 替换所有本地数据</span>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useS3BackupStore } from '@/store/S3Backup'

defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', val: boolean): void }>()

const store = useS3BackupStore()
const activeTab = ref<'recommend' | 'config' | 'ops'>('recommend')
const restorePassword = ref('')
const showBackupPwd = ref(false)
const showRestorePwd = ref(false)

const isConfigValid = computed(() =>
  store.config.endpoint &&
  store.config.accessKeyId &&
  store.config.secretAccessKey &&
  store.config.bucket
)

function handleClose() {
  emit('update:visible', false)
}

function openLink(url: string) {
  window.open(url, '_blank')
}

async function handleTestConnection() {
  const ok = await store.testConnection()
  if (ok) activeTab.value = 'ops'
}

async function handleBackup() {
  await store.backup()
}

async function handleRestore(mode: 'overwrite' | 'merge') {
  await store.restore(mode, restorePassword.value)
  window.location.reload()
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso)
    return d.toLocaleString('zh-CN', { hour12: false })
  } catch {
    return iso
  }
}
</script>

<style lang="scss" scoped>
// ========================================
// Liquid Glass Design — S3 Config Dialog
// ========================================

// --- Overlay ---
.liquid-glass-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(8px) saturate(140%);
  -webkit-backdrop-filter: blur(8px) saturate(140%);
}

// --- Overlay Top Drag Bar ---
.overlay-drag-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 38px;
  z-index: 2;
}

// --- Glass Panel ---
.liquid-glass-panel {
  position: relative;
  width: 500px;
  overflow: hidden;
  border-radius: 22px;
  padding: 28px;

  background: linear-gradient(
    165deg,
    rgba(255, 255, 255, 0.72) 0%,
    rgba(255, 255, 255, 0.58) 35%,
    rgba(255, 255, 255, 0.65) 100%
  );
  backdrop-filter: blur(8px) saturate(200%);
  -webkit-backdrop-filter: blur(8px) saturate(200%);

  border: 1.5px solid rgba(255, 255, 255, 0.45);
  box-shadow:
    0 16px 32px rgba(0, 0, 0, 0.22),
    0 6px 14px rgba(0, 0, 0, 0.12),
    0 2px 6px rgba(0, 0, 0, 0.06),
    inset 0 2px 0 rgba(255, 255, 255, 0.6),
    inset 0 -1px 0 rgba(255, 255, 255, 0.1);
}

// --- Animated refraction border ---
.glass-border-glow {
  position: absolute;
  inset: 0;
  border-radius: 22px;
  padding: 1.5px;
  background: conic-gradient(
    from var(--border-angle, 0deg),
    transparent 0%,
    rgba(120, 180, 255, 0.55) 7%,
    rgba(180, 120, 255, 0.45) 14%,
    rgba(255, 120, 180, 0.4) 22%,
    transparent 34%,
    transparent 66%,
    rgba(120, 255, 200, 0.45) 74%,
    rgba(180, 255, 120, 0.4) 82%,
    rgba(120, 180, 255, 0.55) 94%,
    transparent 100%
  );
  mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
  mask-composite: exclude;
  -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
  -webkit-mask-composite: xor;
  will-change: transform; animation: rotate-border 12s linear infinite;
  pointer-events: none;
  z-index: 0;

  @property --border-angle {
    syntax: '<angle>';
    initial-value: 0deg;
    inherits: false;
  }

  @keyframes rotate-border {
    to { --border-angle: 360deg; }
  }
}

// --- Light sweep ---
.glass-light-sweep {
  position: absolute;
  top: 0;
  left: -100%;
  width: 55%;
  height: 100%;
  background: linear-gradient(
    108deg,
    transparent 35%,
    rgba(255, 255, 255, 0.08) 44%,
    rgba(255, 255, 255, 0.15) 50%,
    rgba(255, 255, 255, 0.08) 56%,
    transparent 65%
  );
  will-change: background-position; animation: light-sweep 9s ease-in-out infinite;
  pointer-events: none;
  z-index: 0;
  border-radius: 22px;

  @keyframes light-sweep {
    0%, 100% { left: -100%; }
    50% { left: 160%; }
  }
}

// --- Ambient inner glow ---
.glass-ambient {
  position: absolute;
  top: -30%;
  right: -20%;
  width: 70%;
  height: 70%;
  background: radial-gradient(ellipse, rgba(120, 160, 255, 0.12) 0%, transparent 70%);
  pointer-events: none;
  z-index: 0;
}

// Content above decorative layers
.liquid-glass-panel > *:not(.glass-border-glow):not(.glass-light-sweep):not(.glass-ambient) {
  position: relative;
  z-index: 1;
}

// ==================
// Header
// ==================
.glass-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 18px;
}

.glass-title-group {
  display: flex;
  align-items: center;
  gap: 12px;
}

.glass-icon {
  width: 42px;
  height: 42px;
  border-radius: 13px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, rgba(var(--td-brand-color-rgb, 0, 82, 204), 0.18), rgba(140, 80, 255, 0.12));
  border: 1px solid rgba(255, 255, 255, 0.18);
  box-shadow: 0 3px 10px rgba(100, 140, 255, 0.12);

  svg {
    color: var(--td-brand-color, #0052d9);
    filter: drop-shadow(0 0 3px rgba(100, 140, 255, 0.25));
  }
}

.glass-title-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.glass-title {
  font-size: 17px;
  font-weight: 600;
  margin: 0;
  color: var(--td-text-color-primary);
  line-height: 1.2;
}

.glass-status {
  display: flex;
  align-items: center;
  gap: 5px;
}

.status-indicator {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--td-text-color-disabled);
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;

  &.connected {
    background: var(--td-success-color, #2ba471);
    box-shadow: 0 0 6px rgba(43, 164, 113, 0.5);
  }
}

.status-label {
  font-size: 11px;
  color: var(--td-text-color-primary);
  opacity: 0.55;
}

.glass-close-btn {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.06);
  color: var(--td-text-color-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;

  &:hover {
    background: rgba(255, 80, 80, 0.15);
    border-color: rgba(255, 80, 80, 0.25);
    color: var(--td-error-color, #d54941);
  }

  .iconfont { font-size: 13px; }
}

// ==================
// Tabs
// ==================
.glass-tabs {
  display: flex;
  gap: 3px;
  padding: 3px;
  border-radius: 11px;
  background: rgba(0, 0, 0, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
  margin-bottom: 18px;
}

.glass-tab {
  flex: 1;
  padding: 7px 14px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--td-text-color-secondary);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;

  svg { flex-shrink: 0; }

  &:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
    color: var(--td-text-color-primary);
  }

  &.active {
    background: rgba(255, 255, 255, 0.14);
    color: var(--td-text-color-primary);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06), inset 0 1px 0 rgba(255, 255, 255, 0.2);
  }

  &:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
}

// ==================
// Recommend Tab
// ==================
.recommend-intro {
  font-size: 12.5px;
  color: var(--td-text-color-primary);
  opacity: 0.65;
  margin-bottom: 14px;
  line-height: 1.5;
}

.recommend-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.recommend-card {
  padding: 14px;
  border-radius: 13px;
  background: rgba(255, 255, 255, 0.3);
  border: 1px solid rgba(0, 0, 0, 0.06);
  cursor: pointer;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;

  &:hover {
    background: rgba(255, 255, 255, 0.45);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.06);
  }

  &:active {
    transform: translateY(0);
  }
}

.recommend-card-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.recommend-badge {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 8px;
  border-radius: 6px;
  letter-spacing: 0.02em;
  flex-shrink: 0;

  &.free {
    background: rgba(43, 164, 113, 0.15);
    color: var(--td-success-color, #2ba471);
    border: 1px solid rgba(43, 164, 113, 0.2);
  }
}

.recommend-card-info {
  h3 {
    font-size: 14px;
    font-weight: 600;
    color: var(--td-text-color-primary);
    margin: 0;
    line-height: 1.3;
  }

  .recommend-url {
    font-size: 11px;
    color: var(--td-brand-color, #0052d9);
    opacity: 0.8;
  }
}

.recommend-features {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
}

.feature-tag {
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.04);
  color: var(--td-text-color-primary);
  opacity: 0.7;

  b {
    font-weight: 600;
    opacity: 1;
  }
}

.recommend-desc {
  font-size: 12px;
  color: var(--td-text-color-primary);
  opacity: 0.5;
  margin: 0;
  line-height: 1.5;
}

// ==================
// Content
// ==================
.glass-content {
  animation: content-in 0.2s ease;
}

@keyframes content-in {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

// ==================
// Form
// ==================
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  margin-bottom: 14px;
}

.form-item {
  display: flex;
  flex-direction: column;
  gap: 5px;

  &.full { grid-column: 1 / -1; }

  label {
    font-size: 12px;
    font-weight: 600;
    color: var(--td-text-color-primary);
    opacity: 0.75;
    letter-spacing: 0.01em;
    padding-left: 2px;
  }

  input {
    width: 100%;
    padding: 9px 12px;
    border-radius: 10px;
    border: 1px solid rgba(0, 0, 0, 0.1);
    background: rgba(255, 255, 255, 0.5);
    color: var(--td-text-color-primary);
    font-size: 13px;
    outline: none;
    transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
    box-sizing: border-box;

    &::placeholder {
      color: var(--td-text-color-placeholder);
    }

    &:focus {
      border-color: var(--td-brand-color, #0052d9);
      background: rgba(255, 255, 255, 0.65);
      box-shadow: 0 0 0 3px rgba(0, 82, 204, 0.12);
    }
  }
}

// ==================
// Buttons
// ==================
.glass-btn {
  width: 100%;
  padding: 10px 18px;
  border-radius: 11px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  background: rgba(255, 255, 255, 0.35);
  color: var(--td-text-color-primary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;

  &:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.55);
    transform: translateY(-1px);
    box-shadow: 0 3px 12px rgba(0, 0, 0, 0.08);
  }

  &:active:not(:disabled) {
    transform: translateY(0);
    box-shadow: none;
  }

  &:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  &.primary {
    background: var(--td-brand-color, #0052d9);
    border-color: var(--td-brand-color, #0052d9);
    color: #fff;

    &:hover:not(:disabled) {
      background: var(--td-brand-color-hover, #4787f0);
      border-color: var(--td-brand-color-hover, #4787f0);
      box-shadow: 0 4px 16px rgba(0, 82, 204, 0.3);
    }

    &:active:not(:disabled) {
      background: var(--td-brand-color-active, #003cab);
    }
  }

  &.accent {
    background: linear-gradient(135deg, rgba(43, 164, 113, 0.2), rgba(0, 130, 200, 0.16));
    border-color: rgba(43, 164, 113, 0.25);
    color: var(--td-success-color, #2ba471);

    &:hover:not(:disabled) {
      background: linear-gradient(135deg, rgba(43, 164, 113, 0.28), rgba(0, 130, 200, 0.22));
      box-shadow: 0 3px 16px rgba(43, 164, 113, 0.15);
    }
  }

  &.danger {
    background: rgba(213, 73, 65, 0.12);
    border-color: rgba(213, 73, 65, 0.2);
    color: var(--td-error-color, #d54941);

    &:hover:not(:disabled) {
      background: rgba(213, 73, 65, 0.2);
      box-shadow: 0 3px 16px rgba(213, 73, 65, 0.12);
    }
  }

  &.outline {
    background: transparent;
    border-color: rgba(255, 255, 255, 0.15);
    color: var(--td-text-color-primary);

    &:hover:not(:disabled) {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.22);
    }
  }
}

// ==================
// Glass Field Group (ops tab top)
// ==================
.glass-field-group {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 10px;
  margin-bottom: 12px;

  .form-item:last-child {
    width: 120px;

    input {
      text-align: center;
    }
  }
}

.input-wrapper {
  position: relative;

  input {
    padding-right: 36px !important;
  }
}

.input-toggle {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  padding: 4px;
  cursor: pointer;
  opacity: 0.5;
  display: flex;
  align-items: center;
  justify-content: center;

  &:hover { opacity: 0.8; }

  svg { color: var(--td-text-color-secondary); }
}

.restore-field {
  .input-wrapper input {
    width: 100%;
    padding: 9px 12px 9px 36px;
    border-radius: 10px;
    border: 1px solid rgba(0, 0, 0, 0.1);
    background: rgba(255, 255, 255, 0.5);
    color: var(--td-text-color-primary);
    font-size: 13px;
    outline: none;
    transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
    box-sizing: border-box;

    &::placeholder { color: var(--td-text-color-placeholder); }

    &:focus {
      border-color: var(--td-brand-color, #0052d9);
      background: rgba(255, 255, 255, 0.65);
      box-shadow: 0 0 0 3px rgba(0, 82, 204, 0.12);
    }
  }
}

// ==================
// Ops Cards
// ==================
.ops-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.25);
  border: 1px solid rgba(0, 0, 0, 0.05);

  & + & {
    margin-top: 10px;
  }
}

.ops-card-header {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.ops-card-text {
  flex: 1;
  min-width: 0;

  h3 {
    font-size: 14px;
    font-weight: 600;
    color: var(--td-text-color-primary);
    margin: 0 0 2px;
  }

  p {
    font-size: 12px;
    color: var(--td-text-color-primary);
    opacity: 0.65;
    margin: 0;
    line-height: 1.4;
  }
}

.ops-icon {
  width: 38px;
  height: 38px;
  border-radius: 11px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;

  &.backup {
    background: rgba(43, 164, 113, 0.1);
    border: 1px solid rgba(43, 164, 113, 0.15);
    svg { color: var(--td-success-color, #2ba471); }
  }

  &.restore {
    background: rgba(0, 82, 204, 0.1);
    border: 1px solid rgba(0, 82, 204, 0.15);
    svg { color: var(--td-brand-color, #0052d9); }
  }
}

.last-time {
  font-size: 11px;
  color: var(--td-text-color-primary);
  opacity: 0.5;
  margin-top: 3px;
  display: block;
}

.restore-actions {
  display: flex;
  gap: 8px;

  .glass-btn {
    flex: 1;
  }
}

// ==================
// Error / Hint
// ==================
.glass-error {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 9px 12px;
  border-radius: 9px;
  background: rgba(213, 73, 65, 0.08);
  border: 1px solid rgba(213, 73, 65, 0.15);
  color: var(--td-error-color, #d54941);
  font-size: 12px;
  margin-top: 10px;

  svg { flex-shrink: 0; }
}

.glass-hint {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  padding: 9px 12px;
  border-radius: 9px;
  background: rgba(0, 82, 204, 0.06);
  border: 1px solid rgba(0, 82, 204, 0.12);
  color: var(--td-text-color-primary);
  opacity: 0.7;
  font-size: 11.5px;
  margin-top: 12px;
  line-height: 1.5;

  svg {
    flex-shrink: 0;
    margin-top: 1px;
    color: var(--td-brand-color, #0052d9);
    opacity: 0.7;
  }

  b {
    font-weight: 600;
    color: var(--td-text-color-primary);
  }
}

// ==================
// Spinner
// ==================
.glass-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.15);
  border-top-color: currentColor;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  display: inline-block;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

// ==================
// Transition
// ==================
.glass-fade-enter-active .liquid-glass-panel {
  animation: glass-in 0.28s cubic-bezier(0.16, 1, 0.3, 1);
}
.glass-fade-leave-active .liquid-glass-panel {
  animation: glass-in 0.18s cubic-bezier(0.16, 1, 0.3, 1) reverse;
}
.glass-fade-enter-active,
.glass-fade-leave-active {
  transition: opacity 0.2s;
}
.glass-fade-enter-from,
.glass-fade-leave-to {
  opacity: 0;
}

@keyframes glass-in {
  from {
    opacity: 0;
    transform: scale(0.96) translateY(8px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

// ==================
// Dark Mode
// ==================
:global([data-theme="dark"]) & {
  .liquid-glass-panel {
    background: linear-gradient(
      165deg,
      rgba(50, 50, 68, 0.9) 0%,
      rgba(38, 38, 55, 0.85) 35%,
      rgba(55, 55, 75, 0.88) 100%
    );
    border-color: rgba(255, 255, 255, 0.12);
    box-shadow:
      0 32px 64px rgba(0, 0, 0, 0.55),
      0 12px 28px rgba(0, 0, 0, 0.35),
      0 2px 6px rgba(0, 0, 0, 0.2),
      inset 0 2px 0 rgba(255, 255, 255, 0.15),
      inset 0 -1px 0 rgba(255, 255, 255, 0.04);
  }

  .glass-border-glow {
    opacity: 0.5;
  }

  .glass-ambient {
    background: radial-gradient(ellipse, rgba(120, 160, 255, 0.06) 0%, transparent 70%);
  }

  .glass-tabs {
    background: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.06);
  }

  .glass-tab.active {
    background: rgba(255, 255, 255, 0.1);
  }

  .form-item {
    label {
      opacity: 0.85;
    }

    input {
      background: rgba(0, 0, 0, 0.2);
      border-color: rgba(255, 255, 255, 0.08);

      &:focus {
        background: rgba(0, 0, 0, 0.15);
        border-color: rgba(100, 160, 255, 0.5);
        box-shadow: 0 0 0 3px rgba(100, 160, 255, 0.12);
      }
    }
  }

  .ops-card {
    background: rgba(0, 0, 0, 0.15);
    border-color: rgba(255, 255, 255, 0.06);

    p {
      opacity: 0.6;
    }
  }

  .restore-field .input-wrapper input {
    background: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.08);

    &:focus {
      background: rgba(0, 0, 0, 0.15);
      border-color: rgba(100, 160, 255, 0.5);
      box-shadow: 0 0 0 3px rgba(100, 160, 255, 0.12);
    }
  }

  .glass-btn {
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(255, 255, 255, 0.1);

    &:hover:not(:disabled) {
      background: rgba(255, 255, 255, 0.1);
    }

    &.primary {
      background: var(--td-brand-color, #4787f0);
      border-color: var(--td-brand-color, #4787f0);

      &:hover:not(:disabled) {
        background: var(--td-brand-color-hover, #6ba3f5);
        border-color: var(--td-brand-color-hover, #6ba3f5);
      }
    }

    &.accent {
      background: linear-gradient(135deg, rgba(60, 200, 140, 0.2), rgba(60, 160, 255, 0.16));
      border-color: rgba(60, 200, 140, 0.2);
    }
  }

  .glass-hint {
    opacity: 0.8;
  }

  .recommend-card {
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.06);

    &:hover {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .feature-tag {
    background: rgba(255, 255, 255, 0.06);
  }

  .glass-spinner {
    border-color: rgba(255, 255, 255, 0.1);
  }
}
</style>

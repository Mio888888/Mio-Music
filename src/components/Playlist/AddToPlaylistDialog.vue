<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { LocalUserDetailStore, type PlaylistRow } from '@/store/LocalUserDetail'
import { MessagePlugin } from 'tdesign-vue-next'
import type { SongList } from '@/types/audio'
import defaultCover from '/default-cover.png'

const props = defineProps<{
  visible: boolean
  songs: SongList[]
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  'added': [playlistId: string]
}>()

const localUserStore = LocalUserDetailStore()
const loading = ref(false)
const creating = ref(false)
const showCreate = ref(false)
const newName = ref('')
const searchQuery = ref('')

const filteredPlaylists = computed(() => {
  const list = localUserStore.playlists
  if (!searchQuery.value.trim()) return list
  const q = searchQuery.value.trim().toLowerCase()
  return list.filter(pl => pl.name.toLowerCase().includes(q))
})

watch(() => props.visible, async (val) => {
  if (val) {
    searchQuery.value = ''
    showCreate.value = false
    newName.value = ''
    if (!localUserStore.playlistsLoaded) await localUserStore.loadPlaylists()
  }
})

const handleAdd = async (playlist: PlaylistRow) => {
  if (props.songs.length === 0) return
  loading.value = true
  try {
    await localUserStore.addSongsToPlaylist(playlist.id, props.songs)
    MessagePlugin.success(`已添加 ${props.songs.length} 首歌曲到「${playlist.name}」`)
    emit('added', playlist.id)
    emit('update:visible', false)
  } catch {
    MessagePlugin.error('添加失败')
  } finally {
    loading.value = false
  }
}

const handleCreate = async () => {
  if (!newName.value.trim()) return
  creating.value = true
  try {
    const pl = await localUserStore.createPlaylist(newName.value.trim())
    if (pl) {
      await handleAdd(pl)
      newName.value = ''
      showCreate.value = false
    }
  } finally {
    creating.value = false
  }
}

const handleClose = () => {
  emit('update:visible', false)
}
</script>

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
                  <path d="M21 15V6a3 3 0 0 0-3-3H6a3 3 0 0 0-3 3v9" />
                  <path d="M3 15l3.5-3.5a2.12 2.12 0 0 1 3 0L12 14" />
                  <path d="M12 14l2.5-2.5a2.12 2.12 0 0 1 3 0L21 15" />
                  <circle cx="9" cy="8" r="1.5" />
                </svg>
              </div>
              <div class="glass-title-text">
                <h2 class="glass-title">添加到歌单</h2>
                <div class="glass-status">
                  <span class="status-label">{{ songs.length }} 首歌曲</span>
                </div>
              </div>
            </div>
            <button class="glass-close-btn" @click="handleClose">
              <i class="iconfont icon-a-quxiaoguanbi" />
            </button>
          </div>

          <!-- Search -->
          <div class="glass-search">
            <svg class="search-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              v-model="searchQuery"
              placeholder="搜索歌单"
              spellcheck="false"
            />
            <button v-if="searchQuery" class="search-clear" @click="searchQuery = ''">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          <!-- Loading -->
          <div v-if="loading" class="glass-loading">
            <span class="glass-spinner" />
          </div>

          <!-- Empty -->
          <div v-else-if="localUserStore.playlists.length === 0" class="glass-empty">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" style="opacity:0.3">
              <path d="M21 15V6a3 3 0 0 0-3-3H6a3 3 0 0 0-3 3v9" />
              <path d="M3 15l3.5-3.5a2.12 2.12 0 0 1 3 0L12 14" />
              <circle cx="9" cy="8" r="1.5" />
            </svg>
            <p>还没有歌单，快去创建一个吧</p>
            <button class="glass-btn primary" style="width:auto;padding:7px 18px" @click="showCreate = true">新建歌单</button>
          </div>

          <!-- No results -->
          <div v-else-if="filteredPlaylists.length === 0" class="glass-empty">
            <p>没有找到匹配的歌单</p>
          </div>

          <!-- Playlist list -->
          <div v-else class="playlist-list">
            <div
              v-for="pl in filteredPlaylists"
              :key="pl.id"
              class="playlist-item"
              @click="handleAdd(pl)"
            >
              <div class="item-cover">
                <img
                  v-if="pl.coverImgUrl && pl.coverImgUrl !== 'default-cover'"
                  :src="pl.coverImgUrl"
                  alt=""
                />
                <img v-else :src="defaultCover" alt="" />
              </div>
              <div class="item-info">
                <span class="item-name">{{ pl.name }}</span>
                <div class="item-meta">
                  <span class="meta-count">{{ pl.songCount }} 首</span>
                  <span v-if="pl.source" class="meta-source">{{ pl.source }}</span>
                </div>
              </div>
              <svg class="item-arrow" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="9 18 15 12 9 6" />
              </svg>
            </div>
          </div>

          <!-- Create entry -->
          <div class="create-entry">
            <button v-if="!showCreate" class="glass-btn outline create-trigger" @click="showCreate = true">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
              </svg>
              新建歌单
            </button>
            <div v-else class="create-form">
              <input
                v-model="newName"
                placeholder="歌单名称"
                class="create-input"
                @keydown.enter="handleCreate"
              />
              <button class="glass-btn primary" style="width:auto;padding:8px 16px" :disabled="!newName.trim() || creating" @click="handleCreate">
                <span v-if="creating" class="glass-spinner" />
                <template v-else>创建并添加</template>
              </button>
              <button class="glass-btn outline" style="width:auto;padding:8px 14px" @click="showCreate = false; newName = ''">取消</button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style lang="scss" scoped>
// ========================================
// Liquid Glass Design — Add to Playlist Dialog
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
  backdrop-filter: blur(20px) saturate(140%);
  -webkit-backdrop-filter: blur(20px) saturate(140%);
}

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
  width: 440px;
  max-height: 520px;
  overflow: hidden;
  border-radius: 22px;
  padding: 28px;
  display: flex;
  flex-direction: column;

  background: linear-gradient(
    165deg,
    rgba(255, 255, 255, 0.72) 0%,
    rgba(255, 255, 255, 0.58) 35%,
    rgba(255, 255, 255, 0.65) 100%
  );
  backdrop-filter: blur(80px) saturate(200%);
  -webkit-backdrop-filter: blur(80px) saturate(200%);

  border: 1.5px solid rgba(255, 255, 255, 0.45);
  box-shadow:
    0 32px 64px rgba(0, 0, 0, 0.22),
    0 12px 28px rgba(0, 0, 0, 0.12),
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
  animation: rotate-border 12s linear infinite;
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
  animation: light-sweep 9s ease-in-out infinite;
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
  transition: all 0.2s;

  &:hover {
    background: rgba(255, 80, 80, 0.15);
    border-color: rgba(255, 80, 80, 0.25);
    color: var(--td-error-color, #d54941);
  }

  .iconfont { font-size: 13px; }
}

// ==================
// Search
// ==================
.glass-search {
  position: relative;
  margin-bottom: 14px;

  .search-icon {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--td-text-color-placeholder);
    pointer-events: none;
  }

  input {
    width: 100%;
    padding: 10px 36px 10px 38px;
    border-radius: 11px;
    border: 1px solid rgba(0, 0, 0, 0.1);
    background: rgba(255, 255, 255, 0.5);
    color: var(--td-text-color-primary);
    font-size: 13px;
    outline: none;
    transition: all 0.2s;
    box-sizing: border-box;

    &::placeholder { color: var(--td-text-color-placeholder); }

    &:focus {
      border-color: var(--td-brand-color, #0052d9);
      background: rgba(255, 255, 255, 0.65);
      box-shadow: 0 0 0 3px rgba(0, 82, 204, 0.12);
    }
  }

  .search-clear {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    opacity: 0.4;
    display: flex;
    align-items: center;
    color: var(--td-text-color-secondary);

    &:hover { opacity: 0.7; }
  }
}

// ==================
// Loading / Empty
// ==================
.glass-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 48px;
}

.glass-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 36px;
  gap: 12px;
  color: var(--td-text-color-secondary);
  font-size: 13px;
}

// ==================
// Playlist List
// ==================
.playlist-list {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.playlist-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 13px;
  cursor: pointer;
  transition: all 0.2s;
  background: rgba(255, 255, 255, 0.15);

  &:hover {
    background: rgba(255, 255, 255, 0.35);
    transform: translateX(3px);
  }

  &:active {
    transform: translateX(0);
  }
}

.item-cover {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  overflow: hidden;
  flex-shrink: 0;
  background: rgba(0, 0, 0, 0.04);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.item-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.item-name {
  font-size: 13.5px;
  font-weight: 500;
  color: var(--td-text-color-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--td-text-color-placeholder);
}

.meta-source {
  padding: 0 6px;
  border-radius: 5px;
  background: rgba(0, 0, 0, 0.04);
  font-size: 10.5px;
}

.item-arrow {
  flex-shrink: 0;
  color: var(--td-text-color-placeholder);
  opacity: 0.4;
  transition: opacity 0.2s;
}

.playlist-item:hover .item-arrow {
  opacity: 0.7;
}

// ==================
// Create Entry
// ==================
.create-entry {
  padding-top: 14px;
  border-top: 1px solid rgba(0, 0, 0, 0.06);
  margin-top: 14px;
  flex-shrink: 0;
}

.create-trigger {
  width: 100%;
  justify-content: center;
  gap: 6px;
}

.create-form {
  display: flex;
  gap: 8px;
  align-items: center;
}

.create-input {
  flex: 1;
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  background: rgba(255, 255, 255, 0.5);
  color: var(--td-text-color-primary);
  font-size: 13px;
  outline: none;
  transition: all 0.2s;

  &::placeholder { color: var(--td-text-color-placeholder); }

  &:focus {
    border-color: var(--td-brand-color, #0052d9);
    background: rgba(255, 255, 255, 0.65);
    box-shadow: 0 0 0 3px rgba(0, 82, 204, 0.12);
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
  transition: all 0.2s;
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

  .glass-border-glow { opacity: 0.5; }
  .glass-ambient { background: radial-gradient(ellipse, rgba(120, 160, 255, 0.06) 0%, transparent 70%); }

  .glass-search input {
    background: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.08);

    &:focus {
      background: rgba(0, 0, 0, 0.15);
      border-color: rgba(100, 160, 255, 0.5);
      box-shadow: 0 0 0 3px rgba(100, 160, 255, 0.12);
    }
  }

  .playlist-item {
    background: rgba(255, 255, 255, 0.04);

    &:hover { background: rgba(255, 255, 255, 0.08); }
  }

  .meta-source { background: rgba(255, 255, 255, 0.06); }

  .create-entry { border-top-color: rgba(255, 255, 255, 0.06); }

  .create-input {
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

    &:hover:not(:disabled) { background: rgba(255, 255, 255, 0.1); }

    &.primary {
      background: var(--td-brand-color, #4787f0);
      border-color: var(--td-brand-color, #4787f0);
    }
  }

  .glass-spinner { border-color: rgba(255, 255, 255, 0.1); }
}
</style>

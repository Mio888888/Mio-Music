<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { LocalUserDetailStore, type PlaylistRow } from '@/store/LocalUserDetail'
import { MessagePlugin } from 'tdesign-vue-next'
import type { SongList } from '@/types/audio'
import { SearchIcon } from 'tdesign-icons-vue-next'
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
  <t-dialog
    :visible="visible"
    header="添加到歌单"
    :close-btn="true"
    attach="body"
    :footer="false"
    width="480px"
    @close="handleClose"
  >
    <div class="playlist-picker">
      <!-- 搜索栏 -->
      <div class="picker-search">
        <t-input
          v-model="searchQuery"
          placeholder="搜索歌单"
          size="small"
          clearable
        >
          <template #prefix-icon>
            <SearchIcon size="16px" />
          </template>
        </t-input>
      </div>

      <!-- 加载中 -->
      <div v-if="loading" class="picker-loading">
        <t-loading size="small" />
      </div>

      <!-- 空状态 -->
      <div v-else-if="localUserStore.playlists.length === 0" class="picker-empty">
        <img :src="defaultCover" alt="" class="empty-icon" />
        <p>还没有歌单，快去创建一个吧</p>
        <t-button size="small" theme="primary" @click="showCreate = true">新建歌单</t-button>
      </div>

      <!-- 搜索无结果 -->
      <div v-else-if="filteredPlaylists.length === 0" class="picker-empty">
        <p>没有找到匹配的歌单</p>
      </div>

      <!-- 歌单列表 -->
      <template v-else>
        <div class="playlist-list">
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
          </div>
        </div>
      </template>

      <!-- 新建歌单入口 -->
      <div class="create-entry">
        <t-button v-if="!showCreate" variant="text" size="small" @click="showCreate = true">
          <template #icon><i class="iconfont icon-zengjia"></i></template>
          新建歌单
        </t-button>
        <div v-else class="create-form">
          <t-input v-model="newName" placeholder="歌单名称" size="small" style="flex:1" @enter="handleCreate" />
          <t-button size="small" theme="primary" :loading="creating" @click="handleCreate">创建</t-button>
          <t-button size="small" variant="text" @click="showCreate = false; newName = ''">取消</t-button>
        </div>
      </div>
    </div>
  </t-dialog>
</template>

<style scoped>
.playlist-picker {
  max-height: 420px;
  display: flex;
  flex-direction: column;
}

.picker-search {
  margin-bottom: 12px;
  flex-shrink: 0;
}

.picker-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px;
}

.picker-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px;
  gap: 12px;
  color: var(--td-text-color-secondary);
  font-size: 14px;
}

.empty-icon {
  width: 48px;
  height: 48px;
  border-radius: 8px;
  opacity: 0.5;
}

.playlist-list {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  min-height: 0;
}

.playlist-item {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px 12px;
  border-radius: 10px;
  cursor: pointer;
  transition: background 0.15s ease, transform 0.15s ease;
}

.playlist-item:hover {
  background: var(--td-bg-color-component-hover);
  transform: translateX(2px);
}

.playlist-item:active {
  transform: translateX(0);
}

.item-cover {
  width: 48px;
  height: 48px;
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;
  background: var(--td-bg-color-secondarycontainer);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
}

.item-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.item-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.item-name {
  font-size: 14px;
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
  font-size: 12px;
  color: var(--td-text-color-placeholder);
}

.meta-source {
  padding: 0 6px;
  border-radius: 4px;
  background: var(--td-bg-color-secondarycontainer);
  font-size: 11px;
}

.create-entry {
  padding-top: 12px;
  border-top: 1px solid var(--td-border-level-1-color);
  margin-top: 12px;
  flex-shrink: 0;
}

.create-form {
  display: flex;
  gap: 8px;
  align-items: center;
}
</style>

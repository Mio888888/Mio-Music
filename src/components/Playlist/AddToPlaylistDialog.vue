<script setup lang="ts">
import { ref, watch } from 'vue'
import { LocalUserDetailStore, type PlaylistRow } from '@/store/LocalUserDetail'
import { MessagePlugin } from 'tdesign-vue-next'
import type { SongList } from '@/types/audio'

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

watch(() => props.visible, async (val) => {
  if (val) {
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
    width="420px"
    @close="handleClose"
  >
    <div class="playlist-picker">
      <div v-if="loading" class="picker-loading">
        <t-loading size="small" />
      </div>
      <div v-else-if="localUserStore.playlists.length === 0" class="picker-empty">
        <p>还没有歌单</p>
        <t-button size="small" @click="showCreate = true">新建歌单</t-button>
      </div>
      <template v-else>
        <div class="playlist-list">
          <div
            v-for="pl in localUserStore.playlists"
            :key="pl.id"
            class="playlist-item"
            @click="handleAdd(pl)"
          >
            <div class="item-cover">
              <img v-if="pl.coverImgUrl" :src="pl.coverImgUrl" alt="" />
              <i v-else class="iconfont icon-gedan"></i>
            </div>
            <div class="item-info">
              <span class="item-name">{{ pl.name }}</span>
              <span class="item-desc">{{ pl.description || `${pl.source || '本地'}歌单` }}</span>
            </div>
          </div>
        </div>
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
      </template>
    </div>
  </t-dialog>
</template>

<style scoped>
.playlist-picker { max-height: 400px; display: flex; flex-direction: column; }
.picker-loading, .picker-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px; gap: 12px; color: var(--td-text-color-secondary); }
.playlist-list { flex: 1; overflow-y: auto; margin: -8px -16px; padding: 8px 16px; }
.playlist-item { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 8px; cursor: pointer; transition: background 0.15s; }
.playlist-item:hover { background: var(--td-bg-color-component-hover); }
.item-cover { width: 40px; height: 40px; border-radius: 6px; overflow: hidden; flex-shrink: 0; display: flex; align-items: center; justify-content: center; background: var(--td-bg-color-secondarycontainer); }
.item-cover img { width: 100%; height: 100%; object-fit: cover; }
.item-cover .iconfont { font-size: 18px; color: var(--td-text-color-placeholder); }
.item-info { display: flex; flex-direction: column; min-width: 0; }
.item-name { font-size: 14px; color: var(--td-text-color-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.item-desc { font-size: 12px; color: var(--td-text-color-placeholder); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.create-entry { padding-top: 12px; border-top: 1px solid var(--td-border-level-1-color); margin-top: 8px; }
.create-form { display: flex; gap: 8px; align-items: center; }
</style>

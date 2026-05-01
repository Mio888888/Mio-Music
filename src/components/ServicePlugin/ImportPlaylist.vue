<template>
  <t-dialog
    :visible="visible"
    :close-btn="true"
    attach="body"
    :footer="false"
    width="700px"
    @close="emit('update:visible', false)"
  >
    <template #header>
      <span>{{ pluginName }} - 导入歌单</span>
    </template>
    <div class="import-container">
      <div v-if="loading" class="state-block">
        <t-loading size="small" />
        <span>正在获取歌单列表...</span>
      </div>
      <div v-else-if="error" class="state-block">
        <p>{{ error }}</p>
        <t-button theme="default" size="small" @click="loadPlaylists">重试</t-button>
      </div>
      <div v-else-if="playlists.length > 0" class="playlist-list">
        <div v-for="pl in playlists" :key="pl.id" class="playlist-item">
          <div class="playlist-cover">
            <img v-if="pl.coverImg" :src="pl.coverImg" alt="cover" />
            <div v-else class="cover-placeholder">
              <t-icon name="queue-music" />
            </div>
          </div>
          <div class="playlist-info">
            <div class="playlist-name">{{ pl.name }}</div>
            <div class="playlist-meta">{{ pl.songCount }} 首歌曲</div>
            <div v-if="pl.description" class="playlist-desc">{{ pl.description }}</div>
          </div>
          <div class="playlist-action">
            <t-button
              theme="primary"
              size="small"
              :loading="importingId === pl.id"
              :disabled="!!importingId"
              @click="doImport(pl)"
            >
              <template #icon><t-icon name="download" /></template>
              导入
            </t-button>
          </div>
        </div>
      </div>
      <div v-else class="state-block">
        <t-icon name="folder-open" style="font-size: 48px" />
        <p>没有找到歌单</p>
      </div>
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import type { SongList } from '@/types/audio'

interface ServicePlaylist {
  id: string
  name: string
  songCount: number
  coverImg?: string
  description?: string
}

const props = defineProps<{
  visible: boolean
  pluginId: string
  pluginName: string
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
}>()

const playlists = ref<ServicePlaylist[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const importingId = ref<string | null>(null)

watch(
  () => props.visible,
  (val) => {
    if (val && props.pluginId) {
      loadPlaylists()
    }
  }
)

async function loadPlaylists() {
  loading.value = true
  error.value = null
  try {
    // For service plugins, use callMethod to invoke getPlaylists
    const res = await (window as any).api.plugins.callMethod(
      props.pluginId,
      'getPlaylists',
      JSON.stringify([])
    )
    if (!res?.success) {
      throw new Error(res?.error || '获取歌单失败')
    }

    const payload = res.data
    playlists.value = Array.isArray(payload)
      ? payload
      : Array.isArray(payload?.playlists)
        ? payload.playlists
        : []
  } catch (e: any) {
    error.value = e.message || '获取歌单失败'
  } finally {
    loading.value = false
  }
}

async function doImport(pl: ServicePlaylist) {
  importingId.value = pl.id
  try {
    const songsRes = await (window as any).api.plugins.callMethod(
      props.pluginId,
      'getPlaylistSongs',
      JSON.stringify([pl.id])
    )

    if (!songsRes?.success) {
      throw new Error(songsRes?.error || '获取歌单歌曲失败')
    }

    const payload = songsRes.data
    const songs: SongList[] = Array.isArray(payload)
      ? payload
      : Array.isArray(payload?.songs)
        ? payload.songs
        : []

    if (songs.length === 0) {
      MessagePlugin.warning(`歌单 "${pl.name}" 没有可导入歌曲`)
      return
    }

    const localUserStore = LocalUserDetailStore()
    const created = await localUserStore.createPlaylist(
      pl.name,
      `从${props.pluginName}导入，共 ${songs.length} 首`,
      'service'
    )

    if (!created) {
      throw new Error('创建本地歌单失败')
    }

    const added = await localUserStore.addSongsToPlaylist(created.id, songs)
    if (pl.coverImg) {
      await localUserStore.updatePlaylistCover(created.id, pl.coverImg)
    }

    if (added > 0) {
      MessagePlugin.success(`成功导入 ${added} 首歌曲到歌单 "${pl.name}"`)
    } else {
      MessagePlugin.warning(`歌单 "${pl.name}" 没有新增歌曲`)
    }
  } catch (e: any) {
    MessagePlugin.error(`导入失败: ${e.message}`)
  } finally {
    importingId.value = null
  }
}
</script>

<style scoped lang="scss">
.import-container {
  max-height: 500px;
  overflow-y: auto;
  padding: 8px 0;
}

.state-block {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px 0;
  gap: 12px;
  color: var(--td-text-color-secondary);
}

.playlist-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.playlist-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: 8px;
  background: var(--td-bg-color-container);
  transition: background 0.2s;

  &:hover {
    background: var(--td-bg-color-secondarycontainer);
  }
}

.playlist-cover {
  width: 48px;
  height: 48px;
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .cover-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--td-bg-color-secondarycontainer);
    font-size: 24px;
    color: var(--td-text-color-placeholder);
  }
}

.playlist-info {
  flex: 1;
  min-width: 0;
}

.playlist-name {
  font-size: 14px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.playlist-meta {
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  margin-top: 2px;
}

.playlist-desc {
  font-size: 12px;
  color: var(--td-text-color-secondary);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.playlist-action {
  flex-shrink: 0;
}
</style>

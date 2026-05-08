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
      <span>{{ t('common.importPlaylist.title', { name: pluginName }) }}</span>
    </template>
    <div class="import-container">
      <div v-if="loading" class="state-block">
        <t-loading size="small" />
        <span>{{ t('common.importPlaylist.fetchingList') }}</span>
      </div>
      <div v-else-if="error" class="state-block">
        <p>{{ error }}</p>
        <t-button theme="default" size="small" @click="loadPlaylists">{{ t('common.retry') }}</t-button>
      </div>
      <div v-else-if="playlists.length > 0" class="playlist-list">
        <div v-for="pl in playlists" :key="pl.id" class="playlist-item">
          <div class="playlist-cover">
            <img v-if="pl.coverImg" :src="pl.coverImg" alt="cover" loading="lazy" />
            <div v-else class="cover-placeholder">
              <t-icon name="queue-music" />
            </div>
          </div>
          <div class="playlist-info">
            <div class="playlist-name">{{ pl.name }}</div>
            <div class="playlist-meta">{{ t('common.songCount', { count: pl.songCount }) }}</div>
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
              {{ t('common.import') }}
            </t-button>
          </div>
        </div>
      </div>
      <div v-else class="state-block">
        <t-icon name="folder-open" style="font-size: 48px" />
        <p>{{ t('common.importPlaylist.noPlaylistFound') }}</p>
      </div>
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import type { SongList } from '@/types/audio'

const { t } = useI18n()

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
      throw new Error(res?.error || t('common.importPlaylist.fetchFailed'))
    }

    const payload = res.data
    playlists.value = Array.isArray(payload)
      ? payload
      : Array.isArray(payload?.playlists)
        ? payload.playlists
        : []
  } catch (e: any) {
    error.value = e.message || t('common.importPlaylist.fetchFailed')
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
      throw new Error(songsRes?.error || t('common.importPlaylist.fetchSongsFailed'))
    }

    const payload = songsRes.data
    const songs: SongList[] = Array.isArray(payload)
      ? payload
      : Array.isArray(payload?.songs)
        ? payload.songs
        : []

    if (songs.length === 0) {
      MessagePlugin.warning(t('common.importPlaylist.noImportableSongs', { name: pl.name }))
      return
    }

    const localUserStore = LocalUserDetailStore()
    const created = await localUserStore.createPlaylist(
      pl.name,
      t('common.importPlaylist.importFrom', { name: props.pluginName, count: songs.length }),
      'service'
    )

    if (!created) {
      throw new Error(t('common.importPlaylist.createLocalFailed'))
    }

    const added = await localUserStore.addSongsToPlaylist(created.id, songs)
    if (pl.coverImg) {
      await localUserStore.updatePlaylistCover(created.id, pl.coverImg)
    }

    if (added > 0) {
      MessagePlugin.success(t('common.importPlaylist.importSuccess', { count: added, name: pl.name }))
    } else {
      MessagePlugin.warning(t('common.importPlaylist.noNewSongs', { name: pl.name }))
    }
  } catch (e: any) {
    MessagePlugin.error(t('common.importPlaylist.importFailed', { error: e.message }))
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

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { musicSdk } from '@/services/musicSdk'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'

const router = useRouter()
const loading = ref(false)
const playlists = ref<any[]>([])
const localUserStore = LocalUserDetailStore()

const fetchPlaylists = async () => {
  loading.value = true
  try {
    const res = await musicSdk.getHotSonglist()
    playlists.value = (res?.list || []).map((item: any) => ({
      id: item.id, title: item.name, cover: item.img,
      playCount: item.playCount || item.play_count,
      source: item.source, author: item.author, desc: item.desc
    }))
  } catch (e) { console.error('获取歌单失败:', e) }
  finally { loading.value = false }
}

const goToPlaylist = (item: any) => {
  router.push({ name: 'list', params: { id: item.id }, query: { title: item.title, source: item.source, cover: item.cover } })
}

onMounted(() => fetchPlaylists())

watch(() => localUserStore.userSource, () => {
  playlists.value = []
  fetchPlaylists()
}, { deep: true })

const formatCount = (count: any) => {
  const n = typeof count === 'number' ? count : parseInt(count) || 0
  if (n >= 10000) return (n / 10000).toFixed(1) + '万'
  return String(n)
}
</script>

<template>
  <div class="songlist-container">
    <div class="songlist-header">
      <h2>推荐歌单</h2>
    </div>

    <div v-if="loading" class="loading-state">
      <div class="loading-spinner"></div>
      <p>加载中...</p>
    </div>
    <div v-else-if="playlists.length > 0" class="playlist-grid">
      <div v-for="item in playlists" :key="item.id" class="playlist-card" @click="goToPlaylist(item)">
        <div class="card-cover">
          <img :src="item.cover || '/default-cover.png'" :alt="item.title" />
          <div v-if="item.playCount" class="play-count">
            <i class="iconfont icon-bofang"></i> {{ formatCount(item.playCount) }}
          </div>
        </div>
        <div class="card-info">
          <h4 class="card-title">{{ item.title }}</h4>
          <p v-if="item.author" class="card-author">{{ item.author }}</p>
        </div>
      </div>
    </div>
    <div v-else class="empty-state"><p>暂无歌单数据</p></div>
  </div>
</template>

<style scoped>
.songlist-container { width: 100%; height: 100%; display: flex; flex-direction: column; overflow: hidden; padding: 20px; }
.songlist-header { flex-shrink: 0; margin-bottom: 16px; }
.songlist-header h2 { font-size: 20px; font-weight: 600; color: var(--td-text-color-primary); margin: 0; }
.playlist-grid { display: grid; gap: 16px; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); overflow-y: auto; flex: 1; }
.playlist-card { background: var(--td-bg-color-container); border-radius: 12px; overflow: hidden; cursor: pointer; transition: all 0.2s ease; }
.playlist-card:hover { transform: translateY(-2px); box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1); }
.card-cover { position: relative; aspect-ratio: 1; overflow: hidden; }
.card-cover img { width: 100%; height: 100%; object-fit: cover; }
.play-count { position: absolute; top: 8px; right: 8px; background: rgba(0,0,0,0.5); color: #fff; font-size: 11px; padding: 2px 6px; border-radius: 10px; }
.card-info { padding: 10px; }
.card-title { font-size: 13px; font-weight: 500; color: var(--td-text-color-primary); margin: 0 0 4px; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; line-height: 1.4; }
.card-author { font-size: 12px; color: var(--td-text-color-secondary); margin: 0; }
.loading-state { display: flex; flex-direction: column; align-items: center; padding: 60px; }
.loading-spinner { width: 40px; height: 40px; border: 3px solid var(--td-bg-color-component); border-top-color: var(--td-brand-color); border-radius: 50%; animation: spin 1s linear infinite; margin-bottom: 12px; }
.loading-state p { color: var(--td-text-color-secondary); }
@keyframes spin { to { transform: rotate(360deg); } }
.empty-state { display: flex; align-items: center; justify-content: center; min-height: 300px; }
.empty-state p { color: var(--td-text-color-secondary); }
</style>

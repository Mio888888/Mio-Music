<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { playSong } from '@/utils/audio/globaPlayList'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { storeToRefs } from 'pinia'

const localUserStore = LocalUserDetailStore()
const playStatus = useGlobalPlayStatusStore()
const { list } = storeToRefs(localUserStore)

const recentList = computed(() => list.value.slice().reverse().slice(0, 200))

const handlePlay = (song: any) => {
  playStatus.updatePlayerInfo(song)
  playSong(song)
}

const formatDuration = (interval?: string) => interval || '--:--'
</script>

<template>
  <div class="recent-container">
    <div class="recent-header">
      <h2>最近播放 <span class="count">({{ list.length }} 首)</span></h2>
    </div>

    <div v-if="recentList.length > 0" class="song-list">
      <div v-for="(song, index) in recentList" :key="song.songmid || index" class="song-row" @click="handlePlay(song)">
        <span class="song-index">{{ index + 1 }}</span>
        <div class="song-info">
          <span class="song-name">{{ song.name }}</span>
          <span class="song-singer">{{ song.singer }}</span>
        </div>
        <span class="song-duration">{{ formatDuration(song.interval) }}</span>
      </div>
    </div>
    <div v-else class="empty-state">
      <p>还没有播放记录</p>
    </div>
  </div>
</template>

<style scoped>
.recent-container { width: 100%; height: 100%; display: flex; flex-direction: column; overflow: hidden; padding: 20px; }
.recent-header { flex-shrink: 0; margin-bottom: 16px; }
.recent-header h2 { font-size: 20px; font-weight: 600; color: var(--td-text-color-primary); margin: 0; }
.count { font-size: 14px; font-weight: 400; color: var(--td-text-color-secondary); }
.song-list { flex: 1; overflow-y: auto; }
.song-row { display: flex; align-items: center; padding: 10px 12px; cursor: pointer; transition: background 0.15s; border-radius: 6px; }
.song-row:hover { background: var(--td-bg-color-component-hover); }
.song-index { width: 32px; font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; }
.song-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.song-name { font-size: 14px; color: var(--td-text-color-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.song-singer { font-size: 12px; color: var(--td-text-color-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.song-duration { font-size: 12px; color: var(--td-text-color-secondary); flex-shrink: 0; margin-left: 12px; }
.empty-state { display: flex; align-items: center; justify-content: center; min-height: 300px; }
.empty-state p { color: var(--td-text-color-secondary); }
</style>

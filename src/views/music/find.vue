<script setup lang="ts">
import { ref } from 'vue'
import LeaderBord from '@/components/Find/LeaderBord.vue'
import PlaylistCategory from '@/components/Find/PlaylistCategory.vue'

const activeTab = ref<'songlist' | 'leaderboard'>('songlist')
</script>

<template>
  <div class="find-container">
    <div class="page-header">
      <h2>发现音乐</h2>
      <p>探索最新最热的音乐内容</p>
    </div>

    <div class="segment-tabs">
      <button
        class="segment-tab"
        :class="{ active: activeTab === 'songlist' }"
        @click="activeTab = 'songlist'"
      >
        歌单
      </button>
      <button
        class="segment-tab"
        :class="{ active: activeTab === 'leaderboard' }"
        @click="activeTab = 'leaderboard'"
      >
        排行榜
      </button>
    </div>

    <div class="tab-content">
      <PlaylistCategory v-show="activeTab === 'songlist'" />
      <div v-show="activeTab === 'leaderboard'" class="leaderboard-pane">
        <LeaderBord />
      </div>
    </div>
  </div>
</template>

<style scoped>
.find-container {
  padding-top: 1rem;
  width: 100%;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.page-header {
  margin: 0 2rem 1rem;
}

.page-header h2 {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  color: var(--td-text-color-primary);
  margin-bottom: 0.5rem;
  font-size: 1.875rem;
  font-weight: 600;
}

.page-header p {
  color: var(--td-text-color-secondary);
  font-size: 0.85rem;
}

/* 分段控件样式 — 对齐参考项目 n-tabs type="segment" */
.segment-tabs {
  display: flex;
  margin: 0 2rem 1rem;
  background: var(--td-bg-color-secondarycontainer);
  border-radius: 8px;
  padding: 3px;
  gap: 2px;
}

.segment-tab {
  flex: 1;
  padding: 6px 16px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--td-text-color-secondary);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.25s ease;
}

.segment-tab:hover {
  color: var(--td-text-color-primary);
}

.segment-tab.active {
  background: var(--td-bg-color-container);
  color: var(--td-brand-color);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
}

.tab-content {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.leaderboard-pane {
  height: 100%;
  overflow-y: auto;
}
</style>

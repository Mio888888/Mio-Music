<script setup lang="ts">
import PlaylistCategory from '@/components/Find/PlaylistCategory.vue'
</script>

<template>
  <div class="songlist-page">
    <div class="page-header">
      <h2>推荐歌单</h2>
      <p>按分类浏览热门歌单</p>
    </div>
    <PlaylistCategory />
  </div>
</template>

<style scoped>
.songlist-page {
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
</style>

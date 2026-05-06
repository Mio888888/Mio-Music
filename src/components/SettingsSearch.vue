<script setup lang="ts">
import { ref, computed } from 'vue'
import { searchItems, type SearchItem } from '@/views/settings/searchIndex'

const props = defineProps<{
  hiddenCategories?: string[]
}>()

const emit = defineEmits<{
  select: [item: SearchItem]
}>()

const searchQuery = ref('')
const showResults = ref(false)
const selectedIndex = ref(-1)
const recentSearches = ref<string[]>([])

const loadRecentSearches = () => {
  try {
    const saved = localStorage.getItem('settings_recent_searches')
    if (saved) {
      recentSearches.value = JSON.parse(saved)
    }
  } catch {}
  return []
}
loadRecentSearches()

const saveRecentSearch = (query: string) => {
  const idx = recentSearches.value.indexOf(query)
  if (idx !== -1) recentSearches.value.splice(idx, 1)
  recentSearches.value.unshift(query)
  if (recentSearches.value.length > 5) recentSearches.value.pop()
  localStorage.setItem('settings_recent_searches', JSON.stringify(recentSearches.value))
}

const filteredItems = computed(() => {
  if (!searchQuery.value.trim()) return []
  const query = searchQuery.value.toLowerCase().trim()
  return searchItems.filter(
    (item) =>
      !props.hiddenCategories?.includes(item.category) &&
      (item.title.toLowerCase().includes(query) ||
        item.description.toLowerCase().includes(query) ||
        item.keywords.some((k) => k.toLowerCase().includes(query)))
  )
})

const handleInput = () => {
  showResults.value = true
  selectedIndex.value = -1
}

const handleSelect = (item: SearchItem) => {
  saveRecentSearch(item.title)
  emit('select', item)
  showResults.value = false
  searchQuery.value = ''
}

const handleKeydown = (e: KeyboardEvent) => {
  if (!showResults.value) return
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, filteredItems.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter' && selectedIndex.value >= 0) {
    e.preventDefault()
    handleSelect(filteredItems.value[selectedIndex.value])
  } else if (e.key === 'Escape') {
    showResults.value = false
  }
}

const handleBlur = () => {
  setTimeout(() => {
    showResults.value = false
  }, 200)
}

const handleFocus = () => {
  if (searchQuery.value.trim()) {
    showResults.value = true
  }
}

const getCategoryLabel = (category: string) => {
  const labels: Record<string, string> = {
    appearance: '外观',
    ai: 'AI',
    playlist: '播放',
    hotkeys: '快捷键',
    plugins: '插件',
    music: '音乐源',
    storage: '存储',
    about: '关于'
  }
  return labels[category] || category
}
</script>

<template>
  <div class="settings-search">
    <t-input
      v-model="searchQuery"
      placeholder="搜索设置..."
      clearable
      @input="handleInput"
      @keydown="(_val: any, ctx: any) => handleKeydown(ctx.e)"
      @blur="handleBlur"
      @focus="handleFocus"
    >
      <template #prefix-icon>
        <t-icon name="search" />
      </template>
    </t-input>
    <div v-if="showResults && (filteredItems.length > 0 || recentSearches.length > 0)" class="search-results">
      <template v-if="filteredItems.length > 0">
        <div
          v-for="(item, index) in filteredItems"
          :key="item.id"
          class="result-item"
          :class="{ selected: index === selectedIndex }"
          @mousedown="handleSelect(item)"
        >
          <div class="result-content">
            <div class="result-title">{{ item.title }}</div>
            <div class="result-desc">{{ item.description }}</div>
          </div>
          <span class="result-category">{{ getCategoryLabel(item.category) }}</span>
        </div>
      </template>
      <template v-else-if="recentSearches.length > 0 && !searchQuery.trim()">
        <div class="recent-header">最近搜索</div>
        <div
          v-for="query in recentSearches"
          :key="query"
          class="result-item"
          @mousedown="searchQuery = query; handleInput()"
        >
          <div class="result-content">
            <div class="result-title">{{ query }}</div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.settings-search {
  position: relative;
  width: 320px;
}

.search-results {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  max-height: 400px;
  overflow-y: auto;
  background: var(--td-bg-color-container);
  border: 1px solid var(--td-border-level-1-color);
  border-radius: var(--td-radius-medium);
  box-shadow: var(--td-shadow-3);
  z-index: 1000;
  margin-top: 4px;
}

.result-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: background 0.15s;
}

.result-item:hover,
.result-item.selected {
  background: var(--td-bg-color-container-hover);
}

.result-content {
  flex: 1;
  min-width: 0;
}

.result-title {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--td-text-color-primary);
}

.result-desc {
  font-size: 0.75rem;
  color: var(--td-text-color-secondary);
  margin-top: 2px;
}

.result-category {
  font-size: 0.75rem;
  color: var(--td-brand-color);
  background: var(--td-brand-color-1);
  padding: 2px 8px;
  border-radius: 10px;
  flex-shrink: 0;
  margin-left: 8px;
}

.recent-header {
  padding: 0.5rem 1rem;
  font-size: 0.75rem;
  color: var(--td-text-color-placeholder);
  font-weight: 500;
}
</style>

<template>
  <div class="music-cache">
    <t-card hover-shadow :loading="cacheInfo.clearing" title="本地歌曲缓存配置">
      <template #actions>
        已有歌曲缓存大小：{{ cacheInfo.sizeFormatted || '0 B' }}
        <span v-if="cacheInfo.count > 0">（{{ cacheInfo.count }} 个文件）</span>
      </template>
      <div class="card-body">
        <t-button
          size="large"
          :loading="cacheInfo.clearing"
          :disabled="!cacheInfo.count || cacheInfo.count === 0"
          @click="clearCache"
        >
          {{ cacheInfo.clearing ? '正在清除...' : '清除本地缓存' }}
        </t-button>
        <div v-if="!cacheInfo.count || cacheInfo.count === 0" class="no-cache-tip">
          暂无缓存文件
        </div>
      </div>
    </t-card>
  </div>
</template>

<script lang="ts" setup>
import { DialogPlugin, MessagePlugin } from 'tdesign-vue-next'
import { onMounted, ref } from 'vue'

// 定义事件
const emit = defineEmits<{
  'cache-cleared': []
}>()

interface CacheInfo {
  count: number
  size: number
  sizeFormatted: string
  clearing: boolean
}

const cacheInfo = ref<CacheInfo>({
  count: 0,
  size: 0,
  sizeFormatted: '0 B',
  clearing: false
})

// 格式化文件大小
const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(2) + ' ' + units[i]
}

const loadCacheInfo = async (forceRefresh = false) => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const res = await invoke('get_cache_info', { forceRefresh }) as { count: number; size: number }
    if (res) {
      cacheInfo.value = {
        count: res.count || 0,
        size: res.size || 0,
        sizeFormatted: formatSize(res.size || 0),
        clearing: false
      }
    }
  } catch (error) {
    console.error('获取缓存信息失败:', error)
    MessagePlugin.error('获取缓存信息失败')
  }
}

onMounted(() => {
  loadCacheInfo()
})

const clearCache = () => {
  const confirm = DialogPlugin.confirm({
    header: '确认清除缓存吗',
    body: '这可能会导致歌曲加载缓慢，你确定要清除所有缓存吗？',
    confirmBtn: '确定清除',
    cancelBtn: '我再想想',
    placement: 'center',
    onClose: () => {
      confirm.hide()
    },
    onConfirm: async () => {
      confirm.hide()

      try {
        // 显示加载状态
        cacheInfo.value = { ...cacheInfo.value, clearing: true }

        // 执行清除操作
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('clear_cache')

        MessagePlugin.success('缓存清除成功')

        // 发射缓存清除事件
        emit('cache-cleared')

        // 立即重置缓存信息显示
        cacheInfo.value = {
          count: 0,
          size: 0,
          sizeFormatted: '0 B',
          clearing: false
        }

        // 多次尝试重新加载，确保获取到最新状态
        let retryCount = 0
        const maxRetries = 3

        const reloadWithRetry = async () => {
          retryCount++
          await loadCacheInfo(true)

          if (cacheInfo.value.count > 0 && retryCount < maxRetries) {
            setTimeout(reloadWithRetry, 1000)
          }
        }

        // 延迟一下再开始重新加载
        setTimeout(reloadWithRetry, 300)
      } catch (error) {
        console.error('清除缓存失败:', error)
        MessagePlugin.error('清除缓存失败，请重试')
        // 清除加载状态
        cacheInfo.value = { ...cacheInfo.value, clearing: false }
      }
    }
  })
}

// 刷新缓存信息（供父组件调用）
const refreshCacheInfo = async () => {
  await loadCacheInfo(true)
}

// 暴露方法给父组件
defineExpose({
  refreshCacheInfo
})
</script>

<style lang="scss" scoped>
.music-cache {
  width: 100%;

  .card-body {
    padding: 20px;
    text-align: center;

    .no-cache-tip {
      margin-top: 10px;
      color: var(--td-text-color-placeholder);
      font-size: 14px;
    }
  }
}
</style>

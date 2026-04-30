<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { TreeRoundDotIcon } from 'tdesign-icons-vue-next'
import fonts from '@/assets/icon_font/icons'
import type { MusicSource } from '@/types/userInfo'

const emit = defineEmits(['switch-category'])

const userStore = LocalUserDetailStore()
const { userInfo } = storeToRefs(userStore)

const hasPluginData = computed(() => {
  return !!(userInfo.value.pluginId && userInfo.value.supportedSources && Object.keys(userInfo.value.supportedSources).length > 0)
})

const currentPluginName = computed(() => {
  if (!hasPluginData.value) return ''
  return userInfo.value.pluginName || userInfo.value.pluginId || '未知插件'
})

const currentSourceQualities = computed(() => {
  if (!hasPluginData.value || !userInfo.value.selectSources) return []
  const selectedSource = userInfo.value.supportedSources?.[userInfo.value.selectSources]
  return selectedSource?.qualitys || []
})

const qualitySliderValue = ref(0)

const qualityMarks = computed(() => {
  const marks: Record<number, string> = {}
  currentSourceQualities.value.forEach((quality: string, index: number) => {
    marks[index] = String(getQualityDisplayName(quality))
  })
  return marks
})

const globalQualityOptions = computed(() => {
  const sources = userInfo.value.supportedSources || {}
  const keys = Object.keys(sources)
  if (keys.length === 0) return []
  const arrays = keys.map((k) => sources[k].qualitys || [])
  const set = new Set(arrays[0])
  for (let i = 1; i < arrays.length; i++) {
    for (const q of Array.from(set)) {
      if (!arrays[i].includes(q)) set.delete(q)
    }
  }
  return Array.from(set)
})

const globalQualitySelected = ref<string>('')

watch(() => globalQualityOptions.value, (opts) => {
  if (!opts || opts.length === 0) { globalQualitySelected.value = ''; return }
  if (!opts.includes(globalQualitySelected.value)) { globalQualitySelected.value = opts[opts.length - 1] }
}, { immediate: true })

const applyGlobalQuality = (q: string) => {
  if (!q) return
  if (!userInfo.value.sourceQualityMap) userInfo.value.sourceQualityMap = {}
  const sources = userInfo.value.supportedSources || {}
  Object.keys(sources).forEach((key) => {
    const arr = sources[key].qualitys || []
    if (arr.includes(q)) {
      userInfo.value.sourceQualityMap![key] = q
    }
  })
  const currentKey = userInfo.value.selectSources as string
  const arr = sources[currentKey]?.qualitys || []
  if (arr.includes(q)) userInfo.value.selectQuality = q
}

watch(
  [() => userInfo.value.selectQuality, () => currentSourceQualities.value],
  ([newQuality, qualities]) => {
    if (qualities.length > 0 && newQuality) {
      const index = qualities.indexOf(newQuality)
      if (index !== -1) {
        qualitySliderValue.value = index
      } else {
        userInfo.value.selectQuality = qualities[qualities.length - 1]
      }
    }
  },
  { immediate: true }
)

const selectSource = (sourceKey: string) => {
  if (!hasPluginData.value) return
  userInfo.value.selectSources = sourceKey
  const source = userInfo.value.supportedSources?.[sourceKey]
  if (!userInfo.value.sourceQualityMap) userInfo.value.sourceQualityMap = {}
  if (source && source.qualitys && source.qualitys.length > 0) {
    const saved = userInfo.value.sourceQualityMap[sourceKey]
    const useQuality = saved && source.qualitys.includes(saved) ? saved : source.qualitys[source.qualitys.length - 1]
    userInfo.value.sourceQualityMap[sourceKey] = useQuality
    userInfo.value.selectQuality = useQuality
  }
}

const onQualityChange = (value: number | number[]) => {
  const v = Array.isArray(value) ? value[0] : value
  if (currentSourceQualities.value.length > 0 && v >= 0 && v < currentSourceQualities.value.length) {
    const q = currentSourceQualities.value[v]
    userInfo.value.selectQuality = q
    if (!userInfo.value.sourceQualityMap) userInfo.value.sourceQualityMap = {}
    const key = userInfo.value.selectSources as string
    userInfo.value.sourceQualityMap[key] = q
  }
}

const getQualityDisplayName = (quality: string) => {
  const qualityMap: Record<string, string> = {
    low: '标准', standard: '高品质', high: '超高品质', lossless: '无损',
    '128k': '标准 128K', '192k': '高品质 192K', '320k': '超高品质 320K',
    flac: '无损 FLAC', flac24bit: '高解析度无损', hires: '高清臻音',
    atmos: '沉浸环绕声', master: '超清母带'
  }
  return qualityMap[quality] || quality
}

const getQualityDescription = (quality: string) => {
  const descriptions: Record<string, string> = {
    low: '适合网络较慢的环境，节省流量', standard: '平衡音质与文件大小，推荐选择',
    high: '高音质体验，适合有线网络', lossless: '最佳音质体验，需要较好的网络环境',
    '128k': '基础音质，文件较小', '192k': '良好音质，适合大多数场景',
    '320k': '高品质音质，接近CD品质', flac: '无损音质，完美还原原始录音',
    flac24bit: '更饱满清晰的高解析度音质，最高192kHz/24bit', hires: '声音听感加强，96kHz/24bit',
    atmos: '沉浸式空间环绕音感，最高5.1声道', master: '母带级音质,192kHz/24bit'
  }
  return descriptions[quality] || '自定义音质设置'
}

const getCurrentSourceName = () => {
  if (!hasPluginData.value || !userInfo.value.selectSources) return '未选择'
  const source = userInfo.value.supportedSources?.[userInfo.value.selectSources]
  return source?.name || userInfo.value.selectSources
}

const goPlugin = () => { emit('switch-category', 'plugins') }

const getSourceIcon = (key: string) => {
  return fonts[key] || null
}
</script>

<template>
  <div class="settings-section">
    <div v-if="hasPluginData" class="music-config-container">
      <div class="setting-group">
        <div class="plugin-info">
          <span class="plugin-name">当前插件: {{ currentPluginName }}</span>
          <span class="plugin-status">已启用</span>
        </div>
      </div>

      <div id="music-source" class="setting-group">
        <h3>音乐源选择</h3>
        <div class="source-cards">
          <div
            v-for="(source, key) in userInfo.supportedSources"
            :key="key"
            class="source-card"
            :class="{ active: userInfo.selectSources === String(key) }"
            @click="selectSource(String(key))"
          >
            <div class="source-icon">
              <component :is="getSourceIcon(String(key))" v-if="getSourceIcon(String(key))" style="font-size: 2em" />
              <span v-else style="font-size: 1.5em">{{ source.name?.charAt(0) || '?' }}</span>
            </div>
            <div class="source-info">
              <div class="source-name">{{ source.name }}</div>
              <div class="source-type">{{ source.type || '音乐源' }}</div>
            </div>
            <div v-if="userInfo.selectSources === String(key)" class="source-check">
              <i class="iconfont icon-check" />
            </div>
          </div>
        </div>
      </div>

      <div v-if="currentSourceQualities.length > 0" id="music-quality" class="setting-group">
        <h3>音质选择</h3>
        <div class="quality-slider-container">
          <t-slider
            v-model="qualitySliderValue"
            :min="0"
            :max="currentSourceQualities.length - 1"
            :step="1"
            :marks="qualityMarks"
            :label="qualityMarks[qualitySliderValue]"
            class="quality-slider"
            @change="onQualityChange"
          />
        </div>
        <div class="quality-description">
          <p>当前选择: <strong>{{ getQualityDisplayName(userInfo.selectQuality || '') }}</strong></p>
          <p class="quality-hint">{{ getQualityDescription(userInfo.selectQuality || '') }}</p>
        </div>
      </div>

      <div v-if="globalQualityOptions.length > 0" class="setting-group">
        <h3>全局音质（支持交集）</h3>
        <div class="quality-slider-container">
          <t-select v-model="globalQualitySelected" @change="(v: any) => applyGlobalQuality(String(v))">
            <t-option v-for="q in globalQualityOptions" :key="q" :value="q" :label="getQualityDisplayName(q)" />
          </t-select>
        </div>
      </div>

      <div class="setting-group">
        <h3>配置状态</h3>
        <div class="config-status">
          <div class="status-item">
            <span class="status-label">音乐源:</span>
            <span class="status-value">{{ getCurrentSourceName() }}</span>
          </div>
          <div class="status-item">
            <span class="status-label">音质:</span>
            <span class="status-value">{{ getQualityDisplayName(userInfo.selectQuality || '') }}</span>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="plugin-prompt">
      <div class="prompt-icon">
        <TreeRoundDotIcon />
      </div>
      <div class="prompt-content">
        <h4>未检测到插件配置</h4>
        <p>请先安装并选择一个音乐插件，然后返回此处配置音乐源和音质选项。</p>
        <t-button theme="primary" @click="goPlugin">
          <i class="iconfont icon-shezhi" />
          前往插件管理
        </t-button>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.settings-section { animation: fadeInUp 0.4s ease-out; animation-fill-mode: both; }
.setting-group {
  background: var(--settings-group-bg, var(--td-bg-color-container));
  border-radius: 0.75rem; padding: 1.5rem; margin-bottom: 1.5rem;
  border: 1px solid var(--settings-group-border, var(--td-border-level-1-color));
  box-shadow: 0 1px 3px rgba(0,0,0,0.05);
  animation: fadeInUp 0.4s ease-out; animation-fill-mode: both;
  @for $i from 1 through 5 { &:nth-child(#{$i}) { animation-delay: #{$i * 0.1}s; } }
  h3 { margin: 0 0 0.5rem; font-size: 1.125rem; font-weight: 600; color: var(--settings-text-primary, var(--td-text-color-primary)); }
}
.music-config-container {
  .plugin-info {
    display: flex; align-items: center; gap: 1rem; padding: 1rem;
    background: linear-gradient(135deg, var(--td-brand-color-1) 0%, var(--td-brand-color-2) 100%);
    border-radius: 0.75rem; border: 1px solid var(--td-brand-color-3);
    .plugin-name { font-weight: 600; font-size: 1rem; color: var(--td-brand-color-6); }
    .plugin-status { background: var(--td-brand-color-5); color: white; padding: 0.25rem 0.75rem; border-radius: 1rem; font-size: 0.75rem; font-weight: 500; }
  }
  .source-cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 1rem; }
  .source-card {
    display: flex; align-items: center; gap: 1rem; padding: 1rem;
    background: var(--settings-source-card-bg, var(--td-bg-color-container));
    border: 2px solid var(--settings-source-card-border, var(--td-border-level-1-color));
    border-radius: 0.75rem; cursor: pointer; transition: all 0.2s ease;
    &:hover { border-color: var(--settings-source-card-hover-border, var(--td-brand-color-3)); box-shadow: 0 4px 6px -1px rgba(0,0,0,0.1); }
    &.active { border-color: var(--settings-source-card-active-border, var(--td-brand-color)); background: var(--settings-source-card-active-bg, var(--td-brand-color-1)); box-shadow: 0 0 0 3px var(--td-brand-color-2); }
    .source-icon {
      width: 2.5rem; height: 2.5rem; background: var(--settings-source-icon-bg, var(--td-brand-color-1)); border-radius: 50%;
      display: flex; align-items: center; justify-content: center; color: var(--settings-text-secondary, var(--td-text-color-secondary));
    }
    .source-info { flex: 1;
      .source-name { font-weight: 600; font-size: 0.875rem; color: var(--settings-text-primary, var(--td-text-color-primary)); margin-bottom: 0.125rem; }
      .source-type { font-size: 0.75rem; color: var(--settings-text-secondary, var(--td-text-color-secondary)); }
    }
    .source-check { color: var(--td-brand-color-5); font-size: 1.125rem; }
  }
  .quality-slider-container {
    background: var(--settings-quality-container-bg, var(--td-bg-color-page)); padding: 1.5rem; border-radius: 0.75rem;
    border: 1px solid var(--settings-quality-container-border, var(--td-border-level-1-color));
    .quality-slider { margin-bottom: 1rem; }
  }
  .quality-description { text-align: center; margin-top: 1rem;
    p { margin: 0.5rem 0;
      &:first-child { font-size: 1rem; font-weight: 600; color: var(--settings-text-primary, var(--td-text-color-primary)); }
      &.quality-hint { font-size: 0.875rem; color: var(--settings-text-secondary, var(--td-text-color-secondary)); }
    }
  }
  .config-status { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;
    .status-item { display: flex; justify-content: space-between; align-items: center; padding: 1rem; background: var(--settings-status-item-bg, var(--td-bg-color-page)); border-radius: 0.5rem; border: 1px solid var(--settings-status-item-border, var(--td-border-level-1-color));
      .status-label { font-weight: 500; color: var(--settings-text-secondary, var(--td-text-color-secondary)); font-size: 0.875rem; }
      .status-value { font-weight: 600; color: var(--settings-text-primary, var(--td-text-color-primary)); font-size: 0.875rem; }
    }
  }
}
.plugin-prompt {
  display: flex; align-items: center; gap: 1.5rem; padding: 2rem;
  background: var(--settings-plugin-prompt-bg, var(--td-bg-color-container)); border-radius: 1rem; border: 2px dashed var(--settings-plugin-prompt-border, var(--td-border-level-1-color));
  .prompt-icon {
    width: 3rem; height: 3rem; background: linear-gradient(135deg, var(--td-brand-color-5) 0%, var(--td-brand-color-6) 100%);
    border-radius: 50%; display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; color: white; font-size: 1.5rem;
  }
  .prompt-content {
    flex: 1;
    h4 { color: var(--settings-text-primary, var(--td-text-color-primary)); margin: 0 0 0.5rem 0; font-size: 1.125rem; font-weight: 600; }
    p { color: var(--settings-text-secondary, var(--td-text-color-secondary)); margin: 0 0 1.5rem 0; line-height: 1.5; }
  }
}
@media (max-width: 768px) {
  .music-config-container {
    .source-cards { grid-template-columns: 1fr; }
    .config-status { grid-template-columns: 1fr; }
  }
  .plugin-prompt { flex-direction: column; text-align: center; gap: 1rem; }
}
@keyframes fadeInUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
</style>

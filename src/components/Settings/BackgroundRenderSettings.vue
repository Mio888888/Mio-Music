<script setup lang="ts">
import { computed, ref } from 'vue'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'
import type { BackgroundRenderPreset } from '@/types/background'
import { BACKGROUND_PRESETS } from '@/types/background'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

// 确保配置存在
if (!settings.value.backgroundRender) {
  settings.value.backgroundRender = {
    fullPlay: {
      preset: 'auto',
      enabled: true,
      audioResponse: true,
      renderScale: 0.5,
      flowSpeed: 1.0,
      staticMode: false,
      fps: 30
    },
    desktopLyric: {
      preset: 'performance',
      enabled: false,
      audioResponse: false,
      renderScale: 0.3,
      flowSpeed: 0.5,
      staticMode: true,
      fps: 15
    }
  }
}

const bgSettings = computed(() => settings.value.backgroundRender!)

// FullPlay 配置
const fullPlayConfig = computed(() => bgSettings.value.fullPlay)

// 桌面歌词配置（可折叠）
const showDesktopLyricSettings = ref(false)
const desktopLyricConfig = computed(() => bgSettings.value.desktopLyric)

// 预设选项
const presetOptions = computed(() => [
  { label: '自动', value: 'auto', desc: '根据设备性能自动调整' },
  { label: '性能模式', value: 'performance', desc: '降低效果以提升性能' },
  { label: '质量模式', value: 'quality', desc: '最佳视觉效果，消耗更多资源' },
  { label: '自定义', value: 'custom', desc: '手动调整所有参数' }
])

// 更新 FullPlay 配置
const updateFullPlayConfig = (key: string, value: any) => {
  settingsStore.updateSettings({
    backgroundRender: {
      ...bgSettings.value,
      fullPlay: {
        ...fullPlayConfig.value,
        [key]: value
      }
    }
  })
}

// 更新桌面歌词配置
const updateDesktopLyricConfig = (key: string, value: any) => {
  settingsStore.updateSettings({
    backgroundRender: {
      ...bgSettings.value,
      desktopLyric: {
        ...desktopLyricConfig.value,
        [key]: value
      }
    }
  })
}

// 应用预设
const applyPreset = (preset: BackgroundRenderPreset, target: 'fullPlay' | 'desktopLyric') => {
  if (preset === 'auto' || preset === 'custom') {
    // auto 和 custom 只更新 preset 字段
    if (target === 'fullPlay') {
      updateFullPlayConfig('preset', preset)
    } else {
      updateDesktopLyricConfig('preset', preset)
    }
    return
  }

  // 应用预设配置
  const presetConfig = BACKGROUND_PRESETS[preset]
  const updateFunc = target === 'fullPlay' ? updateFullPlayConfig : updateDesktopLyricConfig

  updateFunc('preset', preset)
  updateFunc('enabled', presetConfig.enabled)
  updateFunc('audioResponse', presetConfig.audioResponse)
  updateFunc('renderScale', presetConfig.renderScale)
  updateFunc('flowSpeed', presetConfig.flowSpeed)
  updateFunc('staticMode', presetConfig.staticMode)
  updateFunc('fps', presetConfig.fps)
}

// 重置配置
const resetConfig = (target: 'fullPlay' | 'desktopLyric') => {
  if (target === 'fullPlay') {
    applyPreset('auto', 'fullPlay')
  } else {
    applyPreset('performance', 'desktopLyric')
  }
}
</script>

<template>
  <div class="background-render-settings">
    <!-- FullPlay 背景效果 -->
    <div class="settings-section">
      <h3>全屏播放 - 背景效果</h3>

      <!-- 预设选择 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">预设模式</div>
          <div class="item-desc">选择预设快速配置，或选择"自定义"手动调整</div>
        </div>
        <t-select
          :value="fullPlayConfig.preset"
          :options="presetOptions"
          @change="(val) => applyPreset(val as BackgroundRenderPreset, 'fullPlay')"
          style="width: 200px"
        />
      </div>

      <!-- 总开关 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">启用背景效果</div>
          <div class="item-desc">开启后显示动态背景</div>
        </div>
        <t-switch
          :value="fullPlayConfig.enabled"
          @change="updateFullPlayConfig('enabled', $event)"
        />
      </div>

      <!-- 音频响应 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">音频响应</div>
          <div class="item-desc">背景随音乐低频跳动，增强沉浸感</div>
        </div>
        <t-switch
          :value="fullPlayConfig.audioResponse"
          @change="updateFullPlayConfig('audioResponse', $event)"
          :disabled="!fullPlayConfig.enabled"
        />
      </div>

      <!-- 渲染强度 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">效果强度</div>
          <div class="item-desc">
            渲染缩放比例 ({{ fullPlayConfig.renderScale.toFixed(1) }})
            <span v-if="fullPlayConfig.renderScale <= 0.3" class="tag-low">低</span>
            <span v-else-if="fullPlayConfig.renderScale <= 0.6" class="tag-medium">中</span>
            <span v-else class="tag-high">高</span>
          </div>
        </div>
        <t-slider
          :value="fullPlayConfig.renderScale"
          :min="0.1"
          :max="1.0"
          :step="0.1"
          @change="updateFullPlayConfig('renderScale', $event)"
          :disabled="!fullPlayConfig.enabled || fullPlayConfig.preset !== 'custom'"
          style="width: 200px"
        />
      </div>

      <!-- 流动速度 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">流动速度</div>
          <div class="item-desc">背景动画速度 ({{ fullPlayConfig.flowSpeed.toFixed(1) }})</div>
        </div>
        <t-slider
          :value="fullPlayConfig.flowSpeed"
          :min="0.1"
          :max="5.0"
          :step="0.1"
          @change="updateFullPlayConfig('flowSpeed', $event)"
          :disabled="!fullPlayConfig.enabled || fullPlayConfig.preset !== 'custom'"
          style="width: 200px"
        />
      </div>

      <!-- 静态模式 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">静态模式</div>
          <div class="item-desc">开启后背景几乎静止，降低性能消耗</div>
        </div>
        <t-switch
          :value="fullPlayConfig.staticMode"
          @change="updateFullPlayConfig('staticMode', $event)"
          :disabled="!fullPlayConfig.enabled || fullPlayConfig.preset !== 'custom'"
        />
      </div>

      <!-- FPS 限制 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">帧率限制</div>
          <div class="item-desc">FPS 上限 ({{ fullPlayConfig.fps }})</div>
        </div>
        <t-slider
          :value="fullPlayConfig.fps"
          :min="15"
          :max="60"
          :step="5"
          @change="updateFullPlayConfig('fps', $event)"
          :disabled="!fullPlayConfig.enabled || fullPlayConfig.preset !== 'custom'"
          style="width: 200px"
        />
      </div>

      <!-- 重置按钮 -->
      <div class="setting-item">
        <div class="item-info">
          <div class="item-title">重置配置</div>
          <div class="item-desc">恢复到默认预设</div>
        </div>
        <t-button size="small" variant="outline" @click="resetConfig('fullPlay')">
          重置
        </t-button>
      </div>
    </div>

    <!-- 桌面歌词背景效果（可折叠） -->
    <div class="settings-section">
      <div class="section-header" @click="showDesktopLyricSettings = !showDesktopLyricSettings">
        <h3>桌面歌词 - 背景效果</h3>
        <t-icon :name="showDesktopLyricSettings ? 'chevron-up' : 'chevron-down'" />
      </div>

      <div v-if="showDesktopLyricSettings" class="section-content">
        <!-- 预设选择 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">预设模式</div>
          </div>
          <t-select
            :value="desktopLyricConfig.preset"
            :options="presetOptions"
            @change="(val) => applyPreset(val as BackgroundRenderPreset, 'desktopLyric')"
            style="width: 200px"
          />
        </div>

        <!-- 总开关 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">启用背景效果</div>
            <div class="item-desc">桌面歌词窗口显示动态背景</div>
          </div>
          <t-switch
            :value="desktopLyricConfig.enabled"
            @change="updateDesktopLyricConfig('enabled', $event)"
          />
        </div>

        <!-- 音频响应 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">音频响应</div>
          </div>
          <t-switch
            :value="desktopLyricConfig.audioResponse"
            @change="updateDesktopLyricConfig('audioResponse', $event)"
            :disabled="!desktopLyricConfig.enabled"
          />
        </div>

        <!-- 渲染强度 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">效果强度</div>
            <div class="item-desc">
              渲染缩放比例 ({{ desktopLyricConfig.renderScale.toFixed(1) }})
            </div>
          </div>
          <t-slider
            :value="desktopLyricConfig.renderScale"
            :min="0.1"
            :max="1.0"
            :step="0.1"
            @change="updateDesktopLyricConfig('renderScale', $event)"
            :disabled="!desktopLyricConfig.enabled || desktopLyricConfig.preset !== 'custom'"
            style="width: 200px"
          />
        </div>

        <!-- 流动速度 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">流动速度</div>
          </div>
          <t-slider
            :value="desktopLyricConfig.flowSpeed"
            :min="0.1"
            :max="5.0"
            :step="0.1"
            @change="updateDesktopLyricConfig('flowSpeed', $event)"
            :disabled="!desktopLyricConfig.enabled || desktopLyricConfig.preset !== 'custom'"
            style="width: 200px"
          />
        </div>

        <!-- 静态模式 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">静态模式</div>
          </div>
          <t-switch
            :value="desktopLyricConfig.staticMode"
            @change="updateDesktopLyricConfig('staticMode', $event)"
            :disabled="!desktopLyricConfig.enabled || desktopLyricConfig.preset !== 'custom'"
          />
        </div>

        <!-- FPS 限制 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">帧率限制</div>
          </div>
          <t-slider
            :value="desktopLyricConfig.fps"
            :min="15"
            :max="60"
            :step="5"
            @change="updateDesktopLyricConfig('fps', $event)"
            :disabled="!desktopLyricConfig.enabled || desktopLyricConfig.preset !== 'custom'"
            style="width: 200px"
          />
        </div>

        <!-- 重置按钮 -->
        <div class="setting-item">
          <div class="item-info">
            <div class="item-title">重置配置</div>
          </div>
          <t-button size="small" variant="outline" @click="resetConfig('desktopLyric')">
            重置
          </t-button>
        </div>
      </div>
    </div>

    <!-- 性能提示 -->
    <div v-if="fullPlayConfig.renderScale > 0.7 && fullPlayConfig.fps > 45" class="performance-warning">
      <t-icon name="info-circle" />
      <span>当前配置较高，如果遇到卡顿可以降低效果强度或 FPS</span>
    </div>
  </div>
</template>

<style scoped lang="scss">
.background-render-settings {
  .settings-section {
    background: var(--settings-group-bg);
    border-radius: 0.75rem;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
    border: 1px solid var(--settings-group-border);
    box-shadow: 0 1px 3px var(--settings-group-shadow);

    h3 {
      margin: 0 0 1rem;
      font-size: 1.125rem;
      font-weight: 600;
      color: var(--settings-text-primary);
    }

    .section-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      cursor: pointer;
      user-select: none;

      h3 {
        margin: 0;
      }
    }

    .section-content {
      margin-top: 1rem;
    }
  }

  .setting-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.875rem 1rem;
    border: 1px solid var(--settings-feature-border);
    background: var(--settings-feature-bg);
    border-radius: 0.5rem;
    margin-top: 0.75rem;

    .item-info {
      display: flex;
      flex-direction: column;
      gap: 0.25rem;
      flex: 1;
      margin-right: 1rem;

      .item-title {
        font-weight: 600;
        color: var(--settings-text-primary);
        font-size: 0.95rem;
        line-height: 1.2;
      }

      .item-desc {
        color: var(--settings-text-secondary);
        font-size: 0.8rem;
        line-height: 1.2;
      }
    }
  }

  .performance-warning {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: rgba(255, 152, 0, 0.1);
    border: 1px solid rgba(255, 152, 0, 0.3);
    border-radius: 0.5rem;
    color: #ff9800;
    font-size: 0.875rem;

    .t-icon {
      flex-shrink: 0;
    }
  }

  .tag-low,
  .tag-medium,
  .tag-high {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: 600;
    margin-left: 0.5rem;
  }

  .tag-low {
    background: rgba(76, 175, 80, 0.15);
    color: #4caf50;
  }

  .tag-medium {
    background: rgba(255, 193, 7, 0.15);
    color: #ffc107;
  }

  .tag-high {
    background: rgba(244, 67, 54, 0.15);
    color: #f44336;
  }
}
</style>

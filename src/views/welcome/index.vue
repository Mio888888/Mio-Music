<template>
  <div class="welcome-container" :class="{ 'is-newyear': showNewYear }">
    <!-- 背景装饰 -->
    <div class="bg-decoration">
      <div class="bg-circle circle-1"></div>
      <div class="bg-circle circle-2"></div>
    </div>

    <div class="content-wrapper">
      <!-- Logo区域 -->
      <div class="logo-section animate-in-left">
        <div class="logo-wrapper">
          <div class="logo-glow"></div>
          <img v-if="!showNewYear" class="logo-image" src="/default-cover.png" alt="澜音 Music Logo" />
          <img v-else class="logo-image" src="/logo_2026.svg" alt="澜音 Music Logo" />
        </div>
      </div>

      <!-- 内容区域 -->
      <div class="text-section animate-in-right">
        <div v-if="showNewYear" class="newyear-banner">
          <span class="newyear-text">新年快乐</span>
          <span class="newyear-year">2026</span>
        </div>
        <h1 class="brand-title">澜音 Music</h1>
        <p class="brand-subtitle">
          {{
            showNewYear
              ? '每一次播放都是出发，愿你的2026如旋律般自由奔腾，心有所向，皆是坦途。'
              : '一款简洁优雅的音乐播放器'
          }}
        </p>

        <div class="feature-tags animate-in-right-delay">
          <span v-for="(feature, index) in features" :key="index" class="tag">
            {{ feature }}
          </span>
        </div>

        <div class="action-section animate-fade">
          <t-button theme="primary" size="large" @click="$router.push('/home')">
            开始使用
          </t-button>
        </div>
      </div>
    </div>

    <!-- 底部版本信息 -->
    <div class="version-info animate-fade">v1.0.0</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const showNewYear = computed(
  () => settingsStore.shouldUseSpringFestivalTheme() && !settings.value.springFestivalDisabled
)

const features = computed(() =>
  showNewYear.value
    ? ['岁岁长安', '功不唐捐', '马年吉祥', '马越新程']
    : ['Hi-Res Audio', 'Minimalist', 'Plugins', 'Offline']
)
</script>

<style scoped>
.welcome-container {
  width: 100vw;
  height: 100vh;
  background: var(--welcome-bg, #ffffff);
  display: flex;
  justify-content: center;
  align-items: center;
  position: relative;
  overflow: hidden;
  font-family:
    -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  color: var(--td-text-color-primary, #333);
}

.welcome-container.is-newyear .circle-1 {
  background: radial-gradient(circle at 30% 30%, rgba(255, 215, 0, 0.95), rgba(255, 0, 0, 0) 60%);
  opacity: 0.35;
}

.welcome-container.is-newyear .circle-2 {
  background: radial-gradient(circle at 60% 40%, rgba(255, 0, 0, 0.95), rgba(255, 215, 0, 0) 65%);
  opacity: 0.28;
}

.newyear-banner {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.3rem 0.65rem;
  margin-bottom: 1rem;
  border-radius: 999px;
  border: 1px solid rgba(255, 215, 0, 0.22);
  background: linear-gradient(90deg, rgba(255, 0, 0, 0.1), rgba(255, 215, 0, 0.1));
  backdrop-filter: blur(10px);
  box-shadow: 0 10px 28px rgba(255, 0, 0, 0.08);
  position: relative;
  overflow: hidden;
}

.newyear-banner::after {
  content: '';
  position: absolute;
  inset: -40% -60%;
  background: radial-gradient(circle, rgba(255, 215, 0, 0.35) 0 1px, transparent 2px);
  opacity: 0.35;
  animation: nySparkle 2.2s linear infinite;
}

.newyear-text,
.newyear-year {
  position: relative;
  z-index: 1;
  font-size: 0.85rem;
  font-weight: 700;
  letter-spacing: 1px;
}

.newyear-text {
  color: rgba(255, 0, 0, 0.95);
}

.newyear-year {
  font-weight: 900;
  font-size: 0.95rem;
  letter-spacing: 0.5px;
  background: linear-gradient(180deg, #fff0b3, #ffd65a, #ffb84a);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  -webkit-text-stroke: 0.6px rgba(140, 20, 0, 0.35);
  text-shadow: 0 0 10px rgba(255, 215, 0, 0.2);
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New',
    monospace;
}

@keyframes nySparkle {
  0% {
    transform: translateX(-10%) rotate(0deg);
  }
  100% {
    transform: translateX(10%) rotate(180deg);
  }
}

/* 背景装饰 */
.bg-decoration {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  pointer-events: none;
}

.bg-circle {
  position: absolute;
  border-radius: 50%;
  filter: blur(80px);
  opacity: 0.4;
}

.circle-1 {
  width: 60vh;
  height: 60vh;
  top: -10%;
  left: -10%;
  background: linear-gradient(135deg, #a8edea 0%, #fed6e3 100%);
  animation: float 10s infinite ease-in-out;
}

.circle-2 {
  width: 50vh;
  height: 50vh;
  bottom: -10%;
  right: -5%;
  background: linear-gradient(135deg, #e0c3fc 0%, #8ec5fc 100%);
  animation: float 12s infinite ease-in-out reverse;
}

@keyframes float {
  0%, 100% { transform: translate(0, 0); }
  50% { transform: translate(20px, 30px); }
}

/* 内容布局 */
.content-wrapper {
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6rem;
  padding: 0 4rem;
  max-width: 1200px;
}

/* Logo区域 */
.logo-section {
  flex: 0 0 auto;
}

.logo-wrapper {
  position: relative;
  width: 240px;
  height: 240px;
}

.logo-image {
  width: 100%;
  height: 100%;
  object-fit: contain;
  position: relative;
  z-index: 2;
  filter: drop-shadow(0 20px 40px rgba(0, 0, 0, 0.15));
}

.logo-glow {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 150%;
  height: 150%;
  background: radial-gradient(circle, rgba(66, 211, 146, 0.4) 0%, rgba(0, 0, 0, 0) 70%);
  z-index: 1;
  filter: blur(30px);
}

.is-newyear .logo-glow {
  background: radial-gradient(circle, rgba(255, 60, 60, 0.4) 0%, rgba(255, 215, 0, 0) 70%);
}

/* 文字区域 */
.text-section {
  flex: 1;
  max-width: 500px;
}

.brand-title {
  font-size: 4rem;
  font-weight: 800;
  margin: 0 0 0.5rem 0;
  background: linear-gradient(120deg, #42d392, #647eff);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  letter-spacing: -1.5px;
  line-height: 1.1;
}

.is-newyear .brand-title {
  background: linear-gradient(120deg, #ff1f1f, #ffd65a);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

.brand-subtitle {
  font-size: 1.1rem;
  color: var(--td-text-color-secondary, #666);
  font-weight: 400;
  letter-spacing: 2px;
  opacity: 0.8;
  margin: 0 0 2rem 0;
  line-height: 1.6;
}

.feature-tags {
  display: flex;
  gap: 0.75rem;
  flex-wrap: wrap;
  margin-bottom: 2.5rem;
}

.tag {
  padding: 0.4rem 1rem;
  background: rgba(255, 255, 255, 0.5);
  border: 1px solid rgba(0, 0, 0, 0.05);
  border-radius: 100px;
  font-size: 0.8rem;
  color: var(--td-text-color-secondary, #555);
  backdrop-filter: blur(10px);
  transition: all 0.3s ease;
}

.is-newyear .tag {
  background: rgba(255, 60, 60, 0.08);
  border-color: rgba(255, 215, 0, 0.2);
  color: rgba(180, 30, 30, 0.8);
}

.action-section {
  margin-top: 1rem;
}

/* 版本信息 */
.version-info {
  position: absolute;
  bottom: 2rem;
  right: 2rem;
  font-size: 0.75rem;
  color: var(--td-text-color-disabled, #ccc);
  font-family: monospace;
}

/* 动画 */
.animate-in-left {
  animation: slideInLeft 1s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  opacity: 0;
}

.animate-in-right {
  animation: slideInRight 1s cubic-bezier(0.2, 0.8, 0.2, 1) 0.2s forwards;
  opacity: 0;
}

.animate-in-right-delay {
  animation: slideInRight 1s cubic-bezier(0.2, 0.8, 0.2, 1) 0.4s forwards;
  opacity: 0;
}

.animate-fade {
  animation: fadeIn 1s ease 0.6s forwards;
  opacity: 0;
}

@keyframes slideInLeft {
  from { opacity: 0; transform: translateX(-50px); }
  to { opacity: 1; transform: translateX(0); }
}

@keyframes slideInRight {
  from { opacity: 0; transform: translateX(50px); }
  to { opacity: 1; transform: translateX(0); }
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

/* 暗色模式适配 */
@media (prefers-color-scheme: dark) {
  .welcome-container {
    background: #121212;
    color: #fff;
  }
  .bg-circle { opacity: 0.15; }
  .tag {
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.1);
    color: #aaa;
  }
  .is-newyear .tag {
    background: rgba(255, 60, 60, 0.12);
    border-color: rgba(255, 215, 0, 0.15);
    color: rgba(255, 180, 180, 0.8);
  }
}

/* 响应式适配 */
@media (max-width: 900px) {
  .content-wrapper {
    flex-direction: column;
    gap: 3rem;
    text-align: center;
    padding: 0 2rem;
  }
  .text-section { align-items: center; text-align: center; }
  .feature-tags { justify-content: center; }
  .logo-wrapper { width: 160px; height: 160px; }
  .brand-title { font-size: 3rem; }
  .animate-in-left,
  .animate-in-right,
  .animate-in-right-delay {
    animation-name: slideInUp;
  }
}

@keyframes slideInUp {
  from { opacity: 0; transform: translateY(30px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>

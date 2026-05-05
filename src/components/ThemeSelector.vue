<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { useSettingsStore } from '@/store/Settings'

const settingsStore = useSettingsStore()

const themes = [
  { name: 'default', label: '默认', color: '#2ba55b' },
  { name: 'pink', label: '粉色', color: '#fc5e7e' },
  { name: 'blue', label: '蓝色', color: '#57b4ff' },
  { name: 'cyan', label: '青色', color: '#3ac2b8' },
  { name: 'orange', label: '橙色', color: '#fb9458' }
]

const currentTheme = ref('default')
const isDarkMode = ref(false)

const applyTheme = (themeName: string, darkMode: boolean = false) => {
  const documentElement = document.documentElement
  documentElement.removeAttribute('theme-mode')
  documentElement.removeAttribute('data-theme')

  if (themeName !== 'default') {
    documentElement.setAttribute('theme-mode', themeName)
  }

  if (darkMode) {
    documentElement.setAttribute('data-theme', 'dark')
  } else {
    documentElement.setAttribute('data-theme', 'light')
  }

  localStorage.setItem('selected-theme', themeName)
  localStorage.setItem('dark-mode', darkMode.toString())

  settingsStore.updateSettings({ theme: themeName, isDarkMode: darkMode })
  window.dispatchEvent(new CustomEvent('theme-changed'))
}

const selectTheme = (themeName: string) => {
  currentTheme.value = themeName
  applyTheme(themeName, isDarkMode.value)
}

const toggleDarkMode = () => {
  isDarkMode.value = !isDarkMode.value
  applyTheme(currentTheme.value, isDarkMode.value)
}

const detectSystemTheme = () => {
  if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    return true
  }
  return false
}

const loadSavedSettings = () => {
  const savedTheme = localStorage.getItem('selected-theme')
  const savedDarkMode = localStorage.getItem('dark-mode')

  if (savedTheme && themes.some((t) => t.name === savedTheme)) {
    currentTheme.value = savedTheme
  }

  if (savedDarkMode !== null) {
    isDarkMode.value = savedDarkMode === 'true'
  } else {
    isDarkMode.value = detectSystemTheme()
  }

  if (settingsStore.settings.theme) {
    currentTheme.value = settingsStore.settings.theme
  }
  if (typeof settingsStore.settings.isDarkMode !== 'undefined') {
    isDarkMode.value = settingsStore.settings.isDarkMode
  }

  applyTheme(currentTheme.value, isDarkMode.value)
}

watch(
  () => [settingsStore.settings.theme, settingsStore.settings.isDarkMode],
  ([newTheme, newMode]) => {
    if (newTheme && newTheme !== currentTheme.value) {
      currentTheme.value = newTheme as string
      applyTheme(newTheme as string, isDarkMode.value)
    }
    if (typeof newMode !== 'undefined' && newMode !== isDarkMode.value) {
      isDarkMode.value = newMode as boolean
      applyTheme(currentTheme.value, newMode as boolean)
    }
  }
)

onMounted(() => {
  if (window.matchMedia) {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    mediaQuery.addEventListener('change', (e) => {
      const savedDarkMode = localStorage.getItem('dark-mode')
      if (savedDarkMode === null) {
        isDarkMode.value = e.matches
        applyTheme(currentTheme.value, isDarkMode.value)
      }
    })
  }
  loadSavedSettings()
})
</script>

<template>
  <div class="theme-selector">
    <div class="theme-options">
      <div
        v-for="theme in themes"
        :key="theme.name"
        class="theme-option"
        :class="{ active: currentTheme === theme.name }"
        @click="selectTheme(theme.name)"
      >
        <div class="theme-preview" :style="{ backgroundColor: theme.color }"></div>
        <span class="theme-label">{{ theme.label }}</span>
      </div>
    </div>

    <div class="dark-mode-toggle">
      <label class="toggle-switch">
        <input type="checkbox" :checked="isDarkMode" @change="toggleDarkMode" />
        <span class="slider"></span>
        <span class="toggle-label">暗色模式</span>
      </label>
    </div>
  </div>
</template>

<style scoped>
.theme-selector {
  padding: 16px;
  background: var(--td-bg-color-container);
  border-radius: var(--td-radius-medium);
  border: 1px solid var(--td-border-level-1-color);
}

.theme-options {
  display: flex;
  gap: 12px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}

.theme-option {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px;
  border-radius: var(--td-radius-medium);
  cursor: pointer;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
  border: 2px solid transparent;
}

.theme-option:hover {
  background: var(--td-bg-color-container-hover);
}

.theme-option.active {
  border-color: var(--td-brand-color);
  background: var(--td-brand-color-light, rgba(0, 218, 192, 0.1));
}

.theme-preview {
  width: 32px;
  height: 32px;
  border-radius: var(--td-radius-circle);
  border: 2px solid var(--td-border-level-1-color);
}

.theme-label {
  font-size: var(--td-font-size-body-small);
  color: var(--td-text-color-primary);
  font-weight: 500;
}

.dark-mode-toggle {
  padding-top: 16px;
  border-top: 1px solid var(--td-border-level-1-color);
}

.toggle-switch {
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: pointer;
}

.toggle-switch input { display: none; }

.slider {
  position: relative;
  width: 44px;
  height: 24px;
  background: var(--td-bg-color-component);
  border-radius: 12px;
  transition: background-color 0.2s ease;
  border: 1px solid var(--td-border-level-1-color);
}

.slider::before {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  background: var(--td-bg-color-container);
  border-radius: 50%;
  transition: transform 0.2s ease;
  box-shadow: var(--td-shadow-1);
}

.toggle-switch input:checked + .slider {
  background: var(--td-brand-color);
}

.toggle-switch input:checked + .slider::before {
  transform: translateX(20px);
}

.toggle-label {
  font-size: var(--td-font-size-body-medium);
  color: var(--td-text-color-primary);
  font-weight: 500;
}
</style>

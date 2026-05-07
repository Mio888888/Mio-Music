<template>
  <div class="home">
    <HomeLayout>
      <template #body>
        <router-view v-slot="{ Component }">
          <Transition :name="routeDirection === 'forward' ? 'slide-forward' : 'slide-backward'" mode="out-in">
            <KeepAlive exclude="list">
              <component :is="Component" :key="$route.fullPath" />
            </KeepAlive>
          </Transition>
        </router-view>
      </template>
    </HomeLayout>
    <PlayMusic />
    <PluginUpdateNoticeDialog />
  </div>
</template>

<script setup lang="ts">
import HomeLayout from '@/components/layout/HomeLayout.vue'
import PlayMusic from '@/components/Play/PlayMusic.vue'
import PluginUpdateNoticeDialog from '@/components/Plugin/PluginUpdateNoticeDialog.vue'
import { routeDirection } from '@/router/index'
import { onMounted } from 'vue'
import { usePluginStore } from '@/store/plugin'
import PluginRunner from '@/utils/plugin/PluginRunner'

defineOptions({ name: 'HomeView' })

// Session flag: only trigger update check once per app session
let updateCheckDone = false

onMounted(async () => {
  if (updateCheckDone) return
  updateCheckDone = true

  try {
    const pluginStore = usePluginStore()
    if (pluginStore.plugins.length === 0) {
      await pluginStore.initialize()
    }

    for (const plugin of pluginStore.plugins) {
      try {
        await PluginRunner.reloadPlugin(plugin.plugin_id)
      } catch {
        // Reload failure is non-critical
      }
    }
  } catch (e) {
    console.warn('[HomeView] 插件更新检查失败:', e)
  }
})
</script>

<style scoped>
.home {
  height: 100%;
  overflow: hidden;
}

.slide-forward-enter-active,
.slide-forward-leave-active,
.slide-backward-enter-active,
.slide-backward-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}

.slide-forward-enter-from {
  opacity: 0;
  transform: translateX(20px);
}
.slide-forward-leave-to {
  opacity: 0;
  transform: translateX(-20px);
}

.slide-backward-enter-from {
  opacity: 0;
  transform: translateX(-20px);
}
.slide-backward-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
</style>

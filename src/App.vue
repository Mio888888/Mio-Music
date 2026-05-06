<template>
  <Provider v-if="!$route.path.includes('desktop-lyric')">
    <GlobalBackground />
    <div class="app-route-stage">
      <router-view v-slot="{ Component }">
        <Transition name="fade-page">
          <component :is="Component" />
        </Transition>
      </router-view>
    </div>
  </Provider>
  <router-view v-else />
  <GlobalContextMenu v-if="!$route.path.includes('desktop-lyric')" />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import Provider from '@/components/layout/Provider.vue'
import GlobalBackground from '@/components/GlobalBackground.vue'
import GlobalContextMenu from '@/components/ContextMenu/GlobalContextMenu.vue'
import { useAuthStore } from '@/store/Auth'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'

const router = useRouter()
const authStore = useAuthStore()

onMounted(async () => {
  if (window.location.hash.includes('/desktop-lyric')) return

  LocalUserDetailStore().init()
  const url = new URL(window.location.href)
  const code = url.searchParams.get('code')
  const state = url.searchParams.get('state')

  if (code && state) {
    await authStore.handleCallback(window.location.href)
    window.history.replaceState({}, '', window.location.pathname)
    router.push('/home')
  } else {
    await authStore.init()
  }
})
</script>

<style scoped>
.app-route-stage {
  position: relative;
  height: 100%;
  overflow: hidden;
  background: var(--td-bg-color-page);
}

.fade-page-enter-active {
  transition: opacity var(--motion-duration-quick) var(--motion-ease-out), transform var(--motion-duration-quick) var(--motion-ease-out);
}

.fade-page-leave-active {
  position: absolute;
  inset: 0;
  width: 100%;
  z-index: 1;
  pointer-events: none;
  transition: opacity var(--motion-duration-instant) var(--motion-ease-standard), transform var(--motion-duration-instant) var(--motion-ease-standard);
}

.fade-page-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.fade-page-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (prefers-reduced-motion: reduce) {
  .fade-page-enter-active,
  .fade-page-leave-active {
    transition: opacity var(--motion-duration-instant) var(--motion-ease-standard);
  }

  .fade-page-enter-from,
  .fade-page-leave-to {
    transform: none;
  }
}
</style>

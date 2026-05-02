<template>
  <Provider v-if="!$route.path.includes('desktop-lyric')">
    <GlobalBackground />
    <router-view v-slot="{ Component }">
      <Transition name="fade-page" mode="out-in">
        <component :is="Component" />
      </Transition>
    </router-view>
  </Provider>
  <router-view v-else />
  <GlobalContextMenu />
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
.fade-page-enter-active,
.fade-page-leave-active {
  transition: opacity 0.25s ease;
}
.fade-page-enter-from,
.fade-page-leave-to {
  opacity: 0;
}
</style>

<template>
  <Provider v-if="!$route.path.includes('desktop-lyric')">
    <GlobalBackground />
    <router-view v-slot="{ Component }">
      <Transition
        :enter-active-class="`animate__animated animate__fadeIn pagesApp`"
        :leave-active-class="`animate__animated animate__fadeOut pagesApp`"
      >
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

const router = useRouter()
const authStore = useAuthStore()

onMounted(async () => {
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

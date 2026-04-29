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
import Provider from '@/components/layout/Provider.vue'
import GlobalBackground from '@/components/GlobalBackground.vue'
import GlobalContextMenu from '@/components/ContextMenu/GlobalContextMenu.vue'
</script>

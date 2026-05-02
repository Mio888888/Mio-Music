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
  </div>
</template>

<script setup lang="ts">
import HomeLayout from '@/components/layout/HomeLayout.vue'
import PlayMusic from '@/components/Play/PlayMusic.vue'
import { routeDirection } from '@/router/index'

defineOptions({ name: 'HomeView' })
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

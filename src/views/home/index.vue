<template>
  <div class="home">
    <HomeLayout>
      <template #body>
        <router-view v-slot="{ Component, route }">
          <Transition
            name="page"
            :enter-active-class="`animate__animated ${route.meta.transitionIn} animate__fast`"
            :leave-active-class="`animate__animated ${route.meta.transitionOut} animate__fast`"
          >
            <KeepAlive exclude="list">
              <component :is="Component" />
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
</script>

<style scoped>
.home {
  height: 100%;
  overflow: hidden;
}
</style>

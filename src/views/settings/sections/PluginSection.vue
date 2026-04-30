<script setup lang="ts">
import PluginSettings from '@/components/Settings/PluginSettings.vue'
</script>

<template>
  <div class="settings-section">
    <div id="plugin-settings">
      <PluginSettings />
    </div>
  </div>
</template>

<style lang="scss" scoped>
.settings-section { animation: fadeInUp 0.4s ease-out; animation-fill-mode: both; }
@keyframes fadeInUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
</style>

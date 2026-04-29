<template>
  <Teleport v-if="visible" to="body">
    <div ref="menuRef" class="context-menu" :style="menuStyle" @mouseleave="emit('close')">
      <ul class="context-menu__list">
        <template v-for="item in items" :key="item.id">
          <li v-if="item.separator" class="context-menu__separator"><div class="context-menu__separator-line"></div></li>
          <li v-else class="context-menu__item" :class="{ 'context-menu__item--disabled': item.disabled }" @click="handleClick(item, $event)">
            <div v-if="item.icon" class="context-menu__icon"><component :is="item.icon" size="16" /></div>
            <span class="context-menu__label">{{ item.label }}</span>
          </li>
        </template>
      </ul>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted, type CSSProperties } from 'vue'
import type { ContextMenuProps, ContextMenuItem } from './types'

const props = withDefaults(defineProps<ContextMenuProps>(), {
  visible: false, position: () => ({ x: 0, y: 0 }), items: () => [],
  width: 200, maxHeight: 400, zIndex: 999999
})
const emit = defineEmits<{ 'update:visible': [value: boolean]; close: [] }>()

const menuRef = ref<HTMLElement>()

const menuStyle = computed((): CSSProperties => ({
  left: `${props.position.x}px`, top: `${props.position.y}px`,
  minWidth: `${props.width}px`, zIndex: props.zIndex
}))

const handleClick = (item: ContextMenuItem, event: MouseEvent) => {
  if (item.disabled || item.separator) return
  if (item.onClick) item.onClick(item, event)
  emit('update:visible', false)
  emit('close')
}

const handleGlobalMouseDown = (event: MouseEvent) => {
  if (menuRef.value && !menuRef.value.contains(event.target as Node)) {
    emit('update:visible', false); emit('close')
  }
}

watch(() => props.visible, (v) => {
  if (v) nextTick(() => requestAnimationFrame(() => window.addEventListener('mousedown', handleGlobalMouseDown, true)))
  else window.removeEventListener('mousedown', handleGlobalMouseDown, true)
})
onUnmounted(() => window.removeEventListener('mousedown', handleGlobalMouseDown, true))
</script>

<style scoped>
.context-menu {
  position: fixed; background: #fff; border: 1px solid #e0e0e0;
  border-radius: 8px; box-shadow: 0 4px 20px rgba(0,0,0,0.15);
  backdrop-filter: blur(10px); overflow: auto;
  animation: contextMenuEnter 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}
.context-menu__list { list-style: none; margin: 0; padding: 4px 0; }
.context-menu__item {
  display: flex; align-items: center; padding: 8px 12px; margin: 0 4px;
  border-radius: 4px; cursor: pointer; user-select: none; transition: all 0.2s ease; min-height: 32px;
}
.context-menu__item:hover:not(.context-menu__item--disabled) { background: #f5f5f5; }
.context-menu__item--disabled { opacity: 0.5; cursor: not-allowed; }
.context-menu__icon { display: flex; align-items: center; justify-content: center; width: 16px; height: 16px; margin-right: 8px; color: #666; }
.context-menu__label { flex: 1; font-size: 13px; color: #333; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.context-menu__separator { padding: 4px 0; }
.context-menu__separator-line { height: 1px; background: #e0e0e0; margin: 0 8px; }

@keyframes contextMenuEnter {
  from { opacity: 0; transform: scale(0.95) translateY(-8px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}

html[data-theme='dark'] .context-menu {
  background: #2d2d2d; border-color: #404040; box-shadow: 0 4px 20px rgba(0,0,0,0.3);
}
html[data-theme='dark'] .context-menu__item:hover:not(.context-menu__item--disabled) { background: #404040; }
html[data-theme='dark'] .context-menu__label { color: #e0e0e0; }
html[data-theme='dark'] .context-menu__separator-line { background: #555; }
</style>

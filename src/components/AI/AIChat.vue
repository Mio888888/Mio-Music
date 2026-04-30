<script setup lang="ts">
import { ref, nextTick, watch, onMounted, onBeforeUnmount } from 'vue'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useSettingsStore } from '@/store/Settings'

const userStore = LocalUserDetailStore()
const settingsStore = useSettingsStore()

const ball = ref<HTMLElement | null>(null)
const showChat = ref(false)
const inputText = ref('')
const messages = ref<Array<{ type: 'user' | 'ai' | 'loading' | 'error'; content: string }>>([])
const isLoading = ref(false)
const messagesContainer = ref<HTMLElement | null>(null)

// Drag state
const isDragging = ref(false)
const hasDragged = ref(false)
const ballPosition = ref({ x: 0, y: 0 })
const isOnLeft = ref(false)
const dragOffset = ref({ x: 0, y: 0 })
const isFloatBallVisible = ref(true)
let autoHideTimer: number | null = null

onMounted(() => {
  const saved = localStorage.getItem('floatBallPosition')
  if (saved) {
    try {
      const data = JSON.parse(saved)
      ballPosition.value = { x: data.x, y: data.y }
      isOnLeft.value = data.isOnLeft
    } catch { setDefaultPosition() }
  } else { setDefaultPosition() }
  isFloatBallVisible.value = settingsStore.settings.showFloatBall !== false
})

onBeforeUnmount(() => { clearAutoHide() })

function setDefaultPosition() {
  ballPosition.value = { x: window.innerWidth - 106, y: window.innerHeight / 2 - 60 }
  isOnLeft.value = false
}

function startAutoHide() {
  clearAutoHide()
  autoHideTimer = window.setTimeout(() => { /* could hide ball here */ }, 3000)
}

function clearAutoHide() {
  if (autoHideTimer) { clearTimeout(autoHideTimer); autoHideTimer = null }
}

// Drag handlers
function onMouseDown(e: MouseEvent) {
  if (showChat.value) return
  isDragging.value = true; hasDragged.value = false; clearAutoHide()
  const rect = ball.value?.getBoundingClientRect()
  if (rect) dragOffset.value = { x: e.clientX - rect.left, y: e.clientY - rect.top }
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
  e.preventDefault()
}

function onMouseMove(e: MouseEvent) {
  if (!isDragging.value) return
  hasDragged.value = true
  const x = Math.max(0, Math.min(e.clientX - dragOffset.value.x, window.innerWidth - 120))
  const y = Math.max(90, Math.min(e.clientY - dragOffset.value.y, window.innerHeight - 176))
  ballPosition.value = { x, y }
}

function onMouseUp() {
  if (!isDragging.value) return
  isDragging.value = false
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
  if (hasDragged.value) {
    const center = ballPosition.value.x + 60
    isOnLeft.value = center < window.innerWidth / 2
    ballPosition.value.x = isOnLeft.value ? 6 : window.innerWidth - 106
    localStorage.setItem('floatBallPosition', JSON.stringify({ ...ballPosition.value, isOnLeft: isOnLeft.value }))
    startAutoHide()
  }
}

function handleBallClick() {
  if (hasDragged.value) { hasDragged.value = false; return }
  clearAutoHide()
  showChat.value = true
  if (messages.value.length === 0) {
    messages.value.push({ type: 'ai', content: '您好！我是澜音 AI 助手，有什么音乐相关的问题可以问我。' })
  }
}

function closeChat() { showChat.value = false; startAutoHide() }

async function sendMessage() {
  if (!inputText.value.trim() || isLoading.value) return
  const apiKey = userStore.userInfo.deepseekAPIkey
  if (!apiKey) {
    messages.value.push({ type: 'error', content: '请先在设置中配置 DeepSeek API Key。' })
    return
  }

  const userMsg = inputText.value.trim()
  inputText.value = ''
  isLoading.value = true
  messages.value.push({ type: 'user', content: userMsg })
  const aiIdx = messages.value.length
  messages.value.push({ type: 'loading', content: '思考中...' })
  scrollToBottom()

  try {
    const streamId = 'stream_' + Date.now()
    let aiContent = ''

    const cleanup = await (window as any).api?.ai?.askStream?.(userMsg, streamId)

    const unlistenChunk = await (window as any).api?.ai?.onStreamChunk?.((data: any) => {
      if (data.streamId === streamId) {
        aiContent += data.chunk
        messages.value[aiIdx] = { type: 'ai', content: aiContent }
        scrollToBottom()
      }
    })

    const unlistenEnd = await (window as any).api?.ai?.onStreamEnd?.((data: any) => {
      if (data.streamId === streamId) {
        isLoading.value = false
        unlistenChunk?.()
        unlistenEnd?.()
        unlistenError?.()
      }
    })

    const unlistenError = await (window as any).api?.ai?.onStreamError?.((data: any) => {
      if (data.streamId === streamId) {
        if (!aiContent) messages.value[aiIdx] = { type: 'error', content: `发送失败: ${data.error}` }
        isLoading.value = false
        unlistenChunk?.(); unlistenEnd?.(); unlistenError?.()
      }
    })
  } catch (error: any) {
    messages.value[aiIdx] = { type: 'error', content: `发送失败: ${error.message || '未知错误'}` }
    isLoading.value = false
  }
  scrollToBottom()
}

function scrollToBottom() {
  nextTick(() => { if (messagesContainer.value) messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight })
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage() }
}

watch(() => settingsStore.settings.showFloatBall, (v) => { isFloatBallVisible.value = v !== false })
</script>

<template>
  <div v-if="isFloatBallVisible" ref="ball" class="ai-float-ball"
    :style="{ left: ballPosition.x + 'px', top: ballPosition.y + 'px' }"
    @mousedown="onMouseDown" @click="handleBallClick">
    <span class="ball-icon">AI</span>
    <div v-if="!isDragging" class="ball-close" @click.stop="isFloatBallVisible = false; settingsStore.updateSettings({ showFloatBall: false })">×</div>
  </div>

  <Teleport to="body">
    <div v-if="showChat" class="ai-chat-overlay" @click.self="closeChat">
      <div class="ai-chat-panel">
        <div class="chat-header">
          <span>AI 助手</span>
          <button class="close-btn" @click="closeChat">×</button>
        </div>
        <div ref="messagesContainer" class="chat-messages">
          <div v-for="(msg, i) in messages" :key="i" :class="['chat-message', msg.type]">
            <div class="message-content">{{ msg.content }}</div>
          </div>
        </div>
        <div class="chat-input">
          <textarea v-model="inputText" placeholder="输入消息..." @keydown="handleKeydown" :disabled="isLoading" rows="1" />
          <button @click="sendMessage" :disabled="isLoading || !inputText.trim()">发送</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ai-float-ball {
  position: fixed; z-index: 9999; width: 48px; height: 48px; border-radius: 50%;
  background: linear-gradient(135deg, var(--td-brand-color), #8b5cf6); color: #fff;
  display: flex; align-items: center; justify-content: center; cursor: pointer;
  box-shadow: 0 4px 16px rgba(0,0,0,0.2); user-select: none; transition: transform 0.15s;
}
.ai-float-ball:hover { transform: scale(1.1); }
.ball-icon { font-size: 14px; font-weight: 700; }
.ball-close {
  position: absolute; top: -6px; right: -6px; width: 18px; height: 18px; border-radius: 50%;
  background: rgba(0,0,0,0.5); color: #fff; font-size: 12px; display: flex;
  align-items: center; justify-content: center; opacity: 0; transition: opacity 0.15s;
}
.ai-float-ball:hover .ball-close { opacity: 1; }
.ai-chat-overlay {
  position: fixed; inset: 0; z-index: 10000; background: rgba(0,0,0,0.4);
  display: flex; align-items: center; justify-content: center;
}
.ai-chat-panel {
  width: 480px; max-height: 640px; background: var(--td-bg-color-container);
  border-radius: 16px; display: flex; flex-direction: column; overflow: hidden;
  box-shadow: 0 20px 60px rgba(0,0,0,0.3);
}
.chat-header {
  padding: 16px 20px; display: flex; justify-content: space-between; align-items: center;
  border-bottom: 1px solid var(--td-border-level-1-color); font-weight: 600; font-size: 16px;
}
.close-btn {
  background: none; border: none; font-size: 20px; cursor: pointer;
  color: var(--td-text-color-secondary); padding: 0 4px;
}
.chat-messages {
  flex: 1; overflow-y: auto; padding: 16px 20px; min-height: 300px; max-height: 440px;
}
.chat-message { margin-bottom: 12px; }
.chat-message.user .message-content {
  background: var(--td-brand-color); color: #fff; border-radius: 12px 12px 4px 12px;
  padding: 10px 14px; max-width: 80%; margin-left: auto;
}
.chat-message.ai .message-content {
  background: var(--td-bg-color-component); border-radius: 12px 12px 12px 4px;
  padding: 10px 14px; max-width: 85%; white-space: pre-wrap; line-height: 1.6;
}
.chat-message.error .message-content {
  background: #fef2f2; color: #dc2626; border-radius: 8px; padding: 10px 14px; font-size: 13px;
}
.chat-message.loading .message-content {
  color: var(--td-text-color-secondary); font-style: italic; padding: 10px 14px;
}
.chat-input {
  padding: 12px 16px; border-top: 1px solid var(--td-border-level-1-color);
  display: flex; gap: 8px; align-items: flex-end;
}
.chat-input textarea {
  flex: 1; resize: none; border: 1px solid var(--td-border-level-2-color);
  border-radius: 8px; padding: 8px 12px; font-size: 14px; background: var(--td-bg-color-container);
  color: var(--td-text-color-primary); outline: none; font-family: inherit;
}
.chat-input textarea:focus { border-color: var(--td-brand-color); }
.chat-input button {
  padding: 8px 16px; border-radius: 8px; border: none;
  background: var(--td-brand-color); color: #fff; cursor: pointer; font-size: 14px;
  white-space: nowrap;
}
.chat-input button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>

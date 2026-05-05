<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { Loading as TLoading } from 'tdesign-vue-next'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/store/Settings'

const userStore = LocalUserDetailStore()
const settingsStore = useSettingsStore()
const { userInfo } = storeToRefs(userStore)
const { settings } = storeToRefs(settingsStore)

const ball = ref<HTMLElement | null>(null)
const ballClass = ref('hidden-right')
const showAskWindow = ref(false)
const inputText = ref('')
const messages = ref<
  Array<{ type: 'user' | 'ai' | 'loading' | 'error'; content: string; html?: string }>
>([])
const isLoading = ref(false)
const messagesContainer = ref<HTMLElement | null>(null)
let timer: number | null = null

const isDragging = ref(false)
const hasDragged = ref(false)
const ballPosition = ref({ x: 0, y: 0 })
const isOnLeft = ref(false)
const dragOffset = ref({ x: 0, y: 0 })
const windowSize = ref({ width: 0, height: 0 })

const isFloatBallVisible = ref(settings.value.showFloatBall !== false)
const isHovering = ref(false)

const showBall = () => {
  ballClass.value = ''
  clearTimer()
}

const closeBall = (e: MouseEvent) => {
  e.stopPropagation()
  isFloatBallVisible.value = false
  settingsStore.updateSettings({ showFloatBall: false })
}

const handleMouseEnter = () => {
  isHovering.value = true
  showBall()
}

const handleMouseLeave = () => {
  isHovering.value = false
  startAutoHide()
}

const startAutoHide = () => {
  clearTimer()
  timer = window.setTimeout(() => {
    ballClass.value = isOnLeft.value ? 'hidden-left' : 'hidden-right'
  }, 3000)
}

const clearTimer = () => {
  if (timer) {
    clearTimeout(timer)
    timer = null
  }
}

const handleMouseDown = (e: MouseEvent) => {
  if (showAskWindow.value) return

  isDragging.value = true
  hasDragged.value = false
  clearTimer()

  const rect = ball.value?.getBoundingClientRect()
  if (rect) {
    dragOffset.value = {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top
    }
  }

  document.addEventListener('mousemove', handleMouseMove)
  document.addEventListener('mouseup', handleMouseUp)
  e.preventDefault()
}

const handleMouseMove = (e: MouseEvent) => {
  if (!isDragging.value) return
  hasDragged.value = true

  const x = e.clientX - dragOffset.value.x
  const y = e.clientY - dragOffset.value.y

  const maxX = windowSize.value.width - 120
  const maxY = windowSize.value.height - 176
  const minY = 90

  ballPosition.value = {
    x: Math.max(0, Math.min(x, maxX)),
    y: Math.max(minY, Math.min(y, maxY))
  }
}

const handleMouseUp = () => {
  if (!isDragging.value) return

  isDragging.value = false
  document.removeEventListener('mousemove', handleMouseMove)
  document.removeEventListener('mouseup', handleMouseUp)

  if (hasDragged.value) {
    const centerX = ballPosition.value.x + 60
    const screenCenter = windowSize.value.width / 2

    if (centerX < screenCenter) {
      ballPosition.value.x = 6
      isOnLeft.value = true
      ballClass.value = 'hidden-left'
    } else {
      ballPosition.value.x = windowSize.value.width - 106
      isOnLeft.value = false
      ballClass.value = 'hidden-right'
    }
    saveBallPosition()
    clearTimer()
    startAutoHide()
  }
}

const checkAPIKey = async (): Promise<boolean> => {
  if (!userInfo.value.deepseekAPIkey) {
    const errorMessage =
      '请先配置 DeepSeek API Key 才能使用 AI 功能。\n\n请前往 设置 → DeepSeek API Key 配置 进行设置。'
    messages.value.push({
      type: 'error',
      content: errorMessage,
      html: DOMPurify.sanitize(await marked(errorMessage))
    })
    return false
  }
  clearErrorMessages()
  return true
}

const clearErrorMessages = () => {
  messages.value = messages.value.filter((msg) => msg.type !== 'error')
}

const handleBallClick = async () => {
  if (hasDragged.value) {
    hasDragged.value = false
    return
  }

  clearTimer()
  showAskWindow.value = true

  if (!(await checkAPIKey())) {
    return
  }

  if (messages.value.length === 0) {
    const welcomeContent =
      '您好！我是AI助手，有什么可以帮助您的吗？ 您可以向我咨询音乐相关问题，我会尽力回答您的问题。'
    messages.value.push({
      type: 'ai',
      content: welcomeContent,
      html: DOMPurify.sanitize(await marked(welcomeContent))
    })
  }
}

const closeAskWindow = () => {
  showAskWindow.value = false
  startAutoHide()
}

const generateStreamId = () => {
  return 'stream_' + Date.now() + '_' + Math.random().toString(36).substring(2, 9)
}

const sendMessage = async () => {
  if (!inputText.value.trim() || isLoading.value) return

  if (!(await checkAPIKey())) {
    return
  }

  const userMessage = inputText.value
  inputText.value = ''
  isLoading.value = true

  messages.value.push({
    type: 'user',
    content: userMessage,
    html: DOMPurify.sanitize(await marked(userMessage))
  })
  scrollToBottom()

  const aiMessageIndex = messages.value.length
  messages.value.push({
    type: 'loading',
    content: '正在思考中...',
    html: ''
  })
  scrollToBottom()

  const streamId = generateStreamId()
  let aiContent = ''

  try {
    const handleStreamChunk = async (data: { streamId: string; chunk: string }) => {
      if (data.streamId === streamId) {
        aiContent += data.chunk
        messages.value[aiMessageIndex] = {
          type: 'ai',
          content: aiContent,
          html: DOMPurify.sanitize(await marked(aiContent))
        }
        scrollToBottom()
      }
    }

    const handleStreamEnd = (data: { streamId: string }) => {
      if (data.streamId === streamId) {
        isLoading.value = false
        ;(window as any).api?.ai?.removeStreamListeners?.()
      }
    }

    const handleStreamError = async (data: { streamId: string; error: string }) => {
      if (data.streamId === streamId) {
        console.error('AI流式响应错误:', data.error)
        if (!aiContent) {
          messages.value[aiMessageIndex] = {
            type: 'error',
            content: `发送失败: ${data.error}`,
            html: DOMPurify.sanitize(await marked(`发送失败: ${data.error}`))
          }
        }
        isLoading.value = false
        ;(window as any).api?.ai?.removeStreamListeners?.()
      }
    }

    ;(window as any).api?.ai?.onStreamChunk?.(handleStreamChunk)
    ;(window as any).api?.ai?.onStreamEnd?.(handleStreamEnd)
    ;(window as any).api?.ai?.onStreamError?.(handleStreamError)

    await (window as any).api?.ai?.askStream?.(userMessage, streamId)
  } catch (error: any) {
    console.error('AI流式API调用失败:', error)
    if (!aiContent) {
      messages.value[aiMessageIndex] = {
        type: 'error',
        content: `发送失败: ${(error as Error).message || '未知错误'}`,
        html: DOMPurify.sanitize(
          await marked(`发送失败: ${(error as Error).message || '未知错误'}`)
        )
      }
    }
    isLoading.value = false
    ;(window as any).api?.ai?.removeStreamListeners?.()
  }

  scrollToBottom()
}

const scrollToBottom = () => {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
    }
  })
}

watch(
  () => userInfo.value.deepseekAPIkey,
  async (newKey, oldKey) => {
    if (!oldKey && newKey) {
      clearErrorMessages()
      if (showAskWindow.value && messages.value.length === 0) {
        const welcomeContent =
          '您好！我是AI助手，有什么可以帮助您的吗？ 您可以向我咨询音乐相关问题，我会尽力回答您的问题。'
        messages.value.push({
          type: 'ai',
          content: welcomeContent,
          html: DOMPurify.sanitize(await marked(welcomeContent))
        })
        scrollToBottom()
      }
    }
  },
  { immediate: false }
)

watch(
  () => settings.value.showFloatBall,
  (newValue) => {
    isFloatBallVisible.value = newValue
  },
  { immediate: true }
)

const updateWindowSize = () => {
  windowSize.value = {
    width: window.innerWidth,
    height: window.innerHeight
  }
}

const saveBallPosition = () => {
  const positionData = {
    x: ballPosition.value.x,
    y: ballPosition.value.y,
    isOnLeft: isOnLeft.value
  }
  localStorage.setItem('floatBallPosition', JSON.stringify(positionData))
}

const loadBallPosition = () => {
  try {
    const savedPosition = localStorage.getItem('floatBallPosition')
    if (savedPosition) {
      const positionData = JSON.parse(savedPosition)
      ballPosition.value = {
        x: positionData.x,
        y: positionData.y
      }
      isOnLeft.value = positionData.isOnLeft
    } else {
      setDefaultPosition()
    }
  } catch (error) {
    console.error('加载悬浮球位置失败:', error)
    setDefaultPosition()
  }
}

const setDefaultPosition = () => {
  updateWindowSize()
  ballPosition.value = {
    x: windowSize.value.width - 126,
    y: windowSize.value.height - 176
  }
  isOnLeft.value = false
}

const initBallPosition = () => {
  updateWindowSize()
  loadBallPosition()
}

const handleResize = () => {
  updateWindowSize()
  const maxX = windowSize.value.width - 120
  const maxY = windowSize.value.height - 176
  const minY = 90

  if (!isOnLeft.value) {
    ballPosition.value.x = windowSize.value.width - 106
  }

  ballPosition.value.x = Math.max(0, Math.min(ballPosition.value.x, maxX))
  ballPosition.value.y = Math.max(minY, Math.min(ballPosition.value.y, maxY))
}

onMounted(() => {
  initBallPosition()
  startAutoHide()
  handleResize()
  window.addEventListener('resize', handleResize)
})

onBeforeUnmount(() => {
  clearTimer()
  document.removeEventListener('mousemove', handleMouseMove)
  document.removeEventListener('mouseup', handleMouseUp)
  window.removeEventListener('resize', handleResize)
  saveBallPosition()
})
</script>

<template>
  <div>
    <transition name="ball-fade" appear>
      <div
        v-show="!showAskWindow && isFloatBallVisible"
        ref="ball"
        class="float-ball-container"
        :class="{ dragging: isDragging }"
        :style="{
          left: ballPosition.x - 10 + 'px',
          top: ballPosition.y - 10 + 'px',
          right: 'auto',
          bottom: 'auto'
        }"
        @mouseenter="handleMouseEnter"
        @mouseleave="handleMouseLeave"
        @mousedown="handleMouseDown"
      >
        <div
          class="float-ball"
          :class="[ballClass, { hovering: isHovering }]"
          @click="handleBallClick"
        >
          <video autoplay muted loop src="../../assets/videos/AI.mp4" />
        </div>
        <div v-show="isHovering" class="close-ball-btn" @click="closeBall" @mousedown.stop>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </div>
      </div>
    </transition>

    <transition name="window-scale" appear>
      <div
        v-show="showAskWindow"
        class="ask-window"
        :class="{ 'on-left': isOnLeft }"
        :style="{
          left: isOnLeft ? ballPosition.x + 120 + 'px' : 'auto',
          right: isOnLeft ? 'auto' : windowSize.width - ballPosition.x + 20 + 'px',
          bottom: Math.max(20, 176) + 'px'
        }"
      >
        <div class="ask-header">
          <h3>AI助手</h3>
          <button class="close-btn" @click="closeAskWindow">×</button>
        </div>
        <div class="ask-content">
          <div ref="messagesContainer" class="chat-messages">
            <div
              v-for="(message, index) in messages"
              :key="index"
              class="message"
              :class="message.type"
            >
              <div v-if="message.type === 'loading'" class="message-content loading-content">
                <t-loading size="small" />
                <span class="loading-text">{{ message.content }}</span>
              </div>
              <div v-else class="message-content" v-html="message.html || message.content"></div>
            </div>
          </div>
        </div>
        <div class="input-area">
          <input
            v-model="inputText"
            placeholder="请输入您的问题..."
            class="message-input"
            @keyup.enter="sendMessage"
          />
          <button class="send-btn" :disabled="!inputText.trim() || isLoading" @click="sendMessage">
            {{ isLoading ? '发送中...' : '发送' }}
          </button>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.float-ball-container {
  position: fixed;
  width: 120px;
  height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
  user-select: none;
}

.float-ball {
  width: 80px;
  height: 80px;
  border-radius: 50%;
  background: #409eff;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: 14px;
  cursor: pointer;
  overflow: hidden;
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
  padding: 2px;
  background-image: linear-gradient(45deg, #409eff, #ff6600);
}

.float-ball-container.dragging {
  transition: none;
  z-index: 10001;
}

.float-ball-container.dragging .float-ball {
  cursor: grabbing;
}

.float-ball.hidden-right {
  transform: translateX(calc(66px));
}

.float-ball.hidden-left {
  transform: translateX(calc(-66px));
}

video {
  width: 100%;
  height: 100%;
  border-radius: 50%;
  overflow: hidden;
}

.close-ball-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 32px;
  height: 32px;
  background: #ff4757;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  z-index: 10002;
  transition: background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease, transform 0.2s ease;
  box-shadow: 0 2px 8px rgba(255, 71, 87, 0.3);
  color: #fff;
}

.close-ball-btn:hover {
  background: #ff3742;
  transform: scale(1.1);
  box-shadow: 0 4px 12px rgba(255, 71, 87, 0.4);
}

.float-ball.hovering {
  transform: scale(1.05);
}

.ask-window {
  position: fixed;
  width: 400px;
  height: auto;
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  z-index: 10001;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
}

.ask-window.on-left {
  transform-origin: left center;
}

.ask-window:not(.on-left) {
  transform-origin: right center;
}

.ask-header {
  background: linear-gradient(45deg, #409eff, #ff6600);
  color: #fff;
  padding: 16px 20px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.ask-header h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}

.close-btn {
  background: none;
  border: none;
  color: #fff;
  font-size: 24px;
  cursor: pointer;
  padding: 0;
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  transition: background-color 0.2s;
}

.close-btn:hover {
  background-color: rgba(255, 255, 255, 0.2);
}

.ask-content {
  display: flex;
  flex-direction: column;
  height: 350px;
}

.chat-messages {
  flex: 1;
  padding: 20px;
  overflow-y: auto;
  background: #f8f9fa;
}

.message {
  margin-bottom: 16px;
  display: flex;
}

.message.user {
  justify-content: flex-end;
}

.message.ai {
  justify-content: flex-start;
}

.message-content {
  max-width: 80%;
  padding: 12px 16px;
  border-radius: 18px;
  font-size: 14px;
  line-height: 1.4;
}

.message.user .message-content {
  background: #409eff;
  color: #fff;
}

.message.ai .message-content {
  background: #fff;
  color: #333;
  border: 1px solid #e0e0e0;
}

.message.loading .message-content {
  background: #f0f0f0;
  color: #666;
  font-style: italic;
  border: 1px solid #ddd;
}

.loading-content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.loading-text {
  font-size: 14px;
  color: #666;
}

.message.error .message-content {
  background: #fee;
  color: #d63031;
  border: 1px solid #fab1a0;
}

.input-area {
  padding: 20px;
  border-top: 1px solid #e0e0e0;
  display: flex;
  gap: 12px;
  background: #fff;
}

.message-input {
  flex: 1;
  padding: 12px 16px;
  border: 1px solid #ddd;
  border-radius: 24px;
  outline: none;
  font-size: 14px;
  transition: border-color 0.2s;
}

.message-input:focus {
  border-color: #409eff;
}

.send-btn {
  padding: 12px 24px;
  background: #409eff;
  color: #fff;
  border: none;
  border-radius: 24px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  transition: background-color 0.2s;
}

.send-btn:hover:not(:disabled) {
  background: #3a8ee6;
}

.send-btn:disabled {
  background: #c0c4cc;
  cursor: not-allowed;
  opacity: 0.6;
}

.ball-fade-enter-active,
.ball-fade-leave-active {
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
}

.ball-fade-enter-from,
.ball-fade-leave-to {
  opacity: 0;
  transform: scale(0.8) translateX(calc(56px));
}

.ball-fade-enter-to,
.ball-fade-leave-from {
  opacity: 1;
  transform: scale(1);
}

.window-scale-enter-active,
.window-scale-leave-active {
  transition: background-color 0.4s cubic-bezier(0.25, 0.8, 0.25, 1), border-color 0.4s cubic-bezier(0.25, 0.8, 0.25, 1), color 0.4s cubic-bezier(0.25, 0.8, 0.25, 1), box-shadow 0.4s cubic-bezier(0.25, 0.8, 0.25, 1), opacity 0.4s cubic-bezier(0.25, 0.8, 0.25, 1), transform 0.4s cubic-bezier(0.25, 0.8, 0.25, 1);
}

.window-scale-enter-from,
.window-scale-leave-to {
  opacity: 0;
  transform: scale(0.7) translateY(20px);
}

.window-scale-enter-to,
.window-scale-leave-from {
  opacity: 1;
  transform: scale(1) translateY(0);
}

.message-content {
  font-family:
    -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Helvetica Neue', Arial, sans-serif;
  color: #2c3e50;
  line-height: 1.7;
}

.message-content h1,
.message-content h2,
.message-content h3,
.message-content h4,
.message-content h5,
.message-content h6 {
  margin: 1.2em 0 0.8em 0;
  font-weight: 700;
  color: #1a202c;
  letter-spacing: -0.025em;
}

.message-content h1 {
  font-size: 1.875rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.message-content h2 {
  font-size: 1.5rem;
  color: #2d3748;
  border-bottom: 2px solid #e2e8f0;
  padding-bottom: 0.5rem;
}

.message-content h3 {
  font-size: 1.25rem;
  color: #4a5568;
}

.message-content h4,
.message-content h5,
.message-content h6 {
  font-size: 1.125rem;
  color: #718096;
}

.message-content p {
  margin: 1em 0;
  line-height: 1.8;
  color: #4a5568;
}

.message-content ul,
.message-content ol {
  margin: 1em 0;
  padding-left: 2em;
}

.message-content li {
  margin: 0.5em 0;
  line-height: 1.6;
}

.message-content ul li::marker {
  color: #667eea;
}

.message-content blockquote {
  margin: 1.5em 0;
  padding: 1.25rem 1.5rem;
  background: linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%);
  border-left: 4px solid #667eea;
  border-radius: 0 8px 8px 0;
  font-style: italic;
  box-shadow: 0 2px 8px rgba(102, 126, 234, 0.15);
}

.message-content code {
  background: linear-gradient(135deg, #f7fafc 0%, #edf2f7 100%);
  color: #e53e3e;
  padding: 0.25rem 0.5rem;
  border-radius: 6px;
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Consolas', monospace;
  font-size: 0.875em;
  font-weight: 600;
  border: 1px solid #e2e8f0;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.message-content pre {
  background: linear-gradient(135deg, #2d3748 0%, #1a202c 100%);
  color: #e2e8f0;
  padding: 1.5rem;
  border-radius: 12px;
  overflow-x: auto;
  margin: 1.5em 0;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  border: 1px solid #4a5568;
  position: relative;
}

.message-content pre::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
  border-radius: 12px 12px 0 0;
}

.message-content pre code {
  background: none;
  color: inherit;
  padding: 0;
  border: none;
  box-shadow: none;
  font-size: 0.875rem;
  font-weight: 400;
}

.message-content strong {
  font-weight: 700;
  color: #2d3748;
  background: linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%);
  padding: 0.1em 0.3em;
  border-radius: 4px;
}

.message-content a {
  color: #667eea;
  text-decoration: none;
  font-weight: 500;
  border-bottom: 2px solid transparent;
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease, transform 0.3s ease;
}

.message-content a:hover {
  color: #5a67d8;
  border-bottom-color: #667eea;
}

.message-content table {
  border-collapse: separate;
  border-spacing: 0;
  width: 100%;
  margin: 1.5em 0;
  background: #fff;
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
  border: 1px solid #e2e8f0;
}

.message-content th,
.message-content td {
  padding: 1rem 1.25rem;
  text-align: left;
  border-bottom: 1px solid #e2e8f0;
}

.message-content th {
  background: linear-gradient(135deg, #f7fafc 0%, #edf2f7 100%);
  font-weight: 700;
  color: #2d3748;
  font-size: 0.875rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.message-content tbody tr:nth-child(even) {
  background: rgba(247, 250, 252, 0.5);
}

.message-content tbody tr:last-child td {
  border-bottom: none;
}

.message-content hr {
  margin: 2rem 0;
  border: none;
  height: 2px;
  background: linear-gradient(90deg, transparent 0%, #e2e8f0 50%, transparent 100%);
}

@media (max-width: 480px) {
  .ask-window {
    left: 10px !important;
    right: 10px !important;
    bottom: 10px !important;
    width: auto !important;
    height: 60vh;
  }

  .float-ball.hidden-right {
    transform: translateX(calc(50px));
  }

  .float-ball.hidden-left {
    transform: translateX(calc(-50px));
  }
}
</style>

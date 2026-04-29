<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { MessagePlugin } from 'tdesign-vue-next'

const route = useRoute()
const router = useRouter()
const songmid = ref((route.query.songmid as string) || '')
const loading = ref(false)
const saving = ref(false)

const form = ref({
  name: '',
  singer: '',
  albumName: '',
  year: 0,
  filePath: ''
})

const fetchTags = async () => {
  if (!songmid.value) return
  loading.value = true
  try {
    const res = await (window as any).api?.localMusic?.getTags?.(songmid.value, false)
    if (res?.success && res.data) {
      form.value.name = res.data.name || ''
      form.value.singer = res.data.singer || ''
      form.value.albumName = res.data.albumName || ''
      form.value.year = res.data.year || 0
      form.value.filePath = res.data.path || ''
    }
  } catch (e) { console.error('获取标签失败:', e) }
  finally { loading.value = false }
}

const saveTags = async () => {
  if (!form.value.filePath) { MessagePlugin.warning('无文件路径'); return }
  saving.value = true
  try {
    await (window as any).api?.localMusic?.writeTags?.(
      form.value.filePath,
      { name: form.value.name, singer: form.value.singer, albumName: form.value.albumName, year: form.value.year },
      {}
    )
    MessagePlugin.success('标签保存成功')
  } catch (e) { console.error('保存标签失败:', e); MessagePlugin.error('保存失败') }
  finally { saving.value = false }
}

onMounted(() => fetchTags())
</script>

<template>
  <div class="tag-editor-container">
    <div class="editor-header">
      <t-button variant="text" @click="router.back()">
        <template #icon><i class="iconfont icon-xiangzuo"></i></template>
        返回
      </t-button>
      <h2>标签编辑</h2>
    </div>

    <div v-if="loading" class="loading-state">
      <div class="loading-spinner"></div><p>加载中...</p>
    </div>
    <div v-else class="editor-form">
      <div class="form-item">
        <label>歌曲名</label>
        <t-input v-model="form.name" placeholder="歌曲名" />
      </div>
      <div class="form-item">
        <label>歌手</label>
        <t-input v-model="form.singer" placeholder="歌手" />
      </div>
      <div class="form-item">
        <label>专辑</label>
        <t-input v-model="form.albumName" placeholder="专辑" />
      </div>
      <div class="form-item">
        <label>年份</label>
        <t-input-number v-model="form.year" :min="0" :max="2099" placeholder="年份" />
      </div>
      <div class="form-item">
        <label>文件路径</label>
        <div class="file-path">{{ form.filePath }}</div>
      </div>
      <div class="form-actions">
        <t-button theme="primary" :loading="saving" @click="saveTags">保存标签</t-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tag-editor-container { width: 100%; height: 100%; display: flex; flex-direction: column; overflow: hidden; padding: 20px; }
.editor-header { display: flex; align-items: center; gap: 12px; flex-shrink: 0; margin-bottom: 20px; }
.editor-header h2 { font-size: 20px; font-weight: 600; color: var(--td-text-color-primary); margin: 0; }
.editor-form { max-width: 600px; }
.form-item { margin-bottom: 16px; }
.form-item label { display: block; font-size: 13px; color: var(--td-text-color-secondary); margin-bottom: 6px; }
.file-path { font-size: 12px; color: var(--td-text-color-placeholder); word-break: break-all; padding: 8px; background: var(--td-bg-color-page); border-radius: 6px; }
.form-actions { margin-top: 24px; }
.loading-state { display: flex; flex-direction: column; align-items: center; padding: 60px; }
.loading-spinner { width: 40px; height: 40px; border: 3px solid var(--td-bg-color-component); border-top-color: var(--td-brand-color); border-radius: 50%; animation: spin 1s linear infinite; margin-bottom: 12px; }
.loading-state p { color: var(--td-text-color-secondary); }
@keyframes spin { to { transform: rotate(360deg); } }
</style>

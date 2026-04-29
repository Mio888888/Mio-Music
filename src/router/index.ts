import { createWebHashHistory, createRouter, type RouteRecordRaw, type RouterOptions } from 'vue-router'

const appRouter: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'welcome',
    component: () => import('@/views/welcome/index.vue')
  },
  {
    path: '/home',
    name: 'home',
    redirect: '/home/find',
    component: () => import('@/views/home/index.vue'),
    children: [
      { path: 'find', name: 'find', component: () => import('@/views/music/find.vue') },
      { path: 'songlist', name: 'songlist', component: () => import('@/views/music/songlist.vue') },
      { path: 'local', name: 'local', component: () => import('@/views/music/local.vue') },
      { path: 'recent', name: 'recent', component: () => import('@/views/music/recent.vue') },
      { path: 'search', name: 'search', component: () => import('@/views/music/search.vue') },
      { path: 'recognize', name: 'recognize', component: () => import('@/views/music/recognize.vue') },
      { path: 'list/:id', name: 'list', component: () => import('@/views/music/list.vue') },
      { path: 'download', name: 'download', component: () => import('@/views/download/index.vue') },
      { path: 'local/edit-tag', name: 'local-tag-editor', component: () => import('@/views/music/LocalTagEditorPage.vue') },
      { path: 'profile', name: 'profile', component: () => import('@/views/user/Profile.vue') }
    ]
  },
  {
    path: '/settings',
    name: 'settings',
    meta: { transitionIn: 'animate__fadeIn', transitionOut: 'animate__fadeOut' },
    component: () => import('@/views/settings/index.vue')
  }
]

const routes: RouteRecordRaw[] = [
  { path: '/', children: appRouter },
  { path: '/desktop-lyric', name: 'desktop-lyric', component: () => import('@/views/DeskTopLyric/DeskTopLyric.vue') },
  { path: '/recognition-worker', name: 'recognition-worker', component: () => import('@/views/music/RecognitionWorker.vue') }
]

function setAnimate(routerObj: RouteRecordRaw[]) {
  for (const item of routerObj) {
    if (item.children && item.children.length > 0) {
      setAnimate(item.children)
    } else if (!item.meta) {
      item.meta = { transitionIn: 'animate__fadeInRight', transitionOut: 'animate__fadeOutLeft' }
    }
  }
}
setAnimate(routes)

const router = createRouter({
  history: createWebHashHistory(),
  routes
} as RouterOptions)

const getRoutePreloadEnabled = (): boolean => {
  try {
    const saved = localStorage.getItem('appSettings')
    if (saved) {
      const parsed = JSON.parse(saved) as { routePreloadEnabled?: boolean }
      if (typeof parsed.routePreloadEnabled === 'boolean') return parsed.routePreloadEnabled
    }
  } catch {}
  return true
}

function flattenRoutes(rs: RouteRecordRaw[]): RouteRecordRaw[] {
  const result: RouteRecordRaw[] = []
  for (const r of rs) {
    result.push(r)
    if (r.children) result.push(...flattenRoutes(r.children))
  }
  return result
}

const startPreload = () => {
  if (!getRoutePreloadEnabled()) return
  const idleCb = window.requestIdleCallback || ((cb: IdleRequestCallback) => window.setTimeout(cb, 200))
  const queue = flattenRoutes(routes).filter((r): r is RouteRecordRaw & { component: () => Promise<any> } =>
    !!r.component && typeof r.component === 'function'
  )

  const runBatch = () => {
    if (!getRoutePreloadEnabled()) return
    const route = queue.shift()
    if (!route) return
    try { route.component() } catch {}
    idleCb(runBatch)
  }

  const schedule = () => idleCb(runBatch)
  if (document.readyState === 'complete') setTimeout(schedule, 1500)
  else window.addEventListener('load', () => setTimeout(schedule, 1500), { once: true })
}
startPreload()

export default router

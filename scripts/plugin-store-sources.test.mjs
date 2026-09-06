import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import vm from 'node:vm'
import { transform } from 'esbuild'

const source = await readFile(new URL('../src/store/plugin.ts', import.meta.url), 'utf8')
const { code } = await transform(source, { loader: 'ts', format: 'cjs' })
const copy = value => JSON.parse(JSON.stringify(value))
const settle = () => new Promise(resolve => setImmediate(resolve))
const sourceRecord = (id = 'kw', qualities = ['128k', '320k', 'flac']) => ({
  [id]: { name: id.toUpperCase(), type: 'music', qualitys: qualities }
})
const pluginInfo = (id, qualities = ['128k', '320k']) => ({
  plugin_id: id, plugin_name: `${id}.js`, plugin_type: 'music-source',
  plugin_info: { name: id, version: '1.0.0', author: 'fixture', description: '' },
  supported_sources: [{ source_id: 'kw', name: 'KW', qualities }]
})

function deferred() {
  let resolve
  const promise = new Promise(done => { resolve = done })
  return { promise, resolve }
}

export function createHarness({ plugins = [pluginInfo('one')], savedId = '', userInfo = {}, getSources = async () => sourceRecord(), importResult } = {}) {
  const persisted = new Map(savedId ? [['pluginId', savedId], ['pluginName', savedId]] : [])
  const refs = new WeakSet()
  const userStore = {
    userInfo: {
      pluginId: '', pluginName: '', supportedSources: {}, selectSources: '',
      selectQuality: '', sourceQualityMap: {}, ...copy(userInfo)
    },
    mergeBuiltInSources(info, sources) {
      return info.subsonicEnabled
        ? { ...sources, subsonic: { name: 'Subsonic', type: 'built-in', qualitys: ['128k', '320k', 'flac'] } }
        : { ...sources }
    }
  }
  const runtimeCache = new Map()
  const runner = {
    async getSources(id) {
      if (!runtimeCache.has(id)) runtimeCache.set(id, Promise.resolve(getSources(id)))
      return runtimeCache.get(id)
    },
    clearCache(id) { id ? runtimeCache.delete(id) : runtimeCache.clear() }
  }
  let pluginList = copy(plugins)
  let replacement
  const importReplacement = async () => {
    if (importResult) return importResult
    assert.ok(replacement, 'test must configure an import result')
    pluginList = pluginList.filter(item => item.plugin_id !== replacement.plugin_id)
    pluginList.push(copy(replacement))
    return { success: true, data: copy(replacement) }
  }
  const module = { exports: {} }
  const dependencies = {
    pinia: { defineStore: (_id, setup) => () => new Proxy(setup(), {
      get(target, key) { const value = target[key]; return refs.has(value) ? value.value : value },
      set(target, key, value) {
        if (refs.has(target[key])) target[key].value = value
        else target[key] = value
        return true
      }
    }) },
    vue: { ref(value) { const ref = { value }; refs.add(ref); return ref } },
    './LocalUserDetail': { LocalUserDetailStore: () => userStore },
    './ControlAudio': { ControlAudioStore: () => ({ stop: async () => {} }) },
    './GlobalPlayStatus': { useGlobalPlayStatusStore: () => ({ player: {} }) },
    '@/utils/plugin/PluginRunner': runner,
    '@/locales': { global: { t: key => key } }
  }
  vm.runInNewContext(code, {
    module, exports: module.exports,
    require(name) { assert.ok(name in dependencies, `unexpected dependency: ${name}`); return dependencies[name] },
    console,
    localStorage: {
      getItem: key => persisted.get(key) ?? null,
      setItem: (key, value) => persisted.set(key, value),
      removeItem: key => persisted.delete(key)
    },
    window: { api: { plugins: {
      initialize: async () => ({ success: true, data: copy(pluginList) }),
      getList: async () => ({ success: true, data: copy(pluginList) }),
      add: importReplacement, downloadAndAdd: importReplacement, selectAndAdd: importReplacement
    } } }
  }, { filename: 'plugin-store.js' })
  const store = module.exports.usePluginStore()
  store.plugins = copy(pluginList)
  return { store, userStore, setReplacement: value => { replacement = value } }
}

test('selecting a plugin exposes runtime lossless metadata in the list and user settings', async () => {
  const { store, userStore } = createHarness()
  await store.selectPlugin(store.plugins[0])
  await settle()
  assert.deepEqual(copy(store.plugins[0].supported_sources), [
    { source_id: 'kw', name: 'KW', qualities: ['128k', '320k', 'flac'] }
  ])
  assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k', 'flac'])
  assert.equal(userStore.userInfo.pluginId, 'one')
  assert.equal(userStore.userInfo.selectQuality, 'flac')
})

test('startup restores runtime metadata while preserving a valid selected quality and built-in source', async () => {
  const { store, userStore } = createHarness({
    savedId: 'one',
    userInfo: { pluginId: 'one', selectSources: 'kw', selectQuality: '320k', subsonicEnabled: true }
  })
  await store.initialize()
  await settle()
  assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k', 'flac'])
  assert.equal(userStore.userInfo.selectSources, 'kw')
  assert.equal(userStore.userInfo.selectQuality, '320k')
  assert.ok(userStore.userInfo.supportedSources.subsonic)
})

for (const [label, getSources] of [
  ['empty runtime metadata', async () => ({})],
  ['runtime lookup failure', async () => { throw new Error('fixture lookup failure') }]
]) {
  test(`static source metadata remains usable after ${label}`, async () => {
    const { store, userStore } = createHarness({ getSources })
    await store.selectPlugin(store.plugins[0])
    await settle()
    assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k'])
    assert.equal(userStore.userInfo.selectQuality, '320k')
  })
}

test('a late response from the previous plugin cannot overwrite the current selection', async () => {
  const first = deferred()
  const { store, userStore } = createHarness({
    plugins: [pluginInfo('one'), pluginInfo('two')],
    getSources: id => id === 'one' ? first.promise : Promise.resolve(sourceRecord('tx'))
  })
  const firstSelection = store.selectPlugin(store.plugins[0])
  await settle()
  await store.selectPlugin(store.plugins[1])
  await settle()
  first.resolve(sourceRecord('kw'))
  await firstSelection
  await settle()
  assert.equal(store.currentPluginId, 'two')
  assert.equal(userStore.userInfo.pluginId, 'two')
  assert.ok(userStore.userInfo.supportedSources.tx)
  assert.equal(userStore.userInfo.supportedSources.kw, undefined)
})

for (const method of ['addPlugin', 'downloadAndAdd', 'selectAndAdd']) {
  test(`${method} replaces cached runtime metadata for an already selected plugin`, async () => {
    let qualitys = ['128k', '320k']
    const { store, userStore, setReplacement } = createHarness({
      getSources: async () => sourceRecord('kw', qualitys.slice())
    })
    await store.selectPlugin(store.plugins[0])
    await settle()
    qualitys = ['128k', '320k', 'flac']
    setReplacement(pluginInfo('one'))
    await store[method]('synthetic fixture', 'music-source', 'one')
    await settle()
    assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), qualitys)
    assert.deepEqual(copy(store.plugins[0].supported_sources[0].qualities), qualitys)
  })
}

test('refresh removes invalid per-source quality overrides but retains valid built-in choices', async () => {
  const { store, userStore } = createHarness({
    savedId: 'one',
    userInfo: {
      pluginId: 'one', selectSources: 'kw', selectQuality: '320k', subsonicEnabled: true,
      sourceQualityMap: { kw: 'flac24bit', removed: '320k', subsonic: 'flac' }
    }
  })
  await store.initialize()
  await store.refresh()
  await settle()
  assert.equal(userStore.userInfo.sourceQualityMap.kw, undefined)
  assert.equal(userStore.userInfo.sourceQualityMap.removed, undefined)
  assert.equal(userStore.userInfo.sourceQualityMap.subsonic, 'flac')
  assert.equal(userStore.userInfo.selectQuality, '320k')
})

test('startup keeps a selected built-in source instead of switching it to the plugin', async () => {
  const { store, userStore } = createHarness({
    savedId: 'one',
    userInfo: {
      pluginId: 'one', selectSources: 'subsonic', selectQuality: 'flac', subsonicEnabled: true,
      sourceQualityMap: { subsonic: 'flac' }
    }
  })
  await store.initialize()
  await settle()
  assert.equal(userStore.userInfo.selectSources, 'subsonic')
  assert.equal(userStore.userInfo.selectQuality, 'flac')
  assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k', 'flac'])
})

test('startup preserves an existing lossless preference missing from static metadata', async () => {
  const { store, userStore } = createHarness({
    savedId: 'one',
    userInfo: {
      pluginId: 'one', selectSources: 'kw', selectQuality: 'flac', sourceQualityMap: { kw: 'flac' }
    }
  })
  await store.initialize()
  await settle()
  assert.equal(userStore.userInfo.selectQuality, 'flac')
  assert.equal(userStore.userInfo.sourceQualityMap.kw, 'flac')
  assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k', 'flac'])
})

test('canceling an import does not discard an in-flight plugin selection', async () => {
  const pendingSources = deferred()
  const { store, userStore } = createHarness({
    getSources: () => pendingSources.promise,
    importResult: { success: true, data: { canceled: true } }
  })
  const selection = store.selectPlugin(store.plugins[0])
  await settle()
  assert.equal(await store.selectAndAdd('music-source'), null)
  pendingSources.resolve(sourceRecord('kw'))
  await selection
  await settle()
  assert.equal(userStore.userInfo.pluginId, 'one')
  assert.equal(userStore.userInfo.selectQuality, 'flac')
})

test('an old pending source lookup cannot overwrite metadata after plugin replacement', async () => {
  const oldSources = deferred()
  let replaced = false
  const { store, userStore, setReplacement } = createHarness({
    getSources: () => replaced ? Promise.resolve(sourceRecord('kw')) : oldSources.promise
  })
  const oldSelection = store.selectPlugin(store.plugins[0])
  await settle()
  replaced = true
  setReplacement(pluginInfo('one'))
  await store.addPlugin('synthetic replacement', 'one.js', 'one')
  await settle()
  oldSources.resolve(sourceRecord('kw', ['128k']))
  await oldSelection
  await settle()
  assert.deepEqual(copy(userStore.userInfo.supportedSources.kw.qualitys), ['128k', '320k', 'flac'])
  assert.deepEqual(copy(store.plugins[0].supported_sources[0].qualities), ['128k', '320k', 'flac'])
})

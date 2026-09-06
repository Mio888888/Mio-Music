import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import vm from 'node:vm'
import { transform } from 'esbuild'

const workerSource = await readFile(new URL('../src/utils/plugin/pluginWorker.ts', import.meta.url), 'utf8')
const { code: workerCode } = await transform(workerSource, { loader: 'ts', format: 'cjs' })

const dynamicPlugin = `
  const qualities = ['128k', '320k', ['fl', 'ac'].join('')];
  module.exports.sources = Object.fromEntries(['kw', 'tx'].map(source => [source, {
    name: source.toUpperCase(),
    type: 'music',
    qualitys: qualities.slice()
  }]));
  module.exports.musicUrl = async function(source, songInfo, quality) {
    return 'https://fixture.invalid/' + source + '/' + songInfo.id + '/' + quality
      + '?env=' + this.cerumusic.env;
  };
`

export function createWorker(pluginCode, httpProxy) {
  let callId = 0
  const responses = new Map()
  const worker = {
    onmessage: null,
    postMessage(message) {
      if (message.type === 'ipc') {
        assert.ok(message.method === 'plugins.getCode' || (message.method === 'httpProxy' && httpProxy), 'fixtures must not perform network requests')
        const result = message.method === 'httpProxy' ? httpProxy(message.args)
          : Promise.resolve(typeof pluginCode === 'function' ? pluginCode() : pluginCode)
            .then(data => ({ success: true, data }))
        Promise.resolve(result).then(result => worker.onmessage({ data: {
          type: 'ipc-resolve', id: message.id, result
        } }))
      } else if (message.type === 'resolve' || message.type === 'reject') {
        responses.set(message.id, structuredClone(message))
      }
    }
  }
  // Only the locale dependency and host IPC boundary are replaced; execution stays real.
  vm.runInNewContext(workerCode, {
    self: worker,
    require(name) {
      assert.equal(name, '@/locales')
      return { global: { t: (key, params) => `${key}: ${JSON.stringify(params ?? {})}` } }
    },
    console, setTimeout, clearTimeout, setInterval, clearInterval,
    TextEncoder, TextDecoder, atob, btoa,
    fetch() { throw new Error('Unexpected network request') }
  }, { filename: 'pluginWorker.js' })

  return async (method, ...args) => {
    const id = ++callId
    await worker.onmessage({ data: { type: 'call', id, method, args } })
    const response = responses.get(id)
    responses.delete(id)
    assert.ok(response, `worker must respond to ${method}`)
    if (response.type === 'reject') throw new Error(response.error)
    return response.result
  }
}

test('getSources preserves runtime-generated CeruMusic lossless quality metadata', async () => {
  const call = createWorker(dynamicPlugin)
  const sources = await call('getSources', 'dynamic-qualities')
  assert.deepEqual(sources, {
    kw: { name: 'KW', type: 'music', qualitys: ['128k', '320k', 'flac'] },
    tx: { name: 'TX', type: 'music', qualitys: ['128k', '320k', 'flac'] }
  })
})

test('musicUrl keeps the requested lossless quality and CeruMusic context', async () => {
  const call = createWorker(dynamicPlugin)
  assert.equal(
    await call('getMusicUrl', 'dynamic-qualities', 'kw', { id: 'song-42' }, 'flac'),
    'https://fixture.invalid/kw/song-42/flac?env=browser'
  )
})

test('getSources returns an empty record for legacy plugins without source metadata', async () => {
  const call = createWorker('module.exports.musicUrl = async () => "https://fixture.invalid/song.mp3";')
  assert.deepEqual(await call('getSources', 'legacy-plugin'), {})
})

test('getSources accepts quoted metadata without a type and never adds unsupported qualities', async () => {
  const call = createWorker(`module.exports.sources = {
    "kg": {"name":"KG","qualitys":["128k","320k","flac","hires","atmos","master"]},
    "kw": {"name":"KW","qualitys":["128k","320k","flac","hires"]},
    "tx": {"name":"TX","qualitys":["128k","320k"]}
  };`)
  const sources = await call('getSources', 'quoted-metadata')
  assert.deepEqual(sources.kg.qualitys, ['128k', '320k', 'flac', 'hires', 'atmos', 'master'])
  assert.deepEqual(sources.kw.qualitys, ['128k', '320k', 'flac', 'hires'])
  assert.deepEqual(sources.tx.qualitys, ['128k', '320k'])
  assert.equal(sources.kw.type, 'music')
})

test('getSources filters malformed and non-cloneable plugin metadata', async () => {
  const call = createWorker(`module.exports.sources = JSON.parse('{"__proto__":{"qualitys":["flac"]},"constructor":{"qualitys":["flac"]},"kw":{"qualitys":["320k","flac","flac",null,1,""]},"tx":null,"kg":{"qualitys":"flac"}}');
    module.exports.sources.kw.musicUrl = () => 'not metadata';`)
  assert.deepEqual(await call('getSources', 'invalid-metadata'), {
    kw: { name: 'kw', type: 'music', qualitys: ['320k', 'flac'] }
  })
})

test('concurrent metadata and playback loads execute one plugin instance', async () => {
  let reads = 0
  const call = createWorker(() => { reads++; return dynamicPlugin })
  await Promise.all([
    call('getSources', 'shared-plugin'),
    call('getMusicUrl', 'shared-plugin', 'kw', { id: 'song' }, 'flac'),
  ])
  assert.equal(reads, 1)
})

test('a cache clear prevents an old in-flight load from replacing new code', async () => {
  let finishOld
  let reads = 0
  const call = createWorker(() => ++reads === 1 ? new Promise(resolve => { finishOld = resolve }) : dynamicPlugin)
  const oldCall = call('getSources', 'replaced-plugin')
  await call('clearCache', 'replaced-plugin')
  const current = await call('getSources', 'replaced-plugin')
  finishOld('module.exports.sources = { kw: { qualitys: ["128k"] } };')
  await oldCall
  assert.deepEqual(await call('getSources', 'replaced-plugin'), current)
  assert.ok(current.kw.qualitys.includes('flac'))
})

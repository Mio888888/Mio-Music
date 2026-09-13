import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createWorker } from './plugin-sources.test.mjs'

// Compile the real converter without running Tauri's build script, which rewrites
// platform schemas. Run a Cargo build once before this test on a fresh checkout.
const root = fileURLToPath(new URL('..', import.meta.url))
const deps = resolve(root, 'src-tauri/target/debug/deps')
let dependencyFiles
try {
  dependencyFiles = readdirSync(deps)
} catch {
  throw new Error('LX converter tests require a prior Cargo build: src-tauri/target/debug/deps is missing')
}
const library = name => {
  const file = dependencyFiles.find(file => file.startsWith(`lib${name}-`) && file.endsWith('.rlib'))
  assert.ok(file, `LX converter tests require a prior Cargo build with ${name}`)
  return join(deps, file)
}
const temporary = mkdtempSync(join(tmpdir(), 'mio-lx-compat-'))
after(() => rmSync(temporary, { recursive: true, force: true }))
const helper = join(temporary, 'convert.rs')
const executable = join(temporary, process.platform === 'win32' ? 'convert.exe' : 'convert')
const converterPath = resolve(root, 'src-tauri/src/plugin/converter.rs')
writeFileSync(helper, `
#[path = ${JSON.stringify(converterPath)}]
mod converter;
fn main() {
    let mut source = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut source).unwrap();
    print!("{}", converter::convert_lx_plugin(&source));
}
`)
execFileSync('rustc', [
  '--edition=2021', helper,
  '-L', `dependency=${deps}`,
  '--extern', `regex_lite=${library('regex_lite')}`,
  '--extern', `serde_json=${library('serde_json')}`,
  '-o', executable,
], { stdio: 'pipe' })

export function convertLx(source) {
  return execFileSync(executable, { input: source, encoding: 'utf8' })
}

const delayedSource = `
/**
 * @name Async initialization fixture
 */
const { EVENT_NAMES, on, send, request } = globalThis.lx;
request('https://fixture.invalid/bootstrap', {}, (error, response) => {
  if (error) throw error;
  setTimeout(() => {
    on(EVENT_NAMES.request, async ({ source, info }) =>
      response.body.baseUrl + '/' + source + '/' + info.musicInfo.id + '/' + info.type);
    send(EVENT_NAMES.inited, {
      sources: { wy: { name: 'Fixture', type: 'music', actions: ['musicUrl'], qualitys: ['128k', 'flac'] } }
    });
  }, 10);
});
`

const bootstrapResponse = async () => ({
  statusCode: 200,
  headers: {},
  body: { baseUrl: 'https://fixture.invalid/audio' },
})

test('LX metadata waits for asynchronous inited and excludes undeclared fallback sources', async () => {
  const call = createWorker(convertLx(delayedSource), bootstrapResponse)
  const sources = await call('getSources', 'async-lx-metadata')
  assert.deepEqual(Object.keys(sources), ['wy'])
  assert.deepEqual(sources.wy.qualitys, ['128k', 'flac'])
})

test('LX playback waits for asynchronous request handler registration', async () => {
  const call = createWorker(convertLx(delayedSource), bootstrapResponse)
  assert.equal(
    await call('getMusicUrl', 'async-lx-playback', 'wy', { id: 'song-42' }, 'flac'),
    'https://fixture.invalid/audio/wy/song-42/flac',
  )
})

test('LX initialization errors preserve the actual plugin failure', async () => {
  const call = createWorker(convertLx(`throw new Error('fixture service has disabled this version')`))
  await assert.rejects(
    call('getMusicUrl', 'failed-lx', 'wy', { id: 'song-42' }, '128k'),
    /fixture service has disabled this version/,
  )
})

test('LX plugins can use globalThis.BigInt for bundled source encryption', async () => {
  const call = createWorker(convertLx(`
    const { on, send, EVENT_NAMES } = globalThis.lx;
    const createBigInt = globalThis.BigInt;
    on(EVENT_NAMES.request, async () => 'https://fixture.invalid/' + (createBigInt(41) + 1n));
    send(EVENT_NAMES.inited, { sources: { kw: { qualitys: ['128k'] } } });
  `))
  assert.equal(
    await call('getMusicUrl', 'bigint-lx', 'kw', { id: 'song-42' }, '128k'),
    'https://fixture.invalid/42',
  )
})

for (const platform of ['desktop', 'mobile']) {
  test(`LX reports the ${platform} environment accepted by source bootstrap services`, async () => {
    const call = createWorker(convertLx(`
      const { env, on, send, EVENT_NAMES } = globalThis.lx;
      on(EVENT_NAMES.request, async () => 'https://fixture.invalid/' + env);
      send(EVENT_NAMES.inited, { sources: { kw: { qualitys: ['128k'] } } });
    `), undefined, { platform })
    assert.equal(
      await call('getMusicUrl', 'environment-lx', 'kw', { id: 'song-42' }, '128k'),
      'https://fixture.invalid/' + platform,
    )
  })
}

test('LX preserves an asynchronous initialization failure from an HTTP callback', async () => {
  const call = createWorker(convertLx(`
    globalThis.lx.request('https://fixture.invalid/bootstrap', {}, async () => {
      await Promise.resolve();
      throw new Error('fixture callback bootstrap failed');
    });
  `), bootstrapResponse)
  await assert.rejects(call('getSources', 'failed-callback-lx'), /fixture callback bootstrap failed/)
})

test('LX preserves an asynchronous initialization failure from a timer callback', async () => {
  const call = createWorker(convertLx(`
    setTimeout(async () => {
      await Promise.resolve();
      throw new Error('fixture timer bootstrap failed');
    }, 0);
  `))
  await assert.rejects(call('getSources', 'failed-timer-lx'), /fixture timer bootstrap failed/)
})

test('LX send returns a Promise and readiness waits for a subsequently registered handler', async () => {
  const call = createWorker(convertLx(`
    const { on, send, EVENT_NAMES } = globalThis.lx;
    send(EVENT_NAMES.inited, { sources: { kw: { qualitys: ['128k'] } } }).then(() => {
      on(EVENT_NAMES.request, async () => 'https://fixture.invalid/ready.mp3');
    });
  `))
  assert.equal(
    await call('getMusicUrl', 'promise-send-lx', 'kw', { id: 'song-42' }, '128k'),
    'https://fixture.invalid/ready.mp3',
  )
})

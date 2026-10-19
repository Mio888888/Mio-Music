//! Synthetic, offline S1 probes. HTTP, encryption, browser globals, and real
//! plugin samples remain unverified; this is not a production sandbox.
use mio_plugin::converter::convert_lx_plugin;
use rquickjs::{Context, Runtime};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn probe(code: &str, fire_timers: bool) -> Value {
    let runtime = Runtime::new().unwrap();
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(1024 * 1024);
    let deadline = Instant::now() + Duration::from_millis(500);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let context = Context::full(&runtime).unwrap();
    // Timer callbacks are deterministically fired by the fixture driver. No host
    // filesystem, process, fetch, or crypto is exposed to these synthetic cases.
    let script = format!(
        r#"
        globalThis.probeResult = null;
        const timers = new Map(); let sequence = 0;
        const setTimeout = fn => {{ timers.set(++sequence, fn); return sequence; }};
        const clearTimeout = id => timers.delete(id);
        const setInterval = setTimeout, clearInterval = clearTimeout;
        const console = {{log(){{}}, error(){{}}, warn(){{}}}};
        const Buffer = {{}};
        const cerumusic = {{platform: 'desktop', request() {{throw Error('offline fixture');}}, utils: {{buffer: {{}}, crypto: {{}}}}}};
        const plugin = (() => {{ const module = {{exports: {{}}}}; const exports = module.exports;
            {code}
            return module.exports;
        }})();
        Promise.resolve(plugin.ready).then(async () => {{
            probeResult = JSON.stringify({{sources: plugin.sources,
                url: await plugin.musicUrl('wy', {{songmid:'fixture'}}, '320k'),
                pic: await plugin.getPic('wy', {{}}), lyric: await plugin.getLyric('wy', {{}})}});
        }}).catch(error => {{probeResult = JSON.stringify({{error: error.message}});}});
        {}
    "#,
        if fire_timers {
            "for (const callback of [...timers.values()]) callback();"
        } else {
            ""
        }
    );
    context.with(|cx| cx.eval::<(), _>(script)).unwrap();
    let mut jobs = 0;
    while runtime.execute_pending_job().unwrap() {
        jobs += 1;
        assert!(
            jobs < 1000 && Instant::now() < deadline,
            "fixture microtask deadline"
        );
    }
    let result: String = context
        .with(|cx| cx.globals().get("probeResult"))
        .expect("fixture settled");
    serde_json::from_str(&result).unwrap()
}

#[test]
fn ceru_exports_promises_and_bigint() {
    let result = probe(
        r#"
        module.exports = {
            sources: {wy: {name:'fixture', type:'music', qualitys:['128k','320k']}},
            musicUrl: async () => 'https://fixture.invalid/' + (BigInt(7) * BigInt(6)),
            getPic: () => 'cover', getLyric: () => '[00:00.00]fixture'
        };
    "#,
        false,
    );
    assert_eq!(result["url"], "https://fixture.invalid/42");
    assert_eq!(result["sources"]["wy"]["qualitys"], json!(["128k", "320k"]));
    assert_eq!(result["lyric"], "[00:00.00]fixture");
}

#[test]
fn original_lx_wrapper_preserves_runtime_qualities_and_desktop_environment() {
    let code = convert_lx_plugin(
        r#"
        // @name Offline fixture
        if (lx.env !== 'desktop') throw new Error('wrong environment');
        if (typeof globalThis.BigInt !== 'function') throw new Error('missing BigInt');
        lx.on('request', async ({action}) => {
            if (action === 'musicUrl') return {url:'https://fixture.invalid/' + BigInt(42)};
            if (action === 'pic') return {picUrl:'cover'};
            return {lyric:'[00:00.00]fixture', tlyric:'translation'};
        });
        lx.send('inited', {sources:{wy:{type:'music', qualitys:['320k']}}})
            .then(() => { if (typeof lx.send('inited', {}).then !== 'function') throw Error('send must return Promise'); });
    "#,
    );
    let result = probe(&code, false);
    assert_eq!(result["sources"].as_object().unwrap().len(), 1);
    assert_eq!(result["sources"]["wy"]["qualitys"], json!(["320k"]));
    assert_eq!(result["url"]["url"], "https://fixture.invalid/42");
    assert_eq!(result["pic"], "cover");
    assert_eq!(result["lyric"]["tlyric"], "translation");
}

#[test]
fn lx_errors_and_missing_initialization_are_observable() {
    let thrown = probe(
        &convert_lx_plugin("throw new Error('original fixture error');"),
        false,
    );
    assert_eq!(thrown["error"], "original fixture error");
    let missing = probe(
        &convert_lx_plugin("lx.on('request', () => 'unused');"),
        true,
    );
    assert!(missing["error"].as_str().unwrap().contains("初始化超时"));
}

#[test]
fn runaway_javascript_can_be_interrupted() {
    let runtime = Runtime::new().unwrap();
    let deadline = Instant::now() + Duration::from_millis(30);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let context = Context::full(&runtime).unwrap();
    assert!(
        context
            .with(|cx| cx.eval::<(), _>("while (true) {}"))
            .is_err()
    );
}

//! A generic, bounded WebAssembly bridge for plugin scripts (`cerumusic.utils.wasm`).
//!
//! QuickJS has no `WebAssembly`, so a plugin that ships a module (a signal-processing routine,
//! a codec) runs it through the host. The host knows nothing about what the module does: it
//! instantiates the bytes the script supplies, calls exports and table entries on request, and
//! answers each call the module makes to an import by running a JavaScript handler of the
//! script. Memory is exposed as plain reads and writes.
//!
//! Limits: module size, linear memory, instruction fuel, instance count; the 30 second budget
//! and cancellation of the script apply between calls into the host.
use base64::{Engine, engine::general_purpose::STANDARD};
use rquickjs::{Ctx, Function};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Instant,
};
use wasmi::{
    Config, Engine as WasmEngine, Error, Extern, ExternType, Func, Linker, Module, ResumableCall,
    Store, StoreLimits, StoreLimitsBuilder, Val,
    core::{HostError, ValType},
};

const MAX_MODULE_BYTES: usize = 4 * 1024 * 1024;
const MAX_INSTANCES: usize = 4;
const DEFAULT_MEMORY: u64 = 64 * 1024 * 1024;
const MAX_MEMORY: u64 = 128 * 1024 * 1024;
const DEFAULT_FUEL: u64 = 600_000_000;
const MAX_FUEL: u64 = 2_000_000_000;
const MAX_ARGS: usize = 32;
const MAX_TRANSFER: usize = 16 * 1024 * 1024;

struct State {
    limits: StoreLimits,
}
/// What the module asked of the host: raised by every imported function, answered by the
/// script's handler, then the call resumes.
#[derive(Debug)]
struct ImportCall {
    index: u32,
    args: Vec<f64>,
}
impl fmt::Display for ImportCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "import {}", self.index)
    }
}
impl HostError for ImportCall {}

struct Instance {
    store: Mutex<Store<State>>,
    instance: wasmi::Instance,
    /// A module cannot be entered again while one of its imports is being answered.
    busy: AtomicBool,
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

type Instances = Arc<Mutex<HashMap<u32, Arc<Instance>>>>;

fn to_f64(value: &Val) -> f64 {
    match value {
        Val::I32(v) => f64::from(*v),
        Val::I64(v) => *v as f64,
        Val::F32(v) => f64::from(v.to_float()),
        Val::F64(v) => v.to_float(),
        _ => 0.,
    }
}
fn from_f64(ty: ValType, value: f64) -> Val {
    match ty {
        ValType::I32 => Val::I32(value as i64 as i32),
        ValType::I64 => Val::I64(value as i64),
        ValType::F32 => Val::F32((value as f32).into()),
        ValType::F64 => Val::F64(value.into()),
        other => Val::default(other),
    }
}
fn fail<T>(message: impl Into<String>) -> Result<T, String> {
    Err(message.into())
}
fn reply(result: Result<Value, String>) -> String {
    match result {
        Ok(value) => json!({ "result": value }),
        Err(error) => json!({ "error": error }),
    }
    .to_string()
}
fn trap_message(error: &Error) -> String {
    format!("wasm 执行失败：{error}")
}

fn instantiate(
    bytes: &[u8],
    memory: u64,
    fuel: u64,
) -> Result<(Instance, Vec<(String, String)>), String> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = WasmEngine::new(&config);
    let module = Module::new(&engine, bytes).map_err(|_| "无法加载 wasm 模块")?;
    let mut store = Store::new(
        &engine,
        State {
            limits: StoreLimitsBuilder::new()
                .memory_size(memory as usize)
                .table_elements(1_000_000)
                .instances(1)
                .build(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store.set_fuel(fuel).map_err(|_| "wasm 燃料初始化失败")?;
    let mut linker = Linker::new(&engine);
    let mut imports = Vec::new();
    for import in module.imports() {
        let ExternType::Func(ty) = import.ty() else {
            return fail("wasm 模块只能导入函数");
        };
        let index = imports.len() as u32;
        imports.push((import.module().to_owned(), import.name().to_owned()));
        linker
            .func_new(
                import.module(),
                import.name(),
                ty.clone(),
                move |_caller, args, _results| {
                    Err(Error::host(ImportCall {
                        index,
                        args: args.iter().map(to_f64).collect(),
                    }))
                },
            )
            .map_err(|_| "wasm 导入初始化失败")?;
    }
    let instance = linker
        .instantiate(&mut store, &module)
        .and_then(|pre| pre.start(&mut store))
        .map_err(|_| "wasm 模块初始化失败")?;
    Ok((
        Instance {
            store: Mutex::new(store),
            instance,
            busy: AtomicBool::new(false),
        },
        imports,
    ))
}

/// The function a call targets: an export by name, or an element of an exported table.
fn resolve(
    instance: &Instance,
    store: &Store<State>,
    kind: &str,
    target: &str,
    index: u32,
) -> Result<Func, String> {
    match kind {
        "export" => match instance.instance.get_export(store, target) {
            Some(Extern::Func(func)) => Ok(func),
            _ => fail(format!("wasm 模块没有导出函数 {target}")),
        },
        "table" => {
            let table = instance
                .instance
                .get_table(store, target)
                .ok_or_else(|| format!("wasm 模块没有导出表 {target}"))?;
            match table.get(store, index) {
                Some(Val::FuncRef(reference)) => reference
                    .func()
                    .copied()
                    .ok_or_else(|| "wasm 表项为空".to_owned()),
                _ => fail("wasm 表项无效"),
            }
        }
        _ => fail("无效的 wasm 调用"),
    }
}

/// Enter the module, answering each import call through the script's `__wasmDispatch`.
#[allow(clippy::too_many_arguments)]
fn call(
    ctx: &Ctx<'_>,
    id: u32,
    instance: &Instance,
    kind: &str,
    target: &str,
    index: u32,
    args: &[f64],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Value, String> {
    if instance.busy.swap(true, Ordering::Acquire) {
        return fail("wasm 实例正在执行，不能重入");
    }
    let _busy = Busy(&instance.busy);
    let (func, mut outputs, mut step) = {
        let mut store = instance.store.lock().map_err(|_| "wasm 实例不可用")?;
        let func = resolve(instance, &store, kind, target, index)?;
        let ty = func.ty(&*store);
        if ty.params().len() != args.len() {
            return fail("wasm 调用参数个数不符");
        }
        let inputs: Vec<Val> = ty
            .params()
            .iter()
            .zip(args)
            .map(|(ty, value)| from_f64(*ty, *value))
            .collect();
        let mut outputs: Vec<Val> = ty.results().iter().map(|ty| Val::default(*ty)).collect();
        let step = func
            .call_resumable(&mut *store, &inputs, &mut outputs)
            .map_err(|error| trap_message(&error))?;
        (func, outputs, step)
    };
    let _ = func;
    loop {
        let invocation = match step {
            ResumableCall::Finished => break,
            ResumableCall::Resumable(invocation) => invocation,
        };
        if cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
            return fail("插件执行超时或取消");
        }
        let Some(import) = invocation.host_error().downcast_ref::<ImportCall>() else {
            return fail("wasm 执行失败");
        };
        let dispatch: Function = ctx
            .globals()
            .get("__wasmDispatch")
            .map_err(|_| "wasm 导入分发不可用")?;
        let answer = dispatch
            .call::<_, f64>((
                id,
                import.index,
                serde_json::to_string(&import.args).unwrap_or_default(),
            ))
            .map_err(|error| match error {
                rquickjs::Error::Exception => {
                    let caught = ctx.catch();
                    caught
                        .as_object()
                        .and_then(|o| o.get::<_, String>("message").ok())
                        .or_else(|| caught.as_string().and_then(|s| s.to_string().ok()))
                        .unwrap_or_else(|| "wasm 导入处理失败".into())
                }
                _ => "wasm 导入处理失败".into(),
            })?;
        let mut store = instance.store.lock().map_err(|_| "wasm 实例不可用")?;
        let host_results: Vec<Val> = invocation
            .host_func()
            .ty(&*store)
            .results()
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                if i == 0 {
                    from_f64(*ty, answer)
                } else {
                    Val::default(*ty)
                }
            })
            .collect();
        step = invocation
            .resume(&mut *store, &host_results, &mut outputs)
            .map_err(|error| trap_message(&error))?;
    }
    Ok(outputs
        .first()
        .map(to_f64)
        .map(|v| json!(v))
        .unwrap_or(Value::Null))
}

fn memory_of(
    instance: &Instance,
    store: &Store<State>,
    name: &str,
) -> Result<wasmi::Memory, String> {
    instance
        .instance
        .get_memory(store, name)
        .ok_or_else(|| format!("wasm 模块没有导出内存 {name}"))
}

fn find(instances: &Instances, id: u32) -> Result<Arc<Instance>, String> {
    instances
        .lock()
        .map_err(|_| "wasm 实例不可用".to_owned())?
        .get(&id)
        .cloned()
        .ok_or_else(|| "wasm 实例已释放".to_owned())
}

/// Install the `__wasm*` bridges of a list plugin's script (see `runtime.js`).
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<(), String> {
    let instances: Instances = Default::default();
    let next = Arc::new(AtomicU32::new(0));
    let globals = ctx.globals();
    let fail_install = |_| "wasm 桥初始化失败";

    let table = instances.clone();
    globals
        .set(
            "__wasmNew",
            Function::new(ctx.clone(), move |bytes: String, options: String| -> String {
                reply((|| {
                    let options: Value = serde_json::from_str(&options).unwrap_or(Value::Null);
                    let memory = options
                        .get("maxMemory")
                        .and_then(Value::as_u64)
                        .unwrap_or(DEFAULT_MEMORY)
                        .clamp(64 * 1024, MAX_MEMORY);
                    let fuel = options
                        .get("fuel")
                        .and_then(Value::as_u64)
                        .unwrap_or(DEFAULT_FUEL)
                        .clamp(1, MAX_FUEL);
                    let bytes = STANDARD
                        .decode(bytes.trim())
                        .map_err(|_| "无效的 wasm 数据")?;
                    if bytes.is_empty() || bytes.len() > MAX_MODULE_BYTES {
                        return fail("wasm 模块为空或超过 4MiB");
                    }
                    let mut live = table.lock().map_err(|_| "wasm 实例不可用")?;
                    if live.len() >= MAX_INSTANCES {
                        return fail("wasm 实例过多");
                    }
                    let (instance, imports) = instantiate(&bytes, memory, fuel)?;
                    let id = next.fetch_add(1, Ordering::Relaxed);
                    live.insert(id, Arc::new(instance));
                    Ok(json!({
                        "id": id,
                        "imports": imports.iter().enumerate().map(|(index, (module, name))| json!({"index": index, "module": module, "name": name})).collect::<Vec<_>>(),
                    }))
                })())
            }),
        )
        .map_err(fail_install)?;
    let table = instances.clone();
    let (stop, until) = (cancelled.clone(), deadline);
    globals
        .set(
            "__wasmCall",
            Function::new(
                ctx.clone(),
                move |scope: Ctx<'js>,
                      id: u32,
                      kind: String,
                      target: String,
                      index: u32,
                      args: String|
                      -> String {
                    reply((|| {
                        let args: Vec<f64> =
                            serde_json::from_str(&args).map_err(|_| "无效的 wasm 参数")?;
                        if args.len() > MAX_ARGS {
                            return fail("wasm 调用参数过多");
                        }
                        let instance = find(&table, id)?;
                        call(
                            &scope, id, &instance, &kind, &target, index, &args, &stop, until,
                        )
                    })())
                },
            ),
        )
        .map_err(fail_install)?;
    let table = instances.clone();
    globals
        .set(
            "__wasmRead",
            Function::new(
                ctx.clone(),
                move |id: u32, memory: String, ptr: f64, len: f64| -> String {
                    reply((|| {
                        if !(0. ..=MAX_TRANSFER as f64).contains(&len) || ptr < 0. {
                            return fail("无效的 wasm 内存范围");
                        }
                        let instance = find(&table, id)?;
                        let store = instance.store.lock().map_err(|_| "wasm 实例不可用")?;
                        let memory = memory_of(&instance, &store, &memory)?;
                        let data = memory.data(&*store);
                        let (start, len) = (ptr as usize, len as usize);
                        let end = start.checked_add(len).filter(|end| *end <= data.len());
                        let Some(end) = end else {
                            return fail("wasm 内存越界");
                        };
                        Ok(Value::String(STANDARD.encode(&data[start..end])))
                    })())
                },
            ),
        )
        .map_err(fail_install)?;
    let table = instances.clone();
    globals
        .set(
            "__wasmWrite",
            Function::new(
                ctx.clone(),
                move |id: u32, memory: String, ptr: f64, data: String| -> String {
                    reply((|| {
                        let bytes = STANDARD
                            .decode(data.trim())
                            .map_err(|_| "无效的 wasm 数据")?;
                        if bytes.len() > MAX_TRANSFER || ptr < 0. {
                            return fail("无效的 wasm 内存范围");
                        }
                        let instance = find(&table, id)?;
                        let mut store = instance.store.lock().map_err(|_| "wasm 实例不可用")?;
                        let memory = memory_of(&instance, &store, &memory)?;
                        memory
                            .write(&mut *store, ptr as usize, &bytes)
                            .map_err(|_| "wasm 内存越界")?;
                        Ok(Value::Null)
                    })())
                },
            ),
        )
        .map_err(fail_install)?;
    let table = instances;
    globals
        .set(
            "__wasmFree",
            Function::new(ctx.clone(), move |id: u32| {
                if let Ok(mut live) = table.lock() {
                    live.remove(&id);
                }
            }),
        )
        .map_err(fail_install)?;
    Ok(())
}

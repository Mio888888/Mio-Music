// Ceru/LX contracts ported from Mio Music's pluginWorker. Native bridges are bounded.
function __fmt(args){ return Array.from(args, a => { if (typeof a === 'string') return a; try { var j = JSON.stringify(a); return j === undefined ? String(a) : j; } catch (e) { return String(a); } }).join(' '); }
var console = { log(...a){__log('log',__fmt(a));}, info(...a){__log('info',__fmt(a));}, debug(...a){__log('debug',__fmt(a));}, warn(...a){__log('warn',__fmt(a));}, error(...a){__log('error',__fmt(a));}, group(){}, groupEnd(){}, groupCollapsed(){} };
var window = globalThis;
var __timers = new Map(), __timerId = 0;
function setTimeout(fn, ms, ...args) { if (__timers.size >= 128) throw Error('定时器过多'); var id=++__timerId; __timers.set(id,{fn,args,at:__now()+Math.max(0,Number(ms)||0)}); return id; }
function clearTimeout(id) { __timers.delete(id); }
function __runTimers(){ for(var [id,t] of [...__timers]) if(t.at<=__now()){__timers.delete(id);t.fn(...t.args);} }
function setInterval(){ throw Error('当前宿主不支持周期定时器'); }
function clearInterval(id){ clearTimeout(id); }
function __request(url, opts, callback){
  if(typeof opts==='function'){callback=opts;opts={};} opts=opts||{};
  var run=()=>{var r=JSON.parse(__http(String(url),JSON.stringify(opts)));if(r.error)throw Error(r.error);return r.result;};
  if(callback){Promise.resolve().then(run).then(r=>callback(null,r),e=>callback(e,{statusCode:0,headers:{},body:''}));return;}
  return Promise.resolve().then(run);
}
/** Several requests at once; resolves to `[{response}|{error}]` in input order, never rejects per item. */
function __requestAll(items){
  return Promise.resolve().then(()=>JSON.parse(__httpMany(JSON.stringify(items.map(i=>({url:String(i.url),options:i.options||{}}))))).map(r=>r.error?{error:r.error}:{response:r.result}));
}
function fetch(url,opts){return __request(url,opts).then(r=>({ok:r.statusCode>=200&&r.statusCode<300,status:r.statusCode,headers:new Map(Object.entries(r.headers)),json:async()=>typeof r.body==='string'?JSON.parse(r.body):r.body,text:async()=>typeof r.body==='string'?r.body:JSON.stringify(r.body)}));}
function __from(data,encoding){
 var bytes;
 if(typeof data==='string'){
  var s=encoding==='base64'?atob(data):encoding==='hex'?data.match(/.{2}/g)?.map(x=>String.fromCharCode(parseInt(x,16))).join('')||'':unescape(encodeURIComponent(data));
  bytes=Uint8Array.from(s,c=>c.charCodeAt(0));
 } else bytes=new Uint8Array(data);
 bytes.toString=function(enc){if(enc==='hex')return [...bytes].map(x=>x.toString(16).padStart(2,'0')).join('');var s=Array.from(bytes,x=>String.fromCharCode(x)).join('');return enc==='base64'?btoa(s):decodeURIComponent(escape(s));};return bytes;
}
var Buffer={from:__from};
var __crypto={md5:__md5,sha1:__sha1,aes128EcbHex:__aes128ecb,aes128EcbBase64:__aes128ecbB64,aesEncrypt(){throw Error('当前宿主未实现 AES');},rsaEncrypt(){throw Error('当前宿主未实现 RSA');},randomBytes(){throw Error('当前宿主未实现随机字节');}};
var cerumusic={env:'browser',platform:'desktop',version:'1.0.3',request:__request,requestAll:__requestAll,utils:{buffer:{from:__from,bufToString:(b,e)=>__from(b).toString(e)},crypto:__crypto},NoticeCenter(){}};
cerumusic.utils.lyric={validTtml:__validTtml,decodeKuwo:(body,lyricx)=>JSON.parse(__decodeKuwo(String(body),Boolean(lyricx)))};
cerumusic.locale=typeof __locale==='string'?__locale:'zh-CN';
if(typeof __configJson==='string'){try{cerumusic.config=JSON.parse(__configJson);}catch(e){cerumusic.config={};}}else{cerumusic.config={};}
cerumusic.storage={
 get(k){if(typeof __storeGet!=='function')return undefined;var r=__storeGet(String(k));return r===''?undefined:JSON.parse(r);},
 set(k,v){if(typeof __storeSet!=='function')throw Error('当前宿主不支持存储');__storeSet(String(k),JSON.stringify(v===undefined?null:v));},
 remove(k){if(typeof __storeDel==='function')__storeDel(String(k));}
};
var __lxUsed=false,__inited=false,__handler=null,__lxSources={};
var lx={env:'desktop',version:'2.0.0',EVENT_NAMES:{request:'request',inited:'inited',updateAlert:'updateAlert'},utils:cerumusic.utils,request:__request,
 on(event,handler){__lxUsed=true;if(event==='request')__handler=handler;},
 send(event,data){__lxUsed=true;if(event==='inited'){__inited=true;__lxSources=data?.sources||{};}return Promise.resolve();}};
function require(){throw Error('当前宿主不支持 require');}

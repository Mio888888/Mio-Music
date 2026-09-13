use regex_lite::Regex;

/// Refresh only our generated LX wrapper in memory, preserving the installed file.
pub fn refresh_lx_conversion(code: String) -> String {
    if !code.starts_with("/**\n * 由 CeruMusic 插件转换器转换") {
        return code;
    }
    let Some((_, embedded)) = code.split_once("\nconst originalPluginCode = ") else {
        return code;
    };
    let mut values = serde_json::Deserializer::from_str(embedded).into_iter::<String>();
    match values.next() {
        Some(Ok(original)) if embedded[values.byte_offset()..].starts_with(';') => {
            convert_lx_plugin(&original)
        }
        _ => code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_embedded_lx_without_evaluating_javascript() {
        let original = "// @name fixture\nconst tricky = '\";\\n';\nlx.send('inited', {});";
        let legacy = format!("/**\n * 由 CeruMusic 插件转换器转换\n */\nconst originalPluginCode = {};\noldWrapper();", serde_json::to_string(original).unwrap());
        assert_eq!(refresh_lx_conversion(legacy), convert_lx_plugin(original));
    }

    #[test]
    fn leaves_regular_and_malformed_plugins_unchanged() {
        for code in ["module.exports = {};", "/**\n * 由 CeruMusic 插件转换器转换\nconst originalPluginCode = malicious();"] {
            assert_eq!(refresh_lx_conversion(code.to_owned()), code);
        }
    }
}

/// Extract metadata from lx event-driven plugin comments.
struct PluginMeta {
    name: String,
    version: String,
    author: String,
    description: String,
    homepage: String,
}

fn extract_meta(code: &str) -> PluginMeta {
    let name = Regex::new(r"@name\s+(.+)")
        .ok()
        .and_then(|re| re.captures(code))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "未知插件".to_string());

    let version = Regex::new(r"@version\s+(.+)")
        .ok()
        .and_then(|re| re.captures(code))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "1.0.0".to_string());

    let author = Regex::new(r"@author\s+(.+)")
        .ok()
        .and_then(|re| re.captures(code))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let description = Regex::new(r"@description\s+(.+)")
        .ok()
        .and_then(|re| re.captures(code))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "从事件驱动插件转换而来".to_string());

    let homepage = Regex::new(r"@homepage\s+(.+)")
        .ok()
        .and_then(|re| re.captures(code))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    PluginMeta { name, version, author, description, homepage }
}

/// Convert an lx event-driven plugin to standard CeruMusic format.
/// Ported from CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts
pub fn convert_lx_plugin(original_code: &str) -> String {
    let meta = extract_meta(original_code);
    let original_json = serde_json::to_string(original_code)
        .unwrap_or_else(|_| "\"\"".to_string());

    format!(r##"/**
 * 由 CeruMusic 插件转换器转换 - @author sqj
 * @name {name}
 * @author {author}
 * @version {version}
 * @description {description}
 */

const pluginInfo = {{
  name: "{name}",
  version: "{version}",
  author: "{author}",
  description: "{description}"
}};

// 原始插件代码
const originalPluginCode = {original_json};

// 音源信息将通过插件的 send 调用动态获取
let sources = {{}};

function getSourceName(sourceId) {{
  const nameMap = {{
    'kw': '小蜗',
    'kg': '小苟',
    'tx': '小鹅',
    'wy': '小芸',
    'mg': '菇菇'
  }};
  return nameMap[sourceId] || sourceId.toUpperCase() + '音乐';
}}

// 提取默认音源配置作为备用
function extractDefaultSources() {{
  function buildSources(qualityData) {{
    const extractedSources = {{}};
    Object.keys(qualityData).forEach(sourceId => {{
      extractedSources[sourceId] = {{
        name: getSourceName(sourceId),
        type: 'music',
        qualitys: qualityData[sourceId] || ['128k', '320k']
      }};
    }});
    return extractedSources;
  }}

  // 尝试从 MUSIC_QUALITY 常量中提取音源信息 - JSON.parse 格式
  const jsonParseMatch = originalPluginCode.match(/const\s+MUSIC_QUALITY\s*=\s*JSON\.parse\(([^)]+)\)/);
  if (jsonParseMatch) {{
    try {{
      let qualityStr = jsonParseMatch[1].trim();
      if (qualityStr.startsWith("'") && qualityStr.endsWith("'")) {{
        qualityStr = qualityStr.slice(1, -1);
      }} else if (qualityStr.startsWith('"') && qualityStr.endsWith('"')) {{
        qualityStr = qualityStr.slice(1, -1);
      }}

      return buildSources(JSON.parse(qualityStr));
    }} catch (e) {{}}
  }}

  // 尝试直接提取对象字面量格式: {{ kw: ['128k', '320k'], ... }}
  const objectMatch = originalPluginCode.match(/const\s+MUSIC_QUALITY\s*=\s*(\{{[^;]*\}})\s*;?/);
  if (objectMatch) {{
    try {{
      return buildSources((new Function('return ' + objectMatch[1]))());
    }} catch (e) {{}}
  }}

  // 解析失败，使用默认配置
  return {{
    kw: {{ name: "小蜗", type: "music", qualitys: ['128k', '320k'] }},
    kg: {{ name: "小苟", type: "music", qualitys: ['128k', '320k'] }},
    tx: {{ name: "小鹅", type: "music", qualitys: ['128k', '320k'] }},
    wy: {{ name: "小芸", type: "music", qualitys: ['128k', '320k'] }},
    mg: {{ name: "菇菇", type: "music", qualitys: ['128k', '320k'] }}
  }};
}}

// 初始化默认音源
sources = extractDefaultSources();

// LX 必须发送 inited 后才能读取音源和处理播放请求。
let didSendInited = false;
let requestHandler = null;
let settled = false;
let resolveReady;
let rejectReady;
const ready = new Promise((resolve, reject) => {{ resolveReady = resolve; rejectReady = reject; }});
const initTimer = setTimeout(() => failInit(new Error('洛雪音源初始化超时，请检查网络或音源服务')), 15000);
function failInit(error) {{
  if (settled) return;
  settled = true;
  clearTimeout(initTimer);
  rejectReady(error instanceof Error ? error : new Error(String(error)));
}}
function finishInit() {{
  if (settled || !didSendInited || !requestHandler) return;
  settled = true;
  clearTimeout(initTimer);
  resolveReady();
}}
function guard(callback) {{
  return (...args) => {{
    try {{
      const result = callback(...args);
      if (result && typeof result.then === 'function') return result.catch(error => {{ failInit(error); }});
      return result;
    }} catch (error) {{
      if (settled) throw error;
      failInit(error);
    }}
  }};
}}

// 从 cerumusic 获取网络请求和工具函数
const {{ request, utils }} = cerumusic;

initializePlugin();
function initializePlugin() {{
  const mockLx = {{
    EVENT_NAMES: {{
      request: 'request',
      inited: 'inited',
      updateAlert: 'updateAlert'
    }},
    on: (event, handler) => {{
      if (event === 'request') {{
        requestHandler = handler;
        finishInit();
      }}
    }},
    send: (event, data) => {{
      if (event === 'inited' && !settled) {{
        if (!data || !data.sources || typeof data.sources !== 'object') {{
          failInit(new Error('洛雪音源初始化返回了无效的音源信息'));
          return Promise.resolve();
        }}
        // 原位更新 exports.sources，并移除插件未声明的默认音源。
        Object.keys(sources).forEach(key => delete sources[key]);
        Object.keys(data.sources).forEach(sourceId => {{
          if (['__proto__', 'constructor', 'prototype'].includes(sourceId)) return;
          const sourceInfo = data.sources[sourceId];
          if (!sourceInfo || typeof sourceInfo !== 'object') return;
          sources[sourceId] = {{
            name: getSourceName(sourceId),
            type: sourceInfo.type || 'music',
            qualitys: sourceInfo.qualitys || []
          }};
        }});
        didSendInited = true;
        finishInit();
      }}
      return Promise.resolve();
    }},
    request: (url, options, callback) => {{
      if (typeof options === 'function') return request(url, guard(options));
      return request(url, options, typeof callback === 'function' ? guard(callback) : undefined);
    }},
    utils: {{
      buffer: utils.buffer,
      crypto: {{
        aesEncrypt: (data, mode, key, iv) => {{
          try {{ return utils.crypto.aesEncrypt(data, mode, key, iv); }} catch(e) {{ return data; }}
        }},
        md5: (str) => {{
          try {{ return utils.crypto.md5(str); }} catch(e) {{ return str; }}
        }},
        randomBytes: (size) => {{
          try {{ return utils.crypto.randomBytes(size); }} catch(e) {{ return new Uint8Array(size); }}
        }},
        rsaEncrypt: (data, key) => {{
          try {{ return utils.crypto.rsaEncrypt(data, key); }} catch(e) {{ return data; }}
        }}
      }}
    }},
    version: '1.0.0',
    apiVersion: '1.0.0',
    currentScriptInfo: {{
      rawScript: originalPluginCode,
      name: '{name}',
      version: '{version}',
      author: '{author}',
      description: '{description}',
      homepage: '{homepage}'
    }},
    env: cerumusic.platform === 'mobile' ? 'mobile' : 'desktop'
  }};

  try {{
    const pluginFunction = new Function(
      'globalThis', 'lx', 'console', 'setTimeout', 'clearTimeout',
      'setInterval', 'clearInterval', 'Buffer', 'JSON', 'require',
      'module', 'exports', 'process', 'global',
      originalPluginCode
    );

    const pluginGlobal = {{ lx: mockLx, BigInt, Buffer, JSON }};
    pluginFunction(
      pluginGlobal, mockLx, console, (callback, delay, ...args) => setTimeout(guard(callback), delay, ...args), clearTimeout,
      (callback, delay, ...args) => setInterval(guard(callback), delay, ...args), clearInterval, Buffer, JSON, () => ({{}}),
      {{ exports: {{}} }}, {{}}, {{ env: {{ NODE_ENV: 'production' }} }}, pluginGlobal
    );
  }} catch (error) {{
    failInit(error);
  }}
}}

async function musicUrl(source, musicInfo, quality) {{
  await ready;

  if (!requestHandler) {{
    throw new Error('插件请求处理器未初始化');
  }}

  try {{
    const result = await requestHandler({{
      source: source,
      action: 'musicUrl',
      info: {{
        musicInfo: musicInfo,
        type: quality
      }}
    }});

    if (!result) {{
      throw new Error('获取音源链接失败: 返回结果为空');
    }}

    if (typeof result === 'object' && result.error) {{
      throw new Error(result.error || '获取音源链接失败');
    }}

    if (typeof result === 'object' && result.code && result.code !== 200) {{
      throw new Error(result.msg || '接口错误 (Code: ' + result.code + ')');
    }}

    return result;
  }} catch (error) {{
    throw new Error(error.message || '获取音源链接时发生未知错误');
  }}
}}

async function getPic(source, musicInfo) {{
  await ready;

  if (!requestHandler) {{
    throw new Error('插件请求处理器未初始化');
  }}

  try {{
    const result = await requestHandler({{
      source: source,
      action: 'pic',
      info: {{
        musicInfo: musicInfo
      }}
    }});

    if (!result) return '';

    if (typeof result === 'string') return result;
    if (result.url) return result.url;
    if (result.picUrl) return result.picUrl;

    return '';
  }} catch (error) {{
    console.error('[LX插件] getPic error:', error.message);
    return '';
  }}
}}

async function getLyric(source, musicInfo) {{
  await ready;

  if (!requestHandler) {{
    throw new Error('插件请求处理器未初始化');
  }}

  try {{
    const result = await requestHandler({{
      source: source,
      action: 'lyric',
      info: {{
        musicInfo: musicInfo
      }}
    }});

    if (!result) return '';

    if (typeof result === 'string') return result;

    if (typeof result === 'object') {{
      if (result.lyric || result.tlyric || result.rlyric || result.lxlyric) {{
        return {{
          lyric: result.lyric || '',
          tlyric: result.tlyric || '',
          rlyric: result.rlyric || '',
          lxlyric: result.lxlyric || ''
        }};
      }}
      if (result.lrc) return result.lrc;
    }}

    return typeof result === 'string' ? result : '';
  }} catch (error) {{
    console.error('[LX插件] getLyric error:', error.message);
    return '';
  }}
}}

module.exports = {{
  ready,
  pluginInfo,
  sources,
  musicUrl,
  getPic,
  getLyric
}};
"##,
        name = meta.name,
        version = meta.version,
        author = meta.author,
        description = meta.description,
        homepage = meta.homepage,
        original_json = original_json,
    )
}

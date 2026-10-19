'use strict';
(() => {
  const $ = id => document.getElementById(id);
  let language = navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';
  let statusKey = 'waiting';
  const text = key => window.MIO_LAN[language][key] || window.MIO_LAN[language].failed;
  const note = key => { statusKey = key; $('status').textContent = text(key); };
  function translate() {
    document.documentElement.lang = language;
    document.title = 'Mio Music · ' + text('title');
    document.querySelectorAll('[data-i18n]').forEach(element => { element.textContent = text(element.dataset.i18n); });
    note(statusKey);
  }
  $('language').value = language;
  $('language').addEventListener('change', () => { language = $('language').value; translate(); });
  translate();
  let session = null, busy = false;
  function lock(value) { busy = value; document.querySelectorAll('button').forEach(button => { button.disabled = value; }); }
  async function post(path, body, key, name) {
    const headers = {'Authorization': 'Bearer ' + key, 'Content-Type': 'application/octet-stream'};
    if (name) headers['X-Mio-Name'] = encodeURIComponent(name);
    let response;
    try { response = await fetch(path, {method:'POST',headers,body,cache:'no-store',credentials:'omit',redirect:'error'}); } catch (_) { throw new Error('disconnected'); }
    let result;
    try { result = await response.json(); } catch (_) { throw new Error('disconnected'); }
    if (!response.ok || !result.ok) throw new Error(result.message || 'failed');
    return result;
  }
  async function pair() {
    if (busy) return;
    const token = $('code').value.trim();
    if (!/^[0-9a-f]{64}$/.test(token)) { note('enterCode'); return; }
    lock(true); note('connecting');
    try {
      const result = await post('/pair', '', token);
      session = result.session;
      $('code').value = '';
      $('pairing').hidden = true; $('send').hidden = false;
      note('connected');
    } catch (error) { note(String(error.message || 'disconnected').split(':')[0]); }
    finally { lock(false); }
  }
  async function send(path, file, name, limit) {
    if (busy || !session) return;
    if (!file || file.size === 0 || file.size > limit) { note('badFile'); return; }
    lock(true); note('sending');
    try { const result = await post(path, file, session, name); note(result.message); }
    catch (error) { note(String(error.message || 'disconnected').split(':')[0]); }
    finally { lock(false); }
  }
  $('pair').addEventListener('click', pair);
  $('sendAudio').addEventListener('click', () => { const file = $('audio').files[0]; send('/audio', file, file?.name, 256*1024*1024); });
  $('sendPlugin').addEventListener('click', () => { const file = $('plugin').files[0]; send('/plugin', file, file?.name, 1024*1024); });
  $('sendUrl').addEventListener('click', () => send('/plugin-url', new Blob([$('url').value.trim()]), null, 4096));
  const invitation = location.hash.slice(1);
  history.replaceState(null, '', '/');
  if (/^[0-9a-f]{64}$/.test(invitation)) { $('code').value = invitation; }
})();

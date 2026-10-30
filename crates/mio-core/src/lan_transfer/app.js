'use strict';
(() => {
  const $ = id => document.getElementById(id);
  let language = navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';
  const listeners = [];
  let statusKey = 'waiting', session = null, busy = false;
  const text = key => window.MIO_LAN[language][key] || window.MIO_LAN[language].failed;
  const tr = (en, zh) => language === 'zh-CN' ? zh : en;
  // Entries of the app's own locale files, e.g. text('settings.musicSource.navAbout').
  const label = (path, fallback) => {
    let value = window.MIO_TEXT && window.MIO_TEXT[language];
    for (const part of path.split('.')) value = value && value[part];
    return typeof value === 'string' ? value : (fallback || path);
  };
  // Elements are built with textContent only, so plugin or file names can never inject markup.
  function h(tag, props, ...children) {
    const element = document.createElement(tag);
    for (const [key, value] of Object.entries(props || {})) {
      if (value === undefined || value === null || value === false) continue;
      if (key === 'class') element.className = value;
      else if (key === 'text') element.textContent = value;
      else if (key.startsWith('on')) element.addEventListener(key.slice(2), value);
      else if (value === true) element.setAttribute(key, '');
      else element.setAttribute(key, value);
    }
    for (const child of children.flat(Infinity)) {
      if (child === undefined || child === null || child === false) continue;
      element.append(child.nodeType ? child : document.createTextNode(String(child)));
    }
    return element;
  }
  let toastTimer = null;
  function toast(message, bad) {
    const node = $('toast');
    node.replaceChildren(h('div', {class: 'toast' + (bad ? ' bad' : ''), text: message}));
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => node.replaceChildren(), bad ? 5000 : 2200);
  }
  function describe(error) {
    const key = String(error && error.message || 'failed').split(':')[0];
    const known = window.MIO_LAN[language][key];
    if (known) return known;
    const own = {
      queue_empty: tr('The play queue is empty.', '播放列表为空。'),
      invalid_input: tr('That value is not valid.', '输入的内容无效。'),
      busy: tr('The TV is busy. Try again in a moment.', '电视正忙,请稍后再试。'),
      settings_changed: tr('Settings changed on the TV. Reload and try again.', '设置已在电视上变化,请刷新后重试。'),
      stale: tr('Settings changed on the TV. Reload and try again.', '设置已在电视上变化,请刷新后重试。'),
      export_failed: tr('Export failed.', '导出失败。'),
    }[key];
    if (own) return own;
    return tr('Operation failed. Check the TV and try again.', '操作失败，请查看电视后重试。');
  }
  function note(key) { statusKey = key; $('status').textContent = text(key); }
  function translate() {
    document.documentElement.lang = language;
    document.title = 'Mio Music · ' + tr('Settings', '设置');
    $('title').textContent = tr('Settings', '设置');
    document.querySelectorAll('[data-i18n]').forEach(element => { element.textContent = text(element.dataset.i18n); });
    note(statusKey);
    listeners.forEach(fn => fn());
  }
  async function request(path, body, key, headers) {
    let response;
    try {
      response = await fetch(path, {method: 'POST', headers: Object.assign({'Authorization': 'Bearer ' + key}, headers), body, cache: 'no-store', credentials: 'omit', redirect: 'error'});
    } catch (_) { throw new Error('disconnected'); }
    let result;
    try { result = await response.json(); } catch (_) { throw new Error('disconnected'); }
    if (!response.ok || !result.ok) throw new Error(result.message || 'failed');
    return result;
  }
  async function call(op, args) {
    if (!session) throw new Error('unauthorized');
    try {
      return (await request('/api/' + op, JSON.stringify(args === undefined ? {} : args), session, {'Content-Type': 'application/json'})).data;
    } catch (error) {
      const key = String(error.message).split(':')[0];
      if (key === 'unauthorized' || key === 'lan_session_expired' || key === 'disconnected') ended();
      throw error;
    }
  }
  async function upload(path, body, name) {
    const headers = {'Content-Type': 'application/octet-stream'};
    if (name) headers['X-Mio-Name'] = encodeURIComponent(name);
    return request(path, body, session, headers);
  }
  // The session key lives in this tab's session storage, so refreshing the
  // page keeps the connection. It is dropped as soon as the TV rejects it.
  const KEY = 'mio-lan-session';
  const remember = value => { try { if (value) sessionStorage.setItem(KEY, value); else sessionStorage.removeItem(KEY); } catch (_) {} };
  function ended() {
    if (!session) return;
    session = null;
    remember(null);
    $('app').hidden = true; $('pairing').hidden = false;
    note('disconnected');
  }
  function lock(value) { busy = value; $('pair').disabled = value; }
  async function pair() {
    if (busy) return;
    const token = $('code').value.trim();
    if (!/^[0-9a-f]{64}$/.test(token)) { note('enterCode'); return; }
    lock(true); note('connecting');
    try {
      const result = await request('/pair', '', token, {'Content-Type': 'application/octet-stream'});
      session = result.session;
      remember(session);
      $('code').value = '';
      $('pairing').hidden = true; $('app').hidden = false;
      window.MioSettings && window.MioSettings.start();
    } catch (error) { note(String(error.message || 'disconnected').split(':')[0]); }
    finally { lock(false); }
  }
  window.MioWeb = {
    h, tr, label, toast, call, upload, describe,
    get language() { return language; },
    get paired() { return session !== null; },
    onLanguage: fn => listeners.push(fn),
  };
  $('language').value = language;
  $('language').addEventListener('change', () => { language = $('language').value; translate(); });
  translate();
  $('pair').addEventListener('click', pair);
  const invitation = location.hash.slice(1);
  history.replaceState(null, '', '/');
  if (/^[0-9a-f]{64}$/.test(invitation)) { $('code').value = invitation; pair(); }
  else {
    let saved = null;
    try { saved = sessionStorage.getItem(KEY); } catch (_) {}
    if (saved && /^[0-9a-f]{64}$/.test(saved)) resume(saved);
  }
  async function resume(key) {
    session = key;
    note('connecting');
    try {
      await call('about.get');
      $('pairing').hidden = true; $('app').hidden = false;
      window.MioSettings && window.MioSettings.start();
    } catch (_) { session = null; remember(null); note('waiting'); }
  }
})();

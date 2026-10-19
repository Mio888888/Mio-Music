// Stand-in for the Tauri bridge of the original frontend, used only by wk_capture.swift.
// Every command answers with an empty value; the window plugin answers with plausible numbers.
(function () {
  const callbacks = {};
  let next = 1;
  const windowCommands = {
    'plugin:window|is_maximized': false, 'plugin:window|is_fullscreen': false,
    'plugin:window|is_minimized': false, 'plugin:window|is_focused': true,
    'plugin:window|is_visible': true, 'plugin:window|scale_factor': 2,
    'plugin:window|inner_size': { width: innerWidth * 2, height: innerHeight * 2 },
    'plugin:window|outer_size': { width: innerWidth * 2, height: innerHeight * 2 },
    'plugin:window|inner_position': { x: 0, y: 0 }, 'plugin:window|outer_position': { x: 0, y: 0 },
    'plugin:app|version': '0.3.6', 'plugin:app|name': 'Mio Music',
    'plugin:event|listen': 1,
  };
  try {
    if (!localStorage.getItem('appSettings')) {
      localStorage.setItem('appSettings', JSON.stringify({ language: 'zh-CN' }));
    }
  } catch (e) {}
  window.__errs = [];
  addEventListener('error', (e) => window.__errs.push(String(e.message)));
  addEventListener('unhandledrejection', (e) => window.__errs.push(String(e.reason && e.reason.stack || e.reason)));
  window.isTauri = true;
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { windowLabel: 'main', label: 'main' },
      windows: [{ label: 'main' }], webviews: [{ windowLabel: 'main', label: 'main' }],
    },
    plugins: {},
    transformCallback(callback) {
      const id = next++;
      callbacks[id] = callback;
      window['_' + id] = callback;
      return id;
    },
    unregisterCallback(id) { delete callbacks[id]; },
    convertFileSrc(path) { return path; },
    async invoke(command) {
      if (command in windowCommands) return windowCommands[command];
      if (command.startsWith('plugin:')) return null;
      return null;
    },
  };
  window.__TAURI_OS_PLUGIN_INTERNALS__ = { platform: 'macos', os_type: 'macos', family: 'unix', version: '15.0.0', arch: 'aarch64', eol: '\n', exe_extension: '' };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
})();

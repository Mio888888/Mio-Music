'use strict';
(() => {
  const {h, tr, label, toast, call, upload, describe} = window.MioWeb;
  const page = document.getElementById('page');
  const tabs = document.getElementById('tabs');
  let current = 'appearance', token = 0;

  // ---- building blocks -------------------------------------------------
  const card = (title, note, ...rows) => h('section', {class: 'card'},
    title && h('h2', {text: title}), note && h('p', {class: 'note', text: note}), rows);
  const row = (name, desc, control, stack) => h('div', {class: 'row' + (stack ? ' stack' : '')},
    h('div', {class: 'text'}, h('div', {class: 'name', text: name}), desc && h('div', {class: 'desc', text: desc})),
    control && h('div', {class: 'control'}, control));
  const wide = (name, desc, control) => row(name, desc, control, true);
  const button = (text, onclick, kind) => h('button', {type: 'button', class: kind || '', text, onclick});
  const toggle = (value, change, name, disabled) => h('button', {
    type: 'button', class: 'switch', role: 'switch', 'aria-checked': String(!!value), 'aria-label': name || '', disabled: !!disabled,
    onclick: event => change(event.currentTarget.getAttribute('aria-checked') !== 'true'),
  });
  const segmented = (options, value, change) => h('div', {class: 'seg', role: 'group'},
    options.map(([key, text]) => h('button', {type: 'button', 'aria-pressed': String(key === value), text, onclick: () => change(key)})));
  const select = (options, value, change) => {
    const node = h('select', {onchange: () => change(node.value)},
      options.map(([key, text]) => h('option', {value: key, text, selected: key === value})));
    return node;
  };
  function slider(min, max, step, value, format, change) {
    const out = h('output', {text: format(value)});
    const input = h('input', {type: 'range', min, max, step, value, oninput: () => { out.textContent = format(Number(input.value)); }, onchange: () => change(Number(input.value))});
    return h('div', {class: 'range'}, input, out);
  }
  function busy(node, work) {
    const wasDisabled = node && node.disabled;
    if (node) node.disabled = true;
    return work().finally(() => { if (node) node.disabled = wasDisabled; });
  }
  // Runs one call, reports failures and redraws the page so it shows the saved state.
  async function run(work, refresh) {
    try { await work(); if (refresh !== false) await show(current, true); } catch (error) { toast(describe(error), true); await show(current, true).catch(() => {}); }
  }
  const saved = () => toast(tr('Saved', '已保存'));
  const save = (op, args) => run(async () => { await call(op, args); saved(); });
  // In-page replacement for confirm()/prompt(): resolves true / the typed text, or null when cancelled.
  function ask({title, message, input, ok, danger, placeholder}) {
    return new Promise(resolve => {
      const field = input ? h('input', {type: 'text', placeholder: placeholder || '', maxlength: 128, autocomplete: 'off', spellcheck: 'false'}) : null;
      const done = value => { document.removeEventListener('keydown', keys, true); backdrop.remove(); resolve(value); };
      const accept = () => done(input ? (field.value.trim() || null) : true);
      const keys = event => {
        if (event.key === 'Escape') { event.preventDefault(); done(null); }
        else if (event.key === 'Enter' && input) { event.preventDefault(); accept(); }
      };
      const confirmButton = button(ok || tr('OK', '确定'), accept, danger ? 'danger' : 'primary');
      const backdrop = h('div', {class: 'backdrop', onclick: event => { if (event.target === backdrop) done(null); }},
        h('div', {class: 'dialog', role: 'dialog', 'aria-modal': 'true'},
          title && h('h3', {text: title}), message && h('p', {text: message}), field,
          h('div', {class: 'actions'}, button(tr('Cancel', '取消'), () => done(null)), confirmButton)));
      document.addEventListener('keydown', keys, true);
      document.body.append(backdrop);
      (field || confirmButton).focus();
    });
  }
  const pageHead = (title, desc) => h('header', {class: 'pagehead'}, h('h2', {text: title}), desc && h('p', {text: desc}));
  const qualityName = q => {
    const key = {'128k': 'display128k', '320k': 'display320k', flac: 'displayFlac', flac24bit: 'displayFlac24bit', hires: 'displayHires', atmos: 'displayAtmos', atmosPlus: 'displayAtmosPlus', master: 'displayMaster'}[q];
    return key ? label('quality.' + key, q) : q;
  };
  const size = bytes => bytes >= 1073741824 ? (bytes / 1073741824).toFixed(2) + ' GiB' : (bytes / 1048576).toFixed(1) + ' MiB';

  // ---- pages -------------------------------------------------------------
  async function appearance() {
    const [general, visual, background, lyric] = [await call('general.get'), await call('visual.get'), await call('background.get'), await call('lyric.get')];
    const v = (key, name, desc) => row(name, desc, toggle(visual[key], on => save('visual.set', {[key]: on}), name));
    const b = (key, name, desc) => row(name, desc, toggle(background[key], on => save('background.set', {[key]: on}), name));
    return [
      card(label('settings.appearance.title', tr('Basic appearance', '基础外观')), null,
        row(tr('Theme', '主题'), null, segmented([[false, tr('Light', '浅色')], [true, tr('Dark', '深色')]], general.dark, dark => save('general.set', {dark}))),
        row(tr('Language', '语言'), null, select([['system', tr('Follow system', '跟随系统')], ['zh-CN', '简体中文'], ['en-US', 'English']], general.language, language => save('general.set', {language}))),
        v('coverDynamicTheme', label('settings.appearance.coverDynamicTheme'), label('settings.appearance.coverDynamicThemeDesc'))),
      card(tr('Player style', '播放器样式'), null,
        row(tr('Cover layout', '封面布局'), null, segmented([[true, label('play.immersiveCover', tr('Immersive cover', '沉浸封面'))], [false, label('play.classicVinyl', tr('Classic vinyl', '经典黑胶'))]], visual.coverLayout, coverLayout => save('visual.set', {coverLayout}))),
        v('showLeftPanel', label('play.showLeftPanel', tr('Show left panel', '显示左侧面板'))),
        v('immersiveLyricColor', label('play.immersiveLyricColor', tr('Immersive lyric colour', '沉浸歌词配色'))),
        v('blurLyrics', tr('Blur inactive lyrics', '非当前歌词模糊')),
        v('autoHideBottom', tr('Auto-hide bottom bar', '自动隐藏底部栏')),
        v('pauseTransition', tr('Pause transition', '暂停过渡动画'))),
      card(tr('Lyric font', '歌词字体'), null,
        wide(tr('Font family', '字体'), tr('Comma separated, up to 8 names', '多个字体用英文逗号分隔，最多 8 个'),
          (() => { const input = h('input', {type: 'text', value: lyric.family, maxlength: 256}); return h('div', null, input, h('div', {class: 'actions'}, button(tr('Save', '保存'), e => busy(e.currentTarget, () => run(() => call('lyric.set', {family: input.value}))), 'primary'))); })()),
        wide(tr('Size', '字号'), null, slider(0.1, 2, 0.05, lyric.rate, x => x.toFixed(2) + '×', rate => save('lyric.set', {rate}))),
        row(tr('Weight', '字重'), null, select([100, 200, 300, 400, 500, 600, 700, 800, 900].map(x => [String(x), String(x)]), String(lyric.weight), x => save('lyric.set', {weight: Number(x)})))),
      card(tr('Animated background', '动态背景'), null,
        row(tr('Profile', '配置'), null, select([['auto', tr('Auto', '自动')], ['performance', tr('Performance', '性能优先')], ['quality', tr('Quality', '画质优先')], ['custom', tr('Custom', '自定义')]], background.preset, preset => save('background.set', {preset}))),
        b('enabled', tr('Enable background', '启用背景')),
        b('audioResponse', tr('React to audio', '随音频律动')),
        b('staticMode', tr('Static image', '静态画面')),
        wide(tr('Render scale', '渲染比例'), null, slider(0.1, 1, 0.05, background.renderScale, x => Math.round(x * 100) + '%', renderScale => save('background.set', {renderScale}))),
        wide(tr('Flow speed', '流动速度'), null, slider(0.1, 5, 0.1, background.flowSpeed, x => x.toFixed(1) + '×', flowSpeed => save('background.set', {flowSpeed}))),
        row(tr('Frame rate', '帧率'), null, select([15, 20, 24, 30, 45, 60].map(x => [String(x), x + ' fps']), String(background.fps), x => save('background.set', {fps: Number(x)}))),
        h('div', {class: 'actions'}, button(tr('Reset to defaults', '恢复默认'), () => save('background.set', {preset: ''})))),
    ];
  }

  async function playback() {
    const [seamless, visual, general] = [await call('playback.get'), await call('visual.get'), await call('general.get')];
    const v = (key, name, desc) => row(name, desc, toggle(visual[key], on => save('visual.set', {[key]: on}), name));
    return [
      card(label('play.seamlessSwitch', tr('Seamless switching', '无缝切歌')), null,
        row(label('play.seamlessSwitch', tr('Seamless switching', '无缝切歌')), null, toggle(seamless.enabled, enabled => save('playback.set', {enabled}))),
        row(label('play.transitionMode', tr('Transition mode', '过渡方式')), null, segmented([[false, label('play.gaplessMode', tr('Gapless', '无间隙'))], [true, label('play.crossfadeMode', tr('Crossfade', '交叉淡入淡出'))]], seamless.crossfade, crossfade => save('playback.set', {crossfade}))),
        wide(tr('Transition duration', '过渡时长'), null, slider(0, 12000, 100, seamless.durationMs, x => (x / 1000).toFixed(1) + ' s', durationMs => save('playback.set', {durationMs})))),
      card(label('settings.playback.fullscreenPerformance', tr('Full-screen performance', '全屏播放-性能优化')), null,
        v('jumpLyrics', label('settings.playback.jumpLyric'), label('settings.playback.jumpLyricDesc')),
        v('background', label('settings.playback.bgAnimation'), label('settings.playback.bgAnimationDesc')),
        v('spectrum', label('settings.playback.audioVisualizer'), label('settings.playback.audioVisualizerDesc')),
        wide(label('visuals.mode', tr('Render mode', '渲染模式')), label('visuals.autoHint'), segmented([['auto', label('visuals.auto', 'Auto')], ['reduced', label('visuals.reduced', 'Reduced')], ['full', label('visuals.full', 'Full')]], visual.mode, mode => save('visual.set', {mode}))),
        row(label('settings.playback.routePreload'), label('settings.playback.routePreloadDesc'), toggle(general.routePreload, routePreload => save('general.set', {routePreload})))),
      card(label('settings.playback.playlistManagement', tr('Queue', '播放列表')), null,
        row(tr('Songs in queue', '队列歌曲'), tr('About ' + general.artistCount + ' artists', '约 ' + general.artistCount + ' 位歌手'), h('b', {text: String(general.queueCount)})),
        h('div', {class: 'actions'}, button(tr('Clear queue', '清空播放列表'), async e => {
          const node = e.currentTarget;
          if (await ask({title: tr('Clear queue', '清空播放列表'), message: tr('Clear the whole play queue?', '确定清空整个播放列表吗？'), ok: tr('Clear', '清空'), danger: true})) busy(node, () => save('queue.clear'));
        }, 'danger'))),
      await queueTransfer(),
    ];
  }


  async function queueTransfer() {
    const area = h('textarea', {placeholder: tr('Exported queue text appears here, or paste text to import', '导出的播放列表文本显示在这里,或粘贴文本导入'), spellcheck: 'false'});
    const file = h('input', {type: 'file', accept: '.txt,.mio,.json'});
    const result = h('p', {class: 'note'});
    const doImport = async text => {
      const r = await call('queue.import', {text});
      result.textContent = tr(`Imported: ${r.added} added, ${r.skipped} skipped, ${r.total} in file.`, `已导入:新增 ${r.added} 首,跳过 ${r.skipped} 首,文件共 ${r.total} 首。`);
    };
    return card(tr('Import / export queue', '播放列表导入导出'), tr('Merges into the current queue and never starts playback.', '导入会合并到当前播放列表,不会自动播放。'),
      wide(tr('Text', '文本'), null, h('div', null, area, h('div', {class: 'actions'},
        button(label('settings.playlist.exportPlaylist', tr('Export', '导出')), e => busy(e.currentTarget, async () => { try { area.value = (await call('queue.export')).text; result.textContent = ''; } catch (error) { toast(describe(error), true); } })),
        button(label('settings.playlist.importPlaylist', tr('Import', '导入')), e => busy(e.currentTarget, async () => { try { if (!area.value.trim()) throw new Error('invalid_text'); await doImport(area.value); } catch (error) { toast(describe(error), true); } }), 'primary')))),
      wide(tr('From file', '从文件导入'), tr('Reads the file in your browser and sends its text.', '在浏览器中读取文件并发送其文本。'), h('div', null, file, h('div', {class: 'actions'},
        button(tr('Import file', '导入文件'), e => busy(e.currentTarget, async () => { try { const f = file.files[0]; if (!f || f.size === 0 || f.size > 12582912) throw new Error('badFile'); await doImport(await f.text()); } catch (error) { toast(describe(error), true); } }))))),
      result);
  }

  const filters = () => [['peak', 'typePeak'], ['lowShelf', 'typeLowShelf'], ['highShelf', 'typeHighShelf'], ['lowPass', 'typeLowPass'], ['highPass', 'typeHighPass'], ['notch', 'typeNotch']]
    .map(([key, text]) => [key, label('settings.equalizer.' + text, key)]);
  async function effects() {
    const [balance, eq] = [await call('balance.get'), await call('eq.get')];
    const edit = (args) => run(async () => { await call('eq.edit', args); saved(); });
    const bandEdit = (index, band, patch) => edit(Object.assign({action: 'band', index}, band, patch));
    const presetNode = select(eq.presets.map(p => [p.id, p.localeKey ? label(p.localeKey, p.name) : p.name]), eq.currentPresetId, id => edit({action: 'preset', id}));
    const current = eq.presets.find(p => p.id === eq.currentPresetId);
    return [
      card(label('settings.audioEffect.title', tr('Audio effects', '高级音效处理')), null,
        row(tr('Channel balance', '声道平衡'), null, toggle(balance.enabled, enabled => save('balance.set', {enabled}))),
        wide(tr('Balance', '平衡'), tr('Left ↔ Right', '左 ↔ 右'),
          slider(-1, 1, 0.05, balance.balance, x => x === 0 ? tr('Center', '居中') : (x < 0 ? tr('Left ', '左 ') : tr('Right ', '右 ')) + Math.abs(Math.round(x * 100)) + '%', value => save('balance.set', {balance: value}))),
        h('div', {class: 'actions'}, button(label('settings.audioEffect.centerCalibration', tr('Center', '居中校准')), () => save('balance.set', {balance: 0})))),
      card(label('settings.equalizer.title', tr('Equalizer', '音频均衡器')), null,
        row(tr('Enable equalizer', '启用均衡器'), null, toggle(eq.enabled, enabled => edit({action: 'enabled', enabled}))),
        wide(tr('Preset', '预设'), null, presetNode),
        h('div', {class: 'actions'},
          current && !current.builtin && button(tr('Update preset', '更新预设'), () => edit({action: 'updatePreset', id: current.id})),
          current && !current.builtin && button(tr('Delete preset', '删除预设'), () => edit({action: 'deletePreset', id: current.id}), 'danger'),
          button(tr('Save as new…', '另存为…'), async () => { const name = await ask({title: tr('Save as new preset', '另存为新预设'), input: true, placeholder: tr('Preset name', '预设名称'), ok: tr('Save', '保存')}); if (name) edit({action: 'savePreset', name}); }),
          button(tr('Reset', '重置'), () => edit({action: 'reset'}))),
        wide(tr('Global gain', '总增益'), null, slider(-24, 24, 0.5, eq.globalGain, x => (x > 0 ? '+' : '') + x.toFixed(1) + ' dB', gain => edit({action: 'gain', gain})))),
      card(tr('Bands', '频段'), null,
        h('div', {class: 'bands'}, eq.bands.map((band, index) => h('div', {class: 'band'},
          h('div', {class: 'head'}, h('span', {text: Math.round(band.frequency) + ' Hz'}), toggle(band.enabled, enabled => bandEdit(index, band, {enabled}))),
          select(filters(), band.kind, kind => bandEdit(index, band, {kind})),
          slider(-24, 24, 0.5, band.gain, x => (x > 0 ? '+' : '') + x.toFixed(1) + ' dB', gain => bandEdit(index, band, {gain})),
          h('div', {class: 'grid2'},
            h('div', null, h('div', {class: 'desc', text: tr('Frequency (Hz)', '频率 (Hz)')}), (() => { const input = h('input', {type: 'number', min: 20, max: 20000, step: 1, value: Math.round(band.frequency), onchange: () => bandEdit(index, band, {frequency: Number(input.value)})}); return input; })()),
            h('div', null, h('div', {class: 'desc', text: 'Q'}), (() => { const input = h('input', {type: 'number', min: 0.1, max: 10, step: 0.1, value: band.q, onchange: () => bandEdit(index, band, {q: Number(input.value)})}); return input; })())))))),
      card(tr('Import / export', '导入 / 导出'), tr('Share equalizer settings as text.', '以文本形式分享均衡器配置。'), (() => {
        const area = h('textarea', {placeholder: '{ … }', spellcheck: 'false'});
        return h('div', null, area, h('div', {class: 'actions'},
          button(tr('Export', '导出'), async e => busy(e.currentTarget, async () => { try { area.value = (await call('eq.export')).text; } catch (error) { toast(describe(error), true); } })),
          button(tr('Import', '导入'), () => { if (area.value.trim()) edit({action: 'importConfig', text: area.value}); }, 'primary'),
          button(label('settings.equalizer.textImportPresetTitle', tr('Import text preset', '导入文本预设')), async () => {
            if (!area.value.trim()) return;
            const name = await ask({title: label('settings.equalizer.textImportPresetTitle', tr('Import text preset', '导入文本预设')), input: true, placeholder: label('settings.equalizer.textImportPresetNamePlaceholder', tr('Preset name', '预设名称')), ok: tr('Import', '导入')});
            if (name) edit({action: 'importText', name, text: area.value});
          }),
          h('a', {href: 'https://mio888888.github.io/EQVault/', target: '_blank', rel: 'noopener noreferrer', text: label('settings.equalizer.downloadPresets', 'EQVault')})));
      })()),
    ];
  }

  // ---- plugins -----------------------------------------------------------
  const openPlugins = new Set();   // expanded cards survive the redraw after every change
  const freshPlugins = new Set();  // cards that appeared since the page was last drawn
  let knownPlugins = null, installMode = 'url';

  function fieldControl(field, values) {
    const set = value => { values[field.key] = value; };
    const value = field.value === null || field.value === undefined ? '' : field.value;
    set(field.value === undefined ? null : field.value);
    if (field.kind === 'boolean' || field.kind === 'switch' || field.kind === 'checkbox') return toggle(!!field.value, on => { set(on); }, field.label);
    if (field.options && field.options.length) return select(field.options.map(o => [o.value, o.label]), String(value), x => set(x));
    if (field.kind === 'number') { const input = h('input', {type: 'number', value, oninput: () => set(input.value === '' ? null : Number(input.value))}); return input; }
    const input = h('input', {type: field.kind === 'password' ? 'password' : 'text', value, placeholder: field.placeholder || '', autocomplete: 'off', spellcheck: 'false', oninput: () => set(input.value)});
    return input;
  }

  function pluginStatus(plugin) {
    if (plugin.kind === 'playback') return plugin.active ? ['', tr('In use', '使用中')] : ['off', tr('Not in use', '未使用')];
    if (!plugin.configured) return ['warn', tr('Needs setup', '待配置')];
    return plugin.enabled ? ['', tr('Enabled', '已启用')] : ['off', tr('Disabled', '已停用')];
  }

  function pluginCard(plugin) {
    const key = plugin.kind + ':' + plugin.id;
    const expanded = openPlugins.has(key);
    const [tone, stateText] = pluginStatus(plugin);
    const subtitle = [plugin.author, plugin.sources.length ? plugin.sources.map(s => s.name).join(' · ') : ''].filter(Boolean).join(' · ');
    const playback = plugin.kind === 'playback';
    const main = playback
      ? toggle(plugin.active, on => { if (on) save('plugins.activate', {id: plugin.id}); }, tr('Use for playback', '作为播放插件'), plugin.active)
      : toggle(plugin.enabled, enabled => save('plugins.enable', {id: plugin.id, enabled}), tr('Enabled', '启用'));
    const head = h('div', {class: 'ph'},
      h('div', {class: 'avatar', 'aria-hidden': 'true', text: (plugin.name || '?').trim().slice(0, 1).toUpperCase()}),
      h('div', {class: 'info'},
        h('div', {class: 'title'}, h('span', {text: plugin.name}), h('span', {class: 'tag', text: /^v/i.test(plugin.version) ? plugin.version : 'v' + plugin.version}),
          plugin.bundled && h('span', {class: 'tag', text: tr('Built-in', '内置')}),
          h('span', {class: 'tag' + (tone ? ' ' + tone : ''), text: stateText})),
        subtitle && h('div', {class: 'sub', text: subtitle})),
      main,
      h('button', {type: 'button', class: 'more', 'aria-expanded': String(expanded), 'aria-label': tr('Details', '详情'), text: '⌄', onclick: () => {
        if (openPlugins.has(key)) openPlugins.delete(key); else openPlugins.add(key);
        freshPlugins.delete(key);
        show('plugins', true);
      }}));
    const card = h('section', {class: 'card plugin' + (freshPlugins.has(key) ? ' fresh' : '')}, head);
    if (!expanded) return card;

    const values = {};
    const body = h('div', {class: 'pb'}, plugin.description && h('p', {class: 'about', text: plugin.description}));
    if (!playback) {
      body.append(
        row(tr('Provide playback', '提供播放'), tr('Let this plugin resolve song URLs.', '允许该插件解析歌曲播放地址。'), toggle(plugin.providePlay, play => save('plugins.provide', {id: plugin.id, play, download: plugin.provideDownload}), tr('Provide playback', '提供播放'))),
        row(tr('Provide downloads', '提供下载'), tr('Let this plugin resolve download links.', '允许该插件解析下载地址。'), toggle(plugin.provideDownload, download => save('plugins.provide', {id: plugin.id, play: plugin.providePlay, download}), tr('Provide downloads', '提供下载'))));
      if (plugin.fields.length) {
        body.append(h('div', {class: 'sectiontitle', text: tr('Configuration', '配置')}),
          h('div', {class: 'fields'}, plugin.fields.map(field => h('div', null,
            h('div', {class: 'name'}, field.label, field.required && h('span', {class: 'req', text: '*'})),
            field.description && h('div', {class: 'desc', text: field.description}),
            fieldControl(field, values)))),
          h('div', {class: 'actions'}, button(tr('Save configuration', '保存配置'), e => busy(e.currentTarget, () => run(async () => {
            await call('plugins.configure', {id: plugin.id, revision: plugin.revision, values: JSON.stringify(values)});
            saved();
          })), 'primary')));
      }
    }
    const logs = h('pre', {class: 'log', hidden: true});
    body.append(h('div', {class: 'actions'},
      playback && button(tr('View logs', '查看日志'), e => busy(e.currentTarget, async () => {
        try { const r = await call('plugins.logs', {id: plugin.id}); logs.hidden = false; logs.textContent = r.lines.join('\n') || tr('No logs', '暂无日志'); } catch (error) { toast(describe(error), true); }
      })),
      !plugin.bundled && button(tr('Remove plugin', '卸载插件'), async e => {
        const node = e.currentTarget;
        if (await ask({title: tr('Remove plugin', '卸载插件'), message: tr(`Remove “${plugin.name}”? Its settings are deleted too.`, `确定卸载“${plugin.name}”吗？它的配置也会一并删除。`), ok: tr('Remove', '卸载'), danger: true})) {
          openPlugins.delete(key);
          busy(node, () => save('plugins.remove', {kind: plugin.kind, id: plugin.id}));
        }
      }, 'danger')), logs);
    card.append(body);
    return card;
  }

  // One install card with three ways in. Input stays put when an install fails so it can be fixed.
  function installCard() {
    const status = h('p', {class: 'inline-note', role: 'status'});
    const say = (text, bad) => { status.className = 'inline-note' + (bad ? ' bad' : ''); status.textContent = text || ''; };
    async function install(node, work) {
      await busy(node, async () => {
        say(tr('Installing…', '正在安装…'));
        try { await work(); say(''); toast(tr('Plugin installed', '插件已安装')); await show('plugins', true); }
        catch (error) { say(describe(error), true); toast(describe(error), true); }
      });
    }

    const url = h('input', {type: 'url', placeholder: 'https://…/plugin.js', maxlength: 4096, autocomplete: 'off', spellcheck: 'false'});
    const urlButton = button(tr('Install', '安装'), e => {
      if (!url.value.trim()) { say(describe(new Error('invalid_plugin_url')), true); url.focus(); return; }
      install(e.currentTarget, () => call('plugins.installUrl', {url: url.value.trim()}));
    }, 'primary');
    url.addEventListener('keydown', event => { if (event.key === 'Enter') urlButton.click(); });

    const file = h('input', {type: 'file', accept: '.js,.txt'});
    const dropTitle = h('b'), dropHint = h('span');
    const fileButton = button(tr('Install file', '安装文件'), e => {
      const f = file.files[0];
      if (!f || f.size === 0 || f.size > 1048576) { say(describe(new Error('badFile')), true); return; }
      install(e.currentTarget, () => upload('/plugin', f, f.name));
    }, 'primary');
    const drop = h('label', {class: 'drop'}, file, dropTitle, dropHint);
    const drawFile = () => {
      const f = file.files[0];
      dropTitle.textContent = f ? f.name : tr('Choose a plugin file', '选择插件文件');
      dropHint.textContent = f ? size(f.size) : tr('Click to browse or drop a .js file here · up to 1 MiB', '点击选择,或把 .js 文件拖到这里 · 不超过 1 MiB');
      fileButton.disabled = !f;
    };
    file.addEventListener('change', () => { say(''); drawFile(); });
    drop.addEventListener('dragover', event => { event.preventDefault(); drop.classList.add('over'); });
    drop.addEventListener('dragleave', () => drop.classList.remove('over'));
    drop.addEventListener('drop', event => {
      event.preventDefault(); drop.classList.remove('over');
      if (event.dataTransfer && event.dataTransfer.files.length) { file.files = event.dataTransfer.files; file.dispatchEvent(new Event('change')); }
    });
    drawFile();

    const code = h('textarea', {placeholder: tr('Paste the plugin source here', '在这里粘贴插件源码'), spellcheck: 'false'});
    const count = h('span');
    const drawCount = () => { count.textContent = code.value.length ? code.value.length.toLocaleString() + ' ' + tr('characters', '字符') : ''; };
    code.addEventListener('input', drawCount);
    const codeButton = button(tr('Install code', '安装代码'), e => {
      if (!code.value.trim()) { say(describe(new Error('invalid_text')), true); code.focus(); return; }
      install(e.currentTarget, () => call('plugins.installCode', {code: code.value}));
    }, 'primary');

    const modes = {
      url: [tr('Link', '链接'), () => h('div', {class: 'inline'}, url, urlButton)],
      file: [tr('File', '文件'), () => [drop, h('div', {class: 'actions'}, fileButton)]],
      code: [tr('Code', '代码'), () => [code, h('div', {class: 'counter'}, count), h('div', {class: 'actions'}, codeButton)]],
    };
    const panel = h('div', {class: 'panel'});
    const modeBar = h('div', {class: 'seg', role: 'group', 'aria-label': tr('Install method', '安装方式')});
    const draw = () => {
      modeBar.replaceChildren(...Object.entries(modes).map(([key, [name]]) => h('button', {type: 'button', 'aria-pressed': String(key === installMode), text: name, onclick: () => { installMode = key; say(''); draw(); }})));
      panel.replaceChildren(...[].concat(modes[installMode][1]()));
    };
    draw();
    return h('section', {class: 'card install'},
      h('h2', {text: tr('Install a plugin', '安装插件')}),
      h('div', {class: 'callout'}, h('span', {'aria-hidden': 'true', text: '⚠'}), h('span', {text: tr('Plugins run code on your TV. Only install ones you trust.', '插件会在电视上运行代码，请只安装你信任的插件。')})),
      modeBar, panel, status);
  }

  async function plugins() {
    const {plugins: rows} = await call('plugins.list');
    const ids = new Set(rows.map(p => p.kind + ':' + p.id));
    if (knownPlugins) for (const key of ids) if (!knownPlugins.has(key)) { freshPlugins.add(key); openPlugins.add(key); }
    for (const key of [...openPlugins]) if (!ids.has(key)) { openPlugins.delete(key); freshPlugins.delete(key); }
    knownPlugins = ids;
    const playback = rows.filter(p => p.kind === 'playback'), sources = rows.filter(p => p.kind !== 'playback');
    const group = (title, list, empty) => [
      h('div', {class: 'groupname'}, h('span', {text: title}), h('span', {text: String(list.length)})),
      list.length ? list.map(pluginCard) : h('div', {class: 'card empty', text: empty}),
    ];
    return [
      installCard(),
      group(tr('Playback plugins', '播放插件'), playback, tr('No playback plugin installed yet.', '还没有安装播放插件。')),
      group(tr('Source plugins', '音源插件'), sources, tr('No source plugin installed yet.', '还没有安装音源插件。')),
    ];
  }

  async function sources() {
    const data = await call('sources.get');
    const selected = data.sources.find(s => s.id === data.selected) || data.sources[0];
    if (!selected) return card(label('settings.musicSource.noPluginTitle', tr('No music source', '暂无可用音乐源')), label('settings.musicSource.noPluginDesc', tr('Configure a list plugin first.', '请先在插件管理中配置音源插件。')));
    const order = ['128k', '320k', 'flac', 'flac24bit', 'hires', 'atmos', 'master'];
    const rank = q => order.includes(q) ? order.indexOf(q) : order.length;
    const sort = list => [...list].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
    let common = sort(data.sources[0].qualities);
    for (const source of data.sources) common = common.filter(q => source.qualities.includes(q));
    return [
      card(label('settings.musicSource.musicSourceSelect', tr('Music source', '音乐源选择')), null,
        data.sources.map(source => h('button', {type: 'button', class: 'choice', 'aria-pressed': String(source.id === data.selected), onclick: () => save('sources.select', {source: source.id})},
          h('b', {text: source.name}), source.id === data.selected && h('span', {class: 'tag', text: tr('Selected', '当前')}), source.preferredQuality && h('span', {class: 'tag', text: qualityName(source.preferredQuality)})))),
      card(label('settings.musicSource.qualitySelect', tr('Quality', '音质选择')) + ' — ' + selected.name, null,
        h('div', {class: 'chips'}, sort(selected.qualities).map(q => h('button', {type: 'button', class: 'chip', 'aria-pressed': String(q === selected.preferredQuality), text: qualityName(q), onclick: () => save('sources.quality', {quality: q, all: false})})))),
      card(label('settings.musicSource.globalQuality', tr('Global quality', '全局音质')), common.length ? null : tr('No quality is supported by every source.', '各音源暂无共同支持的音质。'),
        common.length && h('div', {class: 'chips'}, common.map(q => h('button', {type: 'button', class: 'chip', 'aria-pressed': String(data.sources.every(s => s.preferredQuality === q)), text: qualityName(q), onclick: () => save('sources.quality', {quality: q, all: true})})))),
    ];
  }

  async function storage() {
    const [s, general] = [await call('storage.get'), await call('general.get')];
    const t = key => label('settings.storage.' + key);
    const toggleRow = (key, name, desc) => row(name, desc, toggle(s[key], on => save('storage.set', {[key]: on}), name));
    const template = h('input', {type: 'text', value: s.filenameTemplate, maxlength: 256, autocomplete: 'off', spellcheck: 'false'});
    const preview = h('div', {class: 'desc mono'});
    const refreshPreview = async () => { try { preview.textContent = t('preview') + ' ' + (await call('filename.preview', {template: template.value, english: tr(true, false)})).name; } catch (_) { preview.textContent = ''; } };
    template.addEventListener('input', refreshPreview);
    refreshPreview();
    const limits = [128, 256, 500, 1024, 2048, 5120];
    const mib = Math.round(s.cacheLimit / 1048576);
    const options = limits.includes(mib) ? limits : [...limits, mib].sort((a, b) => a - b);
    return [
      card(t('cacheStrategy'), null,
        toggleRow('autoCache', t('autoCacheMusic'), t('autoCacheMusicDesc')),
        row(t('cacheSizeLimit'), t('cacheSizeLimitDesc'), select(options.map(x => [String(x), size(x * 1048576)]), String(mib), x => save('storage.set', {cacheLimit: Number(x) * 1048576})))),
      card(t('filenameFormat'), t('filenameFormatDesc'),
        wide(t('filenamePlaceholder'), null, h('div', null, template, preview, h('div', {class: 'actions'}, button(tr('Save', '保存'), e => busy(e.currentTarget, () => run(() => call('storage.set', {filenameTemplate: template.value}))), 'primary'))))),
      card(t('tagWriteSettings'), t('tagWriteDesc'),
        toggleRow('basicInfo', t('tagBasicInfo'), t('tagBasicInfoDesc')),
        toggleRow('cover', t('tagCover'), t('tagCoverDesc')),
        toggleRow('lyrics', t('tagLyricsInfo'), t('tagLyricsInfoDesc')),
        toggleRow('downloadLyrics', t('tagDownloadLyricsFile'), t('tagDownloadLyricsFileDesc'))),
      card(t('localCacheConfig'), null,
        row(t('existingCacheSize'), general.cacheFiles ? t('fileCount').replace('{count}', general.cacheFiles) : t('noCache'), h('b', {text: size(general.cacheBytes)})),
        h('div', {class: 'actions'}, button(t('clearLocalCache'), async e => {
          const node = e.currentTarget;
          if (await ask({title: t('clearLocalCache'), message: t('clearCacheBody'), ok: t('clearLocalCache'), danger: true})) busy(node, () => run(async () => { await call('cache.clear'); toast(t('clearSuccess')); }));
        }, 'danger'))),
      card(t('directoryConfig'), tr('The TV keeps music and cache in its app directory, managed by the system.', '电视的歌曲和缓存都放在应用目录,由系统管理。'),
        row(t('cacheDirectory'), general.paths.cache, null), row(t('downloadDirectory'), general.paths.downloads, null)),
    ];
  }

  async function about() {
    const info = await call('about.get');
    const t = key => label('settings.about.' + key);
    const out = h('div');
    const link = (text, href) => h('a', {href, target: '_blank', rel: 'noopener noreferrer', text});
    const intro = (() => {
      // "{0}" and "{2}" are the project links, "{1}" the original author.
      const parts = t('aboutIntro').split(/(\{\d\})/);
      return h('p', {class: 'desc'}, parts.map(part => part === '{0}' ? link('CeruMusic', 'https://github.com/timeshiftsauce/CeruMusic') : part === '{1}' ? '时迁酱' : part === '{2}' ? link('CeruMusic', 'https://ceru.docs.shiqianjiang.cn/') : part));
    })();
    return [
      card(t('versionInfo'), t('description'),
        row(tr('Version', '版本'), null, h('b', {text: 'v' + info.version})),
        row(t('checkUpdateOnStart'), null, toggle(info.autoUpdate, enabled => save('about.autoUpdate', {enabled})))),
      card(t('checkUpdate'), null, h('div', {class: 'actions'}, button(t('checkUpdate'), e => busy(e.currentTarget, async () => {
        out.textContent = t('checking');
        try {
          const release = await call('about.check');
          out.replaceChildren(release ? h('div', null, h('b', {text: (release.newer ? t('newVersionFound') + ' ' : t('upToDate') + ' ') + release.version}), h('p', {class: 'note', text: t('mobileUpdateHint')}), release.newer && h('pre', {class: 'log', text: release.notes})) : document.createTextNode(t('upToDate')));
        } catch (error) { out.textContent = ''; toast(describe(error), true); }
      }), 'primary')), out),
      card(t('legalNotice'), null,
        row(t('dataResponsibility'), t('dataResponsibilityDesc')), row(t('copyrightCompliance'), t('copyrightComplianceDesc')), row(t('usageRestriction'), t('usageRestrictionDesc'))),
      card(t('aboutUs'), null, intro,
        row(t('supportCurrentAuthorTitle'), t('supportCurrentAuthorText'), link('GitHub', 'https://github.com/Mio888888/Mio-Music')),
        row(t('originalAuthorTitle'), t('originalAuthorPoint') + ' · ' + t('sponsorHint'))),
    ];
  }

  async function transfer() {
    const audio = h('input', {type: 'file', accept: '.mp3,.flac,.wav,.ogg,.m4a,.aac,.wma,.opus,.ape,.alac'});
    return card(tr('Send music to the TV', '发送音乐到电视'), tr('Imported files appear in Local music. Up to 256 MiB per file.', '导入后出现在“本地音乐”。单个文件不超过 256 MiB。'),
      wide(tr('Audio file', '音频文件'), null, h('div', null, audio, h('div', {class: 'actions'}, button(tr('Import', '导入'), e => busy(e.currentTarget, async () => {
        const f = audio.files[0];
        if (!f || f.size === 0 || f.size > 268435456) { toast(tr('Choose a file under 256 MiB', '请选择不超过 256 MiB 的文件'), true); return; }
        toast(tr('Sending…', '正在发送…'));
        try { await upload('/audio', f, f.name); toast(tr('Imported', '已导入')); } catch (error) { toast(describe(error), true); }
      }), 'primary')))));
  }


  async function backup() {
    const saved = await call('backup.get');
    const field = (value, type, placeholder) => h('input', {type: type || 'text', value: value === undefined || value === null ? '' : value, placeholder: placeholder || '', autocomplete: 'off', spellcheck: 'false'});
    const endpoint = field(saved.endpoint, 'url', 'https://s3.example.com'), region = field(saved.region), keyId = field(saved.accessKeyId), bucket = field(saved.bucket);
    const secret = field('', 'password'), password = field('', 'password');
    const keep = field(saved.maxBackups, 'number');
    let pathStyle = saved.pathStyle;
    const config = () => ({endpoint: endpoint.value.trim(), region: region.value.trim(), accessKeyId: keyId.value.trim(), secretAccessKey: secret.value, bucket: bucket.value.trim(), pathStyle});
    const result = h('div');
    const counts = r => tr(`${r.playlists} playlists · ${r.songs} songs · ${r.plugins} plugins · ${r.localReferences} local references`, `${r.playlists} 个歌单 · ${r.songs} 首歌曲 · ${r.plugins} 个插件 · ${r.localReferences} 个本地引用`);
    const show = (...nodes) => result.replaceChildren(...nodes.flat(Infinity).filter(Boolean));
    const guard = (e, work) => busy(e.currentTarget, async () => { try { await work(); } catch (error) { show(); toast(describe(error), true); } });
    return [
      card(tr('S3 connection', 'S3 连接'), tr('The secret key and password are used once and never stored on the TV. They cross your local network unencrypted, so use a trusted network.', '密钥和密码仅本次使用,不会保存在电视上。它们会在局域网内以明文传输,请在可信网络中操作。'),
        wide('Endpoint', null, endpoint), wide(tr('Region', '区域'), null, region), wide('Access key ID', null, keyId),
        wide(tr('Secret access key', 'Secret access key'), tr('Required for every backup or restore', '每次备份或恢复都需要填写'), secret),
        wide(tr('Bucket', '存储桶'), null, bucket),
        row(tr('Path-style addressing', '路径风格寻址'), null, toggle(pathStyle, on => { pathStyle = on; }, 'path style')),
        wide(tr('Backups to keep', '保留备份数'), null, keep),
        h('div', {class: 'actions'}, button(tr('Save connection', '保存连接'), e => guard(e, async () => { await call('backup.save', Object.assign({maxBackups: Number(keep.value)}, {endpoint: endpoint.value.trim(), region: region.value.trim(), accessKeyId: keyId.value.trim(), bucket: bucket.value.trim(), pathStyle})); toast(tr('Saved', '已保存')); }), 'primary')),
        saved.lastBackupTime && h('p', {class: 'note', text: tr('Last backup: ', '上次备份:') + saved.lastBackupTime})),
      card(tr('Encrypted backup', '加密备份'), tr('Backs up playlists, plugins and settings to your bucket.', '把歌单、插件和设置备份到你的存储桶。'),
        wide(tr('Encryption password', '加密密码'), tr('Needed again to restore', '恢复时需要再次输入'), password),
        h('div', {class: 'actions'}, button(tr('Back up now', '立即备份'), e => guard(e, async () => {
          if (!password.value) { toast(tr('Enter an encryption password', '请输入加密密码'), true); return; }
          const r = await call('backup.uploadPrepare', Object.assign({maxBackups: Number(keep.value)}, {config: config()}));
          show(h('p', {text: tr('Ready to back up: ', '即将备份:') + counts(r)}), h('div', {class: 'actions'},
            button(tr('Confirm backup', '确认备份'), ev => guard(ev, async () => { const o = await call('backup.uploadRun', {password: password.value}); show(h('p', {text: tr('Backup complete: ', '备份完成:') + o.timestamp}), o.warnings.map(w => h('p', {class: 'note', text: w}))); }), 'primary'),
            button(tr('Cancel', '取消'), () => { call('backup.cancel').catch(() => {}); show(); })));
        }), 'primary')), result),
      (() => {
        const output = h('div');
        const outShow = (...nodes) => output.replaceChildren(...nodes.flat(Infinity).filter(Boolean));
        let overwrite = false;
        return card(tr('Restore', '恢复'), tr('Adds your backed-up playlists, plugins and settings to this TV.', '把备份的歌单、插件和设置恢复到这台电视。'),
          h('div', {class: 'actions'}, button(tr('Load latest backup', '读取最新备份'), e => busy(e.currentTarget, async () => {
            try {
              if (!password.value) { toast(tr('Enter the encryption password above', '请先在上方输入加密密码'), true); return; }
              const r = await call('backup.restorePrepare', {config: config(), password: password.value});
              outShow(h('p', {text: tr('Backup from ', '备份时间:') + r.timestamp + ' — ' + counts(r)}),
                row(tr('Overwrite existing items', '覆盖已有项目'), null, toggle(false, on => { overwrite = on; }, 'overwrite')),
                h('div', {class: 'actions'},
                  button(tr('Restore', '恢复'), ev => busy(ev.currentTarget, async () => {
                    try {
                      const o = await call('backup.restoreRun', {overwrite});
                      outShow(h('p', {text: tr(`Restored: ${o.playlistsAdded} playlists, ${o.songsAdded} songs added (${o.songsSkipped} skipped), ${o.pluginsAdded} plugins added.`, `已恢复:新增 ${o.playlistsAdded} 个歌单、${o.songsAdded} 首歌曲(跳过 ${o.songsSkipped} 首)、${o.pluginsAdded} 个插件。`)}), o.warnings.map(w => h('p', {class: 'note', text: w})));
                    } catch (error) { toast(describe(error), true); }
                  }), 'danger'),
                  button(tr('Cancel', '取消'), () => { call('backup.cancel').catch(() => {}); outShow(); })));
            } catch (error) { outShow(); toast(describe(error), true); }
          }), 'primary')), output);
      })(),
    ];
  }

  // [key, name, page, group, description]; the group only shows in the wide sidebar.
  const PAGES = [
    ['appearance', () => label('settings.musicSource.navAppearance', tr('Appearance', '外观设置')), appearance, () => tr('Player', '播放'), () => tr('Theme, language, player style and lyric font.', '主题、语言、播放器样式和歌词字体。')],
    ['playback', () => label('settings.musicSource.navPlaylist', tr('Playback', '播放设置')), playback, () => tr('Player', '播放'), () => tr('Transitions, full-screen performance and the play queue.', '切歌过渡、全屏播放性能和播放列表。')],
    ['effects', () => label('settings.musicSource.navEffects', tr('Audio effects', '音效设置')), effects, () => tr('Player', '播放'), () => tr('Channel balance and the equalizer.', '声道平衡和均衡器。')],
    ['plugins', () => label('settings.musicSource.navPlugins', tr('Plugins', '插件管理')), plugins, () => tr('Library', '音乐库'), () => tr('Install, configure and remove playback and source plugins.', '安装、配置和卸载播放插件与音源插件。')],
    ['sources', () => label('settings.musicSource.navMusic', tr('Music source', '音乐源')), sources, () => tr('Library', '音乐库'), () => tr('Choose the source and the sound quality to request.', '选择音乐源和请求的音质。')],
    ['storage', () => label('settings.musicSource.navStorage', tr('Storage', '存储管理')), storage, () => tr('Library', '音乐库'), () => tr('Cache, download file names and tags.', '缓存、下载文件名和标签。')],
    ['backup', () => tr('Backup & restore', '备份与恢复'), backup, () => tr('Device', '设备'), () => tr('Encrypted backups to your own S3 bucket.', '把加密备份存到你自己的 S3 存储桶。')],
    ['transfer', () => tr('Send files', '传输文件'), transfer, () => tr('Device', '设备'), () => tr('Send audio files from this device to the TV.', '把这台设备上的音频文件发送到电视。')],
    ['about', () => label('settings.musicSource.navAbout', tr('About', '关于')), about, () => tr('Device', '设备'), () => '']
  ];

  function drawTabs() {
    const nodes = [];
    let group = null;
    for (const [key, name, , groupName] of PAGES) {
      if (groupName() !== group) { group = groupName(); nodes.push(h('div', {class: 'group', 'aria-hidden': 'true', text: group})); }
      nodes.push(h('button', {type: 'button', 'aria-current': key === current ? 'page' : null, text: name(), onclick: () => show(key)}));
    }
    tabs.replaceChildren(...nodes);
    // Keep the active chip in view on phones.
    const active = tabs.querySelector('[aria-current=page]');
    if (active && tabs.scrollWidth > tabs.clientWidth && tabs.scrollHeight <= tabs.clientHeight + 1) tabs.scrollLeft = active.offsetLeft - (tabs.clientWidth - active.offsetWidth) / 2;
  }
  async function show(key, keepScroll) {
    const mine = ++token, scroll = window.scrollY;
    current = key;
    drawTabs();
    const entry = PAGES.find(p => p[0] === key);
    if (!keepScroll) page.replaceChildren(h('p', {class: 'muted', text: tr('Loading…', '加载中…')}));
    try {
      const nodes = await entry[2]();
      if (mine !== token) return;
      page.replaceChildren(pageHead(entry[1](), entry[4]()), ...[].concat(nodes).flat(Infinity).filter(Boolean));
      window.scrollTo(0, keepScroll ? scroll : 0);
    } catch (error) {
      if (mine !== token) return;
      page.replaceChildren(pageHead(entry[1](), entry[4]()), card(null, null, h('p', {text: describe(error)}), button(tr('Retry', '重试'), () => show(key))));
    }
  }
  window.MioSettings = {start: () => show('appearance')};
  // The invitation in the address can pair before this script has run.
  if (window.MioWeb.paired) window.MioSettings.start();
  window.MioWeb.onLanguage(() => { if (!document.getElementById('app').hidden) show(current, true); });
})();

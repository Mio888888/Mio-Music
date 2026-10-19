import 'dart:async';
import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import '../core_controller.dart';
import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'device.dart';
import 'resources.dart';
import 'settings_layout.dart';
import 'workspace_background.dart';

class PluginsView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool embedded;
  final bool compact;
  final bool active;
  final bool ready;
  final int sourceRevision;
  const PluginsView({
    super.key,
    required this.core,
    required this.palette,
    this.embedded = false,
    this.compact = false,
    required this.active,
    required this.ready,
    required this.sourceRevision,
  });
  @override
  State<PluginsView> createState() => _PluginsViewState();
}

class _PluginsViewState extends State<PluginsView> with WidgetsBindingObserver {
  final _url = TextEditingController();
  final _urlFocus = FocusNode();
  final _scroll = ScrollController();
  List<api.PluginInfo> _items = [];
  api.PluginInfo? _editing;
  RawDialogRoute<_PluginInstallRequest>? _installRoute;
  String? _confirmRemove;
  String? _error;
  bool _busy = false;
  bool _loading = false;
  bool _foreground = true;
  int _epoch = 0;
  int _selectionEpoch = 0;
  PluginFileSelection? _selection;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
    if (_live) unawaited(_reload());
  }

  @override
  void didUpdateWidget(PluginsView old) {
    super.didUpdateWidget(old);
    if (old.compact && !widget.compact) _closeInstallDialog();
    if (!widget.active || !widget.ready) {
      _closeInstallDialog();
      _selectionEpoch++;
      unawaited(_selection?.cancel());
      _epoch++;
      _loading = false;
    }
    if (_live &&
        (!_busy &&
            (!old.active ||
                !old.ready ||
                old.sourceRevision != widget.sourceRevision))) {
      unawaited(_reload());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _closeInstallDialog();
      _epoch++;
      _loading = false;
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live && !_busy) unawaited(_reload());
    }
  }

  @override
  void dispose() {
    _closeInstallDialog();
    _selectionEpoch++;
    unawaited(_selection?.cancel());
    _epoch++;
    _url.dispose();
    _urlFocus.dispose();
    _scroll.dispose();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _reload() async {
    if (!_live || _busy || _installRoute != null) return;
    final epoch = ++_epoch;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final items = await api.installedPlugins();
      if (_live && epoch == _epoch) setState(() => _items = items);
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(() => _error = p.text('settings.plugin.loadError'));
      }
    } finally {
      if (mounted && epoch == _epoch) setState(() => _loading = false);
    }
  }

  Future<void> _change(
    Future<void> Function() action, {
    bool closeForm = false,
  }) async {
    if (!_live || _busy || _loading) return;
    _epoch++;
    setState(() {
      _busy = true;
      _error = null;
      _confirmRemove = null;
    });
    try {
      await action();
      if (mounted && closeForm) setState(() => _editing = null);
    } catch (error) {
      if (mounted) {
        setState(
          () => _error = error is PluginFileException
              ? _fileError(error.code)
              : p.english
              ? 'Change failed. Check the plugin or configuration, refresh and retry.'
              : '变更失败，请检查插件或配置，刷新后重试。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        if (_live) {
          final error = _error;
          await _reload();
          if (mounted && error != null) setState(() => _error = error);
        }
      }
    }
  }

  String _fileError(String code) => switch (code) {
    'picker_unavailable' =>
      p.english
          ? 'File picker unavailable. Install using a URL instead.'
          : '文件选择器不可用，请使用 URL 安装。',
    'file_too_large' =>
      p.english
          ? 'Choose a non-empty plugin file up to 1 MiB.'
          : '请选择不超过 1 MiB 的非空插件文件。',
    'file_invalid_utf8' =>
      p.english
          ? 'The plugin file must use UTF-8 encoding.'
          : '插件文件需要使用 UTF-8 编码。',
    _ =>
      p.english
          ? 'Unable to read the selected file. Choose it again.'
          : '无法读取所选文件，请重新选择。',
  };

  Future<void> _installFile() => _change(() async {
    final epoch = ++_selectionEpoch;
    final revision = widget.sourceRevision;
    final selection = PluginFileSelection();
    _selection = selection;
    try {
      final code = await selection.readCode();
      if (code == null ||
          !mounted ||
          !widget.active ||
          !widget.ready ||
          epoch != _selectionEpoch) {
        return;
      }
      await api.installPluginCode(code: code, expectedRevision: revision);
    } finally {
      if (identical(_selection, selection)) _selection = null;
    }
  });

  Widget _button(
    String label,
    VoidCallback? action, {
    bool primary = false,
    bool danger = false,
  }) => DecoratedBox(
    decoration: BoxDecoration(
      borderRadius: BorderRadius.circular(widget.compact ? 6 : 4),
      border: widget.compact && danger
          ? Border.all(color: p.color('--td-error-color').withValues(alpha: .5))
          : null,
    ),
    child: MioButton(
      palette: p,
      label: label,
      onPressed: !_busy && !_loading && (!widget.compact || _live)
          ? action
          : null,
      radius: widget.compact ? 6 : 4,
      insetFocusBorder: !widget.compact,
      padding: widget.compact
          ? const EdgeInsets.symmetric(horizontal: 10, vertical: 6)
          : const EdgeInsets.all(8),
      background: widget.compact
          ? (primary
                ? p.brand
                : danger
                ? const Color(0x00000000)
                : p.color('--td-bg-color-specialcomponent'))
          : null,
      child: Text(
        label,
        style: widget.compact
            ? TextStyle(
                color: danger
                    ? p.color('--td-error-color')
                    : primary
                    ? p.color('--td-text-color-anti')
                    : p.color('--td-text-color-primary'),
              )
            : null,
      ),
    ),
  );
  Widget _configuration(api.PluginInfo item) => _PluginForm(
    key: ValueKey('${item.id}:${item.revision}'),
    item: item,
    palette: p,
    busy: _busy || _loading,
    onCancel: () => setState(() => _editing = null),
    onSave: (values) => unawaited(
      _change(
        () => api.configureListPlugin(
          id: item.id,
          valuesJson: jsonEncode(values),
          expectedRevision: item.revision,
        ),
        closeForm: true,
      ),
    ),
  );

  /// A list plugin without its required fields (a server address) offers no sources yet.
  String _listState(api.PluginInfo item) => !item.enabled
      ? (p.english ? 'Disabled' : '已停用')
      : !item.configured
      ? (p.english ? 'Needs setup' : '待配置')
      : (p.english ? 'Enabled' : '已启用');

  Widget _pluginCard(api.PluginInfo item) => Container(
    margin: const EdgeInsets.symmetric(vertical: 8),
    padding: const EdgeInsets.all(16),
    decoration: BoxDecoration(
      color: widget.compact && item.active
          ? Color.lerp(p.surface, p.color('--td-text-color-primary'), .18)
          : p.surface,
      borderRadius: BorderRadius.circular(widget.compact ? 8 : 12),
      border: Border.all(
        color: widget.compact && item.active ? p.brand : p.border,
      ),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (widget.compact) ...[
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(
                item.name,
                style: const TextStyle(
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                ),
              ),
              Text(
                item.version.startsWith('v')
                    ? item.version
                    : 'v${item.version}',
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
              if (item.bundled) _tag(p.english ? 'Built-in' : '内置'),
              if (item.active || item.kind == api.PluginKind.list)
                _tag(
                  item.kind == api.PluginKind.playback
                      ? p.text('settings.plugin.currentUsing')
                      : _listState(item),
                  highlighted:
                      item.active || (item.enabled && item.configured),
                ),
            ],
          ),
          if (item.author.isNotEmpty &&
              item.kind == api.PluginKind.playback) ...[
            const SizedBox(height: 8),
            Text(
              p
                  .text('settings.plugin.author')
                  .replaceAll('{name}', item.author),
              style: TextStyle(color: p.secondary, fontSize: 13),
            ),
          ],
          if (item.description.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              item.description,
              style: TextStyle(color: p.secondary, fontSize: 13, height: 1.5),
            ),
          ],
          if (item.sources.isNotEmpty) ...[
            const SizedBox(height: 8),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                if (item.kind == api.PluginKind.playback)
                  Text(
                    p.text('settings.plugin.supportedSources'),
                    style: TextStyle(color: p.secondary, fontSize: 13),
                  ),
                for (final source in item.sources) _tag(source.name),
              ],
            ),
          ],
        ] else ...[
          Text(
            [
              '${item.name} ${item.version}',
              if (item.bundled) p.english ? '· Built-in' : '· 内置',
              if (item.kind == api.PluginKind.list) '· ${_listState(item)}',
            ].join(' '),
            style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 17),
          ),
          const SizedBox(height: 6),
          Text(item.description, style: TextStyle(color: p.secondary)),
          Text(
            item.sources.map((s) => s.name).join(' · '),
            style: TextStyle(color: p.secondary),
          ),
        ],
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            if (item.kind == api.PluginKind.playback &&
                (!widget.compact || !item.active))
              _button(
                p.text(
                  item.active
                      ? 'settings.plugin.currentUsing'
                      : 'settings.plugin.use',
                ),
                item.active
                    ? null
                    : () => unawaited(
                        _change(() => api.activatePlaybackPlugin(id: item.id)),
                      ),
              ),
            if (widget.compact && item.kind == api.PluginKind.playback)
              _button(
                p.text('settings.plugin.logs'),
                () => unawaited(_showLogs(item)),
              ),
            if (item.kind == api.PluginKind.list) ...[
              _button(
                p.english
                    ? (item.enabled ? 'Disable' : 'Enable')
                    : (item.enabled ? (widget.compact ? '停用' : '禁用') : '启用'),
                () => unawaited(
                  _change(
                    () => api.setListPluginEnabled(
                      id: item.id,
                      enabled: !item.enabled,
                    ),
                  ),
                ),
              ),
              if (item.fields.isNotEmpty)
                _button(
                  p.text('settings.plugin.config'),
                  () => setState(() => _editing = item),
                ),
              if (item.canPlay)
                _button(
                  p.english
                      ? 'Playback: ${item.providePlay ? "on" : "off"}'
                      : '提供播放：${item.providePlay ? "开" : "关"}',
                  () => unawaited(
                    _change(
                      () => api.setListPluginProvide(
                        id: item.id,
                        play: !item.providePlay,
                        download: item.provideDownload,
                      ),
                    ),
                  ),
                ),
              if (item.canDownload)
                _button(
                  p.english
                      ? 'Download: ${item.provideDownload ? "on" : "off"}'
                      : '提供下载：${item.provideDownload ? "开" : "关"}',
                  () => unawaited(
                    _change(
                      () => api.setListPluginProvide(
                        id: item.id,
                        play: item.providePlay,
                        download: !item.provideDownload,
                      ),
                    ),
                  ),
                ),
            ],
            // Bundled plugins are always installed; they are switched off instead.
            if (!item.bundled)
              _button(
                p.text('settings.plugin.uninstall'),
                () => setState(() => _confirmRemove = item.id),
                danger: true,
              ),
            if (_confirmRemove == item.id && !item.bundled) ...[
              _button(
                p.english ? 'Confirm removal' : '确认卸载',
                () => unawaited(
                  _change(() => api.removePlugin(kind: item.kind, id: item.id)),
                ),
              ),
              _button(
                p.text('common.cancel'),
                () => setState(() => _confirmRemove = null),
              ),
            ],
          ],
        ),
      ],
    ),
  );

  Future<void> _showLogs(api.PluginInfo item) async {
    if (!_live) return;
    await showGeneralDialog<void>(
      context: context,
      barrierDismissible: true,
      barrierLabel: p.text('common.close'),
      barrierColor: const Color(0x88000000),
      pageBuilder: (dialogContext, _, _) =>
          _PluginLogsDialog(palette: p, item: item),
    );
  }

  Widget _tag(String label, {bool highlighted = false}) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
    decoration: BoxDecoration(
      color: highlighted
          ? p.brand
          : p.color('--td-text-color-primary').withValues(alpha: .08),
      borderRadius: BorderRadius.circular(999),
    ),
    child: Text(
      label,
      style: TextStyle(
        fontSize: 12,
        height: 1.2,
        color: highlighted ? p.color('--td-text-color-anti') : p.secondary,
      ),
    ),
  );

  void _closeInstallDialog() {
    final route = _installRoute;
    _installRoute = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
      WidgetsBinding.instance.ensureVisualUpdate();
    }
  }

  Future<void> _openInstallDialog(api.PluginKind kind) async {
    if (!_live || _busy || _loading || _installRoute != null) return;
    final route = RawDialogRoute<_PluginInstallRequest>(
      barrierDismissible: true,
      barrierLabel: p.text('common.cancel'),
      barrierColor: p.color('--td-mask-active'),
      transitionDuration: Duration.zero,
      pageBuilder: (_, _, _) => _PluginInstallDialog(
        palette: p,
        kind: kind,
        statusBackground: mioCompactSettingsHeaderBackground(
          p,
          songTheme: widget.core.player?.coverTheme != null,
        ),
      ),
    );
    _installRoute = route;
    final request = await Navigator.of(context).push(route);
    await route.completed;
    if (!identical(_installRoute, route)) return;
    _installRoute = null;
    if (!_live) return;
    if (request == null) {
      await _reload();
      return;
    }
    if (request.url == null) {
      await _installFile();
    } else {
      await _change(() => api.installPluginUrl(url: request.url!));
    }
  }

  Widget _compactSection(api.PluginKind kind) {
    final playback = kind == api.PluginKind.playback;
    final items = _items.where((item) => item.kind == kind).toList();
    return MioSettingsSection(
      palette: p,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          LayoutBuilder(
            builder: (context, box) {
              final title = Text(
                playback
                    ? p.text('settings.plugin.title')
                    : p.english
                    ? 'Music list plugins'
                    : '音乐列表插件',
                style: TextStyle(
                  fontSize:
                      MioDeviceScope.wide(
                        context,
                        MediaQuery.sizeOf(context).width,
                      )
                      ? 18
                      : 16,
                  fontWeight: FontWeight.w600,
                ),
              );
              final actions = Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _button(
                    playback
                        ? p.text('settings.plugin.addPlugin')
                        : p.english
                        ? 'Install list plugin'
                        : '安装列表插件',
                    () => unawaited(_openInstallDialog(kind)),
                    primary: true,
                  ),
                  if (playback)
                    _button(
                      p.english ? 'Refresh' : '刷新',
                      () => unawaited(_reload()),
                    ),
                ],
              );
              if (box.maxWidth < 285 ||
                  MediaQuery.textScalerOf(context).scale(16) > 20) {
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [title, const SizedBox(height: 10), actions],
                );
              }
              return Row(
                children: [
                  Expanded(child: title),
                  const SizedBox(width: 8),
                  actions,
                ],
              );
            },
          ),
          const SizedBox(height: 8),
          if (items.isEmpty && !_loading)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 16),
              child: Text(
                p.text('settings.plugin.noPlugins'),
                style: TextStyle(color: p.secondary),
              ),
            ),
          for (final item in items) ...[
            _pluginCard(item),
            if (_editing?.id == item.id) _configuration(item),
          ],
        ],
      ),
    );
  }

  Widget _compactBuild() => MioSettingsBody(
    embedded: widget.embedded,
    padding: const EdgeInsets.all(16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (_busy || _loading) Text(p.text('common.loading')),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Text(
              _error!,
              style: TextStyle(color: p.color('--td-error-color')),
            ),
          ),
        _compactSection(api.PluginKind.playback),
        _compactSection(api.PluginKind.list),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) => widget.compact
      ? _compactBuild()
      : ListView(
          controller: widget.embedded ? null : _scroll,
          shrinkWrap: widget.embedded,
          physics: widget.embedded
              ? const NeverScrollableScrollPhysics()
              : null,
          padding: widget.embedded ? EdgeInsets.zero : const EdgeInsets.all(24),
          children: [
            if (!widget.embedded) ...[
              Text(
                p.text('settings.plugin.title'),
                style: const TextStyle(
                  fontSize: 24,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 18),
            ],
            MioInput(
              palette: p,
              controller: _url,
              focusNode: _urlFocus,
              placeholder: p.english
                  ? 'Plugin URL (HTTPS / HTTP)'
                  : '插件 URL（HTTPS / HTTP）',
              keyboardType: TextInputType.url,
              readOnly: _busy || _loading,
              onSubmitted: (_) {
                if (!_busy && !_loading) {
                  unawaited(
                    _change(() => api.installPluginUrl(url: _url.text.trim())),
                  );
                }
              },
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                _button(
                  p.text('settings.plugin.addPlugin'),
                  () => unawaited(
                    _change(() => api.installPluginUrl(url: _url.text.trim())),
                  ),
                ),
                _button(
                  p.english ? 'Refresh' : '刷新',
                  () => unawaited(_reload()),
                ),
                _button(
                  p.english ? 'Install from file' : '从文件安装',
                  () => unawaited(_installFile()),
                ),
              ],
            ),
            if (_busy || _loading)
              Padding(
                padding: const EdgeInsets.all(16),
                child: Text(p.text('common.loading')),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 12),
                child: Text(
                  _error!,
                  style: TextStyle(color: p.color('--td-error-color')),
                ),
              ),
            if (!_busy && !_loading && _items.isEmpty)
              Padding(
                padding: const EdgeInsets.all(16),
                child: Text(p.text('settings.plugin.noPlugins')),
              ),
            for (final item in _items) _pluginCard(item),
            if (_editing case final item?) _configuration(item),
          ],
        );
}

class _PluginForm extends StatefulWidget {
  final api.PluginInfo item;
  final MioPalette palette;
  final bool busy;
  final VoidCallback onCancel;
  final ValueChanged<Map<String, Object?>> onSave;
  const _PluginForm({
    super.key,
    required this.item,
    required this.palette,
    required this.busy,
    required this.onCancel,
    required this.onSave,
  });
  @override
  State<_PluginForm> createState() => _PluginFormState();
}

class _PluginFormState extends State<_PluginForm> {
  final _texts = <String, TextEditingController>{};
  final _focus = <String, FocusNode>{};
  final _values = <String, Object?>{};
  String? _error;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    for (final field in widget.item.fields) {
      final value = jsonDecode(field.valueJson);
      _values[field.key] = field.kind == 'select'
          ? (value is String || value is num ? value.toString() : '')
          : value;
      if (field.kind != 'switch' && field.kind != 'select') {
        _texts[field.key] = TextEditingController(
          text: value?.toString() ?? '',
        );
        _focus[field.key] = FocusNode();
      }
    }
  }

  @override
  void dispose() {
    for (final controller in _texts.values) {
      controller.dispose();
    }
    for (final focus in _focus.values) {
      focus.dispose();
    }
    super.dispose();
  }

  void _save() {
    final values = <String, Object?>{};
    for (final field in widget.item.fields) {
      final text = _texts[field.key]?.text ?? '';
      if (field.kind == 'switch') {
        values[field.key] = _values[field.key] == true;
      } else if (field.kind == 'select') {
        values[field.key] = _values[field.key] is String
            ? _values[field.key]
            : '';
      } else if (field.kind == 'number') {
        if (text.isEmpty && !field.required_) continue;
        final number = num.tryParse(text);
        if (number == null || !number.isFinite) {
          setState(() => _error = p.english ? 'Invalid number' : '数字格式无效');
          return;
        }
        values[field.key] = number;
      } else {
        values[field.key] = text;
      }
      if (field.required_ &&
          (values[field.key] == null ||
              values[field.key] is String &&
                  (values[field.key] as String).trim().isEmpty)) {
        setState(
          () => _error = p.english ? 'Required field is empty' : '必填项未填写',
        );
        return;
      }
    }
    setState(() => _error = null);
    widget.onSave(values);
  }

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(16),
    decoration: BoxDecoration(
      color: p.surface,
      border: Border.all(color: p.border),
      borderRadius: BorderRadius.circular(12),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '${widget.item.name} · ${p.text('settings.plugin.config')}',
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
        ),
        for (final field in widget.item.fields)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('${field.label}${field.required_ ? " *" : ""}'),
                const SizedBox(height: 6),
                if (field.kind == 'switch')
                  MioButton(
                    palette: p,
                    label: field.label,
                    onPressed: widget.busy
                        ? null
                        : () => setState(
                            () =>
                                _values[field.key] = _values[field.key] != true,
                          ),
                    child: Text(
                      _values[field.key] == true
                          ? (p.english ? 'On' : '开')
                          : (p.english ? 'Off' : '关'),
                    ),
                  )
                else if (field.kind == 'select')
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      for (final option in field.options)
                        MioButton(
                          palette: p,
                          label: option.label,
                          onPressed: widget.busy
                              ? null
                              : () => setState(
                                  () => _values[field.key] = option.value,
                                ),
                          child: Text(
                            option.label,
                            style: TextStyle(
                              color: _values[field.key] == option.value
                                  ? p.brand
                                  : p.secondary,
                            ),
                          ),
                        ),
                    ],
                  )
                else
                  MioInput(
                    palette: p,
                    controller: _texts[field.key]!,
                    focusNode: _focus[field.key]!,
                    placeholder: field.placeholder,
                    obscureText: field.kind == 'password',
                    readOnly: widget.busy,
                    maxLines: field.kind == 'textarea' ? 3 : 1,
                    height: field.kind == 'textarea' ? 80 : 38,
                    keyboardType: field.kind == 'number'
                        ? const TextInputType.numberWithOptions(
                            decimal: true,
                            signed: true,
                          )
                        : TextInputType.text,
                  ),
                if (field.description.isNotEmpty)
                  Text(
                    field.description,
                    style: TextStyle(color: p.secondary, fontSize: 12),
                  ),
              ],
            ),
          ),
        if (_error != null)
          Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
        Wrap(
          spacing: 8,
          children: [
            MioButton(
              palette: p,
              label: p.text('common.save'),
              onPressed: widget.busy ? null : _save,
              child: Text(p.text('common.save')),
            ),
            MioButton(
              palette: p,
              label: p.text('common.cancel'),
              onPressed: widget.busy ? null : widget.onCancel,
              child: Text(p.text('common.cancel')),
            ),
          ],
        ),
      ],
    ),
  );
}

class _PluginInstallRequest {
  final String? url;
  const _PluginInstallRequest(this.url);
}

class _PluginInstallDialog extends StatefulWidget {
  final MioPalette palette;
  final api.PluginKind kind;
  final Color statusBackground;
  const _PluginInstallDialog({
    required this.palette,
    required this.kind,
    required this.statusBackground,
  });
  @override
  State<_PluginInstallDialog> createState() => _PluginInstallDialogState();
}

class _PluginInstallDialogState extends State<_PluginInstallDialog> {
  final _url = TextEditingController();
  final _urlFocus = FocusNode();
  bool _online = false, _lx = false;
  String? _error;
  MioPalette get p => widget.palette;
  String t(String key) => p.text('settings.plugin.$key');
  @override
  void dispose() {
    _url.dispose();
    _urlFocus.dispose();
    super.dispose();
  }

  void _confirm() {
    final url = _url.text.trim();
    if (_online) {
      final parsed = Uri.tryParse(url);
      if (url.isEmpty ||
          url.length > 4096 ||
          parsed == null ||
          !['https', 'http'].contains(parsed.scheme) ||
          parsed.host.isEmpty) {
        setState(() => _error = t(url.isEmpty ? 'enterUrl' : 'invalidUrl'));
        return;
      }
    }
    // As on desktop, format selection is a hint; the installer detects the
    // actual plugin format from its contents before registration.
    Navigator.of(context).pop(_PluginInstallRequest(_online ? url : null));
  }

  Widget _choice(String label, bool selected, VoidCallback action) => Semantics(
    selected: selected,
    child: MioButton(
      palette: p,
      label: label,
      radius: 6,
      background: selected
          ? p.brand
          : p.color('--td-bg-color-specialcomponent'),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      onPressed: action,
      child: Text(
        label,
        style: TextStyle(
          fontSize: 14,
          color: p.color(
            selected ? '--td-text-color-anti' : '--td-text-color-primary',
          ),
        ),
      ),
    ),
  );
  @override
  Widget build(BuildContext context) {
    final playback = widget.kind == api.PluginKind.playback;
    final title = playback
        ? t('addPlugin')
        : p.english
        ? 'Install music list plugin'
        : '安装音乐列表插件';
    final masked = Color.alphaBlend(
      p.color('--td-mask-active'),
      widget.statusBackground,
    );
    final light = masked.computeLuminance() > .179;
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: SystemUiOverlayStyle(
        statusBarColor: const Color(0x00000000),
        statusBarIconBrightness: light ? Brightness.dark : Brightness.light,
        statusBarBrightness: light ? Brightness.light : Brightness.dark,
        systemStatusBarContrastEnforced: false,
      ),
      child: SizedBox.expand(
        child: Semantics(
          scopesRoute: true,
          explicitChildNodes: true,
          namesRoute: true,
          label: title,
          child: Padding(
            padding: EdgeInsets.only(
              bottom: MediaQuery.viewInsetsOf(context).bottom,
            ),
            child: SafeArea(
              minimum: const EdgeInsets.all(16),
              child: Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 480),
                  child: SingleChildScrollView(
                    child: Container(
                      width: double.infinity,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 28,
                        vertical: 32,
                      ),
                      decoration: BoxDecoration(
                        color: p.color('--td-bg-color-container'),
                        border: Border.all(color: p.border),
                        borderRadius: BorderRadius.circular(22),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            title,
                            style: const TextStyle(
                              fontSize: 17,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 24),
                          if (playback) ...[
                            Text(
                              t('pluginTypeTip'),
                              style: TextStyle(
                                fontSize: 13,
                                height: 1.6,
                                color: p.secondary,
                              ),
                            ),
                            const SizedBox(height: 12),
                            Wrap(
                              spacing: 8,
                              runSpacing: 8,
                              children: [
                                _choice(
                                  t('cranPlugin'),
                                  !_lx,
                                  () => setState(() => _lx = false),
                                ),
                                _choice(
                                  t('lxPlugin'),
                                  _lx,
                                  () => setState(() => _lx = true),
                                ),
                              ],
                            ),
                            const SizedBox(height: 24),
                          ],
                          Text(
                            t('selectImportMethod'),
                            style: TextStyle(fontSize: 13, color: p.secondary),
                          ),
                          const SizedBox(height: 12),
                          Wrap(
                            spacing: 8,
                            runSpacing: 8,
                            children: [
                              _choice(
                                t('localImport'),
                                !_online,
                                () => setState(() {
                                  _online = false;
                                  _error = null;
                                }),
                              ),
                              _choice(
                                t('onlineImport'),
                                _online,
                                () => setState(() {
                                  _online = true;
                                  _error = null;
                                }),
                              ),
                            ],
                          ),
                          const SizedBox(height: 14),
                          if (_online) ...[
                            MioInput(
                              palette: p,
                              controller: _url,
                              focusNode: _urlFocus,
                              placeholder: t('onlineUrlPlaceholder'),
                              keyboardType: TextInputType.url,
                              maxLength: 4096,
                              onSubmitted: (_) => _confirm(),
                            ),
                            const SizedBox(height: 10),
                          ],
                          Text(
                            t(_online ? 'onlineUrlTip' : 'localImportTip'),
                            style: TextStyle(
                              fontSize: 13,
                              height: 1.6,
                              color: p.secondary,
                            ),
                          ),
                          if (_error != null)
                            Padding(
                              padding: const EdgeInsets.only(top: 10),
                              child: Semantics(
                                liveRegion: true,
                                child: Text(
                                  _error!,
                                  style: TextStyle(
                                    color: p.color('--td-error-color'),
                                  ),
                                ),
                              ),
                            ),
                          const SizedBox(height: 24),
                          Row(
                            mainAxisAlignment: MainAxisAlignment.end,
                            children: [
                              _choice(
                                p.text('common.cancel'),
                                false,
                                () => Navigator.of(context).pop(),
                              ),
                              const SizedBox(width: 10),
                              _choice(p.text('common.confirm'), true, _confirm),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PluginLogsDialog extends StatefulWidget {
  final MioPalette palette;
  final api.PluginInfo item;
  const _PluginLogsDialog({required this.palette, required this.item});
  @override
  State<_PluginLogsDialog> createState() => _PluginLogsDialogState();
}

class _PluginLogsDialogState extends State<_PluginLogsDialog> {
  late Future<List<String>> _lines;
  @override
  void initState() {
    super.initState();
    _lines = api.playbackPluginLogs(id: widget.item.id);
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final size = MediaQuery.sizeOf(context);
    return Padding(
      padding: EdgeInsets.fromLTRB(16, size.height * .15, 16, 16),
      child: Align(
        alignment: Alignment.topCenter,
        child: SingleChildScrollView(
          child: Container(
            width: size.width - 32,
            padding: const EdgeInsets.all(24),
            decoration: BoxDecoration(
              color: p.surface.withValues(alpha: 1),
              borderRadius: BorderRadius.circular(22),
              border: Border.all(color: p.border),
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  '${p.text('settings.plugin.pluginLog')} · ${widget.item.name}',
                  style: const TextStyle(
                    fontSize: 16,
                    height: 1.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    MioButton(
                      palette: p,
                      label: p.text('common.refresh'),
                      onPressed: () => setState(
                        () =>
                            _lines = api.playbackPluginLogs(id: widget.item.id),
                      ),
                      child: Text(p.text('common.refresh')),
                    ),
                    const SizedBox(width: 8),
                    MioButton(
                      palette: p,
                      label: p.text('common.close'),
                      onPressed: () => Navigator.of(context).pop(),
                      child: Text(p.text('common.close')),
                    ),
                  ],
                ),
                const SizedBox(height: 16),
                Container(
                  height: size.height * .5,
                  padding: const EdgeInsets.all(14),
                  decoration: BoxDecoration(
                    color: const Color(0xff1e1e1e),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: SingleChildScrollView(
                    child: FutureBuilder<List<String>>(
                      future: _lines,
                      builder: (_, snapshot) => Text(
                        snapshot.hasError
                            ? (p.english ? 'Unable to read logs.' : '无法读取日志。')
                            : !snapshot.hasData
                            ? (p.english ? 'Loading…' : '正在加载…')
                            : snapshot.data!.isEmpty
                            ? p.text('settings.plugin.noLogs')
                            : snapshot.data!.join('\n'),
                        style: const TextStyle(
                          fontSize: 12,
                          height: 1.5,
                          fontFamily: 'monospace',
                          color: Color(0xffd4d4d4),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

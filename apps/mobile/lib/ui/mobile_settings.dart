import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'audio_effects.dart';
import 'controls.dart';
import 'device.dart';
import 'equalizer.dart';
import 'lyric_settings.dart';
import 'playback_settings.dart';
import 'plugins.dart';
import 'releases.dart';
import 'resources.dart';
import 'source_icon.dart';
import 'settings_layout.dart';
import 'native_lyric_settings.dart';
import 'storage_settings.dart';
import 'visual_settings.dart';

enum CompactSettingsCategory {
  appearance,
  playback,
  effects,
  plugins,
  music,
  storage,
  about,
  lyrics,
  equalizer,
  visual,
}

class MobileSettingsPage extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active;
  final double viewportWidth;
  final CompactSettingsCategory category;
  final VoidCallback onPlugins, onBackup;
  final VoidCallback? onLocal;
  const MobileSettingsPage({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.viewportWidth,
    required this.category,
    required this.onPlugins,
    required this.onBackup,
    this.onLocal,
  });
  @override
  State<MobileSettingsPage> createState() => _MobileSettingsPageState();
}

class _MobileSettingsPageState extends State<MobileSettingsPage>
    with WidgetsBindingObserver {
  api.GeneralSettingsSession? _session;
  api.SourceSelectionSession? _sources;
  api.SourceSelectionSnapshot? _source;
  String? _activePlaybackName;
  api.SettingsOverview? _overview;
  api.GeneralSettings? _general;
  api.QueueSession? _queue;
  RawDialogRoute<bool>? _confirmation;
  bool _foreground = true, _busy = false;
  int _epoch = 0;
  bool _attempted = false;
  int? _queueRevision, _sourceRevision, _selectionRevision;
  String? _error;
  MioPalette get p => widget.palette;
  bool get ready => widget.core.player != null;
  bool get live => mounted && widget.active && ready && _foreground;
  String tr(String en, String zh) => p.english ? en : zh;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    if (live) unawaited(_load());
  }

  void _close() {
    ++_epoch;
    _session?.close();
    _session?.dispose();
    _session = null;
    _sources?.close();
    _sources?.dispose();
    _sources = null;
    _queue?.close();
    _queue?.dispose();
    _queue = null;
    final route = _confirmation;
    _confirmation = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
      WidgetsBinding.instance.ensureVisualUpdate();
    }
    _busy = false;
  }

  @override
  void dispose() {
    _close();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didUpdateWidget(MobileSettingsPage old) {
    super.didUpdateWidget(old);
    if (!live) {
      _close();
      return;
    }
    if (!_busy &&
        (!old.active ||
            !_attempted ||
            old.category != widget.category ||
            (widget.category == CompactSettingsCategory.playback &&
                _queueRevision != widget.core.player?.queueRevision) ||
            (widget.category == CompactSettingsCategory.music &&
                (_sourceRevision != widget.core.player?.sourceRevision ||
                    _selectionRevision !=
                        widget.core.player?.sourceSelectionRevision)))) {
      unawaited(_load());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (live) {
      unawaited(_load());
    } else {
      _close();
    }
    if (mounted) setState(() {});
  }

  Future<void> _load() async {
    _close();
    if (!live) return;
    _attempted = true;
    final epoch = _epoch;
    bool current() => live && epoch == _epoch;
    api.GeneralSettingsSession? general;
    api.SourceSelectionSession? source;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      general = await api.beginGeneralSettings();
      if (!current()) return;
      final player = await api.playerSnapshot();
      if (!current()) return;
      final overview = await general.overview();
      if (!current()) return;
      api.SourceSelectionSnapshot? sources;
      String? activePlaybackName;
      if (widget.category == CompactSettingsCategory.music) {
        source = await api.beginSourceSelection(
          sourceRevision: player.sourceRevision,
          selectionRevision: player.sourceSelectionRevision,
        );
        if (!current()) return;
        sources = await source.snapshot();
        if (!mounted || !current()) return;
        if (!MioDeviceScope.wide(context, widget.viewportWidth)) {
          final plugins = await api.installedPlugins();
          if (!current()) return;
          activePlaybackName = plugins
              .where(
                (item) => item.kind == api.PluginKind.playback && item.active,
              )
              .firstOrNull
              ?.name;
        }
      }
      setState(() {
        _session = general;
        general = null;
        _sources = source;
        source = null;
        _queueRevision = player.queueRevision;
        _sourceRevision = player.sourceRevision;
        _selectionRevision = player.sourceSelectionRevision;
        _overview = overview;
        _general = overview.settings;
        _source = sources;
        _activePlaybackName = activePlaybackName;
      });
    } catch (e) {
      if (current()) {
        setState(() {
          _error = '$e';
          _source = null;
          _queueRevision = widget.core.player?.queueRevision;
          _sourceRevision = widget.core.player?.sourceRevision;
          _selectionRevision = widget.core.player?.sourceSelectionRevision;
        });
      }
    } finally {
      general?.close();
      general?.dispose();
      source?.close();
      source?.dispose();
      if (current()) setState(() => _busy = false);
    }
  }

  Future<void> _run(Future<void> Function() action) async {
    if (!live || _busy) return;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (e) {
      if (live && epoch == _epoch) setState(() => _error = '$e');
    } finally {
      if (live && epoch == _epoch) setState(() => _busy = false);
    }
  }

  Future<void> _save({String? language, bool? dark, bool? preload}) async {
    final session = _session, previous = _general;
    if (session == null || previous == null) return;
    await _run(() async {
      final saved = await session.save(
        expected: previous,
        next: api.GeneralSettings(
          language: language ?? previous.language,
          dark: dark ?? previous.dark,
          routePreload: preload ?? previous.routePreload,
        ),
      );
      if (live && identical(_session, session)) {
        setState(() => _general = saved);
      }
    });
  }

  Future<bool> _confirm(String title, String message) async {
    if (!live) return false;
    final route = RawDialogRoute<bool>(
      barrierDismissible: false,
      barrierColor: const Color(0x66000000),
      pageBuilder: (context, _, _) => Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: MioFocusPanel(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: MioSettingsSection(
                palette: p,
                title: title,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(message),
                    const SizedBox(height: 20),
                    Wrap(
                      spacing: 12,
                      children: [
                        MioButton(
                          palette: p,
                          label: p.text('common.cancel'),
                          onPressed: () => Navigator.pop(context, false),
                          child: Text(p.text('common.cancel')),
                        ),
                        MioButton(
                          palette: p,
                          label: p.text('common.confirm'),
                          onPressed: () => Navigator.pop(context, true),
                          child: Text(p.text('common.confirm')),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
    _confirmation = route;
    final answer = await Navigator.of(context).push(route);
    final current = identical(_confirmation, route) && live;
    if (identical(_confirmation, route)) _confirmation = null;
    return current && answer == true;
  }

  Future<void> _clearQueue() => _run(() async {
    final epoch = _epoch;
    final snapshot = await api.playerSnapshot();
    if (!live || epoch != _epoch) return;
    final session = await api.beginQueue(
      revision: snapshot.queueRevision,
      sourceRevision: snapshot.sourceRevision,
    );
    if (!live || epoch != _epoch) {
      session.close();
      session.dispose();
      return;
    }
    _queue = session;
    try {
      if (await _confirm(
        tr('Clear playlist', '清空播放列表'),
        tr('Remove all tracks from the current queue?', '确定移除当前播放列表中的全部歌曲？'),
      )) {
        if (live && epoch == _epoch) await session.clear();
        if (live && epoch == _epoch) await _load();
      }
    } finally {
      if (identical(_queue, session)) {
        _queue = null;
        session.close();
        session.dispose();
      }
    }
  });
  Future<void> _clearCache() => _run(() async {
    final session = _session;
    if (session == null) return;
    if (!await _confirm(
      tr('Clear audio cache', '清理音频缓存'),
      tr(
        'Completed audio cache files will be removed. Downloads and library files are kept.',
        '将清理已完成的音频缓存，下载和曲库文件会保留。',
      ),
    )) {
      return;
    }
    if (!live || !identical(_session, session)) return;
    await session.clearAudioCache();
    if (live && identical(_session, session)) await _load();
  });
  Widget section(String title, List<Widget> children) => MioSettingsSection(
    palette: p,
    title: title,
    appearance: widget.category == CompactSettingsCategory.appearance,
    compactAppearance:
        _narrow && widget.category == CompactSettingsCategory.appearance,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: children,
    ),
  );
  Widget field(String title, Widget child, {String? description}) =>
      MioSettingsField(
        palette: p,
        title: title,
        description: description,
        child: child,
      );
  Widget button(String label, VoidCallback action) => _narrow
      ? MioButton(
          palette: p,
          label: label,
          onPressed: live && !_busy ? action : null,
          child: Text(label),
        )
      : Align(
          alignment: Alignment.centerLeft,
          widthFactor: 1,
          child: MioSettingsButton(
            palette: p,
            label: label,
            onPressed: live && !_busy ? action : null,
          ),
        );
  Widget visual(VisualSettingsSection type) => VisualSettingsPage(
    key: ValueKey(type),
    palette: p,
    ready: ready,
    active: live,
    embedded: true,
    settingsSection: type,
  );
  Widget lyrics() => LyricSettingsView(
    core: widget.core,
    palette: p,
    viewportWidth: widget.viewportWidth,
    active: live,
    ready: ready,
    embedded: true,
  );
  Widget equalizer({String? title}) => EqualizerView(
    core: widget.core,
    palette: p,
    viewportWidth: widget.viewportWidth,
    active: live,
    ready: ready,
    embedded: true,
    sectionTitle: title,
  );

  /// Radio-style source choice: the whole card selects, the ring on the right
  /// shows the current one.
  Widget _sourceCard(api.SourceOption source, bool on, VoidCallback onTap) {
    final anti = p.color('--td-text-color-anti');
    final quality = source.preferredQuality;
    return Container(
      decoration: BoxDecoration(
        color: on ? p.brand.withValues(alpha: .1) : null,
        border: Border.all(
          color: on ? p.brand : p.color('--td-component-border'),
          width: on ? 1.5 : 1,
        ),
        borderRadius: BorderRadius.circular(14),
      ),
      child: Semantics(
        inMutuallyExclusiveGroup: true,
        checked: on,
        child: MioButton(
          palette: p,
          label: source.name,
          onPressed: live && !_busy ? (on ? () {} : onTap) : null,
          radius: 14,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
          background: const Color(0x00000000),
          child: Row(
            children: [
              Container(
                width: 44,
                height: 44,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: p.color('--td-bg-color-specialcomponent'),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(8),
                  child: SourceIcon(
                    value: source.icon,
                    color: p.secondary,
                    size: 30,
                  ),
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      source.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 15,
                        height: 1.3,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      source.pluginName,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                    if (quality != null) ...[
                      const SizedBox(height: 6),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 1,
                        ),
                        decoration: BoxDecoration(
                          color: p.brand.withValues(alpha: .14),
                          border: Border.all(
                            color: p.brand.withValues(alpha: .6),
                          ),
                          borderRadius: BorderRadius.circular(10),
                        ),
                        child: Text(
                          _qualityName(quality),
                          style: TextStyle(fontSize: 12, color: p.brand),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: 8),
              Container(
                width: 24,
                height: 24,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: on ? p.brand : null,
                  shape: BoxShape.circle,
                  border: on
                      ? null
                      : Border.all(
                          color: p.color('--td-component-border'),
                          width: 1.5,
                        ),
                ),
                child: on ? MioIcon('TD-Check', color: anti, size: 14) : null,
              ),
            ],
          ),
        ),
      ),
    );
  }

  List<Widget> _music() {
    final sources = _source?.sources ?? <api.SourceOption>[];
    if (sources.isEmpty) {
      return [
        section(p.text('settings.musicSource.noPluginTitle'), [
          Text(p.text('settings.musicSource.noPluginDesc')),
          button(tr('Manage plugins', '前往插件管理'), widget.onPlugins),
        ]),
      ];
    }
    final selected =
        sources.where((s) => s.id == _source!.selected).firstOrNull ??
        sources.first;
    final common = sources
        .skip(1)
        .fold(
          sources.first.qualities.toSet(),
          (set, s) => set.intersection(s.qualities.toSet()),
        )
        .toList();
    const order = [
      '128k',
      '320k',
      'flac',
      'flac24bit',
      'hires',
      'atmos',
      'master',
    ];
    int rank(String s) => order.contains(s) ? order.indexOf(s) : order.length;
    common.sort(
      (a, b) =>
          rank(a) == rank(b) ? a.compareTo(b) : rank(a).compareTo(rank(b)),
    );
    Future<void> change(
      Future<void> Function(api.SourceSelectionSession) action,
    ) => _run(() async {
      final session = _sources;
      if (session == null) return;
      await action(session);
      if (live && identical(_sources, session)) await _load();
    });
    if (!MioDeviceScope.wide(context, widget.viewportWidth)) {
      return _compactMusic(
        sources,
        selected,
        common,
        onSource: (id) => unawaited(change((s) => s.select(source: id))),
        onQuality: (q) => unawaited(
          change((s) => s.selectQuality(quality: q, allSources: false)),
        ),
        onGlobal: (q) => unawaited(
          change((s) => s.selectQuality(quality: q, allSources: true)),
        ),
      );
    }
    return [
      section(tr('Music sources', '音乐源'), [
        LayoutBuilder(
          builder: (context, box) {
            const gap = 12.0;
            final width = (box.maxWidth - gap) / 2;
            return Wrap(
              spacing: gap,
              runSpacing: gap,
              children: [
                for (final source in sources)
                  SizedBox(
                    width: width,
                    child: _sourceCard(
                      source,
                      source.id == selected.id,
                      () =>
                          unawaited(change((s) => s.select(source: source.id))),
                    ),
                  ),
              ],
            );
          },
        ),
      ]),
      section(tr('Quality settings', '音质设置'), [
        field(
          tr('Current source quality', '当前音源音质'),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final quality in selected.qualities)
                button(
                  '${_qualityName(quality)}${quality == selected.preferredQuality ? ' ✓' : ''}',
                  () => unawaited(
                    change(
                      (s) =>
                          s.selectQuality(quality: quality, allSources: false),
                    ),
                  ),
                ),
            ],
          ),
        ),
        field(
          tr('Global quality', '全局音质'),
          MioSettingsSelect(
            palette: p,
            label: tr('Apply to all sources', '应用到全部音源'),
            value:
                sources.every(
                  (s) => s.preferredQuality == selected.preferredQuality,
                )
                ? selected.preferredQuality
                : null,
            options: [
              for (final quality in common)
                (value: quality, label: _qualityName(quality)),
            ],
            onChanged: live && !_busy
                ? (q) => unawaited(
                    change(
                      (s) => s.selectQuality(quality: q, allSources: true),
                    ),
                  )
                : null,
          ),
          description: common.isEmpty
              ? tr('No quality is supported by every source.', '各音源暂无共同支持的音质。')
              : null,
        ),
      ]),
      section(tr('Configuration status', '配置状态'), [
        Row(
          children: [
            Expanded(child: _statusTile(tr('Source:', '音乐源:'), selected.name)),
            const SizedBox(width: 12),
            Expanded(
              child: _statusTile(
                tr('Quality:', '音质:'),
                selected.preferredQuality == null
                    ? tr('Default quality', '默认音质')
                    : _qualityName(selected.preferredQuality!),
              ),
            ),
          ],
        ),
      ]),
    ];
  }

  Widget _statusTile(String label, String value) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
    decoration: BoxDecoration(
      color: p.color('--td-bg-color-page'),
      border: Border.all(color: p.color('--td-component-border')),
      borderRadius: BorderRadius.circular(12),
    ),
    child: Row(
      children: [
        Text(label, style: TextStyle(fontSize: 14, color: p.secondary)),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.end,
            style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w600),
          ),
        ),
      ],
    ),
  );

  String _qualityKey(String quality) => switch (quality) {
    '128k' => 'quality128k',
    '320k' => 'quality320k',
    'flac' => 'qualityFlac',
    'flac24bit' => 'qualityFlac24bit',
    'hires' => 'qualityHires',
    'atmos' => 'qualityAtmos',
    'master' => 'qualityMaster',
    _ => '',
  };
  String _qualityName(String quality) {
    final key = _qualityKey(quality);
    return key.isEmpty ? quality : p.text('settings.musicSource.$key');
  }

  Widget _sourceBadge(String text, Color color) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
    decoration: BoxDecoration(
      color: color.withValues(alpha: .12),
      border: Border.all(color: color.withValues(alpha: .35)),
      borderRadius: BorderRadius.circular(999),
    ),
    child: Text(
      text,
      style: TextStyle(fontSize: 11.5, color: color, height: 1.2),
    ),
  );

  /// The icon the list plugin gives its source, as on desktop.
  Widget _sourceIcon(api.SourceOption source) => ClipOval(
    child: Container(
      width: 36,
      height: 36,
      color: p.brand.withValues(alpha: .12),
      alignment: Alignment.center,
      child: SourceIcon(value: source.icon, color: p.brand, size: 36),
    ),
  );

  List<Widget> _compactMusic(
    List<api.SourceOption> sources,
    api.SourceOption selected,
    List<String> common, {
    required ValueChanged<String> onSource,
    required ValueChanged<String> onQuality,
    required ValueChanged<String> onGlobal,
  }) {
    String t(String key) => p.text('settings.musicSource.$key');
    final success = p.color('--td-success-color');
    final quality = selected.preferredQuality;
    final qualityKey = quality == null ? '' : _qualityKey(quality);
    final qualities = [...selected.qualities];
    const order = [
      '128k',
      '320k',
      'flac',
      'flac24bit',
      'hires',
      'atmos',
      'master',
    ];
    int rank(String q) => order.contains(q) ? order.indexOf(q) : order.length;
    qualities.sort(
      (a, b) =>
          rank(a) == rank(b) ? a.compareTo(b) : rank(a).compareTo(rank(b)),
    );
    return [
      MioSettingsSection(
        palette: p,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          decoration: BoxDecoration(
            border: Border.all(color: p.border),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Row(
            children: [
              Container(
                width: 8,
                height: 8,
                decoration: BoxDecoration(
                  color: success,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  t('currentConfig').replaceAll(
                    '{name}',
                    _activePlaybackName ?? selected.pluginName,
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 14.4,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              _sourceBadge(t('enabled'), success),
            ],
          ),
        ),
      ),
      section(t('musicSourceSelect'), [
        for (final source in sources)
          Padding(
            padding: const EdgeInsets.only(top: 4, bottom: 6),
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Container(
                  decoration: BoxDecoration(
                    color: source.id == selected.id
                        ? p.brand.withValues(alpha: .09)
                        : p.surface,
                    border: Border.all(
                      color: source.id == selected.id ? p.brand : p.border,
                    ),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Semantics(
                    selected: source.id == selected.id,
                    child: MioButton(
                      palette: p,
                      label: source.name,
                      radius: 12,
                      insetFocusBorder: false,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 14,
                        vertical: 11,
                      ),
                      onPressed: live && !_busy
                          ? () => onSource(source.id)
                          : null,
                      child: Row(
                        children: [
                          _sourceIcon(source),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  source.name,
                                  style: const TextStyle(
                                    fontSize: 14,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                const SizedBox(height: 4),
                                Wrap(
                                  spacing: 6,
                                  runSpacing: 4,
                                  crossAxisAlignment: WrapCrossAlignment.center,
                                  children: [
                                    Text(
                                      t('sourceType'),
                                      style: TextStyle(
                                        fontSize: 12,
                                        color: p.secondary,
                                      ),
                                    ),
                                    if (source.preferredQuality != null)
                                      _sourceBadge(
                                        _qualityName(source.preferredQuality!),
                                        [
                                              'master',
                                              'hires',
                                              'flac24bit',
                                            ].contains(source.preferredQuality)
                                            ? p.color('--td-warning-color')
                                            : p.brand,
                                      ),
                                  ],
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                if (source.id == selected.id)
                  Positioned(
                    top: -5,
                    right: -5,
                    child: IgnorePointer(
                      child: Container(
                        width: 18,
                        height: 18,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          color: p.brand,
                          shape: BoxShape.circle,
                        ),
                        child: Text(
                          '✓',
                          style: TextStyle(
                            fontSize: 12,
                            color: p.color('--td-text-color-anti'),
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
      ]),
      section('${t('qualitySelect')} — ${selected.name}', [
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            for (final q in qualities)
              Container(
                decoration: BoxDecoration(
                  color: q == quality
                      ? p.brand.withValues(alpha: .12)
                      : const Color(0x00000000),
                  border: Border.all(color: q == quality ? p.brand : p.border),
                  borderRadius: BorderRadius.circular(999),
                ),
                child: Semantics(
                  selected: q == quality,
                  child: MioButton(
                    palette: p,
                    label: _qualityName(q),
                    radius: 999,
                    insetFocusBorder: false,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 8,
                    ),
                    onPressed: live && !_busy ? () => onQuality(q) : null,
                    child: Text(
                      '${q == quality ? '✓  ' : ''}${_qualityName(q)}',
                      style: TextStyle(
                        fontSize: 13,
                        color: q == quality ? p.brand : p.secondary,
                        fontWeight: q == quality
                            ? FontWeight.w600
                            : FontWeight.normal,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
        if (quality != null) ...[
          const SizedBox(height: 12),
          Text(
            t(qualityKey.isEmpty ? 'qualityCustomDesc' : '${qualityKey}Desc'),
            style: TextStyle(fontSize: 13, color: p.secondary),
          ),
        ],
      ]),
      section(t('globalQuality'), [
        MioSettingsSelect(
          palette: p,
          label: t('globalQuality'),
          compact: true,
          value: sources.every((s) => s.preferredQuality == quality)
              ? quality
              : null,
          options: [for (final q in common) (value: q, label: _qualityName(q))],
          onChanged: live && !_busy && common.isNotEmpty ? onGlobal : null,
        ),
        if (common.isEmpty)
          Text(tr('No quality is supported by every source.', '各音源暂无共同支持的音质。')),
      ]),
      section(t('configStatus'), [
        _statusRow(t('musicSourceLabel'), selected.name),
        const SizedBox(height: 10),
        _statusRow(
          t('qualityLabel'),
          quality == null ? t('noSelection') : _qualityName(quality),
        ),
      ]),
    ];
  }

  Widget _statusRow(String label, String value) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
    decoration: BoxDecoration(
      color: p.color('--td-bg-color-specialcomponent'),
      borderRadius: BorderRadius.circular(6),
    ),
    child: Row(
      children: [
        Expanded(
          child: Text(
            '$label:',
            style: TextStyle(fontSize: 13, color: p.secondary),
          ),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            value,
            textAlign: TextAlign.right,
            style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
          ),
        ),
      ],
    ),
  );

  Widget _compactAction(
    String label,
    VoidCallback? action, {
    bool primary = false,
  }) => MioButton(
    palette: p,
    label: label,
    radius: 6,
    background: primary ? p.brand : p.surface,
    onPressed: live && !_busy ? action : null,
    child: Text(
      label,
      style: TextStyle(
        fontSize: 13,
        color: primary ? p.color('--td-text-color-anti') : null,
      ),
    ),
  );

  Widget _supportCard({required bool original}) {
    String t(String key) => p.text('settings.about.$key');
    final image = original
        ? Image.network(
            'https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png',
            width: 132,
            height: 132,
            errorBuilder: (_, _, _) =>
                Text(tr('QR image unavailable', '二维码暂时无法加载')),
          )
        : Image.asset(
            'assets/images/current-author-sponsor-qr.jpg',
            width: 132,
            height: 132,
          );
    return Container(
      margin: const EdgeInsets.only(top: 18),
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: p.color('--td-bg-color-specialcomponent'),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            t(original ? 'originalAuthorTitle' : 'supportCurrentAuthorTitle'),
            style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 10),
          Text(
            t(original ? 'sponsorText' : 'supportCurrentAuthorText'),
            style: TextStyle(fontSize: 13.6, height: 1.7, color: p.secondary),
          ),
          const SizedBox(height: 4),
          for (final key
              in original
                  ? ['originalAuthorPoint', 'sponsorHint']
                  : [
                      'supportPointMaintenance',
                      'supportPointCompatibility',
                      'supportPointTransparency',
                    ])
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                '· ${t(key)}',
                style: TextStyle(
                  fontSize: 13.6,
                  height: 1.7,
                  color: p.secondary,
                ),
              ),
            ),
          const SizedBox(height: 18),
          Center(
            child: ClipRRect(
              borderRadius: BorderRadius.circular(8),
              child: image,
            ),
          ),
          const SizedBox(height: 8),
          Center(
            child: Text(
              t(original ? 'sponsorQrCaption' : 'supportQrCaption'),
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: p.secondary,
              ),
            ),
          ),
        ],
      ),
    );
  }

  bool get _narrow => !MioDeviceScope.wide(context, widget.viewportWidth);

  Widget _aboutField(String title, Widget child) => !_narrow
      ? field(title, child)
      : Padding(
          padding: const EdgeInsets.only(top: 24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: const TextStyle(
                  fontSize: 14.5,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 8),
              DefaultTextStyle.merge(
                style: TextStyle(fontSize: 14, height: 1.6, color: p.secondary),
                child: child,
              ),
            ],
          ),
        );

  Widget _aboutHero() => MioSettingsSection(
    palette: p,
    child: Padding(
      padding: EdgeInsets.symmetric(vertical: _narrow ? 0 : 26),
      child: Column(
        children: [
          Wrap(
            alignment: WrapAlignment.center,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 16,
            runSpacing: 10,
            children: [
              ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: Image.asset(
                  'assets/images/icon.png',
                  width: 56,
                  height: 56,
                ),
              ),
              Row(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.baseline,
                textBaseline: TextBaseline.alphabetic,
                children: [
                  const Text(
                    'Mio Music',
                    style: TextStyle(fontSize: 28, fontWeight: FontWeight.w800),
                  ),
                  const SizedBox(width: 12),
                  ListenableBuilder(
                    listenable: widget.core.updates,
                    builder: (context, _) {
                      final version = widget.core.updates.installedVersion;
                      return Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 10,
                          vertical: 3,
                        ),
                        decoration: BoxDecoration(
                          color: p.color('--td-brand-color-1'),
                          border: Border.all(
                            color: p.color('--td-brand-color-3'),
                          ),
                          borderRadius: BorderRadius.circular(16),
                        ),
                        child: Text(
                          version == null
                              ? '—'
                              : version.startsWith('v')
                              ? version
                              : 'v$version',
                          style: TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                            color: p.color(
                              _narrow
                                  ? '--td-text-color-primary'
                                  : '--td-brand-color-6',
                            ),
                          ),
                        ),
                      );
                    },
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 16),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Text(
              p.text('settings.about.description'),
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 14.4, height: 1.6, color: p.secondary),
            ),
          ),
        ],
      ),
    ),
  );

  Widget _storagePath(
    String title,
    String description,
    String path, {
    String? size,
  }) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Wrap(
        spacing: 8,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          Text(
            title,
            style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
          ),
          if (size != null)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
              decoration: BoxDecoration(
                color: p.brand.withValues(alpha: .16),
                borderRadius: BorderRadius.circular(3),
              ),
              child: Text(size, style: TextStyle(fontSize: 12, color: p.brand)),
            ),
        ],
      ),
      const SizedBox(height: 8),
      Text(
        description,
        style: TextStyle(fontSize: 13, height: 1.5, color: p.secondary),
      ),
      const SizedBox(height: 12),
      Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        decoration: BoxDecoration(
          color: p.surface,
          border: Border.all(color: p.border),
          borderRadius: BorderRadius.circular(4),
        ),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Text(path, style: const TextStyle(fontSize: 13)),
        ),
      ),
    ],
  );

  /// Desktop directory block: title with size tag, description, read-only path.
  Widget _wideDirectory(
    String title,
    String description,
    String path, {
    String? size,
    required bool brand,
  }) {
    final tagBg = p.color(
      brand ? '--td-brand-color-1' : '--td-success-color-1',
    );
    final tagFg = p.color(brand ? '--td-brand-color' : '--td-success-color');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Text(
              title,
              style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
            ),
            if (size != null) ...[
              const SizedBox(width: 8),
              Container(
                height: 22,
                padding: const EdgeInsets.symmetric(horizontal: 8),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: tagBg,
                  borderRadius: BorderRadius.circular(3),
                ),
                child: Text(size, style: TextStyle(fontSize: 12, color: tagFg)),
              ),
            ],
          ],
        ),
        const SizedBox(height: 4),
        Text(description, style: TextStyle(fontSize: 13, color: p.secondary)),
        const SizedBox(height: 12),
        Container(
          height: 32,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          alignment: Alignment.centerLeft,
          decoration: BoxDecoration(
            color: p.color('--td-bg-color-specialcomponent'),
            border: Border.all(color: p.color('--td-border-level-2-color')),
            borderRadius: BorderRadius.circular(3),
          ),
          child: Text(
            path,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ],
    );
  }

  List<Widget> _wideStorage(api.SettingsOverview? overview) {
    String t(String key) => p.text('settings.storage.$key');
    final size = overview == null
        ? null
        : '${(overview.cacheBytes / 1048576).toStringAsFixed(2)} MB';
    final files = overview?.cacheFiles ?? 0;
    return [
      section(t('directoryConfig'), [
        const SizedBox(height: 8),
        if (overview != null) ...[
          _wideDirectory(
            t('cacheDirectory'),
            t('cacheDirectoryDesc'),
            overview.paths.cache,
            size: size,
            brand: true,
          ),
          Container(
            height: 1,
            margin: const EdgeInsets.symmetric(vertical: 24),
            color: p.color('--td-border-level-1-color'),
          ),
          _wideDirectory(
            t('downloadDirectory'),
            t('downloadDirectoryDesc'),
            overview.paths.downloads,
            brand: false,
          ),
          const SizedBox(height: 16),
        ],
        Text(
          tr(
            'Directories are managed by the mobile system. Use backup and restore to transfer data.',
            '目录由移动系统沙盒管理，可使用备份与恢复转移数据。',
          ),
          style: TextStyle(fontSize: 13, color: p.secondary),
        ),
      ]),
      MioSettingsSection(
        palette: p,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.only(bottom: 16),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      t('localCacheConfig'),
                      style: const TextStyle(
                        fontSize: 18,
                        height: 1.35,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  if (overview != null)
                    Text(
                      '${t('existingCacheSize')}$size${files > 0 ? '（${t('fileCount').replaceAll('{count}', '$files')}）' : ''}',
                      style: TextStyle(fontSize: 13, color: p.secondary),
                    ),
                ],
              ),
            ),
            Row(
              children: [
                MioSettingsButton(
                  palette: p,
                  label: t('clearLocalCache'),
                  onPressed: live && !_busy && files > 0
                      ? () => unawaited(_clearCache())
                      : null,
                ),
                if (files == 0) ...[
                  const SizedBox(width: 12),
                  Text(
                    t('noCache'),
                    style: TextStyle(fontSize: 13, color: p.secondary),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
      StorageSettingsView(
        core: widget.core,
        palette: p,
        active: live,
        ready: ready,
        viewportWidth: widget.viewportWidth,
        embedded: true,
        onLocal: widget.onLocal,
        onBackup: widget.onBackup,
      ),
    ];
  }

  List<Widget> _compactStorage(api.SettingsOverview? overview) {
    String t(String key) => p.text('settings.storage.$key');
    final size = overview == null
        ? null
        : '${(overview.cacheBytes / 1048576).toStringAsFixed(2)} MB';
    return [
      section(t('directoryConfig'), [
        if (overview != null) ...[
          _storagePath(
            t('cacheDirectory'),
            t('cacheDirectoryDesc'),
            overview.paths.cache,
            size: size,
          ),
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: Container(height: 1, color: p.border),
          ),
          _storagePath(
            t('downloadDirectory'),
            t('downloadDirectoryDesc'),
            overview.paths.downloads,
          ),
          const SizedBox(height: 16),
        ],
        Text(
          tr(
            'Directories are managed by the mobile system. Use backup and restore to transfer data.',
            '目录由移动系统沙盒管理，可使用备份与恢复转移数据。',
          ),
          style: TextStyle(fontSize: 12, height: 1.5, color: p.secondary),
        ),
      ]),
      section(t('localCacheConfig'), [
        if (overview != null)
          Text(
            '${t('existingCacheSize')}$size（${t('fileCount').replaceAll('{count}', '${overview.cacheFiles}')}）',
            style: TextStyle(fontSize: 13, height: 1.5, color: p.secondary),
          ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _compactAction(
              t('clearLocalCache'),
              () => unawaited(_clearCache()),
            ),
            _compactAction(p.text('common.refresh'), () => unawaited(_load())),
          ],
        ),
      ]),
      StorageSettingsView(
        core: widget.core,
        palette: p,
        active: live,
        ready: ready,
        viewportWidth: widget.viewportWidth,
        embedded: true,
        onLocal: widget.onLocal,
        onBackup: widget.onBackup,
      ),
      section(tr('Backup and restore', '备份与恢复'), [
        if (overview != null) ...[
          _storagePath(
            tr('Data directory', '数据目录'),
            tr('Application settings and library data', '应用设置与音乐库数据'),
            overview.paths.data,
          ),
          const SizedBox(height: 12),
        ],
        button(tr('Backup and restore', '备份与恢复'), widget.onBackup),
      ]),
    ];
  }

  Future<void> _openAbout(String which) => _run(() async {
    await const MethodChannel('mio/host')
        .invokeMethod<void>('openAboutLink', which);
  });

  Widget _legalNotice(String title, String text) => Padding(
    padding: const EdgeInsets.only(top: 12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 4),
        Text(
          text,
          style: TextStyle(fontSize: 13.6, height: 1.6, color: p.secondary),
        ),
      ],
    ),
  );

  /// "About us" paragraph with the two project links highlighted and tappable.
  Widget _aboutIntro() {
    final template = p.text('settings.about.aboutIntro');
    const values = ['CeruMusic', '时迁酱', 'CeruMusic'];
    const targets = ['original', null, 'docs'];
    final spans = <InlineSpan>[];
    var rest = template;
    for (var i = 0; i < values.length; i++) {
      final at = rest.indexOf('{$i}');
      if (at < 0) continue;
      if (at > 0) spans.add(TextSpan(text: rest.substring(0, at)));
      final target = targets[i];
      spans.add(
        TextSpan(
          text: values[i],
          style: target == null ? null : TextStyle(color: p.brand),
          recognizer: target == null
              ? null
              : (TapGestureRecognizer()
                  ..onTap = () => unawaited(_openAbout(target))),
        ),
      );
      rest = rest.substring(at + 3);
    }
    if (rest.isNotEmpty) spans.add(TextSpan(text: rest));
    return Text.rich(
      TextSpan(children: spans),
      style: TextStyle(fontSize: 13.6, height: 1.7, color: p.secondary),
    );
  }

  /// Desktop `.support-card`: text on the left, QR code and caption on the right.
  Widget _wideSupportCard({required bool original}) {
    String t(String key) => p.text('settings.about.$key');
    final image = original
        ? Image.network(
            'https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png',
            width: 132,
            height: 132,
            fit: BoxFit.contain,
            errorBuilder: (_, _, _) => SizedBox(
              width: 132,
              height: 132,
              child: Center(
                child: Text(tr('QR image unavailable', '二维码暂时无法加载')),
              ),
            ),
          )
        : Image.asset(
            'assets/images/current-author-sponsor-qr.jpg',
            width: 132,
            height: 132,
            fit: BoxFit.contain,
          );
    final points = original
        ? ['originalAuthorPoint', 'sponsorHint']
        : [
            'supportPointMaintenance',
            'supportPointCompatibility',
            'supportPointTransparency',
          ];
    return Container(
      margin: const EdgeInsets.only(top: 16),
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: p.color('--td-bg-color-page'),
        border: Border.all(color: p.color('--td-border-level-1-color')),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  t(
                    original
                        ? 'originalAuthorTitle'
                        : 'supportCurrentAuthorTitle',
                  ),
                  style: const TextStyle(
                    fontSize: 15,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    t(original ? 'sponsorText' : 'supportCurrentAuthorText'),
                    style: TextStyle(
                      fontSize: 13.6,
                      height: 1.6,
                      color: p.secondary,
                    ),
                  ),
                ),
                for (final key in points)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(
                      '· ${t(key)}',
                      style: TextStyle(fontSize: 13, color: p.secondary),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(width: 16),
          Column(
            children: [
              ClipRRect(borderRadius: BorderRadius.circular(8), child: image),
              const SizedBox(height: 6),
              Text(
                t(original ? 'sponsorQrCaption' : 'supportQrCaption'),
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
            ],
          ),
        ],
      ),
    );
  }

  List<Widget> _wideAbout() => [
    _aboutHero(),
    section(p.text('settings.about.versionInfo'), [
      ReleaseSettingsView(
        updates: widget.core.updates,
        palette: p,
        embedded: true,
        compact: true,
      ),
    ]),
    section(p.text('settings.about.legalNotice'), [
      _legalNotice(
        p.text('settings.about.dataResponsibility'),
        p.text('settings.about.dataResponsibilityDesc'),
      ),
      _legalNotice(
        p.text('settings.about.copyrightCompliance'),
        p.text('settings.about.copyrightComplianceDesc'),
      ),
      _legalNotice(
        p.text('settings.about.usageRestriction'),
        p.text('settings.about.usageRestrictionDesc'),
      ),
      Padding(
        padding: const EdgeInsets.only(top: 24, bottom: 8),
        child: Text(
          p.text('settings.about.aboutUs'),
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
        ),
      ),
      _aboutIntro(),
      _wideSupportCard(original: false),
      _wideSupportCard(original: true),
    ]),
  ];

  List<Widget> _about() => !_narrow
      ? _wideAbout()
      : [
          if (_narrow)
            _aboutHero()
          else
            section('Mio Music', [
              Text(
                widget.core.updates.installedVersion ?? '',
                style: TextStyle(color: p.secondary),
              ),
              Text(p.text('settings.about.description')),
            ]),
          section(p.text('settings.about.versionInfo'), [
            ReleaseSettingsView(
              updates: widget.core.updates,
              palette: p,
              embedded: true,
              compact: _narrow,
            ),
          ]),
          section(p.text('settings.about.legalNotice'), [
            if (!_narrow)
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  button(
                    'CeruMusic',
                    () => unawaited(
                      _run(() async {
                        await const MethodChannel('mio/host')
                            .invokeMethod<void>('openAboutLink', 'original');
                      }),
                    ),
                  ),
                  button(
                    tr('Documentation', '使用文档'),
                    () => unawaited(
                      _run(() async {
                        await const MethodChannel('mio/host')
                            .invokeMethod<void>('openAboutLink', 'docs');
                      }),
                    ),
                  ),
                ],
              ),
            for (final key in [
              'dataResponsibility',
              'copyrightCompliance',
              'usageRestriction',
            ])
              _aboutField(
                p.text('settings.about.$key'),
                Text(p.text('settings.about.${key}Desc')),
              ),
            _aboutField(
              p.text('settings.about.aboutUs'),
              Text(
                p
                    .text('settings.about.aboutIntro')
                    .replaceAll('{0}', 'CeruMusic')
                    .replaceAll('{1}', '时迁酱')
                    .replaceAll('{2}', 'CeruMusic'),
              ),
            ),
            if (_narrow)
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  button(
                    'CeruMusic',
                    () => unawaited(
                      _run(() async {
                        await const MethodChannel('mio/host')
                            .invokeMethod<void>('openAboutLink', 'original');
                      }),
                    ),
                  ),
                  button(
                    tr('Documentation', '使用文档'),
                    () => unawaited(
                      _run(() async {
                        await const MethodChannel('mio/host')
                            .invokeMethod<void>('openAboutLink', 'docs');
                      }),
                    ),
                  ),
                ],
              ),
            if (_narrow)
              _supportCard(original: false)
            else
              _aboutField(
                p.text('settings.about.supportCurrentAuthorTitle'),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(p.text('settings.about.supportCurrentAuthorText')),
                    for (final key in [
                      'supportPointMaintenance',
                      'supportPointCompatibility',
                      'supportPointTransparency',
                    ])
                      Text('· ${p.text('settings.about.$key')}'),
                    const SizedBox(height: 12),
                    Image.asset(
                      'assets/images/current-author-sponsor-qr.jpg',
                      width: 132,
                      height: 132,
                    ),
                    Text(p.text('settings.about.supportQrCaption')),
                  ],
                ),
              ),
            if (_narrow)
              _supportCard(original: true)
            else
              _aboutField(
                p.text('settings.about.originalAuthorTitle'),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(p.text('settings.about.sponsorText')),
                    Text(p.text('settings.about.originalAuthorPoint')),
                    Text(p.text('settings.about.sponsorHint')),
                    Image.network(
                      'https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png',
                      width: 132,
                      height: 132,
                      errorBuilder: (_, _, _) =>
                          Text(tr('QR image unavailable', '二维码暂时无法加载')),
                    ),
                    Text(p.text('settings.about.sponsorQrCaption')),
                  ],
                ),
              ),
          ]),
        ];
  @override
  Widget build(BuildContext context) {
    final general = _general, overview = _overview;
    final children = switch (widget.category) {
      CompactSettingsCategory.appearance => [
        section(p.text('settings.appearance.title'), [
          ...[
            Text(
              p.text('settings.language'),
              style: TextStyle(
                fontSize: 14,
                height: (_narrow ? 18 : 22) / 14,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              p.text('settings.languageDesc'),
              style: TextStyle(
                fontSize: 12,
                height: (_narrow ? 17 : 22) / 12,
                color: p.secondary,
              ),
            ),
            SizedBox(height: _narrow ? 12 : 16),
            Align(
              alignment: Alignment.centerLeft,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 260),
                child: MioSettingsSelect(
                  palette: p,
                  label: p.text('settings.language'),
                  outlined: true,
                  value: general?.language,
                  options: [
                    (value: 'system', label: tr('Follow system', '跟随系统')),
                    (value: 'zh-CN', label: '简体中文'),
                    (value: 'en-US', label: 'English'),
                  ],
                  onChanged: live && !_busy && general != null
                      ? (v) => unawaited(_save(language: v))
                      : null,
                ),
              ),
            ),
            SizedBox(height: _narrow ? 4 : 16),
          ],
          visual(VisualSettingsSection.appearance),
          if (_narrow)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Container(height: 1, color: p.border),
            ),
        ]),
        section(p.text('settings.lyricFont.title'), [lyrics()]),
        section(tr('Floating lyrics', '悬浮歌词'), [
          NativeLyricSettings(palette: p, active: live),
        ]),
      ],
      CompactSettingsCategory.playback => [
        PlaybackSettingsView(
          core: widget.core,
          palette: p,
          active: live,
          ready: ready,
          viewportWidth: widget.viewportWidth,
          embedded: true,
          compact: true,
          overview: overview,
          onClear: live && !_busy ? () => unawaited(_clearQueue()) : null,
        ),
        if (_narrow)
          section(p.text('settings.backgroundRender.title'), [
            visual(VisualSettingsSection.background),
          ])
        else
          MioSettingsSection(
            palette: p,
            child: visual(VisualSettingsSection.background),
          ),
        section(p.text('settings.playback.fullscreenPerformance'), [
          visual(VisualSettingsSection.performance),
          field(
            p.text('settings.playback.routePreload'),
            description: p.text('settings.playback.routePreloadDesc'),
            MioSettingsSwitch(
              palette: p,
              label: p.text('settings.playback.routePreload'),
              value: general?.routePreload ?? true,
              onPressed: live && !_busy && general != null
                  ? () => unawaited(_save(preload: !general.routePreload))
                  : null,
            ),
          ),
        ]),
      ],
      CompactSettingsCategory.effects => [
        MioSettingsSection(
          palette: p,
          title: _narrow ? p.text('settings.effects.equalizer') : null,
          child: equalizer(title: p.text('settings.effects.equalizer')),
        ),
        MioSettingsSection(
          palette: p,
          title: _narrow ? p.text('settings.effects.audioEffect') : null,
          child: AudioEffectsView(
            core: widget.core,
            palette: p,
            active: live,
            ready: ready,
            viewportWidth: widget.viewportWidth,
            embedded: true,
            sectionTitle: p.text('settings.effects.audioEffect'),
          ),
        ),
      ],
      CompactSettingsCategory.plugins => [
        PluginsView(
          compact: true,
          core: widget.core,
          palette: p,
          active: live,
          ready: ready,
          sourceRevision: widget.core.player?.sourceRevision ?? 0,
          embedded: true,
        ),
      ],
      CompactSettingsCategory.music => _music(),
      CompactSettingsCategory.storage =>
        _narrow ? _compactStorage(overview) : _wideStorage(overview),
      CompactSettingsCategory.about => _about(),
      CompactSettingsCategory.lyrics => [
        section(tr('Lyric style', '歌词样式'), [lyrics()]),
      ],
      CompactSettingsCategory.equalizer => [
        section(p.text('settings.effects.equalizer'), [equalizer()]),
      ],
      CompactSettingsCategory.visual => [
        section(
          _narrow
              ? p.text('settings.backgroundRender.title')
              : tr('Background rendering', '背景渲染'),
          [visual(VisualSettingsSection.background)],
        ),
        section(
          _narrow
              ? p.text('settings.playback.fullscreenPerformance')
              : tr('Performance', '性能设置'),
          [visual(VisualSettingsSection.performance)],
        ),
      ],
    };
    return ListView(
      key: PageStorageKey(widget.category),
      // Wide: the cards sit close to the frame like the desktop pane.
      padding: EdgeInsets.all(
        _narrow ? MioPalette.gutter(widget.viewportWidth) : 12,
      ),
      children: [
        if (_busy)
          Semantics(liveRegion: true, child: Text(tr('Working…', '正在处理…'))),
        if (_error != null)
          section(tr('Unable to complete', '未能完成操作'), [
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            button(p.text('common.refresh'), () => unawaited(_load())),
          ]),
        ...children,
      ],
    );
  }
}

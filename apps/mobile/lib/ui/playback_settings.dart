import 'device.dart';
import 'settings_layout.dart';
import 'player_style_controls.dart';

import 'dart:async';
import 'dart:io' show Platform;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart' show LogicalKeyboardKey;

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'queue_transfer.dart';
import 'dlna.dart';

class PlaybackSettingsView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final double viewportWidth;
  final bool playerPanel;
  final bool embedded;
  final bool compact;
  final api.SettingsOverview? overview;
  final VoidCallback? onClear;
  final VoidCallback? onPanelLoaded;

  /// Compact cards only: `queue` is the playlist management group, `extras`
  /// the seamless and DLNA groups, `all` both.
  final PlaybackPart part;

  /// Inserted between the playlist management group and the extras so the
  /// whole page shares one settings session.
  const PlaybackSettingsView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.playerPanel = false,
    this.embedded = false,
    this.compact = false,
    this.overview,
    this.onClear,
    this.onPanelLoaded,
    this.part = PlaybackPart.all,
  });
  @override
  State<PlaybackSettingsView> createState() => _PlaybackSettingsViewState();
}

enum PlaybackPart { all, queue, extras }

class _PlaybackSettingsViewState extends State<PlaybackSettingsView>
    with WidgetsBindingObserver {
  final _panelToggle = FocusNode(debugLabel: 'Style seamless');
  final _panelGapless = FocusNode(debugLabel: 'Style gapless');
  final _panelCrossfade = FocusNode(debugLabel: 'Style crossfade');
  final _panelDuration = FocusNode(debugLabel: 'Style duration');
  Widget _panelKeys(
    Widget child, {
    FocusNode? up,
    FocusNode? down,
    FocusNode? left,
    FocusNode? right,
  }) => !widget.playerPanel
      ? child
      : CallbackShortcuts(
          bindings: {
            if (up != null)
              SingleActivator(LogicalKeyboardKey.arrowUp): up.requestFocus,
            if (down != null)
              SingleActivator(LogicalKeyboardKey.arrowDown): down.requestFocus,
            if (left != null)
              SingleActivator(LogicalKeyboardKey.arrowLeft): left.requestFocus,
            if (right != null)
              SingleActivator(LogicalKeyboardKey.arrowRight):
                  right.requestFocus,
          },
          child: child,
        );
  api.PlaybackSettingsSession? _session;
  api.SeamlessSettings? _value;
  // Presentation only: retain panel height while its Rust session reconnects.
  api.SeamlessSettings? _panelValue;
  bool _loading = false, _busy = false, _foreground = true, _queueModal = false;
  String? _queueMessage;
  String? _error;
  int _epoch = 0;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;

  @override
  void initState() {
    super.initState();
    _foreground = ![
      AppLifecycleState.paused,
      AppLifecycleState.detached,
    ].contains(WidgetsBinding.instance.lifecycleState);
    WidgetsBinding.instance.addObserver(this);
    if (_live) unawaited(_open());
  }

  void _release() {
    final session = _session;
    _session = null;
    _value = null;
    session?.close();
    session?.dispose();
  }

  void _invalidate() {
    _epoch++;
    _release();
    _busy = false;
    _loading = false;
  }

  @override
  void didUpdateWidget(PlaybackSettingsView old) {
    super.didUpdateWidget(old);
    if ((!widget.active && old.active) || (!widget.ready && old.ready)) {
      _invalidate();
    }
    if (_live &&
        ((!old.active && widget.active) || (!old.ready && widget.ready))) {
      unawaited(_open());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live && !_queueModal) unawaited(_open());
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _invalidate();
    }
  }

  Future<void> _open() async {
    if (!_live) return;
    final epoch = ++_epoch;
    _release();
    setState(() {
      _loading = true;
      _busy = false;
      _error = null;
    });
    api.PlaybackSettingsSession? candidate;
    try {
      candidate = await api.beginPlaybackSettings();
      final value = await candidate.state();
      if (!_live || epoch != _epoch) return;
      _session = candidate;
      candidate = null;
      setState(() {
        _value = value;
        _panelValue = value;
        _loading = false;
      });
      widget.onPanelLoaded?.call();
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Unable to load playback settings.'
              : '无法读取播放设置。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _transfer(bool exporting) async {
    final snapshot = widget.core.player;
    if (!_live || _loading || _busy || _queueModal || snapshot == null) return;
    setState(() {
      _queueModal = true;
      _queueMessage = null;
    });
    try {
      final message = await showQueueTransfer(
        context: context,
        palette: p,
        snapshot: snapshot,
        exporting: exporting,
        isCurrent: () =>
            mounted && widget.active && widget.ready && _queueModal,
      );
      if (mounted && message != null) setState(() => _queueMessage = message);
    } catch (_) {
      if (mounted) {
        setState(
          () => _queueMessage = p.english
              ? 'Queue changed or transfer unavailable. Reopen to retry.'
              : '队列已变化或无法传输，请重新打开后重试。',
        );
      }
    } finally {
      if (mounted) setState(() => _queueModal = false);
      if (_live) unawaited(_open());
    }
  }

  Future<void> _save(api.SeamlessSettings next) async {
    final session = _session;
    final expected = _value;
    if (!_live || _busy || _loading || session == null || expected == null) {
      return;
    }
    final epoch = _epoch;
    setState(() => _busy = true);
    api.SeamlessSettings? saved;
    await widget.core.command(() async {
      saved = await session.save(expected: expected, next: next);
    });
    if (!_live || epoch != _epoch || !identical(session, _session)) return;
    setState(() {
      _busy = false;
      if (saved != null) {
        _value = saved;
        _panelValue = saved;
      } else {
        _release();
        _error = p.english
            ? 'Save failed or settings changed. Reload before trying again.'
            : '保存失败或设置已变更，请重新加载后重试。';
      }
    });
  }

  @override
  void dispose() {
    _invalidate();
    for (final node in [
      _panelToggle,
      _panelGapless,
      _panelCrossfade,
      _panelDuration,
    ]) {
      node.dispose();
    }
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final value = _value ?? (widget.playerPanel ? _panelValue : null);
    final enabled =
        _live && !_busy && !_loading && !_queueModal && _value != null;
    final wide = MioDeviceScope.wide(context, widget.viewportWidth);
    final gutter = wide ? 24.0 : (widget.viewportWidth * .04).clamp(16.0, 24.0);
    api.SeamlessSettings changed({bool? on, bool? crossfade, int? duration}) =>
        api.SeamlessSettings(
          enabled: on ?? value!.enabled,
          crossfade: crossfade ?? value!.crossfade,
          durationMs: duration ?? value!.durationMs,
        );
    final playbackControls = <Widget>[
      if (value != null) ...[
        if (widget.playerPanel)
          _panelKeys(
            PlayerStyleSwitch(
              focusNode: _panelToggle,
              palette: p,
              label: p.text('play.seamlessSwitch'),
              checked: value.enabled,
              onPressed: enabled
                  ? () => unawaited(_save(changed(on: !value.enabled)))
                  : null,
            ),
            down: value.enabled
                ? (value.crossfade ? _panelCrossfade : _panelGapless)
                : null,
          )
        else if (widget.compact && !wide)
          Align(
            alignment: Alignment.centerLeft,
            child: MioSettingsSwitch(
              palette: p,
              label: p.text('play.seamlessSwitch'),
              value: value.enabled,
              onPressed: enabled
                  ? () => unawaited(_save(changed(on: !value.enabled)))
                  : null,
            ),
          )
        else if (widget.embedded)
          MioSettingsField(
            palette: p,
            title: p.text('play.seamlessSwitch'),
            child: MioSettingsSwitch(
              palette: p,
              label: p.text('play.seamlessSwitch'),
              value: value.enabled,
              onPressed: enabled
                  ? () => unawaited(_save(changed(on: !value.enabled)))
                  : null,
            ),
          )
        else
          Row(
            children: [
              Expanded(
                child: Text(
                  p.text('play.seamlessSwitch'),
                  style: const TextStyle(fontSize: 14),
                ),
              ),
              Semantics(
                toggled: value.enabled,
                child: SizedBox(
                  width: 60,
                  height: 44,
                  child: MioButton(
                    palette: p,
                    label: p.text('play.seamlessSwitch'),
                    onPressed: enabled
                        ? () => unawaited(_save(changed(on: !value.enabled)))
                        : null,
                    radius: 22,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 5,
                      vertical: 8,
                    ),
                    child: Container(
                      height: 28,
                      decoration: BoxDecoration(
                        color: value.enabled ? p.brand : p.border,
                        borderRadius: BorderRadius.circular(14),
                      ),
                      alignment: value.enabled
                          ? Alignment.centerRight
                          : Alignment.centerLeft,
                      padding: const EdgeInsets.all(3),
                      child: Container(
                        width: 22,
                        height: 22,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: p.color('--td-text-color-anti'),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        if (value.enabled) ...[
          const SizedBox(height: 8),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(p.text('play.transitionMode')),
              for (final crossfade in [false, true])
                _panelKeys(
                  Semantics(
                    selected: value.crossfade == crossfade,
                    child: MioButton(
                      palette: p,
                      focusNode: widget.playerPanel
                          ? (crossfade ? _panelCrossfade : _panelGapless)
                          : null,
                      label: p.text(
                        crossfade ? 'play.crossfadeMode' : 'play.gaplessMode',
                      ),
                      background: widget.playerPanel
                          ? Color(
                              value.crossfade == crossfade
                                  ? 0x33ffffff
                                  : 0x12ffffff,
                            )
                          : value.crossfade == crossfade
                          ? p.brand.withValues(alpha: .2)
                          : p.surface,
                      onPressed: enabled
                          ? () =>
                                unawaited(_save(changed(crossfade: crossfade)))
                          : null,
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(minHeight: 28),
                        child: Center(
                          widthFactor: 1,
                          child: Text(
                            p.text(
                              crossfade
                                  ? 'play.crossfadeMode'
                                  : 'play.gaplessMode',
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                  up: _panelToggle,
                  down: value.crossfade ? _panelDuration : null,
                  left: crossfade ? _panelGapless : null,
                  right: crossfade ? null : _panelCrossfade,
                ),
            ],
          ),
          if (value.crossfade) ...[
            const SizedBox(height: 8),
            Text(
              p
                  .text('play.transitionDuration')
                  .replaceAll(
                    '{duration}',
                    (value.durationMs / 1000).toStringAsFixed(1),
                  ),
            ),
            SizedBox(
              width: wide ? 160 : double.infinity,
              child: _panelKeys(
                MioSeekBar(
                  focusNode: widget.playerPanel ? _panelDuration : null,
                  palette: p,
                  value: value.durationMs / 1000 - .5,
                  maximum: 7.5,
                  step: .5,
                  height: 44,
                  semanticLabel: p.text('play.crossfadeMode'),
                  semanticValue: (seconds) =>
                      '${(seconds + .5).toStringAsFixed(1)} s',
                  onChanged: enabled
                      ? (seconds) => unawaited(
                          _save(
                            changed(
                              duration: (((seconds + .5) * 2).round() * 500)
                                  .clamp(500, 8000),
                            ),
                          ),
                        )
                      : null,
                ),
                up: _panelCrossfade,
              ),
            ),
          ],
        ],
      ],
    ];
    if (widget.playerPanel) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_loading) Text(p.english ? 'Loading…' : '正在加载…'),
          if (_error != null) ...[
            Text(_error!, style: const TextStyle(color: Color(0xffffb4b4))),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          ...playbackControls,
          const SizedBox(height: 24),
          Text(
            p.text('play.lyricSettings'),
            style: const TextStyle(
              fontSize: 18,
              height: 28 / 18,
              fontWeight: FontWeight.w600,
              color: Color(0xf2ffffff),
            ),
          ),
          const SizedBox(height: 20),
          for (final key in ['filterLyricSongInfo', 'strictFilterMode'])
            PlayerStyleSwitch(
              palette: p,
              label: p.text('play.$key'),
              checked: false,
              onPressed: null,
            ),
        ],
      );
    }
    if (widget.compact && !widget.playerPanel) {
      String t(String key) => p.text('settings.playlist.$key');
      final overview = widget.overview;
      Widget stat(String label, String value) => Padding(
        padding: EdgeInsets.only(bottom: wide ? 0 : 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(label, style: TextStyle(fontSize: 12, color: p.secondary)),
            const SizedBox(height: 2),
            Text(
              value,
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
          ],
        ),
      );
      Widget action(
        String key,
        VoidCallback? callback, {
        bool danger = false,
      }) => wide
          ? MioSettingsButton(
              palette: p,
              label: t(key),
              primary: !danger,
              danger: danger,
              onPressed: callback,
            )
          : MioButton(
              palette: p,
              label: t(key),
              radius: 6,
              insetFocusBorder: false,
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
              background: danger
                  ? p.color('--td-error-color').withValues(alpha: .12)
                  : p.brand,
              onPressed: callback,
              child: Text(
                t(key),
                style: TextStyle(
                  fontSize: 13,
                  color: p.color(
                    danger ? '--td-error-color' : '--td-text-color-anti',
                  ),
                ),
              ),
            );
      final seconds = overview?.queueSeconds ?? 0;
      final duration = seconds >= 3600
          ? t('formatDuration')
                .replaceAll('{hours}', '${seconds ~/ 3600}')
                .replaceAll('{minutes}', '${seconds % 3600 ~/ 60}')
          : t('formatDurationShort')
                .replaceAll('{minutes}', '${seconds ~/ 60}')
                .replaceAll('{seconds}', '${seconds % 60}');
      final showQueue = widget.part != PlaybackPart.extras;
      final showExtras = widget.part != PlaybackPart.queue;
      final stats = [
        stat(
          t('songCount'),
          overview == null
              ? '—'
              : t('songUnit').replaceAll('{count}', '${overview.queueCount}'),
        ),
        stat(t('totalDuration'), overview == null ? '—' : duration),
        stat(
          t('artistCount'),
          overview == null
              ? '—'
              : t('artistUnit')
                    .replaceAll('{count}', '${overview.artistCount}'),
        ),
      ];
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (showQueue)
            MioSettingsSection(
              palette: p,
              title: wide
                  ? p.text('settings.playback.playlistManagement')
                  : t('management'),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  MioSettingsField(
                    palette: p,
                    title: t('stats'),
                    queueCard: true,
                    child: wide
                        ? Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              for (var i = 0; i < stats.length; i++) ...[
                                if (i > 0) const SizedBox(width: 16),
                                Expanded(child: stats[i]),
                              ],
                            ],
                          )
                        : Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: stats,
                          ),
                  ),
                  MioSettingsField(
                    palette: p,
                    title: t('management'),
                    queueCard: true,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Wrap(
                          spacing: 12,
                          runSpacing: 12,
                          children: [
                            action(
                              'exportPlaylist',
                              enabled ? () => unawaited(_transfer(true)) : null,
                            ),
                            action(
                              'importPlaylist',
                              enabled
                                  ? () => unawaited(_transfer(false))
                                  : null,
                            ),
                            if (wide)
                              action(
                                'clearPlaylist',
                                enabled ? widget.onClear : null,
                                danger: true,
                              ),
                          ],
                        ),
                        if (!wide) ...[
                          const SizedBox(height: 12),
                          action(
                            'clearPlaylist',
                            enabled ? widget.onClear : null,
                            danger: true,
                          ),
                        ],
                        const SizedBox(height: 16),
                        Text(
                          t('featureDescription'),
                          style: const TextStyle(
                            fontSize: 14,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        for (final key in [
                          'exportDesc',
                          'importDesc',
                          'clearDesc',
                        ])
                          Padding(
                            padding: const EdgeInsets.only(bottom: 6),
                            child: Text(
                              '• ${t(key)}',
                              style: TextStyle(
                                fontSize: 13,
                                height: 1.6,
                                color: p.secondary,
                              ),
                            ),
                          ),
                        if (_queueMessage != null)
                          Semantics(
                            liveRegion: true,
                            child: Text(_queueMessage!),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          if (_error != null && showQueue) ...[
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          if (showExtras) ...[
            // DLNA is Android-only; an empty card would otherwise remain.
            if (Platform.isAndroid || !wide)
              MioSettingsSection(
                palette: p,
                child: DlnaSettings(
                  palette: p,
                  active: widget.active,
                  ready: widget.ready,
                ),
              ),
          ],
        ],
      );
    }
    return MioSettingsBody(
      embedded: widget.embedded,
      padding: EdgeInsets.all(gutter),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!widget.embedded)
            Text(
              p.text('play.playbackSettings'),
              style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
            ),
          if (!widget.embedded) const SizedBox(height: 24),
          if (_loading || _busy)
            Semantics(
              liveRegion: true,
              child: Text(p.english ? 'Working…' : '正在处理…'),
            ),
          if (_error != null)
            Semantics(
              liveRegion: true,
              child: Text(
                _error!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
            ),
          if (_error != null)
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          SizedBox(height: widget.embedded && wide ? 0 : 16),
          if (!widget.embedded)
            Text(
              p.text('settings.playlist.management'),
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
            ),
          SizedBox(height: widget.embedded && wide ? 8 : 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final entry in [
                (
                  label: p.text('settings.playlist.exportPlaylist'),
                  run: enabled ? () => unawaited(_transfer(true)) : null,
                ),
                (
                  label: p.text('settings.playlist.importPlaylist'),
                  run: enabled ? () => unawaited(_transfer(false)) : null,
                ),
              ])
                wide
                    ? MioSettingsButton(
                        palette: p,
                        label: entry.label,
                        onPressed: entry.run,
                      )
                    : MioButton(
                        palette: p,
                        label: entry.label,
                        onPressed: entry.run,
                        child: Text(entry.label),
                      ),
            ],
          ),
          DlnaSettings(palette: p, active: widget.active, ready: widget.ready),
          if (_queueMessage != null)
            Semantics(liveRegion: true, child: Text(_queueMessage!)),
          const SizedBox(height: 24),
          ...playbackControls,
        ],
      ),
    );
  }
}

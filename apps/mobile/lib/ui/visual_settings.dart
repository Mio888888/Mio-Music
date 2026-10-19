import 'dart:async';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'visual_policy.dart';
import 'player_style_controls.dart';
import 'settings_layout.dart';
import 'device.dart';

enum VisualSettingsSection { appearance, background, performance }

class VisualSettingsPage extends StatefulWidget {
  final MioPalette palette;
  final bool active, ready;
  final bool playerPanel;
  final bool embedded;
  final VisualSettingsSection? settingsSection;
  final VoidCallback? onPanelLoaded;
  const VisualSettingsPage({
    super.key,
    required this.palette,
    required this.active,
    required this.ready,
    this.playerPanel = false,
    this.embedded = false,
    this.settingsSection,
    this.onPanelLoaded,
  });
  @override
  State<VisualSettingsPage> createState() => _VisualSettingsPageState();
}

class _VisualSettingsPageState extends State<VisualSettingsPage>
    with WidgetsBindingObserver {
  api.VisualSettingsSession? _session;
  api.VisualSettings? _value;
  // Presentation only: retain panel height while its Rust session reconnects.
  api.VisualSettings? _panelValue;
  api.BackgroundSettings? _background;
  Future<void>? _operation;
  Future<void> _cleanup = Future.value();
  bool _foreground = true, _busy = false;
  String? _error;
  int _epoch = 0;
  bool get _live => mounted && _foreground && widget.active && widget.ready;
  bool _current(int epoch) => _live && epoch == _epoch;
  String t(String key) => widget.palette.text('visuals.$key');
  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    if (_live) unawaited(_open());
  }

  void _release() {
    ++_epoch;
    _session?.close();
    _value = null;
    _background = null;
    _busy = false;
    final previous = _cleanup, pending = _operation;
    _cleanup = () async {
      await previous;
      await pending;
      final session = _session;
      _session = null;
      session?.close();
      session?.dispose();
    }();
  }

  Future<void> _open() async {
    _release();
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
    });
    await _cleanup;
    if (!_current(epoch)) return;
    final operation = () async {
      try {
        final session = await api.beginVisualSettings();
        _session = session;
        if (!_current(epoch)) {
          session.close();
          return;
        }
        final value = await session.state();
        final background = await session.background();
        if (_current(epoch)) {
          setState(() {
            _value = value;
            _panelValue = value;
            _background = background;
          });
          widget.onPanelLoaded?.call();
        }
      } catch (_) {
        if (_current(epoch)) setState(() => _error = 'loadFailed');
      }
    }();
    _operation = operation;
    await operation;
    if (_current(epoch)) setState(() => _busy = false);
  }

  Future<void> _save(api.VisualSettings next) async {
    final session = _session, expected = _value;
    if (!_live || _busy || session == null || expected == null) return;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
    });
    final operation = () async {
      try {
        final saved = await session.save(expected: expected, next: next);
        if (_current(epoch)) {
          setState(() {
            _value = saved;
            _panelValue = saved;
            if (_background != null) {
              _background = _backgroundNext(enabled: saved.background);
            }
          });
        }
      } catch (_) {
        if (_current(epoch)) {
          setState(() {
            _value = null;
            _background = null;
            _error = 'saveFailed';
          });
        }
      }
    }();
    _operation = operation;
    await operation;
    if (_current(epoch)) setState(() => _busy = false);
  }

  Future<void> _saveBackground({
    api.BackgroundSettings? next,
    String? preset,
    bool reset = false,
  }) async {
    final session = _session, expected = _background;
    if (!_live || _busy || session == null || expected == null) return;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
    });
    final operation = () async {
      try {
        final saved = next != null
            ? await session.saveBackground(expected: expected, next: next)
            : await session.presetBackground(
                expected: expected,
                preset: reset ? null : preset,
              );
        if (_current(epoch)) {
          setState(() {
            _background = saved;
            if (_value != null) _value = _next(background: saved.enabled);
          });
        }
      } catch (_) {
        if (_current(epoch)) {
          setState(() {
            _value = null;
            _background = null;
            _error = 'saveFailed';
          });
        }
      }
    }();
    _operation = operation;
    await operation;
    if (_current(epoch)) setState(() => _busy = false);
  }

  api.BackgroundSettings _backgroundNext({
    bool? enabled,
    bool? audioResponse,
    double? renderScale,
    double? flowSpeed,
    bool? staticMode,
    int? fps,
  }) {
    final old = _background!;
    return api.BackgroundSettings(
      preset: old.preset,
      enabled: enabled ?? old.enabled,
      audioResponse: audioResponse ?? old.audioResponse,
      renderScale: renderScale ?? old.renderScale,
      flowSpeed: flowSpeed ?? old.flowSpeed,
      staticMode: staticMode ?? old.staticMode,
      fps: fps ?? old.fps,
    );
  }

  String bg(String key) =>
      widget.palette.text('settings.backgroundRender.$key');
  Widget _backgroundControls(api.BackgroundSettings value) {
    // Embedded cards use the desktop control set at both widths.
    final narrow = widget.embedded;
    final wideScreen = MioDeviceScope.wide(
      context,
      MediaQuery.sizeOf(context).width,
    );
    final editable = _live && !_busy;
    final custom = editable && value.enabled && value.preset == 'custom';
    Widget toggle(String key, bool value, bool enabled, VoidCallback action) =>
        widget.embedded
        ? MioSettingsField(
            palette: widget.palette,
            backgroundEffect: narrow,
            title: bg(key),
            description: bg('${key}Desc'),
            child: MioSettingsSwitch(
              palette: widget.palette,
              label: bg(key),
              value: value,
              onPressed: enabled ? action : null,
            ),
          )
        : _button('${value ? '✓' : '○'} ${bg(key)}', enabled ? action : null);
    Widget slider(
      String key,
      String description,
      double value,
      double min,
      double max,
      double step,
      ValueChanged<double> change,
    ) {
      final control = MioSeekBar(
        palette: widget.palette,
        value: value - min,
        maximum: max - min,
        step: step,
        height: narrow ? 12 : 44,
        showThumb: narrow,
        semanticLabel: bg(key),
        semanticValue: (v) => (v + min).toStringAsFixed(step < 1 ? 1 : 0),
        onChanged: custom
            ? (v) => change(((v + min) / step).round() * step)
            : null,
      );
      if (widget.embedded) {
        return MioSettingsField(
          palette: widget.palette,
          backgroundEffect: narrow,
          title: bg(key),
          description: description,
          child: narrow
              ? SizedBox(
                  width: 200,
                  child: Opacity(opacity: custom ? 1 : .45, child: control),
                )
              : control,
        );
      }
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [Text(bg(key)), Text(description), control],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (!widget.embedded) ...[
          const SizedBox(height: 24),
          Text(
            bg('title'),
            style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 12),
        ],
        if (narrow && wideScreen)
          Text(
            bg('title'),
            style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
          ),
        if (narrow) ...[
          MioSettingsField(
            palette: widget.palette,
            backgroundEffect: narrow,
            inline: true,
            title: bg('presetMode'),
            description: bg('presetModeDesc'),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: MioSettingsSelect(
                palette: widget.palette,
                label: bg('presetMode'),
                compact: true,
                value: value.preset,
                options: [
                  for (final preset in [
                    'auto',
                    'performance',
                    'quality',
                    'custom',
                  ])
                    (
                      value: preset,
                      label: bg(
                        'preset${preset[0].toUpperCase()}${preset.substring(1)}',
                      ),
                    ),
                ],
                onChanged: editable
                    ? (v) => unawaited(_saveBackground(preset: v))
                    : null,
              ),
            ),
          ),
          toggle(
            'enableBgEffect',
            value.enabled,
            editable,
            () =>
                _saveBackground(next: _backgroundNext(enabled: !value.enabled)),
          ),
        ] else ...[
          Text(bg('presetModeDesc')),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            children: [
              for (final preset in ['auto', 'performance', 'quality', 'custom'])
                _button(
                  '${value.preset == preset ? '✓ ' : ''}${bg('preset${preset[0].toUpperCase()}${preset.substring(1)}')}',
                  editable ? () => _saveBackground(preset: preset) : null,
                ),
            ],
          ),
        ],
        toggle(
          'audioResponse',
          value.audioResponse,
          editable && value.enabled,
          () => _saveBackground(
            next: _backgroundNext(audioResponse: !value.audioResponse),
          ),
        ),
        if (!widget.embedded) Text(bg('audioResponseDesc')),
        slider(
          'effectIntensity',
          bg('renderScaleDesc')
                  .replaceAll('{value}', value.renderScale.toStringAsFixed(1)) +
              (narrow
                  ? ' · ${bg(value.renderScale <= .3
                        ? 'intensityLow'
                        : value.renderScale <= .6
                        ? 'intensityMedium'
                        : 'intensityHigh')}'
                  : ''),
          value.renderScale,
          .1,
          1,
          .1,
          (v) => _saveBackground(next: _backgroundNext(renderScale: v)),
        ),
        slider(
          'flowSpeed',
          bg('flowSpeedDesc')
              .replaceAll('{value}', value.flowSpeed.toStringAsFixed(1)),
          value.flowSpeed,
          .1,
          5,
          .1,
          (v) => _saveBackground(next: _backgroundNext(flowSpeed: v)),
        ),
        toggle(
          'staticMode',
          value.staticMode,
          custom,
          () => _saveBackground(
            next: _backgroundNext(staticMode: !value.staticMode),
          ),
        ),
        if (!widget.embedded) Text(bg('staticModeDesc')),
        slider(
          'fpsLimit',
          bg('fpsLimitDesc').replaceAll('{value}', '${value.fps}'),
          value.fps.toDouble(),
          15,
          60,
          1,
          (v) => _saveBackground(next: _backgroundNext(fps: v.round())),
        ),
        if (narrow)
          MioSettingsField(
            palette: widget.palette,
            backgroundEffect: narrow,
            inline: true,
            title: bg('resetConfig'),
            description: bg('resetConfigDesc'),
            child: _button(
              bg('resetConfig'),
              editable ? () => _saveBackground(reset: true) : null,
            ),
          )
        else ...[
          _button(
            bg('resetConfig'),
            editable ? () => _saveBackground(reset: true) : null,
          ),
          Text(bg('resetConfigDesc')),
        ],
        if (value.renderScale > .7 && value.fps > 45)
          Text(bg('performanceWarning')),
      ],
    );
  }

  @override
  void didUpdateWidget(VisualSettingsPage old) {
    super.didUpdateWidget(old);
    if (!_live) {
      _release();
    } else if (!old.active || !old.ready) {
      unawaited(_open());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (!_foreground) {
      _release();
    } else if (_live) {
      unawaited(_open());
    }
  }

  @override
  void dispose() {
    _release();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  api.VisualSettings _next({
    bool? background,
    bool? coverTheme,
    bool? immersiveLyricColor,
    bool? coverLayout,
    bool? showLeftPanel,
    bool? autoHide,
    bool? pauseTransition,
    bool? blur,
    bool? spectrum,
    bool? jump,
    String? mode,
  }) => api.VisualSettings(
    background: background ?? _value!.background,
    coverDynamicTheme: coverTheme ?? _value!.coverDynamicTheme,
    immersiveLyricColor: immersiveLyricColor ?? _value!.immersiveLyricColor,
    coverLayout: coverLayout ?? _value!.coverLayout,
    showLeftPanel: showLeftPanel ?? _value!.showLeftPanel,
    autoHideBottom: autoHide ?? _value!.autoHideBottom,
    pauseTransition: pauseTransition ?? _value!.pauseTransition,
    blurLyrics: blur ?? _value!.blurLyrics,
    spectrum: spectrum ?? _value!.spectrum,
    jumpLyrics: jump ?? _value!.jumpLyrics,
    mode: mode ?? _value!.mode,
  );
  Widget _button(String label, VoidCallback? action) =>
      widget.embedded &&
          MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width)
      ? MioSettingsButton(
          palette: widget.palette,
          label: label,
          onPressed: action,
        )
      : MioButton(
          palette: widget.palette,
          label: label,
          onPressed: action,
          child: Text(label),
        );
  Widget _toggle(String key, bool selected, VoidCallback action) => _button(
    '${selected ? '✓' : '○'} ${t(key)}',
    _live && !_busy ? action : null,
  );
  @override
  Widget build(BuildContext context) {
    final value = _value, policy = MioVisualPolicy.of(context);
    if (widget.settingsSection != null) {
      final p = widget.palette;
      Widget field(
        String label,
        String description,
        bool value,
        VoidCallback action,
      ) => MioSettingsField(
        palette: p,
        title: p.text(label),
        appearance:
            widget.settingsSection == VisualSettingsSection.appearance &&
            !MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width),
        description: p.text(description),
        child: MioSettingsSwitch(
          palette: p,
          label: p.text(label),
          value: value,
          onPressed: _live && !_busy ? action : null,
        ),
      );
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_busy && value == null) Text(t('working')),
          if (_error != null) ...[
            Text(
              t(_error!),
              style: TextStyle(color: p.color('--td-error-color')),
            ),
            _button(t('reload'), _live && !_busy ? _open : null),
          ],
          if (value != null) ...[
            if (widget.settingsSection == VisualSettingsSection.appearance)
              field(
                'settings.appearance.coverDynamicTheme',
                'settings.appearance.coverDynamicThemeDesc',
                value.coverDynamicTheme,
                () => unawaited(
                  _save(_next(coverTheme: !value.coverDynamicTheme)),
                ),
              ),
            if (widget.settingsSection == VisualSettingsSection.background &&
                _background != null)
              _backgroundControls(_background!),
            if (widget.settingsSection ==
                VisualSettingsSection.performance) ...[
              field(
                'settings.playback.jumpLyric',
                'settings.playback.jumpLyricDesc',
                value.jumpLyrics,
                () => unawaited(_save(_next(jump: !value.jumpLyrics))),
              ),
              field(
                'settings.playback.bgAnimation',
                'settings.playback.bgAnimationDesc',
                value.background,
                () => unawaited(_save(_next(background: !value.background))),
              ),
              field(
                'settings.playback.audioVisualizer',
                'settings.playback.audioVisualizerDesc',
                value.spectrum,
                () => unawaited(_save(_next(spectrum: !value.spectrum))),
              ),
              MioSettingsField(
                palette: p,
                title: t('mode'),
                description: t('autoHint'),
                child: Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    for (final mode in ['auto', 'reduced', 'full'])
                      _button(
                        '${value.mode == mode ? '✓ ' : ''}${t(mode)}',
                        _live && !_busy ? () => _save(_next(mode: mode)) : null,
                      ),
                  ],
                ),
              ),
            ],
          ],
        ],
      );
    }
    if (widget.playerPanel) {
      return PlayerStyleControls(
        palette: widget.palette,
        value: value ?? _panelValue,
        busy: _busy || !_live || value == null,
        error: _error == null ? null : t(_error!),
        onReload: _live && !_busy ? _open : null,
        onCover: (cover) => _save(_next(coverLayout: cover)),
        onLeft: () => _save(_next(showLeftPanel: !value!.showLeftPanel)),
        onColor: () =>
            _save(_next(immersiveLyricColor: !value!.immersiveLyricColor)),
        onBlur: () => _save(_next(blur: !value!.blurLyrics)),
        onSpectrum: () => _save(_next(spectrum: !value!.spectrum)),
        onAutoHide: () => _save(_next(autoHide: !value!.autoHideBottom)),
        onPause: () => _save(_next(pauseTransition: !value!.pauseTransition)),
      );
    }
    return ListView(
      shrinkWrap: widget.embedded,
      physics: widget.embedded ? const NeverScrollableScrollPhysics() : null,
      padding: widget.embedded ? EdgeInsets.zero : const EdgeInsets.all(24),
      children: [
        if (!widget.embedded)
          Text(
            t('title'),
            style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
          ),
        const SizedBox(height: 16),
        Text(t('intro')),
        if (_busy) Text(t('working')),
        if (_error != null) ...[
          Text(t(_error!)),
          _button(t('reload'), _live && !_busy ? _open : null),
        ],
        if (value != null) ...[
          const SizedBox(height: 16),
          Text(widget.palette.text('play.playerStyle')),
          Wrap(
            spacing: 8,
            children: [
              for (final cover in [false, true])
                _button(
                  '${value.coverLayout == cover ? '✓' : '○'} ${widget.palette.text(cover ? 'play.immersiveCover' : 'play.classicVinyl')}',
                  _live && !_busy
                      ? () => _save(_next(coverLayout: cover))
                      : null,
                ),
            ],
          ),
          _button(
            '${value.showLeftPanel ? '✓' : '○'} ${widget.palette.text('play.showLeftPanel')}',
            _live && !_busy
                ? () => _save(_next(showLeftPanel: !value.showLeftPanel))
                : null,
          ),
          _button(
            '${value.immersiveLyricColor ? '✓' : '○'} ${widget.palette.text('play.immersiveLyricColor')}',
            _live && !_busy
                ? () => _save(
                    _next(immersiveLyricColor: !value.immersiveLyricColor),
                  )
                : null,
          ),
          _button(
            '${value.coverDynamicTheme ? '✓' : '○'} ${widget.palette.text('settings.appearance.coverDynamicTheme')}',
            _live && !_busy
                ? () => _save(_next(coverTheme: !value.coverDynamicTheme))
                : null,
          ),
          Text(
            widget.palette.text('settings.appearance.coverDynamicThemeDesc'),
          ),
          _toggle(
            'background',
            value.background,
            () => _save(_next(background: !value.background)),
          ),
          _toggle(
            'blur',
            value.blurLyrics,
            () => _save(_next(blur: !value.blurLyrics)),
          ),
          _toggle(
            'jump',
            value.jumpLyrics,
            () => _save(_next(jump: !value.jumpLyrics)),
          ),
          _toggle(
            'spectrum',
            value.spectrum,
            () => _save(_next(spectrum: !value.spectrum)),
          ),
          const SizedBox(height: 16),
          Text(t('mode')),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            children: [
              for (final mode in ['auto', 'reduced', 'full'])
                _button(
                  '${value.mode == mode ? '✓ ' : ''}${t(mode)}',
                  _live && !_busy ? () => _save(_next(mode: mode)) : null,
                ),
            ],
          ),
          const SizedBox(height: 12),
          Text(t('autoHint')),
          Text(t(policy.reduced ? 'reducedHint' : 'fullHint')),
          if (policy.slow) Text(t('slowHint')),
          if (_background != null) _backgroundControls(_background!),
        ],
      ],
    );
  }
}

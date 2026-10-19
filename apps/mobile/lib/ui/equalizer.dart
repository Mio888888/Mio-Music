import 'settings_layout.dart';
import 'device.dart';

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import '../core_controller.dart';
import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'eq_response.dart';

class EqualizerView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final double viewportWidth;
  final bool embedded;

  /// Wide embedded only: drawn on the same row as the on/off and reset controls.
  final String? sectionTitle;
  const EqualizerView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.embedded = false,
    this.sectionTitle,
  });
  @override
  State<EqualizerView> createState() => _EqualizerViewState();
}

class _EqualizerViewState extends State<EqualizerView>
    with WidgetsBindingObserver {
  api.EqualizerSession? _session;
  api.EqSnapshot? _value;
  final _name = TextEditingController();
  final _nameFocus = FocusNode();
  final _autoEq = TextEditingController();
  final _autoEqFocus = FocusNode();
  EqDocumentSelection? _document;
  Completer<void>? _foregroundWaiter;
  String? _pendingConfig, _message;
  bool _showTextImport = false;
  RawDialogRoute<String>? _presetRoute;
  int? _expanded;
  String? _deleteId;
  bool _showPresets = false;
  bool _loading = false, _busy = false, _foreground = true;
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
    final route = _presetRoute;
    _presetRoute = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
    }
    final session = _session;
    _session = null;
    _value = null;
    _deleteId = null;
    _showPresets = false;
    _name.clear();
    _autoEq.clear();
    _pendingConfig = null;
    _message = null;
    _showTextImport = false;
    unawaited(_document?.cancel());
    _document = null;
    _foregroundWaiter?.complete();
    _foregroundWaiter = null;
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
  void didUpdateWidget(EqualizerView old) {
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
      _foregroundWaiter?.complete();
      _foregroundWaiter = null;
      if (_live && _document == null && !_busy) unawaited(_open());
    } else if (state == AppLifecycleState.paused) {
      _foreground = false;
      // System document activities cover the app. Retain only this original
      // operation and defer processing its result until the app resumes.
      if (_document == null) _invalidate();
    } else if (state == AppLifecycleState.detached) {
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
    api.EqualizerSession? candidate;
    try {
      candidate = await api.beginEqualizer();
      final value = await candidate.state();
      if (!_live || epoch != _epoch) return;
      _session = candidate;
      candidate = null;
      setState(() {
        _value = value;
        _loading = false;
      });
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(() {
          _loading = false;
          _error = p.english ? 'Unable to load equalizer.' : '无法读取均衡器。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _save(
    Future<api.EqSnapshot> Function(api.EqualizerSession, int) edit,
  ) async {
    final session = _session;
    final expected = _value;
    if (!_live || _busy || _loading || session == null || expected == null) {
      return;
    }
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    api.EqSnapshot? saved;
    await widget.core.command(() async {
      saved = await edit(session, expected.revision);
    });
    if (!_live || epoch != _epoch || !identical(session, _session)) return;
    setState(() {
      _busy = false;
      if (saved != null) {
        _value = saved;
        _pendingConfig = null;
        _autoEq.clear();
        _showTextImport = false;
        _message = p.english ? 'Equalizer saved.' : '均衡器已保存。';
        _deleteId = null;
        _name.clear();
      } else {
        _release();
        _error = p.english
            ? 'Save failed or settings changed. Reload before trying again.'
            : '保存失败或设置已变更，请重新加载后重试。';
      }
    });
  }

  void _submitPreset() {
    if (!_canEdit) return;
    final name = _name.text;
    final text = _autoEq.text;
    final importing = _showTextImport;
    unawaited(
      _save(
        (s, r) => importing
            ? s.importText(revision: r, name: name, text: text)
            : s.savePreset(revision: r, name: name),
      ),
    );
  }

  Future<void> _savePresetDialog() async {
    if (!_canEdit || _presetRoute != null) return;
    final epoch = _epoch;
    _name.clear();
    final route = RawDialogRoute<String>(
      barrierDismissible: true,
      barrierLabel: p.text('common.cancel'),
      barrierColor: p.color('--td-mask-active'),
      pageBuilder: (dialogContext, _, _) => LayoutBuilder(
        builder: (_, box) => Padding(
          padding: EdgeInsets.fromLTRB(16, box.maxHeight * .2, 16, 16),
          child: Align(
            alignment: Alignment.topCenter,
            child: SingleChildScrollView(
              child: Container(
                width: math.min(420, box.maxWidth - 32),
                padding: const EdgeInsets.all(32),
                decoration: BoxDecoration(
                  color: p
                      .color('--td-bg-color-container')
                      .withValues(alpha: 1),
                  border: Border.all(
                    color: p.color('--td-border-level-1-color'),
                  ),
                  borderRadius: BorderRadius.circular(22),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(
                      t('saveNewPresetTitle'),
                      style: const TextStyle(
                        fontSize: 16,
                        height: 1.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 16),
                    MioInput(
                      palette: p,
                      controller: _name,
                      focusNode: _nameFocus,
                      placeholder: t('presetNamePlaceholder'),
                      height: 32,
                      radius: 6,
                      textInputAction: TextInputAction.done,
                      maxLength: 100,
                      onSubmitted: (name) {
                        if (name.trim().isNotEmpty) {
                          Navigator.pop(dialogContext, name.trim());
                        }
                      },
                    ),
                    const SizedBox(height: 24),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      children: [
                        MioButton(
                          palette: p,
                          label: p.text('common.cancel'),
                          padding: const EdgeInsets.symmetric(
                            horizontal: 10,
                            vertical: 6,
                          ),
                          onPressed: () => Navigator.pop(dialogContext),
                          child: Text(p.text('common.cancel')),
                        ),
                        const SizedBox(width: 8),
                        ValueListenableBuilder<TextEditingValue>(
                          valueListenable: _name,
                          builder: (_, value, _) => MioButton(
                            palette: p,
                            label: p.text('common.confirm'),
                            padding: const EdgeInsets.symmetric(
                              horizontal: 10,
                              vertical: 6,
                            ),
                            background: p.brand,
                            onPressed: value.text.trim().isEmpty
                                ? null
                                : () => Navigator.pop(
                                    dialogContext,
                                    value.text.trim(),
                                  ),
                            child: Text(p.text('common.confirm')),
                          ),
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
    _presetRoute = route;
    final name = await Navigator.of(context).push(route);
    if (identical(_presetRoute, route)) _presetRoute = null;
    if (!_live || epoch != _epoch || name == null) return;
    await _save((s, r) => s.savePreset(revision: r, name: name));
  }

  Future<void> _documentTransfer({
    required bool exporting,
    bool autoEq = false,
  }) async {
    if (!_canEdit) return;
    final session = _session!;
    final revision = _value!.revision;
    final epoch = _epoch;
    final selection = EqDocumentSelection();
    _document = selection;
    setState(() {
      _busy = true;
      _error = null;
      _message = null;
    });
    bool current() =>
        mounted &&
        widget.active &&
        widget.ready &&
        epoch == _epoch &&
        identical(session, _session) &&
        identical(selection, _document);
    Future<bool> visible() async {
      if (!current()) return false;
      if (!_foreground) {
        _foregroundWaiter ??= Completer<void>();
        await _foregroundWaiter!.future;
      }
      return current() && _live;
    }

    try {
      String? text;
      var saved = false;
      if (exporting) {
        final config = await session.exportConfig(revision: revision);
        if (!current()) return;
        saved = await selection.save(config);
      } else {
        text = await selection.read();
      }
      if (!await visible()) return;
      if (autoEq && text != null && utf8.encode(text).length > 64 * 1024) {
        throw const PluginFileException('file_too_large');
      }
      setState(() {
        if (exporting && saved) {
          _message = p.english ? 'Configuration exported.' : '配置已导出。';
        } else if (text != null) {
          if (autoEq) {
            _autoEq.text = text;
            _showTextImport = true;
          } else {
            _pendingConfig = text;
          }
        }
      });
    } catch (error) {
      if (await visible()) {
        setState(() {
          if (error is! PluginFileException) _release();
          _error = exporting
              ? (p.english
                    ? 'Export failed or was interrupted. The destination may be incomplete.'
                    : '导出失败或中断，目标文件可能不完整。')
              : (p.english
                    ? 'Unable to read the file. Use UTF-8; JSON ≤512 KiB and AutoEq ≤64 KiB.'
                    : '无法读取文件，请使用 UTF-8；JSON 上限 512 KiB，AutoEq 上限 64 KiB。');
        });
      }
    } finally {
      unawaited(selection.cancel());
      if (identical(_document, selection)) _document = null;
      if (mounted && epoch == _epoch) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    _invalidate();
    WidgetsBinding.instance.removeObserver(this);
    _name.dispose();
    _nameFocus.dispose();
    _autoEq.dispose();
    _autoEqFocus.dispose();
    super.dispose();
  }

  String t(String key) => p.text('settings.equalizer.$key');
  bool get _compact => !MioDeviceScope.wide(context, widget.viewportWidth);
  bool get _screenWide => MioDeviceScope.wide(context, widget.viewportWidth);
  bool get _readyToEdit => _live && !_busy && !_loading && _value != null;
  bool get _canEdit => _readyToEdit && _pendingConfig == null;
  String _presetName(api.EqPreset preset) =>
      preset.localeKey == null ? preset.name : p.text(preset.localeKey!);
  Widget _button(
    String label,
    VoidCallback action, {
    bool selected = false,
    bool? toggled,
    bool confirmation = false,
  }) => widget.embedded && _screenWide
      ? Semantics(
          selected: selected,
          toggled: toggled,
          child: MioSettingsButton(
            palette: p,
            label: label,
            outline: label != t('reset'),
            primary: selected || label == t('saveAsPreset'),
            onPressed: (_canEdit || (confirmation && _readyToEdit))
                ? action
                : null,
          ),
        )
      : Semantics(
          selected: selected,
          toggled: toggled,
          child: DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(6),
              border: _compact && label == t('saveAsPreset')
                  ? Border.all(color: p.brand)
                  : null,
            ),
            child: MioButton(
              palette: p,
              label: label,
              radius: _compact ? 6 : 4,
              insetFocusBorder: !_compact,
              padding: _compact
                  ? const EdgeInsets.symmetric(horizontal: 10, vertical: 6)
                  : const EdgeInsets.all(8),
              background:
                  _compact &&
                      (label == t('saveAsPreset') || label == t('reset'))
                  ? const Color(0x00000000)
                  : selected
                  ? p.brand.withValues(alpha: .2)
                  : _compact
                  ? p.color('--td-bg-color-specialcomponent')
                  : p.surface,
              onPressed: (_canEdit || (confirmation && _readyToEdit))
                  ? action
                  : null,
              child: ConstrainedBox(
                constraints: BoxConstraints(minHeight: _compact ? 20 : 28),
                child: Center(
                  widthFactor: 1,
                  child: Text(
                    label,
                    style: _compact
                        ? TextStyle(
                            fontSize: 13,
                            color: label == t('saveAsPreset') ? p.brand : null,
                          )
                        : null,
                  ),
                ),
              ),
            ),
          ),
        );
  Widget _range(
    String label,
    double value,
    double min,
    double max,
    double step,
    String Function(double) format,
    void Function(double) apply,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text('$label: ${format(value)}'),
      MioSeekBar(
        palette: p,
        height: 44,
        value: value - min,
        maximum: max - min,
        step: step,
        semanticLabel: label,
        semanticValue: (v) => format(v + min),
        onChanged: _canEdit
            ? (v) => apply((min + (v / step).round() * step).clamp(min, max))
            : null,
      ),
    ],
  );
  Future<void> _band(
    int index, {
    double? frequency,
    double? gain,
    double? q,
    api.EqFilter? kind,
    bool? enabled,
  }) async {
    final old = _value?.bands[index];
    if (old == null) return;
    final type = kind ?? old.kind;
    final hasGain = type.index < 3;
    await _save(
      (s, r) => s.setBand(
        revision: r,
        index: index,
        value: api.EqBand(
          frequency: frequency ?? old.frequency,
          gain: hasGain ? (gain ?? old.gain) : 0,
          q:
              q ??
              (kind == null
                  ? old.q
                  : (type.index == 1 || type.index == 2 ? .7 : 1.414)),
          kind: type,
          enabled: enabled ?? old.enabled,
        ),
      ),
    );
  }

  static const _types = [
    'typePeak',
    'typeLowShelf',
    'typeHighShelf',
    'typeLowPass',
    'typeHighPass',
    'typeNotch',
  ];

  /// Desktop selected-band panel: title and frequency, then frequency, Q and
  /// filter type rows.
  Widget _bandPanel(int index, api.EqBand band) {
    final enabled = _canEdit && (_value?.enabled ?? false);
    final tint = Color.alphaBlend(
      p.brand.withValues(alpha: .05),
      p.color('--td-bg-color-container'),
    );
    String freq(double f) => f >= 1000
        ? '${(f / 1000).toStringAsFixed(f % 1000 == 0 ? 0 : 1)}kHz'
        : '${f.toStringAsFixed(0)}Hz';
    Widget row(String label, Widget control, String value) => Row(
      children: [
        SizedBox(
          width: 64,
          child: Text(
            label,
            style: TextStyle(fontSize: 12, color: p.secondary),
          ),
        ),
        const SizedBox(width: 14),
        Expanded(child: control),
        const SizedBox(width: 14),
        SizedBox(
          width: 56,
          child: Text(
            value,
            textAlign: TextAlign.right,
            style: TextStyle(
              fontSize: 12,
              color: p.color('--td-text-color-primary'),
            ),
          ),
        ),
      ],
    );
    Widget divider() => Container(
      height: 1,
      color: Color.alphaBlend(
        p.color('--td-component-border').withValues(alpha: .6),
        p.color('--td-bg-color-container'),
      ),
    );
    Widget slider(
      double value,
      double min,
      double max,
      double step,
      String Function(double) format,
      void Function(double) apply,
      String label,
    ) => MioSeekBar(
      palette: p,
      height: 24,
      value: value - min,
      maximum: max - min,
      step: step,
      showThumb: true,
      semanticLabel: label,
      semanticValue: (v) => format(v + min),
      onChanged: enabled
          ? (v) => apply((min + (v / step).round() * step).clamp(min, max))
          : null,
    );
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 16),
      decoration: BoxDecoration(
        color: tint,
        border: Border.all(
          color: Color.alphaBlend(
            p.brand.withValues(alpha: .12),
            p.color('--td-border-level-1-color'),
          ),
        ),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.only(bottom: 10),
            decoration: BoxDecoration(
              border: Border(
                bottom: BorderSide(color: p.color('--td-border-level-1-color')),
              ),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    t('bandParams').replaceAll('{index}', '${index + 1}'),
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: p.color('--td-text-color-primary'),
                    ),
                  ),
                ),
                Text(
                  freq(band.frequency),
                  style: TextStyle(fontSize: 12, color: p.secondary),
                ),
              ],
            ),
          ),
          const SizedBox(height: 14),
          row(
            t('frequency'),
            slider(
              math.log(band.frequency / 20) / math.ln10,
              0,
              3,
              .01,
              (v) => freq((20 * math.pow(10, v)).toDouble()),
              (v) => unawaited(
                _band(
                  index,
                  frequency: (20 * math.pow(10, v)).toDouble().clamp(20, 20000),
                ),
              ),
              t('frequency'),
            ),
            freq(band.frequency),
          ),
          const SizedBox(height: 14),
          divider(),
          const SizedBox(height: 14),
          row(
            t('qFactor'),
            slider(
              band.q,
              .1,
              10,
              .1,
              (v) => v.toStringAsFixed(2),
              (v) => unawaited(_band(index, q: v)),
              t('qFactor'),
            ),
            band.q.toStringAsFixed(2),
          ),
          const SizedBox(height: 14),
          divider(),
          const SizedBox(height: 14),
          row(
            t('filterType'),
            MioSettingsSelect(
              palette: p,
              label: t('filterType'),
              compact: true,
              value: '${band.kind.index}',
              options: [
                for (final type in api.EqFilter.values)
                  (value: '${type.index}', label: t(_types[type.index])),
              ],
              onChanged: enabled
                  ? (v) => unawaited(
                      _band(index, kind: api.EqFilter.values[int.parse(v)]),
                    )
                  : null,
            ),
            '',
          ),
        ],
      ),
    );
  }

  Widget _bandCard(int index, api.EqBand band) {
    final label = t('bandParams').replaceAll('{index}', '${index + 1}');
    final expanded = _expanded == index;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: p.surface,
        border: Border.all(color: p.border),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              _button(
                '$label · ${band.frequency.toStringAsFixed(0)} Hz',
                () => setState(() => _expanded = expanded ? null : index),
                selected: expanded,
              ),
              _button(
                t(band.enabled ? 'on' : 'off'),
                () => unawaited(_band(index, enabled: !band.enabled)),
                toggled: band.enabled,
              ),
            ],
          ),
          if (band.kind.index < 3)
            _range(
              p.english ? 'Gain' : '增益',
              band.gain,
              -24,
              24,
              .5,
              (v) => '${v.toStringAsFixed(1)} dB',
              (v) => unawaited(_band(index, gain: v)),
            ),
          if (expanded) ...[
            _range(
              t('frequency'),
              math.log(band.frequency / 20) / math.ln10,
              0,
              3,
              .01,
              (v) => '${(20 * math.pow(10, v)).toStringAsFixed(0)} Hz',
              (v) => unawaited(
                _band(
                  index,
                  frequency: (20 * math.pow(10, v)).toDouble().clamp(20, 20000),
                ),
              ),
            ),
            _range(
              t('qFactor'),
              band.q,
              .1,
              10,
              .1,
              (v) => v.toStringAsFixed(2),
              (v) => unawaited(_band(index, q: v)),
            ),
            Text(t('filterType')),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final type in api.EqFilter.values)
                  _button(
                    t(_types[type.index]),
                    () => unawaited(_band(index, kind: type)),
                    selected: type == band.kind,
                  ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _openPresetVault() async {
    try {
      await const MethodChannel('mio/host')
          .invokeMethod<void>('openAboutLink', 'eqVault');
    } catch (_) {
      if (mounted) {
        setState(
          () => _message = p.english
              ? 'Unable to open preset library.'
              : '无法打开预设库。',
        );
      }
    }
  }

  Widget _compactPreamp(api.EqSnapshot value) => Container(
    padding: const EdgeInsets.fromLTRB(14, 12, 14, 4),
    decoration: BoxDecoration(
      color: p.surface,
      border: Border.all(color: p.border),
      borderRadius: BorderRadius.circular(8),
    ),
    child: Column(
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                t('globalGain'),
                style: const TextStyle(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            Text(
              '${value.globalGain.toStringAsFixed(1)}dB',
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
          ],
        ),
        MioSeekBar(
          palette: p,
          value: value.globalGain + 24,
          maximum: 48,
          height: 28,
          step: .5,
          showThumb: true,
          semanticLabel: t('globalGain'),
          semanticValue: (v) => '${(v - 24).toStringAsFixed(1)} dB',
          onChanged: _canEdit
              ? (v) => unawaited(
                  _save(
                    (s, r) => s.setGlobalGain(
                      revision: r,
                      gain: ((v - 24) * 2).round() / 2,
                    ),
                  ),
                )
              : null,
        ),
      ],
    ),
  );

  Widget _response(api.EqSnapshot value, bool wide) => EqResponsePlot(
    session: _session,
    active: _live && _document == null && _pendingConfig == null,
    playing: widget.core.player?.status == 'playing',
    generation: widget.core.player?.playbackGeneration ?? 0,
    value: value,
    palette: p,
    wide: wide,
    compact: !wide,
    selected: _expanded,
    onSelected: _canEdit ? (index) => setState(() => _expanded = index) : null,
  );

  Widget _presetSelect(api.EqSnapshot value) => Align(
    alignment: Alignment.centerLeft,
    child: ConstrainedBox(
      constraints: BoxConstraints(maxWidth: _screenWide ? 180 : 260),
      child: MioSettingsSelect(
        palette: p,
        label: t('selectPreset'),
        compact: true,
        outlined: _screenWide,
        value: value.currentPresetId,
        options: [
          for (final preset in value.presets)
            (value: preset.id, label: _presetName(preset)),
        ],
        onChanged: _canEdit
            ? (id) => unawaited(
                _save((s, r) => s.selectPreset(revision: r, id: id)),
              )
            : null,
      ),
    ),
  );

  Widget _compactControls(api.EqSnapshot value) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          if (_screenWide && widget.sectionTitle != null)
            Expanded(
              child: Text(
                widget.sectionTitle!,
                style: const TextStyle(
                  fontSize: 18,
                  height: 1.35,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          MioSettingsSwitch(
            palette: p,
            label: t('title'),
            value: value.enabled,
            onPressed: _canEdit
                ? () => unawaited(
                    _save(
                      (s, r) =>
                          s.setEnabled(revision: r, enabled: !value.enabled),
                    ),
                  )
                : null,
          ),
          const SizedBox(width: 8),
          Text(t(value.enabled ? 'on' : 'off')),
          const SizedBox(width: 8),
          _button(
            t('reset'),
            () => unawaited(_save((s, r) => s.reset(revision: r))),
          ),
        ],
      ),
      const SizedBox(height: 14),
      _response(value, _screenWide),
      const SizedBox(height: 14),
      LayoutBuilder(
        builder: (context, box) {
          // Preserve ten columns at phone widths; allow scrolling for large text.
          final scale = MediaQuery.textScalerOf(context).scale(10) / 10;
          final width = math.max(
            box.maxWidth,
            value.bands.length * 28.0 * scale,
          );
          return SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: SizedBox(
              width: width,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (var i = 0; i < value.bands.length; i++)
                    Expanded(
                      child: _compactBand(i, value.bands[i], value.enabled),
                    ),
                ],
              ),
            ),
          );
        },
      ),
      const SizedBox(height: 24),
      if (_expanded != null && _expanded! < value.bands.length) ...[
        _screenWide
            ? _bandPanel(_expanded!, value.bands[_expanded!])
            : _bandCard(_expanded!, value.bands[_expanded!]),
        const SizedBox(height: 16),
      ],
      if (!_screenWide) _presetSelect(value),
    ],
  );

  Widget _compactBand(int index, api.EqBand band, bool enabled) {
    final frequency = band.frequency >= 1000
        ? '${(band.frequency / 1000).toStringAsFixed(band.frequency % 1000 == 0 ? 0 : 1)}k'
        : band.frequency.toStringAsFixed(0);
    final editable = _canEdit && enabled && band.kind.index < 3;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 1, vertical: 8),
      decoration: BoxDecoration(
        color: _expanded == index ? p.brand.withValues(alpha: .12) : null,
        borderRadius: BorderRadius.circular(6),
      ),
      child: Column(
        children: [
          Opacity(
            opacity: enabled && band.kind.index < 3 ? 1 : .45,
            child: _EqVerticalGain(
              palette: p,
              value: band.gain + 24,
              height: _screenWide ? 210 : 168,
              label:
                  '${t('bandParams').replaceAll('{index}', '${index + 1}')} · ${band.frequency.toStringAsFixed(0)} Hz',
              onChanged: editable
                  ? (v) => unawaited(
                      _band(index, gain: ((v - 24) * 10).round() / 10),
                    )
                  : null,
            ),
          ),
          const SizedBox(height: 10),
          MioButton(
            palette: p,
            label:
                '${t('bandParams').replaceAll('{index}', '${index + 1}')} · $frequency Hz',
            padding: const EdgeInsets.symmetric(vertical: 4),
            onPressed: _canEdit
                ? () => setState(
                    () => _expanded = _expanded == index ? null : index,
                  )
                : null,
            child: Column(
              children: [
                Text(
                  frequency,
                  style: TextStyle(
                    fontSize: _screenWide ? 12 : 10,
                    color: p.secondary,
                  ),
                ),
                const SizedBox(height: 4),
                FittedBox(
                  fit: BoxFit.scaleDown,
                  child: Text(
                    band.kind.index < 3
                        ? '${band.gain.toStringAsFixed(1)}dB'
                        : '--',
                    style: TextStyle(
                      fontSize: _screenWide ? 10 : 9,
                      color: p.secondary,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final value = _value;
    final wide = MioDeviceScope.wide(context, widget.viewportWidth);
    // Embedded cards use the card layout (plot, vertical bands, preset
    // controls) at both widths; only the metrics differ.
    final card = widget.embedded || !wide;
    final gutter = wide ? 24.0 : (widget.viewportWidth * .04).clamp(16.0, 24.0);
    api.EqPreset? current;
    if (value != null) {
      for (final preset in value.presets) {
        if (preset.id == value.currentPresetId) current = preset;
      }
    }
    return MioSettingsBody(
      embedded: widget.embedded,
      padding: EdgeInsets.all(gutter),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!widget.embedded) ...[
            Text(
              t('title'),
              style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 20),
          ],
          if (_loading || (_busy && !card))
            Semantics(
              liveRegion: true,
              child: Text(p.english ? 'Working…' : '正在处理…'),
            ),
          if (_message != null && !card)
            Semantics(liveRegion: true, child: Text(_message!)),
          if (_error != null) ...[
            Semantics(
              liveRegion: true,
              child: Text(
                _error!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
            ),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          if (value != null) ...[
            if (card) _compactControls(value),
            if (!card)
              Wrap(
                spacing: 12,
                runSpacing: 8,
                children: [
                  if (widget.embedded)
                    MioSettingsSwitch(
                      palette: p,
                      label: t('title'),
                      value: value.enabled,
                      onPressed: _canEdit
                          ? () => unawaited(
                              _save(
                                (s, r) => s.setEnabled(
                                  revision: r,
                                  enabled: !value.enabled,
                                ),
                              ),
                            )
                          : null,
                    )
                  else
                    _button(
                      t(value.enabled ? 'on' : 'off'),
                      () => unawaited(
                        _save(
                          (s, r) => s.setEnabled(
                            revision: r,
                            enabled: !value.enabled,
                          ),
                        ),
                      ),
                      toggled: value.enabled,
                    ),
                  _button(
                    t('reset'),
                    () => unawaited(_save((s, r) => s.reset(revision: r))),
                  ),
                  _button(
                    '${t('selectPreset')}: ${current == null ? value.currentPresetId : _presetName(current)}',
                    () => setState(() => _showPresets = !_showPresets),
                    selected: _showPresets,
                  ),
                ],
              ),
            if (!card && _showPresets)
              Container(
                constraints: const BoxConstraints(maxHeight: 280),
                margin: const EdgeInsets.symmetric(vertical: 12),
                child: ListView.builder(
                  shrinkWrap: true,
                  itemCount: value.presets.length,
                  itemBuilder: (context, i) {
                    final preset = value.presets[i];
                    return Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: _button(_presetName(preset), () {
                        setState(() => _showPresets = false);
                        unawaited(
                          _save(
                            (s, r) =>
                                s.selectPreset(revision: r, id: preset.id),
                          ),
                        );
                      }, selected: preset.id == value.currentPresetId),
                    );
                  },
                ),
              ),
            const SizedBox(height: 12),
            if (current != null && !current.builtin) ...[
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _button(
                    t('savePreset'),
                    () => unawaited(
                      _save(
                        (s, r) => s.updatePreset(revision: r, id: current!.id),
                      ),
                    ),
                  ),
                  _button(
                    t('deletePreset'),
                    () => setState(() => _deleteId = current!.id),
                  ),
                ],
              ),
              if (_deleteId == current.id) ...[
                const SizedBox(height: 8),
                Text(
                  t('deletePresetBody')
                      .replaceAll('{name}', _presetName(current)),
                ),
                Wrap(
                  spacing: 8,
                  children: [
                    _button(
                      p.text('common.confirm'),
                      () => unawaited(
                        _save(
                          (s, r) => s.deletePreset(revision: r, id: _deleteId!),
                        ),
                      ),
                    ),
                    _button(
                      p.text('common.cancel'),
                      () => setState(() => _deleteId = null),
                    ),
                  ],
                ),
              ],
              const SizedBox(height: 12),
            ],
            if (card && _screenWide) ...[
              _presetSelect(value),
              const SizedBox(height: 12),
            ],
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (card)
                  _button(
                    t('saveAsPreset'),
                    () => unawaited(_savePresetDialog()),
                  ),
                if (card)
                  _button(
                    t('exportConfig'),
                    () => unawaited(_documentTransfer(exporting: true)),
                  ),
                _button(
                  t('importConfig'),
                  () => unawaited(_documentTransfer(exporting: false)),
                ),
                if (!card)
                  _button(
                    t('exportConfig'),
                    () => unawaited(_documentTransfer(exporting: true)),
                  ),
                if (card)
                  _button(
                    t('downloadPresets'),
                    () => unawaited(_openPresetVault()),
                  ),
                if (!card || _showTextImport)
                  _button(
                    t('textImportPresetTitle'),
                    () => setState(() => _showTextImport = !_showTextImport),
                    selected: _showTextImport,
                  ),
              ],
            ),
            if (_pendingConfig != null) ...[
              const SizedBox(height: 12),
              Text(
                p.english
                    ? 'Replace the current equalizer and all user presets with this file?'
                    : '使用此文件替换当前均衡器配置及全部用户预设？',
              ),
              Wrap(
                spacing: 8,
                children: [
                  _button(p.text('common.confirm'), () {
                    final text = _pendingConfig!;
                    unawaited(
                      _save((s, r) => s.importConfig(revision: r, text: text)),
                    );
                  }, confirmation: true),
                  _button(
                    p.text('common.cancel'),
                    () => setState(() => _pendingConfig = null),
                    confirmation: true,
                  ),
                ],
              ),
            ],
            if (_showTextImport) ...[
              const SizedBox(height: 12),
              MioInput(
                palette: p,
                controller: _autoEq,
                focusNode: _autoEqFocus,
                placeholder: 'Preamp: -6 dB\nFilter 1: ON PK Fc 100 Hz Gain 3 dB Q 1.414',
                maxLength: 65536,
                maxLines: 6,
                height: 160,
                readOnly: !_canEdit,
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _button(
                    p.english ? 'Read AutoEq file' : '读取 AutoEq 文件',
                    () => unawaited(
                      _documentTransfer(exporting: false, autoEq: true),
                    ),
                  ),
                  _button(t('textImportPresetTitle'), _submitPreset),
                ],
              ),
              const SizedBox(height: 8),
            ],
            const SizedBox(height: 12),
            if (!card || _showTextImport) ...[
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 480),
                child: MioInput(
                  palette: p,
                  controller: _name,
                  focusNode: _nameFocus,
                  maxLength: 100,
                  height: 44,
                  readOnly: !_canEdit,
                  placeholder: t(
                    _showTextImport
                        ? 'textImportPresetNamePlaceholder'
                        : 'presetNamePlaceholder',
                  ),
                  onSubmitted: (_) => _submitPreset(),
                ),
              ),
              const SizedBox(height: 8),
              _button(
                t(
                  _showTextImport
                      ? 'textImportPresetTitle'
                      : 'saveNewPresetTitle',
                ),
                _submitPreset,
              ),
            ],
            if (card && _message != null)
              Semantics(liveRegion: true, child: Text(_message!)),
            SizedBox(height: !card ? 20 : 12),
            if (!card) _response(value, wide),
            if (!card) const SizedBox(height: 16),
            if (card)
              _compactPreamp(value)
            else
              _range(
                t('globalGain'),
                value.globalGain,
                -24,
                24,
                .5,
                (v) => '${v.toStringAsFixed(1)} dB',
                (v) => unawaited(
                  _save((s, r) => s.setGlobalGain(revision: r, gain: v)),
                ),
              ),
            if (!card)
              LayoutBuilder(
                builder: (context, box) {
                  // Keep each band usable with large text and narrow split windows.
                  final columns = wide && box.maxWidth >= 700 ? 2 : 1;
                  final width = (box.maxWidth - (columns - 1) * 12) / columns;
                  return Wrap(
                    spacing: 12,
                    runSpacing: 12,
                    children: [
                      for (var i = 0; i < value.bands.length; i++)
                        SizedBox(
                          width: width,
                          child: _bandCard(i, value.bands[i]),
                        ),
                    ],
                  );
                },
              ),
          ],
        ],
      ),
    );
  }
}

/// Vertical gestures use the screen's vertical axis; rotating a horizontal
/// drag recognizer alone would still compete with horizontal gestures.
class _EqVerticalGain extends StatefulWidget {
  final MioPalette palette;
  final double value;
  final String label;
  final double height;
  final ValueChanged<double>? onChanged;
  const _EqVerticalGain({
    required this.palette,
    required this.value,
    required this.label,
    this.height = 168,
    this.onChanged,
  });
  @override
  State<_EqVerticalGain> createState() => _EqVerticalGainState();
}

class _EqVerticalGainState extends State<_EqVerticalGain> {
  double? _draft;
  double get _value => (_draft ?? widget.value).clamp(0, 48);
  double _position(double y) => ((1 - y / widget.height) * 48).clamp(0, 48);
  void _step(double delta) =>
      widget.onChanged?.call((_value + delta).clamp(0, 48));
  @override
  void didUpdateWidget(_EqVerticalGain old) {
    super.didUpdateWidget(old);
    if (widget.onChanged == null || old.value != widget.value) _draft = null;
  }

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onChanged != null;
    return Semantics(
      slider: true,
      enabled: enabled,
      label: widget.label,
      value: '${(_value - 24).toStringAsFixed(1)} dB',
      increasedValue:
          '${(_value - 24 + .1).clamp(-24.0, 24.0).toStringAsFixed(1)} dB',
      decreasedValue:
          '${(_value - 24 - .1).clamp(-24.0, 24.0).toStringAsFixed(1)} dB',
      onIncrease: enabled ? () => _step(.1) : null,
      onDecrease: enabled ? () => _step(-.1) : null,
      child: CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.arrowUp): () => _step(.1),
          const SingleActivator(LogicalKeyboardKey.arrowDown): () => _step(-.1),
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTapUp: enabled
              ? (e) => widget.onChanged!(_position(e.localPosition.dy))
              : null,
          onVerticalDragStart: enabled
              ? (e) => setState(() => _draft = _position(e.localPosition.dy))
              : null,
          onVerticalDragUpdate: enabled
              ? (e) => setState(() => _draft = _position(e.localPosition.dy))
              : null,
          onVerticalDragEnd: enabled
              ? (_) {
                  final value = _value;
                  setState(() => _draft = null);
                  widget.onChanged!(value);
                }
              : null,
          onVerticalDragCancel: () => setState(() => _draft = null),
          child: ExcludeSemantics(
            child: IgnorePointer(
              child: SizedBox(
                height: widget.height,
                child: RotatedBox(
                  quarterTurns: 3,
                  child: MioSeekBar(
                    palette: widget.palette,
                    height: 24,
                    value: _value,
                    maximum: 48,
                    step: .1,
                    showThumb: true,
                    hollowThumb: !enabled,
                    onChanged: widget.onChanged,
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

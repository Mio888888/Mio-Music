import 'settings_layout.dart';
import 'device.dart';

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

class AudioEffectsView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final double viewportWidth;
  final bool embedded;
  final VoidCallback? onEqualizer;

  /// Wide embedded only: drawn on the same row as the reset button.
  final String? sectionTitle;
  const AudioEffectsView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.embedded = false,
    this.onEqualizer,
    this.sectionTitle,
  });
  @override
  State<AudioEffectsView> createState() => _AudioEffectsViewState();
}

class _AudioEffectsViewState extends State<AudioEffectsView>
    with WidgetsBindingObserver {
  api.AudioEffectsSession? _session;
  api.BalanceSettings? _value;
  bool _loading = false, _busy = false, _foreground = true;
  String? _error;
  int _epoch = 0;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;

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
  void didUpdateWidget(AudioEffectsView old) {
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
      if (_live) unawaited(_open());
    } else {
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
    api.AudioEffectsSession? candidate;
    try {
      candidate = await api.beginAudioEffects();
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
          _error = p.english ? 'Unable to load audio effects.' : '无法读取音效设置。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _save(api.BalanceSettings next) async {
    final session = _session;
    final expected = _value;
    if (!_live || _busy || _loading || session == null || expected == null) {
      return;
    }
    final epoch = _epoch;
    setState(() => _busy = true);
    api.BalanceSettings? saved;
    await widget.core.command(() async {
      saved = await session.save(expected: expected, next: next);
    });
    if (!_live || epoch != _epoch || !identical(session, _session)) return;
    setState(() {
      _busy = false;
      if (saved != null) {
        _value = saved;
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
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  String _position(double balance) {
    if (balance.abs() < .001) {
      return p.text('settings.audioEffect.balanceCenter');
    }
    return p
        .text(
          balance < 0
              ? 'settings.audioEffect.balanceLeft'
              : 'settings.audioEffect.balanceRight',
        )
        .replaceAll('{percent}', (balance.abs() * 100).round().toString());
  }

  @override
  Widget build(BuildContext context) {
    final value = _value;
    final enabled = _live && !_busy && !_loading && value != null;
    final wide = MioDeviceScope.wide(context, widget.viewportWidth);
    final gutter = wide ? 24.0 : (widget.viewportWidth * .04).clamp(16.0, 24.0);
    Widget badge(String text, bool active) => Container(
      width: 32,
      height: 32,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: p.surface,
        shape: BoxShape.circle,
        border: Border.all(color: active ? p.brand : p.border),
      ),
      child: Text(
        text,
        style: TextStyle(
          fontSize: 13,
          fontWeight: FontWeight.w600,
          color: active ? p.brand : p.secondary,
        ),
      ),
    );
    if (widget.embedded) {
      String t(String key) => p.text('settings.audioEffect.$key');
      // Desktop: side by side cards, 16px padding and gaps, a ruled header.
      Widget divider() => wide
          ? const SizedBox(height: 16)
          : Padding(
              padding: const EdgeInsets.symmetric(vertical: 14),
              child: Container(height: 1, color: p.border),
            );
      Widget head(Widget row) => wide
          ? Container(
              padding: const EdgeInsets.only(bottom: 8),
              decoration: BoxDecoration(
                border: Border(
                  bottom: BorderSide(color: p.color('--td-component-stroke')),
                ),
              ),
              child: row,
            )
          : row;
      Widget card(List<Widget> children) => Container(
        margin: EdgeInsets.only(top: wide ? 0 : 12),
        padding: EdgeInsets.all(wide ? 16 : 14),
        decoration: BoxDecoration(
          color: wide ? p.color('--td-bg-color-container') : null,
          border: Border.all(
            color: wide ? p.color('--td-component-border') : p.border,
          ),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: children,
        ),
      );
      final Widget surroundCard = card([
        head(
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      t('surround'),
                      style: const TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    if (!wide) ...[
                      const SizedBox(height: 8),
                      Text(
                        p.english ? 'Not available' : '暂不可用',
                        style: TextStyle(fontSize: 12, color: p.secondary),
                      ),
                    ],
                  ],
                ),
              ),
              MioSettingsSwitch(palette: p, label: t('surround'), value: false),
            ],
          ),
        ),
        divider(),
        Text(
          t('surroundSimulation'),
          style: TextStyle(fontSize: 13, color: p.secondary),
        ),
        const SizedBox(height: 10),
        Container(
          decoration: BoxDecoration(
            border: Border.all(color: p.border),
            borderRadius: BorderRadius.circular(4),
          ),
          child: Row(
            children: [
              for (final key in [
                'surroundOff',
                'surroundSmall',
                'surroundMedium',
                'surroundLarge',
              ])
                Expanded(
                  child: Container(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    color: key == 'surroundOff'
                        ? p.brand.withValues(alpha: .18)
                        : null,
                    child: Text(
                      t(key),
                      textAlign: TextAlign.center,
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        Text(
          p.english
              ? 'Surround processing is not implemented in the current audio engine.'
              : '当前音频引擎尚未实现环绕处理。',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 12, color: p.secondary),
        ),
      ]);
      final Widget balanceCard = value == null
          ? const SizedBox.shrink()
          : card([
              head(
                Row(
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            t('balance'),
                            style: const TextStyle(
                              fontSize: 15,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          if (!wide) ...[
                            const SizedBox(height: 8),
                            Text(
                              _position(value.balance),
                              style: TextStyle(
                                fontSize: 12,
                                color: p.secondary,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    MioSettingsSwitch(
                      palette: p,
                      label: t('balance'),
                      value: value.enabled,
                      onPressed: enabled
                          ? () => unawaited(
                              _save(
                                api.BalanceSettings(
                                  enabled: !value.enabled,
                                  balance: value.balance,
                                ),
                              ),
                            )
                          : null,
                    ),
                  ],
                ),
              ),
              divider(),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    'Left',
                    style: TextStyle(fontSize: 12, color: p.secondary),
                  ),
                  Text(
                    'Right',
                    style: TextStyle(fontSize: 12, color: p.secondary),
                  ),
                ],
              ),
              MioSeekBar(
                palette: p,
                value: value.balance + 1,
                maximum: 2,
                height: 28,
                step: .02,
                showThumb: true,
                semanticLabel: t('balance'),
                semanticValue: (v) => _position(v - 1),
                onChanged: enabled && value.enabled
                    ? (v) => unawaited(
                        _save(
                          api.BalanceSettings(
                            enabled: true,
                            balance: (v - 1).clamp(-1, 1),
                          ),
                        ),
                      )
                    : null,
              ),
              const SizedBox(height: 12),
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: wide
                      ? p.color('--td-bg-color-secondarycontainer')
                      : p.color('--td-bg-color-specialcomponent'),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceAround,
                  children: [
                    badge('L', value.enabled && value.balance <= 0),
                    Text(
                      wide ? 'Center' : _position(value.balance),
                      style: TextStyle(
                        fontSize: 12,
                        color: wide
                            ? p.color('--td-text-color-placeholder')
                            : p.secondary,
                      ),
                    ),
                    badge('R', value.enabled && value.balance >= 0),
                  ],
                ),
              ),
              const SizedBox(height: 16),
              Align(
                child: wide
                    ? MioSettingsButton(
                        palette: p,
                        label: t('centerCalibration'),
                        outline: true,
                        onPressed: enabled && value.enabled
                            ? () => unawaited(
                                _save(
                                  api.BalanceSettings(
                                    enabled: value.enabled,
                                    balance: 0,
                                  ),
                                ),
                              )
                            : null,
                      )
                    : MioButton(
                        palette: p,
                        radius: 6,
                        background: p.color('--td-bg-color-specialcomponent'),
                        label: t('centerCalibration'),
                        onPressed: enabled && value.enabled
                            ? () => unawaited(
                                _save(
                                  api.BalanceSettings(
                                    enabled: value.enabled,
                                    balance: 0,
                                  ),
                                ),
                              )
                            : null,
                        child: Text(t('centerCalibration')),
                      ),
              ),
            ]);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_loading) Text(p.english ? 'Loading…' : '加载中…'),
          if (_error != null) ...[
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          () {
            final reset = MioSettingsButton(
              palette: p,
              label: wide
                  ? t('resetAll')
                  : (p.english ? 'Reset balance' : '重置声道平衡'),
              outline: true,
              onPressed: enabled
                  ? () => unawaited(
                      _save(
                        const api.BalanceSettings(enabled: true, balance: 0),
                      ),
                    )
                  : null,
            );
            if (wide && widget.sectionTitle != null) {
              return Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  children: [
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
                    reset,
                  ],
                ),
              );
            }
            return Align(alignment: Alignment.centerRight, child: reset);
          }(),
          if (wide) const SizedBox(height: 14),
          if (wide) ...[
            surroundCard,
            const SizedBox(height: 16),
            if (value != null) balanceCard,
          ] else ...[
            surroundCard,
            if (value != null) balanceCard,
          ],
        ],
      );
    }
    if (widget.embedded) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_loading || _busy) Text(p.english ? 'Working…' : '正在处理…'),
          if (_error != null) ...[
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          if (value != null) ...[
            MioSettingsField(
              palette: p,
              title: p.text('settings.audioEffect.balance'),
              description: _position(value.balance),
              child: MioSettingsSwitch(
                palette: p,
                label: p.text('settings.audioEffect.balance'),
                value: value.enabled,
                onPressed: enabled
                    ? () => unawaited(
                        _save(
                          api.BalanceSettings(
                            enabled: !value.enabled,
                            balance: value.balance,
                          ),
                        ),
                      )
                    : null,
              ),
            ),
            MioSettingsField(
              palette: p,
              title: p.english ? 'Left / right channel' : '左 / 右声道',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  MioSeekBar(
                    palette: p,
                    value: value.balance + 1.0,
                    maximum: 2,
                    step: .02,
                    height: 44,
                    semanticLabel: p.text('settings.audioEffect.balance'),
                    semanticValue: (v) => _position(v - 1),
                    onChanged: enabled && value.enabled
                        ? (v) => unawaited(
                            _save(
                              api.BalanceSettings(
                                enabled: value.enabled,
                                balance: (v - 1).clamp(-1, 1),
                              ),
                            ),
                          )
                        : null,
                  ),
                  Text(
                    '${p.english ? 'Left' : '左'}  ${((1 - value.balance.clamp(0, 1)) * 100).round()}% · ${p.english ? 'Right' : '右'}  ${((1 + value.balance.clamp(-1, 0)) * 100).round()}%',
                  ),
                  MioButton(
                    palette: p,
                    label: p.text('settings.audioEffect.centerCalibration'),
                    onPressed: enabled && value.enabled
                        ? () => unawaited(
                            _save(
                              api.BalanceSettings(
                                enabled: value.enabled,
                                balance: 0,
                              ),
                            ),
                          )
                        : null,
                    child: Text(
                      p.text('settings.audioEffect.centerCalibration'),
                    ),
                  ),
                ],
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
          if (!widget.embedded) ...[
            Text(
              p.text('settings.audioEffect.title'),
              style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 24),
          ],
          if (widget.onEqualizer != null) ...[
            MioButton(
              palette: p,
              label: p.text('settings.equalizer.title'),
              onPressed: widget.ready ? widget.onEqualizer : null,
              child: Text(p.text('settings.equalizer.title')),
            ),
            const SizedBox(height: 16),
          ],
          if (_loading || _busy)
            Semantics(
              liveRegion: true,
              child: Text(p.english ? 'Working…' : '正在处理…'),
            ),
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
          if (value != null)
            Align(
              alignment: Alignment.topLeft,
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: wide ? 442 : double.infinity,
                ),
                child: Container(
                  padding: const EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: p.surface,
                    border: Border.all(color: p.border),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  p.text('settings.audioEffect.balance'),
                                  style: const TextStyle(
                                    fontSize: 16,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                const SizedBox(height: 4),
                                Text(
                                  value.enabled
                                      ? _position(value.balance)
                                      : p.text('settings.equalizer.off'),
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: p.secondary,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          Semantics(
                            toggled: value.enabled,
                            child: SizedBox(
                              width: 60,
                              height: 44,
                              child: MioButton(
                                palette: p,
                                label: p.text('settings.audioEffect.balance'),
                                onPressed: enabled
                                    ? () => unawaited(
                                        _save(
                                          api.BalanceSettings(
                                            enabled: !value.enabled,
                                            balance: value.balance,
                                          ),
                                        ),
                                      )
                                    : null,
                                child: Text(
                                  value.enabled
                                      ? p.text('settings.equalizer.on')
                                      : p.text('settings.equalizer.off'),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 16),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Text(
                            p.english ? 'Left' : '左',
                            style: TextStyle(fontSize: 12, color: p.secondary),
                          ),
                          Text(
                            p.english ? 'Right' : '右',
                            style: TextStyle(fontSize: 12, color: p.secondary),
                          ),
                        ],
                      ),
                      MioSeekBar(
                        palette: p,
                        value: value.balance + 1,
                        maximum: 2,
                        step: .05,
                        height: 44,
                        semanticLabel: p.text('settings.audioEffect.balance'),
                        semanticValue: (v) => _position(v - 1),
                        onChanged: enabled && value.enabled
                            ? (v) => unawaited(
                                _save(
                                  api.BalanceSettings(
                                    enabled: value.enabled,
                                    balance:
                                        ((v - 1) * 20).round().clamp(-20, 20) /
                                        20,
                                  ),
                                ),
                              )
                            : null,
                      ),
                      const SizedBox(height: 10),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 16,
                          vertical: 12,
                        ),
                        decoration: BoxDecoration(
                          color: p.color('--td-bg-color-secondarycontainer'),
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceAround,
                          children: [
                            badge('L', value.balance <= 0),
                            Text(
                              p.text('settings.audioEffect.balanceCenter'),
                              style: TextStyle(
                                fontSize: 12,
                                color: p.secondary,
                              ),
                            ),
                            badge('R', value.balance >= 0),
                          ],
                        ),
                      ),
                      const SizedBox(height: 10),
                      Center(
                        child: MioButton(
                          palette: p,
                          label: p.text(
                            'settings.audioEffect.centerCalibration',
                          ),
                          onPressed: enabled && value.enabled
                              ? () => unawaited(
                                  _save(
                                    api.BalanceSettings(
                                      enabled: value.enabled,
                                      balance: 0,
                                    ),
                                  ),
                                )
                              : null,
                          child: Text(
                            p.text('settings.audioEffect.centerCalibration'),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

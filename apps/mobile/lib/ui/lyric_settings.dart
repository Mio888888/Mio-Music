import 'settings_layout.dart';
import 'native_lyric_settings.dart';
import 'device.dart';

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart' show TextInputAction;

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'lyric_style.dart';

class LyricSettingsView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final double viewportWidth;
  final bool embedded;
  const LyricSettingsView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.embedded = false,
  });
  @override
  State<LyricSettingsView> createState() => _LyricSettingsViewState();
}

class _LyricSettingsViewState extends State<LyricSettingsView>
    with WidgetsBindingObserver {
  final _family = TextEditingController();
  final _familyFocus = FocusNode();
  api.LyricSettingsSession? _session;
  api.LyricStyleSettings? _value;
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
  void didUpdateWidget(LyricSettingsView old) {
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
    api.LyricSettingsSession? candidate;
    try {
      candidate = await api.beginLyricSettings();
      final value = await candidate.state();
      if (!_live || epoch != _epoch) return;
      _session = candidate;
      candidate = null;
      setState(() {
        _value = value;
        _family.text = value.family;
        _loading = false;
      });
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(() {
          _loading = false;
          _error = p.english ? 'Unable to load lyric settings.' : '无法读取歌词设置。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _save(api.LyricStyleSettings next) async {
    final session = _session;
    final expected = _value;
    if (!_live || _busy || _loading || session == null || expected == null) {
      return;
    }
    final epoch = _epoch;
    setState(() => _busy = true);
    api.LyricStyleSettings? saved;
    await widget.core.command(() async {
      saved = await session.save(expected: expected, next: next);
    });
    if (!_live || epoch != _epoch || !identical(session, _session)) return;
    setState(() {
      _busy = false;
      if (saved != null) {
        _value = saved;
        if (next.family != expected.family) _family.text = saved!.family;
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
    _family.dispose();
    _familyFocus.dispose();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  bool _customFamily = false;

  api.LyricStyleSettings _next({String? family, double? rate, int? weight}) =>
      api.LyricStyleSettings(
        family: family ?? _value!.family,
        rate: rate ?? _value!.rate,
        weight: weight ?? _value!.weight,
      );

  @override
  Widget build(BuildContext context) {
    final value = _value;
    final enabled = _live && !_busy && !_loading && value != null;
    final wideScreen = MioDeviceScope.wide(context, widget.viewportWidth);
    // Embedded cards use the desktop control set (select, stepper, 200px
    // slider, preview box) at both widths; only their metrics differ.
    final wide = wideScreen && !widget.embedded;
    final rem = MioPalette.rem(widget.viewportWidth);
    final gutter = wide ? 24.0 : (widget.viewportWidth * .04).clamp(16.0, 24.0);
    String t(String name) => p.text('settings.lyricFont.$name');
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
              title: t('lyricFont'),
              appearance: !wide,
              topSpacing: !wide ? 0 : null,
              description: !wide
                  ? t('lyricFontDesc')
                  : p.english
                  ? 'Bundled or installed fonts; unavailable fonts use system fallback.'
                  : '支持内置或系统已安装字体；不可用字体将使用系统回退。',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (!wide)
                    MioSettingsSelect(
                      palette: p,
                      label: t('lyricFont'),
                      outlined: true,
                      value: value.family,
                      options: [
                        for (final family in {
                          value.family,
                          'lyricfont',
                          'sans-serif',
                        })
                          (
                            value: family,
                            label: family.isEmpty
                                ? (p.english ? 'System font' : '系统字体')
                                : family,
                          ),
                        (
                          value: '__custom__',
                          label: p.english ? 'Custom font…' : '自定义字体…',
                        ),
                      ],
                      onChanged: enabled
                          ? (family) {
                              if (family == '__custom__') {
                                setState(() => _customFamily = true);
                              } else {
                                setState(() => _customFamily = false);
                                unawaited(_save(_next(family: family)));
                              }
                            }
                          : null,
                    ),
                  if (wide || _customFamily) ...[
                    MioInput(
                      palette: p,
                      controller: _family,
                      focusNode: _familyFocus,
                      placeholder: t('lyricFont'),
                      height: 44,
                      maxLength: 256,
                      readOnly: !enabled,
                      textInputAction: TextInputAction.done,
                      onSubmitted: enabled
                          ? (_) => unawaited(
                              _save(_next(family: _family.text.trim())),
                            )
                          : null,
                    ),
                    const SizedBox(height: 8),
                    Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        MioButton(
                          palette: p,
                          label: p.text('common.save'),
                          onPressed: enabled
                              ? () => unawaited(
                                  _save(_next(family: _family.text.trim())),
                                )
                              : null,
                          child: Text(p.text('common.save')),
                        ),
                        MioButton(
                          palette: p,
                          label: p.english ? 'Bundled font' : '内置字体',
                          onPressed: enabled
                              ? () {
                                  _family.text = 'lyricfont';
                                  unawaited(_save(_next(family: 'lyricfont')));
                                }
                              : null,
                          child: Text(p.english ? 'Bundled font' : '内置字体'),
                        ),
                      ],
                    ),
                  ],
                ],
              ),
            ),
            MioSettingsField(
              palette: p,
              title: !wide
                  ? t('fontRate')
                  : '${t('fontRate')}: ${value.rate.toStringAsFixed(1)}×',
              appearance: !wide,
              topSpacing: !wide ? 10 : null,
              description: !wide
                  ? '${value.rate.toStringAsFixed(1)}x'
                  : p.english
                  ? 'Applies to the wide player, matching desktop.'
                  : '与桌面一致，应用于宽屏播放器。',
              child: !wide
                  ? MioSettingsStepper(
                      palette: p,
                      label: t('fontRate'),
                      value: value.rate.toStringAsFixed(1),
                      onDecrease: enabled && value.rate > .1
                          ? () => unawaited(
                              _save(
                                _next(
                                  rate: ((value.rate * 10).round() - 1) / 10,
                                ),
                              ),
                            )
                          : null,
                      onIncrease: enabled && value.rate < 2
                          ? () => unawaited(
                              _save(
                                _next(
                                  rate: ((value.rate * 10).round() + 1) / 10,
                                ),
                              ),
                            )
                          : null,
                    )
                  : MioSeekBar(
                      palette: p,
                      value: value.rate - .1,
                      maximum: 1.9,
                      step: .1,
                      height: 44,
                      semanticLabel: t('fontRate'),
                      onChanged: enabled
                          ? (v) => unawaited(
                              _save(
                                _next(
                                  rate:
                                      ((v + .1) * 10).round().clamp(1, 20) / 10,
                                ),
                              ),
                            )
                          : null,
                    ),
            ),
            MioSettingsField(
              palette: p,
              title: !wide
                  ? t('fontWeight')
                  : '${t('fontWeight')}: ${value.weight}',
              description: !wide ? '${value.weight}' : null,
              appearance: !wide,
              topSpacing: !wide ? 10 : null,
              child: SizedBox(
                width: wide ? double.infinity : 200,
                child: MioSeekBar(
                  palette: p,
                  value: value.weight - 100.0,
                  maximum: 800,
                  step: 100,
                  height: wide ? 44 : 12,
                  showThumb: !wide,
                  semanticLabel: t('fontWeight'),
                  onChanged: enabled
                      ? (v) => unawaited(
                          _save(
                            _next(
                              weight: ((v / 100).round() * 100 + 100).clamp(
                                100,
                                900,
                              ),
                            ),
                          ),
                        )
                      : null,
                ),
              ),
            ),
            if (!wide)
              Container(
                margin: EdgeInsets.only(top: wideScreen ? 12 : 10),
                padding: EdgeInsets.all(wideScreen ? rem : 12),
                decoration: BoxDecoration(
                  color: p.color('--td-bg-color-page'),
                  border: Border.all(
                    color: p.color('--td-border-level-1-color'),
                  ),
                  borderRadius: BorderRadius.circular(
                    wideScreen ? rem * .5 : 8,
                  ),
                ),
                child: Column(
                  children: [
                    Text(
                      t('fontPreview'),
                      style: TextStyle(
                        fontSize: wideScreen ? rem * .75 : 12,
                        height: wideScreen ? 22 / (rem * .75) : 22 / 12,
                        color: p.secondary,
                      ),
                    ),
                    SizedBox(height: wideScreen ? rem * .5 : 8),
                    Text(
                      t('fontPreviewText'),
                      textAlign: TextAlign.center,
                      style:
                          lyricTextStyle(
                            value,
                            wideScreen ? (36 * value.rate).roundToDouble() : 32,
                            color: p.color('--td-text-color-primary'),
                          ).copyWith(
                            height: wideScreen
                                ? ((36 * value.rate).roundToDouble() * 1.4 -
                                          .9) /
                                      (36 * value.rate).roundToDouble()
                                : 44 / 32,
                          ),
                    ),
                  ],
                ),
              )
            else
              MioSettingsField(
                palette: p,
                title: t('fontPreview'),
                appearance: !wide,
                topSpacing: !wide ? 10 : null,
                child: Text(
                  t('fontPreviewText'),
                  style: lyricTextStyle(
                    value,
                    32,
                    color: p.color('--td-text-color-primary'),
                  ).copyWith(height: 44 / 32),
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
              t('title'),
              style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 24),
          ],
          if (!MioDeviceScope.isTv(context))
            MioSettingsSection(
              palette: p,
              title: p.english ? 'Floating lyrics' : '悬浮歌词',
              child: NativeLyricSettings(palette: p, active: _live),
            ),
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
          if (value != null) ...[
            Text(
              t('lyricFont'),
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            Text(
              p.english
                  ? 'Use the bundled font, or enter up to 8 installed font families separated by commas. Unavailable fonts use system fallback.'
                  : '可使用内置字体，或输入最多 8 个以逗号分隔的已安装字体名称。不可用的字体使用系统回退。',
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
            const SizedBox(height: 8),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 560),
              child: MioInput(
                palette: p,
                controller: _family,
                focusNode: _familyFocus,
                placeholder: t('lyricFont'),
                height: 44,
                maxLength: 256,
                readOnly: !enabled,
                textInputAction: TextInputAction.done,
                onSubmitted: enabled
                    ? (_) =>
                          unawaited(_save(_next(family: _family.text.trim())))
                    : null,
              ),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                MioButton(
                  palette: p,
                  label: p.text('common.save'),
                  onPressed: enabled
                      ? () =>
                            unawaited(_save(_next(family: _family.text.trim())))
                      : null,
                  child: Text(p.text('common.save')),
                ),
                MioButton(
                  palette: p,
                  label: p.english ? 'Bundled font' : '内置字体',
                  onPressed: enabled
                      ? () {
                          _family.text = 'lyricfont';
                          unawaited(_save(_next(family: 'lyricfont')));
                        }
                      : null,
                  child: Text(p.english ? 'Bundled font' : '内置字体'),
                ),
              ],
            ),
            const SizedBox(height: 24),
            Text(
              '${t('fontRate')}: ${value.rate.toStringAsFixed(1)}×',
              style: const TextStyle(fontSize: 16),
            ),
            Text(
              p.english
                  ? 'Applies to the wide player, matching desktop.'
                  : '与桌面一致，应用于宽屏播放器。',
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
            Semantics(
              label: t('fontRate'),
              value: '${value.rate.toStringAsFixed(1)}×',
              child: SizedBox(
                width: 200,
                child: Row(
                  children: [
                    MioButton(
                      palette: p,
                      label: '${t('fontRate')} −',
                      onPressed: enabled && value.rate > .1001
                          ? () => unawaited(
                              _save(
                                _next(
                                  rate:
                                      ((value.rate * 10).round() - 1).clamp(
                                        1,
                                        20,
                                      ) /
                                      10,
                                ),
                              ),
                            )
                          : null,
                      child: const Text('−'),
                    ),
                    Expanded(
                      child: Center(child: Text(value.rate.toStringAsFixed(1))),
                    ),
                    MioButton(
                      palette: p,
                      label: '${t('fontRate')} +',
                      onPressed: enabled && value.rate < 1.9999
                          ? () => unawaited(
                              _save(
                                _next(
                                  rate:
                                      ((value.rate * 10).round() + 1).clamp(
                                        1,
                                        20,
                                      ) /
                                      10,
                                ),
                              ),
                            )
                          : null,
                      child: const Text('+'),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 24),
            Text(
              '${t('fontWeight')}: ${value.weight}',
              style: const TextStyle(fontSize: 16),
            ),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: MioSeekBar(
                palette: p,
                value: value.weight - 100.0,
                maximum: 800,
                step: 100,
                height: 44,
                semanticLabel: t('fontWeight'),
                semanticValue: (v) => '${(v + 100).round()}',
                onChanged: enabled
                    ? (v) => unawaited(
                        _save(
                          _next(
                            weight: ((v / 100).round() * 100 + 100).clamp(
                              100,
                              900,
                            ),
                          ),
                        ),
                      )
                    : null,
              ),
            ),
            const SizedBox(height: 24),
            Text(
              t('fontPreview'),
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 12),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(20),
              decoration: BoxDecoration(
                color: p.surface,
                border: Border.all(color: p.border),
                borderRadius: BorderRadius.circular(12),
              ),
              child: Text(
                t('fontPreviewText'),
                style:
                    lyricTextStyle(
                      value,
                      wide ? (36 * value.rate).roundToDouble() : 32,
                      color: p.color('--td-text-color-primary'),
                    ).copyWith(
                      height: wide
                          ? 1.4 - .9 / (36 * value.rate).round()
                          : 44 / 32,
                    ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

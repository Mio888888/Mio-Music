import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'resources.dart';
import 'settings_layout.dart';

/// Platform presentation preferences only. Lyrics and transport stay in Rust.
class NativeLyricSettings extends StatefulWidget {
  final MioPalette palette;
  final bool active;
  const NativeLyricSettings({
    super.key,
    required this.palette,
    required this.active,
  });
  @override
  State<NativeLyricSettings> createState() => _NativeLyricSettingsState();
}

class _NativeLyricSettingsState extends State<NativeLyricSettings>
    with WidgetsBindingObserver {
  static const host = MethodChannel('mio/host');
  Map<String, dynamic>? _state;
  bool _busy = false, _foreground = true;
  int _epoch = 0;
  String? _error;
  MioPalette get p => widget.palette;
  bool get live => mounted && widget.active && _foreground;
  String tr(String en, String zh) => p.english ? en : zh;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    if (live) unawaited(_call('lyricOverlayState'));
  }

  @override
  void didUpdateWidget(NativeLyricSettings old) {
    super.didUpdateWidget(old);
    if (!widget.active && old.active) {
      ++_epoch;
      _busy = false;
    }
    if (widget.active && !old.active && live) {
      unawaited(_call('lyricOverlayState'));
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    ++_epoch;
    _busy = false;
    if (live) unawaited(_call('lyricOverlayState'));
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    ++_epoch;
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _call(String method, [Map<String, Object>? args]) async {
    if (!live || _busy) return;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final result = await host.invokeMethod<Object?>(method, args);
      if (live && epoch == _epoch && result is Map) {
        setState(() => _state = Map<String, dynamic>.from(result));
      }
    } catch (e) {
      if (live && epoch == _epoch) {
        setState(
          () => _error = e is PlatformException ? e.message ?? e.code : '$e',
        );
      }
    } finally {
      if (live && epoch == _epoch) setState(() => _busy = false);
    }
  }

  void _save({bool? enabled, bool? locked, double? scale}) => unawaited(
    _call('configureLyricOverlay', {
      'enabled': enabled ?? _state?['enabled'] == true,
      'locked': locked ?? _state?['locked'] == true,
      'scale': scale ?? (_state?['scale'] as num?)?.toDouble() ?? 1.0,
    }),
  );
  String get _hint => Platform.isAndroid
      ? tr('Visible while playback service is active.', '播放服务运行期间显示。')
      : tr(
          'Start playback to display. After dismissal, turn this off and on to show it again.',
          '开始播放后显示；手动关闭实时活动后，可重新开关此项恢复显示。',
        );

  Widget _note(String text, {bool error = false}) => Padding(
    padding: const EdgeInsets.only(bottom: 4),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 13,
        height: 20 / 13,
        color: error ? p.color('--td-error-color') : p.secondary,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final supported = _state?['supported'] == true;
    final permission = _state?['permission'] == true;
    final enabled = _state?['enabled'] == true;
    final usable = live && !_busy && supported;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _note(
          Platform.isAndroid
              ? tr(
                  'Lyrics follow playback in a system overlay. Drag to move and pinch to resize. Locking lets touches pass through; unlock here. Secure lock screens may hide overlays.',
                  '歌词随播放显示在系统悬浮窗中。拖动可移动，双指可缩放；锁定后点击穿透，请回此处解锁。安全锁屏可能隐藏悬浮窗。',
                )
              : tr(
                  'Show lyrics in Live Activities on the Lock Screen and supported Dynamic Island. Requires iOS 16.2 / iPadOS 17. System duration and refresh limits apply; enable while the app is open.',
                  '在锁屏及支持的灵动岛显示歌词，需要 iOS 16.2 / iPadOS 17。展示时长和刷新频率受系统限制，请在应用前台启用。',
                ),
        ),
        if (_state != null && !supported)
          _note(
            tr('This device does not support this feature.', '当前设备不支持此功能。'),
          ),
        if (_error != null) _note(_error!, error: true),
        if ((_state?['error'] as String?)?.isNotEmpty == true)
          _note(_state!['error'] as String, error: true),
        if (supported && !permission)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Align(
              alignment: Alignment.centerLeft,
              child: MioSettingsButton(
                palette: p,
                label: tr('Open system permissions', '打开系统权限设置'),
                outline: true,
                primary: true,
                onPressed: usable
                    ? () => unawaited(_call('requestLyricOverlayPermission'))
                    : null,
              ),
            ),
          ),
        MioSettingsField(
          palette: p,
          title: Platform.isAndroid
              ? tr('Floating lyrics', '悬浮歌词')
              : tr('Live lyrics', '实时活动歌词'),
          description: enabled ? _hint : null,
          child: MioSettingsSwitch(
            palette: p,
            label: tr('Enable lyrics', '启用歌词'),
            value: enabled,
            onPressed: usable && (permission || enabled)
                ? () => _save(enabled: !enabled)
                : null,
          ),
        ),
        if (Platform.isAndroid && supported) ...[
          MioSettingsField(
            palette: p,
            title: tr('Lock / touch through', '锁定与点击穿透'),
            child: MioSettingsSwitch(
              palette: p,
              label: tr('Lock overlay', '锁定悬浮窗'),
              value: _state?['locked'] == true,
              onPressed: usable
                  ? () => _save(locked: _state?['locked'] != true)
                  : null,
            ),
          ),
          MioSettingsField(
            palette: p,
            title: tr('Text size', '文字缩放'),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final scale in [.75, 1.0, 1.25, 1.5, 2.0])
                  MioButton(
                    palette: p,
                    label: '${(scale * 100).round()}%',
                    onPressed: usable ? () => _save(scale: scale) : null,
                    child: Text(
                      '${(scale * 100).round()}%${(_state?['scale'] as num?)?.toDouble() == scale ? ' ✓' : ''}',
                    ),
                  ),
              ],
            ),
          ),
        ],
        Padding(
          padding: const EdgeInsets.only(top: 12),
          child: Align(
            alignment: Alignment.centerLeft,
            child: MioSettingsButton(
              palette: p,
              label: p.text('common.refresh'),
              outline: true,
              onPressed: live && !_busy
                  ? () => unawaited(_call('lyricOverlayState'))
                  : null,
            ),
          ),
        ),
      ],
    );
  }
}

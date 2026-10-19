import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

class DlnaSettings extends StatefulWidget {
  final MioPalette palette;
  final bool active, ready;
  const DlnaSettings({
    super.key,
    required this.palette,
    required this.active,
    required this.ready,
  });
  @override
  State<DlnaSettings> createState() => _DlnaSettingsState();
}

class _DlnaSettingsState extends State<DlnaSettings>
    with WidgetsBindingObserver {
  api.DlnaSession? _session;
  api.DlnaSession? _inFlightSession;
  api.DlnaDevices? _devices;
  api.DlnaCastState? _cast;
  Timer? _timer;
  bool _foreground = true, _busy = false, _polling = false;
  int _epoch = 0;
  String? _message;
  MioPalette get p => widget.palette;
  String t(String en, String zh) => p.english ? en : zh;
  bool get _live =>
      mounted &&
      Platform.isAndroid &&
      widget.active &&
      widget.ready &&
      _foreground;
  bool get _connected =>
      _cast != null && !['stopped', 'stop_unconfirmed'].contains(_cast!.phase);

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    _timer = Timer.periodic(
      const Duration(seconds: 1),
      (_) => unawaited(_poll()),
    );
    unawaited(_poll());
  }

  void _invalidate() {
    _epoch++;
    final session = _session;
    _session = null;
    _devices = null;
    session?.close();
    if (!identical(session, _inFlightSession)) session?.dispose();
  }

  @override
  void didUpdateWidget(DlnaSettings oldWidget) {
    super.didUpdateWidget(oldWidget);
    if ((!widget.active && oldWidget.active) ||
        (!widget.ready && oldWidget.ready)) {
      _invalidate();
    }
    if (_live) unawaited(_poll());
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _invalidate();
    }
    if (_live) unawaited(_poll());
    if (mounted) setState(() {});
  }

  Future<void> _poll() async {
    if (!_live || _busy || _polling) return;
    final epoch = _epoch;
    _polling = true;
    try {
      final state = await api.dlnaCastState();
      if (_live && epoch == _epoch) setState(() => _cast = state);
    } catch (_) {
      // A later poll can observe the original Rust worker after reconnecting.
    } finally {
      _polling = false;
    }
  }

  Future<void> _search() async {
    if (!_live || _busy) return;
    final epoch = _epoch;
    api.DlnaSession? session = _session;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      session ??= await api.beginDlna();
      if (!_live || epoch != _epoch) return;
      _session = session;
      _inFlightSession = session;
      final devices = await session.search();
      if (_live && epoch == _epoch) _devices = devices;
    } catch (_) {
      if (_live && epoch == _epoch) {
        _message = t(
          'Discovery failed or is cooling down. Check your local network and retry.',
          '搜索失败或仍在冷却中，请检查局域网连接后重试。',
        );
      }
    } finally {
      if (session != null && !identical(_session, session)) {
        session.close();
        session.dispose();
      }
      if (identical(_inFlightSession, session)) _inFlightSession = null;
      _busy = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _select(api.DlnaDevice device) async {
    final session = _session;
    final devices = _devices;
    if (!_live || _busy || session == null || devices == null || _connected) {
      return;
    }
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    _inFlightSession = session;
    try {
      final state = await session.select(
        revision: devices.revision,
        index: device.index,
      );
      if (_live && epoch == _epoch) _cast = state;
    } catch (_) {
      if (_live && epoch == _epoch) {
        _message = t(
          'Could not connect. Refresh the devices and current casting status.',
          '未能连接，请刷新设备和当前投放状态。',
        );
      }
    } finally {
      if (!identical(_session, session)) {
        session.close();
        session.dispose();
      }
      if (identical(_inFlightSession, session)) _inFlightSession = null;
      _busy = false;
      if (mounted) setState(() {});
      unawaited(_poll());
    }
  }

  Future<void> _stop() async {
    final cast = _cast;
    if (!_live || _busy || cast == null || !_connected) return;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final state = await api.stopDlnaCast(token: cast.token);
      if (_live && epoch == _epoch) _cast = state;
    } catch (_) {
      if (_live && epoch == _epoch) {
        _message = t(
          'Stop was not confirmed. Check the renderer and refresh.',
          '尚未确认停止，请检查投放设备并刷新。',
        );
      }
    } finally {
      _busy = false;
      if (mounted) setState(() {});
      unawaited(_poll());
    }
  }

  String _phase(String phase) => switch (phase) {
    'connecting' => t('Connecting…', '正在连接…'),
    'waiting' => t('Connected; choose an online song', '已选择设备，等待播放在线歌曲'),
    'playing' => t('Casting', '正在投放'),
    'paused' => t('Paused', '已暂停'),
    'stop_unconfirmed' => t(
      'Stop not confirmed; the renderer may still be playing',
      '未确认停止，设备可能仍在播放',
    ),
    _ => t('Disconnected', '已断开'),
  };
  @override
  void dispose() {
    _invalidate();
    _timer?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!Platform.isAndroid) return const SizedBox.shrink();
    final cast = _cast;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 24),
        Text(
          p.text('settings.dlna.title'),
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 8),
        Text(
          t(
            'Choose a renderer on your local network. Online song addresses are sent to it. Playback controls follow the local player; casting continues when you leave this page.',
            '选择同一局域网的设备，在线歌曲地址将发送给该设备。投放跟随本机播放控制，离开此页后继续。',
          ),
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            MioButton(
              palette: p,
              label: t('Find renderers', '搜索设备'),
              onPressed: _live && !_busy ? () => unawaited(_search()) : null,
              child: Text(t('Find renderers', '搜索设备')),
            ),
            if (_connected)
              MioButton(
                palette: p,
                label: p.text('settings.dlna.stopCast'),
                onPressed: _live && !_busy ? () => unawaited(_stop()) : null,
                child: Text(p.text('settings.dlna.stopCast')),
              ),
          ],
        ),
        if (_busy)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Semantics(
              liveRegion: true,
              child: Text(t('Working…', '正在处理…')),
            ),
          ),
        if (cast != null)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 12),
            child: Semantics(
              liveRegion: true,
              child: Text(
                '${cast.name} · ${cast.address}\n${_phase(cast.phase)}${cast.song == null ? '' : '\n${cast.song}'}${cast.error == null ? '' : '\n${t('Casting ended with an error. Check the renderer and song source before retrying.', '投放出现错误，请检查设备和歌曲来源后重试。')}'}',
              ),
            ),
          ),
        if (_message != null)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Semantics(
              liveRegion: true,
              child: Text(
                _message!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
            ),
          ),
        if (_devices?.rows.isEmpty == true && !_busy)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(p.text('settings.dlna.noDevice')),
          ),
        for (final device in _devices?.rows ?? <api.DlnaDevice>[])
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: MioButton(
              palette: p,
              label: '${device.name} ${device.address}',
              onPressed: _live && !_busy && !_connected
                  ? () => unawaited(_select(device))
                  : null,
              child: Text(
                '${device.name} · ${device.address}',
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ),
      ],
    );
  }
}

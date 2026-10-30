import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../src/rust/api.dart' as api;
import '../controls.dart';
import '../resources.dart';
import 'tv_widgets.dart';

/// TV settings entry: a remote is a poor way to fill in forms, so every
/// settings page is served as a web page on the local network. This page opens
/// that session and shows its QR code for a phone on the same Wi-Fi.
class TvSettingsQrPage extends StatefulWidget {
  final MioPalette palette;
  final bool ready;
  const TvSettingsQrPage({
    super.key,
    required this.palette,
    required this.ready,
  });
  @override
  State<TvSettingsQrPage> createState() => _TvSettingsQrPageState();
}

class _TvSettingsQrPageState extends State<TvSettingsQrPage>
    with WidgetsBindingObserver {
  api.LanTransferSession? _session;
  api.LanTransferView? _view;
  List<String> _addresses = [];
  String? _address, _error;
  Future<void>? _operation;
  Future<void> _cleanup = Future.value();
  Timer? _timer;
  bool _foreground = true, _starting = false;
  int _epoch = 0, _polls = 0;
  final _refreshFocus = FocusNode(debugLabel: 'Settings QR refresh');
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.ready && _foreground;
  bool _current(int epoch) => _live && epoch == _epoch;
  String tr(String en, String zh) => p.english ? en : zh;

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    if (_live) unawaited(_begin());
  }

  @override
  void didUpdateWidget(TvSettingsQrPage old) {
    super.didUpdateWidget(old);
    if (!_live) {
      _stop();
    } else if (!old.ready && _view == null) {
      unawaited(_begin());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (!_foreground) {
      _stop();
      if (mounted) setState(() {});
    } else if (_live && _view == null) {
      unawaited(_begin());
    }
  }

  @override
  void dispose() {
    _stop();
    WidgetsBinding.instance.removeObserver(this);
    _refreshFocus.dispose();
    super.dispose();
  }

  /// Closes the listener at once; the session object is released after any
  /// request in flight has finished.
  void _stop() {
    final epoch = ++_epoch;
    _timer?.cancel();
    _timer = null;
    _session?.close();
    _view = null;
    final pending = _operation;
    final previous = _cleanup;
    _cleanup =
        () async {
          await previous;
          await pending;
          final session = _session;
          _session = null;
          if (session != null) {
            session.close();
            try {
              await session.finishClose();
            } finally {
              session.dispose();
            }
          }
        }().catchError((Object error) {
          if (_current(epoch)) setState(() => _error = 'closeFailed');
        });
    _starting = false;
  }

  Future<void> _begin() async {
    final epoch = _epoch;
    try {
      final addresses = await api.lanTransferAddresses();
      if (!_current(epoch)) return;
      setState(() {
        _addresses = addresses;
        _address = addresses.contains(_address)
            ? _address
            : addresses.firstOrNull;
        _error = addresses.isEmpty ? 'noAddress' : null;
      });
      if (_address != null) {
        await _start();
      } else {
        // No Wi-Fi or cable yet: keep looking, so connecting is enough.
        _timer = Timer(const Duration(seconds: 3), () => unawaited(_begin()));
      }
    } catch (error) {
      if (_current(epoch)) setState(() => _error = '$error');
    }
  }

  Future<void> _start() async {
    // A stopped session is released in the background; only a running one
    // (it still has a view) blocks a new start, which waits for that release.
    if (!_live || _starting || _view != null || _address == null) return;
    final epoch = ++_epoch;
    final address = _address!;
    setState(() {
      _starting = true;
      _error = null;
    });
    await _cleanup;
    if (!_current(epoch)) return;
    final operation = _open(epoch, address);
    _operation = operation;
    await operation;
    if (!_current(epoch)) return;
    if (_error?.startsWith('busy') == true) {
      // A page that just left may still be releasing the single receiver slot.
      _stop();
      Future<void>.delayed(const Duration(milliseconds: 700), () {
        if (_live && _view == null && !_starting) unawaited(_begin());
      });
    } else if (_view == null || _view!.closed) {
      _stop();
    } else {
      _timer = Timer(const Duration(milliseconds: 750), _poll);
      _starting = false;
    }
    if (mounted) setState(() {});
  }

  Future<void> _open(int epoch, String address) async {
    try {
      final session = await api.beginLanTransfer();
      _session = session;
      if (!_current(epoch)) {
        session.close();
        return;
      }
      final value = await session.start(address: address);
      if (_current(epoch)) _view = value;
    } catch (error) {
      if (_current(epoch)) _error = '$error';
    }
  }

  Future<void> _poll() async {
    final session = _session;
    if (!_live || session == null) return;
    final epoch = _epoch;
    final operation = () async {
      try {
        final value = await session.state();
        if (_current(epoch)) {
          _view = value;
          _error = value.error;
        }
      } catch (error) {
        if (_current(epoch)) _error = '$error';
        session.close();
      }
    }();
    _operation = operation;
    await operation;
    if (!_current(epoch)) return;
    if (_view?.closed == true || _error != null) {
      // An expired or failed session is replaced, so the code on screen is
      // always one a phone can still use.
      final failed = _error != null;
      _stop();
      setState(() => _error = failed ? _error : null);
      if (!failed) unawaited(_start());
      return;
    }
    if (++_polls % 8 == 0 && await _addressMoved(epoch)) return;
    _timer = Timer(const Duration(milliseconds: 750), _poll);
    setState(() {});
  }

  /// The network can change under an open page (Wi-Fi reconnects, cable
  /// plugged in); a code for an address the TV no longer has reaches nobody.
  Future<bool> _addressMoved(int epoch) async {
    try {
      final addresses = await api.lanTransferAddresses();
      if (!_current(epoch)) return true;
      if (addresses.contains(_address)) {
        if (addresses.length != _addresses.length) {
          setState(() => _addresses = addresses);
        }
        return false;
      }
      _stop();
      setState(() {
        _addresses = addresses;
        _address = addresses.firstOrNull;
        _error = _address == null ? 'noAddress' : null;
      });
      unawaited(_begin());
      return true;
    } catch (_) {
      return false;
    }
  }

  /// A fresh invitation, for another phone or after the old one was used.
  void _renew() {
    _stop();
    setState(() => _error = null);
    unawaited(_start());
  }

  void _choose(String address) {
    if (address == _address) return;
    _stop();
    setState(() {
      _address = address;
      _error = null;
    });
    unawaited(_start());
  }

  String _message(String message) {
    final key = message.split(':').first;
    final value = p.text('lanTransfer.$key');
    return value == 'lanTransfer.$key' ? p.text('lanTransfer.failed') : value;
  }

  @override
  Widget build(BuildContext context) {
    final view = _view;
    final running = view != null && !view.closed;
    final paired = running && view.paired;
    final invite = running && !view.paired && view.code.isNotEmpty;
    final primary = p.color('--td-text-color-primary');
    final success = p.color('--td-success-color');
    final dot = paired
        ? success
        : (_error != null ? p.color('--td-error-color') : p.brand);
    final status = _error != null
        ? _message(_error!)
        : paired
        ? tr('Phone connected', '手机已连接')
        : invite
        ? tr('Waiting for your phone to scan', '等待手机扫码')
        : tr('Preparing the connection…', '正在准备连接…');
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Container(
          constraints: const BoxConstraints(maxWidth: 820),
          padding: const EdgeInsets.all(20),
          decoration: BoxDecoration(
            color: p.color('--td-bg-color-container'),
            border: Border.all(color: p.color('--td-border-level-1-color')),
            borderRadius: BorderRadius.circular(28),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              _qrBox(invite ? view.url : null, paired),
              const SizedBox(width: 24),
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      tr('Set up with your phone', '用手机扫码设置'),
                      style: const TextStyle(
                        fontSize: 26,
                        height: 1.25,
                        fontWeight: FontWeight.w800,
                      ),
                    ),
                    const SizedBox(height: 12),
                    for (final step in [
                      tr(
                        'Connect your phone to the same Wi-Fi as this TV',
                        '手机连接与电视相同的 Wi-Fi',
                      ),
                      tr(
                        'Scan the QR code with your camera or browser',
                        '用相机或浏览器扫描二维码',
                      ),
                      tr(
                        'Change appearance, playback, plugins, backups and more',
                        '在网页中调整外观、播放、插件、备份等设置',
                      ),
                    ].indexed)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 6),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Container(
                              width: 22,
                              height: 22,
                              alignment: Alignment.center,
                              decoration: BoxDecoration(
                                color: p.brand.withValues(alpha: .16),
                                shape: BoxShape.circle,
                              ),
                              child: Text(
                                '${step.$1 + 1}',
                                style: TextStyle(
                                  fontSize: 12,
                                  fontWeight: FontWeight.w800,
                                  color: p.brand,
                                ),
                              ),
                            ),
                            const SizedBox(width: 10),
                            Expanded(
                              child: Text(
                                step.$2,
                                style: TextStyle(
                                  fontSize: 16,
                                  height: 1.35,
                                  color: primary,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        Container(
                          width: 10,
                          height: 10,
                          decoration: BoxDecoration(
                            color: dot,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Semantics(
                            liveRegion: true,
                            child: Text(
                              status,
                              style: const TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                    if (running)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: Text(
                          view.address,
                          style: TextStyle(
                            fontSize: 15,
                            color: p.secondary,
                            fontFeatures: const [FontFeature.tabularFigures()],
                          ),
                        ),
                      ),
                    const SizedBox(height: 14),
                    Wrap(
                      spacing: 10,
                      runSpacing: 8,
                      children: [
                        TvPill(
                          palette: p,
                          label: paired
                              ? tr('Pair another phone', '连接另一部手机')
                              : tr('Refresh QR code', '刷新二维码'),
                          icon: 'Refresh',
                          filled: true,
                          autofocus: true,
                          focusNode: _refreshFocus,
                          onPressed: _live && !_starting ? _renew : null,
                        ),
                        if (_addresses.length > 1)
                          for (final address in _addresses)
                            TvPill(
                              palette: p,
                              label: address,
                              selected: address == _address,
                              outlined: true,
                              onPressed: _live && !_starting
                                  ? () => _choose(address)
                                  : null,
                            ),
                      ],
                    ),
                    const SizedBox(height: 12),
                    Text(
                      tr(
                        'This link is plain HTTP, valid only on your local network, and closes after 30 minutes without use. Only share it with devices you trust.',
                        '此链接为局域网内的普通 HTTP,闲置 30 分钟后自动关闭,请只分享给你信任的设备。',
                      ),
                      style: TextStyle(
                        fontSize: 14,
                        height: 1.4,
                        color: p.secondary,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _qrBox(String? url, bool paired) => Container(
    width: 280,
    height: 280,
    alignment: Alignment.center,
    decoration: BoxDecoration(
      color: const Color(0xffffffff),
      borderRadius: BorderRadius.circular(20),
    ),
    padding: const EdgeInsets.all(10),
    child: url != null
        ? QrImageView(
            data: url,
            size: 260,
            backgroundColor: const Color(0xffffffff),
            semanticsLabel: tr('QR code for phone setup', '手机设置二维码'),
          )
        : Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              MioIcon(
                paired ? 'TD-Check' : 'Refresh',
                size: 56,
                color: paired
                    ? p.color('--td-success-color')
                    : const Color(0xff9aa5a0),
              ),
              const SizedBox(height: 10),
              Text(
                paired ? tr('Connected', '已连接') : tr('Loading…', '加载中…'),
                style: const TextStyle(
                  fontSize: 15,
                  color: Color(0xff4a5651),
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
  );
}

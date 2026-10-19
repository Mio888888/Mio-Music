import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'device.dart';
import 'resources.dart';

class LanTransferPage extends StatefulWidget {
  final MioPalette palette;
  final bool active, ready, television;
  const LanTransferPage({
    super.key,
    required this.palette,
    required this.active,
    required this.ready,
    required this.television,
  });
  @override
  State<LanTransferPage> createState() => _LanTransferPageState();
}

class _LanTransferPageState extends State<LanTransferPage>
    with WidgetsBindingObserver {
  api.LanTransferSession? _session;
  api.LanTransferView? _view;
  List<String> _addresses = [];
  String? _address, _error;
  Future<void>? _operation;
  Future<void> _cleanup = Future.value();
  Timer? _timer;
  bool _foreground = true, _busy = false;
  int _epoch = 0;
  bool get _live =>
      mounted &&
      widget.active &&
      widget.ready &&
      widget.television &&
      _foreground;
  String _text(String key) => widget.palette.text('lanTransfer.$key');
  String _message(String message) {
    final key = message.split(':').first;
    final value = _text(key);
    return value == 'lanTransfer.$key' ? _text('failed') : value;
  }

  bool _current(int epoch) => _live && epoch == _epoch;

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    if (_live) unawaited(_load());
  }

  @override
  void didUpdateWidget(LanTransferPage old) {
    super.didUpdateWidget(old);
    if (!_live) {
      _stop();
    } else if (!old.active || !old.ready || !old.television) {
      unawaited(_load());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (!_foreground) {
      _stop();
      if (mounted) setState(() {});
    } else if (_live) {
      unawaited(_load());
    }
  }

  void _stop({bool receipts = false}) {
    final epoch = ++_epoch;
    _timer?.cancel();
    _timer = null;
    _session?.close(); // Synchronous listener/socket cancellation, even during start.
    _view = null; // Never retain an invitation after page/background departure.
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
              if (receipts && _current(epoch)) {
                final value = await session.state();
                if (_current(epoch)) {
                  setState(() {
                    _view = value;
                    _error = value.error ?? _error;
                  });
                }
              }
            } finally {
              session.dispose();
            }
          }
        }().catchError((Object error) {
          if (_current(epoch)) setState(() => _error = 'closeFailed');
        });
    _busy = false;
  }

  Future<void> _load() async {
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
    } catch (error) {
      if (_current(epoch)) setState(() => _error = '$error');
    }
  }

  Future<void> _start() async {
    if (!_live || _busy || _session != null || _address == null) return;
    final epoch = ++_epoch;
    final address = _address!;
    setState(() {
      _busy = true;
      _error = null;
    });
    await _cleanup;
    if (!_current(epoch)) return;
    final operation = _open(epoch, address);
    _operation = operation;
    await operation;
    if (!_current(epoch)) return;
    if (_view == null || _view!.closed) {
      _stop();
    } else {
      _timer = Timer(const Duration(milliseconds: 750), _poll);
      _busy = false;
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
      _stop(receipts: true);
    } else {
      _timer = Timer(const Duration(milliseconds: 750), _poll);
    }
    setState(() {});
  }

  @override
  void dispose() {
    _stop();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Widget _button(String text, VoidCallback? action) => MioButton(
    palette: widget.palette,
    label: text,
    onPressed: action,
    child: Text(text),
  );
  @override
  Widget build(BuildContext context) {
    final view = _view;
    final running = view != null && !view.closed;
    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Text(_text('title'), style: const TextStyle(fontSize: 24)),
        const SizedBox(height: 12),
        Text(_text('intro')),
        const SizedBox(height: 16),
        if (!running && !_busy) ...[
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              for (final address in _addresses)
                _button(
                  '${address == _address ? '✓ ' : ''}$address',
                  () => setState(() => _address = address),
                ),
              _button(_text('refresh'), _live ? _load : null),
            ],
          ),
          const SizedBox(height: 12),
          _button(_text('start'), _live && _address != null ? _start : null),
        ],
        if (_busy) Text(_text('starting')),
        if (running) ...[
          _LanReadout(
            palette: widget.palette,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(view.address, style: const TextStyle(fontSize: 20)),
                if (!view.paired && view.code.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  QrImageView(
                    data: view.url,
                    size: 240,
                    backgroundColor: const Color(0xffffffff),
                  ),
                  Text(_text('invitation')),
                  Text(view.code, style: const TextStyle(fontSize: 16)),
                ] else
                  Text(_text('paired')),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Text(
            _text('progress')
                .replaceAll('{phase}', _message(view.phase))
                .replaceAll(
                  '{size}',
                  (view.receivedBytes / 1048576).toStringAsFixed(1),
                ),
          ),
          Text(
            _text('budget').replaceAll(
              '{size}',
              (view.totalReserved / 1048576).toStringAsFixed(1),
            ),
          ),
        ],
        if (running || _busy)
          _button(_text('stop'), () {
            _stop(receipts: true);
            setState(() {});
          }),
        if (_error != null) Text(_message(_error!)),
        if (view?.closed == true) Text(_text('closed')),
        const SizedBox(height: 16),
        if (view != null && view.events.isNotEmpty) ...[
          Text(_text('results'), style: const TextStyle(fontSize: 20)),
          for (final event in view.events.reversed)
            _LanReadout(
              key: ValueKey(event.id),
              palette: widget.palette,
              child: Text(
                '${event.success ? '✓' : '✕'} '
                '${event.name == 'Upload'
                    ? _text('upload')
                    : event.name == 'Plugin URL'
                    ? _text('pluginUrl')
                    : event.name}: '
                '${_message(event.message)}',
              ),
            ),
        ],
      ],
    );
  }
}

/// Read-only content is reachable by the remote so long receipt lists can scroll.
class _LanReadout extends StatefulWidget {
  final MioPalette palette;
  final Widget child;
  const _LanReadout({super.key, required this.palette, required this.child});
  @override
  State<_LanReadout> createState() => _LanReadoutState();
}

class _LanReadoutState extends State<_LanReadout> {
  bool _focused = false;
  @override
  Widget build(BuildContext context) => Focus(
    canRequestFocus: MioDeviceScope.isTv(context),
    onFocusChange: (focused) {
      setState(() => _focused = focused);
      if (focused) revealTvFocus(context);
    },
    child: Container(
      width: double.infinity,
      margin: const EdgeInsets.symmetric(vertical: 4),
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          width: 2,
          color: _focused ? widget.palette.brand : const Color(0x00000000),
        ),
      ),
      child: widget.child,
    ),
  );
}

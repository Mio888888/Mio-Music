import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'src/rust/api.dart' as api;

/// Display/lifecycle only. Rust owns version comparison, settings and requests.
class ReleaseController extends ChangeNotifier {
  static const _host = MethodChannel('mio/host');
  api.ReleaseSession? _session;
  api.MobileRelease? release;
  String? installedVersion, errorCode;
  bool? autoUpdate;
  bool busy = false, ready = false;
  bool _disposed = false, _startupAttempted = false;
  int _epoch = 0;
  String? _dismissedTag;

  bool get hasNotice => release?.newer == true && release?.tag != _dismissedTag;
  bool _current(int epoch) => !_disposed && epoch == _epoch;
  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void disconnect() {
    ++_epoch;
    final old = _session;
    _session = null;
    old?.close();
    old?.dispose();
    ready = false;
    busy = false;
    _notify();
  }

  Future<void> connect({bool manual = false}) async {
    if (_disposed) return;
    disconnect();
    final epoch = _epoch;
    busy = true;
    errorCode = null;
    _notify();
    try {
      final version =
          await (Platform.isAndroid
                  ? const MethodChannel('mio/system_media')
                  : _host)
              .invokeMethod<String>('appVersion');
      if (!_current(epoch)) return;
      if (version == null || version.isEmpty) {
        throw StateError('invalid_version');
      }
      final session = await api.beginReleaseSession(currentVersion: version);
      if (!_current(epoch)) {
        session.close();
        session.dispose();
        return;
      }
      _session = session;
      final enabled = await session.state();
      if (!_current(epoch)) return;
      if (installedVersion != version) release = null;
      installedVersion = version;
      autoUpdate = enabled;
      ready = true;
      busy = false;
      _notify();
      final shouldCheck = manual || (!_startupAttempted && enabled);
      _startupAttempted = true;
      if (shouldCheck) await check(startup: !manual);
    } catch (error) {
      if (_current(epoch)) {
        errorCode = _code(error);
        ready = false;
        _session?.close();
        _session?.dispose();
        _session = null;
      }
    } finally {
      if (_current(epoch)) {
        busy = false;
        _notify();
      }
    }
  }

  Future<void> check({bool startup = false}) async {
    final session = _session;
    if (!ready || busy || session == null || _disposed) return;
    final epoch = _epoch;
    busy = true;
    errorCode = null;
    // A failed refresh cannot leave an old "up to date" result on screen.
    release = null;
    _notify();
    try {
      final result = await session.check(startup: startup);
      if (_current(epoch)) release = result;
    } catch (error) {
      if (_current(epoch)) errorCode = _code(error);
    } finally {
      if (_current(epoch)) {
        busy = false;
        _notify();
      }
    }
  }

  Future<void> setAutoUpdate(bool enabled) async {
    final session = _session, expected = autoUpdate;
    if (!ready || busy || session == null || expected == null || _disposed) {
      return;
    }
    final epoch = _epoch;
    busy = true;
    errorCode = null;
    _notify();
    try {
      final actual = await session.save(expected: expected, enabled: enabled);
      if (_current(epoch)) autoUpdate = actual;
      // After a background close, the next connect reads the saved actual state.
    } catch (error) {
      if (_current(epoch)) {
        errorCode = _code(error);
        try {
          final actual = await session.state();
          if (_current(epoch)) autoUpdate = actual;
        } catch (_) {
          if (_current(epoch)) {
            ready = false;
            _session?.close();
            _session?.dispose();
            _session = null;
          }
        }
      }
    } finally {
      if (_current(epoch)) {
        busy = false;
        _notify();
      }
    }
  }

  void dismissNotice() {
    _dismissedTag = release?.tag;
    _notify();
  }

  Future<void> openRelease() async {
    final tag = release?.tag;
    if (!ready || busy || _disposed || tag == null) return;
    final epoch = _epoch;
    busy = true;
    errorCode = null;
    _notify();
    try {
      // Only a validated tag crosses the channel; the host constructs a fixed
      // repository URL, never a provider-controlled external URL or installer.
      await _host.invokeMethod<void>('openRelease', tag);
    } catch (_) {
      if (_current(epoch)) errorCode = 'release_open_failed';
    } finally {
      if (_current(epoch)) {
        busy = false;
        _notify();
      }
    }
  }

  static String _code(Object error) {
    final text = error.toString();
    for (final code in [
      'release_rate_limited',
      'release_not_public',
      'release_cooldown',
      'release_network',
      'release_invalid',
      'invalid_version',
      'settings_changed',
      'settings_not_saved',
      'busy',
    ]) {
      if (text.contains(code)) return code;
    }
    return 'release_unavailable';
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    disconnect();
    super.dispose();
  }
}

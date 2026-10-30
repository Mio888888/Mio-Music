import 'dart:async';
import 'dart:io';
import 'dart:ffi' as ffi;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    as frb;

import 'release_controller.dart';
import 'android_system_media.dart';
import 'src/rust/api.dart' as core;
import 'src/rust/frb_generated.dart';

/// View lifetime is independent of the process-owned Rust runtime.
class CoreController extends ChangeNotifier with WidgetsBindingObserver {
  static const _host = MethodChannel('mio/host');
  static Future<void>? _library;
  static Map<String, int>? _iosMedia;
  final updates = ReleaseController();
  core.PlayerSnapshot? player;

  /// Every snapshot, including the several-per-second position reports. Listeners
  /// of the controller itself are notified only when something else changed.
  final playback = ValueNotifier<core.PlayerSnapshot?>(null);
  core.DeviceCapabilities? capabilities;
  String? error;
  core.PlayerSubscription? _subscription;
  int _epoch = 0;
  bool _disposed = false;
  core.HostPaths? _paths;

  CoreController() {
    WidgetsBinding.instance.addObserver(this);
  }

  static Future<void> _loadLibrary() async {
    if (RustLib.instance.initialized) return;
    final attempt = _library ??= _initializeLibrary();
    try {
      await attempt;
    } catch (_) {
      if (identical(_library, attempt)) _library = null;
      rethrow;
    }
  }

  static Future<void> _initializeLibrary() async {
    if (!Platform.isIOS) {
      await RustLib.init();
      return;
    }
    // Use the same loaded image as FRB. Swift receives function addresses,
    // avoiding a second loader or an assumed native-assets framework name.
    final library = await frb.loadExternalLibrary(
      RustLib.kDefaultExternalLibraryLoaderConfig,
    );
    final pointers = <String, int>{
      for (final symbol in [
        'snapshot',
        'free',
        'command',
        'register_prepare',
        'register_microphone',
        'cancel_microphone',
      ])
        symbol: library.ffiDynamicLibrary
            .lookup<ffi.Void>('mio_media_$symbol')
            .address,
    };
    _iosMedia = pointers;
    await RustLib.init(externalLibrary: library);
  }

  // The connection loop owns disposal. A competing reconnect may already
  // have disposed this handle after closing its pending next call.
  static Future<void> _close(core.PlayerSubscription subscription) async {
    try {
      await subscription.close();
    } catch (_) {
      // Closing an already disposed/disconnected view handle is idempotent.
    }
  }

  Future<void> connect() async {
    if (_disposed) return;
    updates.disconnect();
    final epoch = ++_epoch;
    final previous = _subscription;
    _subscription = null;
    core.PlayerSubscription? subscription;
    try {
      if (previous != null) await _close(previous);
      await _loadLibrary();
      if (_disposed || epoch != _epoch) return;
      if (Platform.isAndroid) await AndroidSystemMedia.initialize();
      if (Platform.isIOS) {
        await _host.invokeMethod<void>(
          'initializeAudioContext',
          Platform.isIOS ? _iosMedia : null,
        );
      }
      core.setSystemLanguage(
        english:
            WidgetsBinding.instance.platformDispatcher.locale.languageCode !=
            'zh',
      );
      capabilities = core.capabilities();
      final data = await getApplicationSupportDirectory();
      final cache = await getTemporaryDirectory();
      // A TV keeps downloads in the app's own directory, like the cache: it
      // has no file manager to browse a shared folder and frees the space on
      // uninstall.
      final downloads = Platform.isAndroid && !capabilities!.television
          ? await getExternalStorageDirectory() ?? data
          : Platform.isAndroid
          ? data
          : await getApplicationDocumentsDirectory();
      _paths ??= core.HostPaths(
        data: data.path,
        cache: cache.path,
        downloads: '${downloads.path}/Downloads',
      );
      final initial = await core.initialize(paths: _paths!);
      if (_disposed || epoch != _epoch) return;
      player = initial;
      playback.value = initial;
      unawaited(updates.connect());
      error = null;
      notifyListeners();
      subscription = await core.subscribePlayer();
      if (_disposed || epoch != _epoch) return;
      _subscription = subscription;
      int? revision;
      while (!_disposed && epoch == _epoch) {
        final state = await subscription.next(afterRevision: revision);
        if (_disposed || epoch != _epoch) break;
        if (state != null) {
          revision = state.revision;
          final previous = player;
          player = state;
          playback.value = state;
          if (previous == null ||
              _withoutClock(previous) != _withoutClock(state)) {
            notifyListeners();
          }
        }
      }
    } catch (_) {
      if (!_disposed && epoch == _epoch) {
        updates.disconnect();
        error = '无法连接音乐服务，请重试';
        notifyListeners();
      }
    } finally {
      if (identical(_subscription, subscription)) _subscription = null;
      if (subscription != null) {
        await _close(subscription);
        subscription.dispose();
      }
    }
  }

  static core.PlayerSnapshot _withoutClock(core.PlayerSnapshot s) =>
      core.PlayerSnapshot(
        libraryRevision: s.libraryRevision,
        localRevision: s.localRevision,
        coverTheme: s.coverTheme,
        backgroundRendering: s.backgroundRendering,
        visuals: s.visuals,
        lyricStyle: s.lyricStyle,
        revision: 0,
        status: s.status,
        current: s.current,
        queueCount: s.queueCount,
        queueRevision: s.queueRevision,
        playbackGeneration: s.playbackGeneration,
        sourceRevision: s.sourceRevision,
        selectedSource: s.selectedSource,
        sourceSelectionRevision: s.sourceSelectionRevision,
        position: 0,
        duration: s.duration,
        volume: s.volume,
        playMode: s.playMode,
        dark: s.dark,
        routePreload: s.routePreload,
        english: s.english,
        error: s.error,
      );

  Future<void> command(Future<void> Function() action) async {
    if (_disposed) return;
    try {
      await action();
      if (!_disposed) {
        error = null;
        notifyListeners();
      }
    } catch (_) {
      if (!_disposed) {
        error = '操作未完成，请重试';
        notifyListeners();
      }
    }
  }

  Future<void> disconnect() async {
    updates.disconnect();
    ++_epoch;
    final subscription = _subscription;
    _subscription = null;
    if (subscription != null) await _close(subscription);
    // The connect loop disposes the handle after its pending next call returns.
  }

  @override
  void didChangeLocales(List<Locale>? locales) {
    if (!_disposed && RustLib.instance.initialized) {
      core.setSystemLanguage(
        english:
            WidgetsBinding.instance.platformDispatcher.locale.languageCode !=
            'zh',
      );
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      unawaited(connect());
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      unawaited(disconnect());
    }
  }

  @override
  void dispose() {
    _disposed = true;
    updates.dispose();
    WidgetsBinding.instance.removeObserver(this);
    unawaited(disconnect());
    playback.dispose();
    super.dispose();
  }
}

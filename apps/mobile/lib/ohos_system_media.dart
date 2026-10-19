import 'dart:async';
import 'dart:convert';
import 'dart:ffi' as ffi;
import 'dart:io';
import 'dart:isolate';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

typedef _SnapshotC = ffi.Pointer<ffi.Uint8> Function(ffi.Uint64, ffi.Uint8);
typedef _SnapshotDart = ffi.Pointer<ffi.Uint8> Function(int, int);
typedef _FreeC = ffi.Void Function(ffi.Pointer<ffi.Uint8>);
typedef _FreeDart = void Function(ffi.Pointer<ffi.Uint8>);
typedef _CommandC = ffi.Int32 Function(ffi.Uint64, ffi.Uint32, ffi.Double);
typedef _CommandDart = int Function(int, int, double);
typedef _IssueC = ffi.Int32 Function(ffi.Uint64);
typedef _IssueDart = int Function(int);
typedef _ReleaseC = ffi.Void Function(ffi.Uint64);
typedef _ReleaseDart = void Function(int);
typedef _ForegroundC = ffi.Void Function(ffi.Uint8);
typedef _ForegroundDart = void Function(int);

bool get isOhos => Platform.operatingSystem == 'ohos';

/// HarmonyOS system presentation: AVSession (lock screen, control centre) and the
/// audio-playback background task. Rust remains the only player and queue owner;
/// this mirrors its media snapshot into ArkTS and routes system commands back,
/// the way the Android host does through JNI.
class OhosSystemMedia {
  static const _host = MethodChannel('mio/system_media');
  static OhosSystemMedia? _instance;

  static Future<void> initialize(ffi.DynamicLibrary library) async {
    if (_instance != null) return;
    final media = OhosSystemMedia._(library);
    _host.setMethodCallHandler(media._receive);
    await _host.invokeMethod<void>('initialize');
    _instance = media;
    Timer.periodic(const Duration(milliseconds: 500), (_) => media._observe());
    // Leaving the foreground revokes a microphone grant, as the Android host does.
    AppLifecycleListener(
      onStateChange: (state) =>
          media._foreground(state == AppLifecycleState.resumed ? 1 : 0),
    );
  }

  /// Hands Rust the token ArkTS just obtained microphone permission for.
  static bool issueMicrophone(int token) =>
      _instance != null && _instance!._issue(token) != 0;

  static void releaseMicrophone(int token) => _instance?._release(token);

  OhosSystemMedia._(ffi.DynamicLibrary library)
    : _snapshot = library.lookupFunction<_SnapshotC, _SnapshotDart>(
        'mio_media_snapshot',
      ),
      _free = library.lookupFunction<_FreeC, _FreeDart>('mio_media_free'),
      _issue = library.lookupFunction<_IssueC, _IssueDart>(
        'mio_ohos_microphone_issue',
      ),
      _release = library.lookupFunction<_ReleaseC, _ReleaseDart>(
        'mio_ohos_microphone_release',
      ),
      _foreground = library.lookupFunction<_ForegroundC, _ForegroundDart>(
        'mio_ohos_foreground',
      ),
      _command = library
          .lookup<ffi.NativeFunction<_CommandC>>('mio_media_command')
          .address;

  final _SnapshotDart _snapshot;
  final _FreeDart _free;
  final _IssueDart _issue;
  final _ReleaseDart _release;
  final _ForegroundDart _foreground;
  final int _command;
  int _runtime = 0, _artworkRevision = 0;
  bool _sending = false;
  Map<String, Object?>? _sent;
  DateTime _sentAt = DateTime.now();

  Future<Object?> _receive(MethodCall call) async {
    if (call.method != 'command') throw MissingPluginException(call.method);
    final args = Map<String, Object?>.from(call.arguments as Map);
    final runtime = _runtime;
    final action = args['command'] as int;
    final value = (args['value'] as num?)?.toDouble() ?? 0;
    if (runtime == 0) return false;
    final address = _command;
    // Rust waits for its owner to accept the command; keep that off the UI isolate.
    final ok = await Isolate.run(
      () => ffi.Pointer<ffi.NativeFunction<_CommandC>>.fromAddress(
        address,
      ).asFunction<_CommandDart>()(runtime, action, value),
    );
    return ok != 0;
  }

  Future<void> _observe() async {
    if (_sending) return;
    final raw = _snapshot(_artworkRevision, 0);
    if (raw == ffi.nullptr) {
      if (_runtime != 0) {
        _runtime = 0;
        _artworkRevision = 0;
        _sent = null;
        await _send('clear', null);
      }
      return;
    }
    final Map<String, dynamic> snapshot;
    try {
      snapshot = jsonDecode(_utf8(raw)) as Map<String, dynamic>;
    } finally {
      _free(raw);
    }
    final id = snapshot['runtime_id'] as int;
    if (id <= 0 || id < _runtime) return;
    if (id != _runtime) {
      _runtime = id;
      _artworkRevision = 0;
      _sent = null;
    }
    final current = snapshot['current'] as Map?;
    final update = <String, Object?>{
      'assetId': '$id:${current?['id'] ?? ''}',
      'title': current?['title'] as String? ?? 'Mio Music',
      'artist': current?['artist'] as String?,
      'album': current?['album'] as String?,
      'duration': _millis(snapshot['duration']),
      'position': _millis(snapshot['position']),
      'status': snapshot['status'] as String? ?? 'idle',
    };
    final revision = snapshot['artwork_revision'] as int;
    final artworkChanged = revision != _artworkRevision;
    if (artworkChanged) update['artwork'] = _artwork(snapshot['artwork_png']);
    if (!artworkChanged && !_changed(update)) return;
    if (await _send('update', update)) {
      if (_runtime == id) _artworkRevision = revision;
      _sent = update..remove('artwork');
      _sentAt = DateTime.now();
    }
  }

  /// AVSession extrapolates the position while playing, so only a seek or a
  /// state change needs a new update.
  bool _changed(Map<String, Object?> update) {
    final sent = _sent;
    if (sent == null) return true;
    for (final key in update.keys) {
      if (key != 'position' && update[key] != sent[key]) return true;
    }
    final elapsed = sent['status'] == 'playing'
        ? DateTime.now().difference(_sentAt).inMilliseconds
        : 0;
    final expected = (sent['position'] as int) + elapsed;
    return ((update['position'] as int) - expected).abs() > 1500;
  }

  Future<bool> _send(String method, Object? arguments) async {
    _sending = true;
    try {
      await _host.invokeMethod<void>(method, arguments);
      return true;
    } on PlatformException {
      return false;
    } finally {
      _sending = false;
    }
  }

  static Uint8List? _artwork(Object? encoded) {
    if (encoded is! String || encoded.isEmpty || encoded.length > 700000) {
      return null;
    }
    try {
      final bytes = base64Decode(encoded);
      return bytes.length <= 512 * 1024 ? bytes : null;
    } on FormatException {
      return null;
    }
  }

  static int _millis(Object? seconds) {
    final value = (seconds as num?)?.toDouble() ?? 0;
    return value.isFinite ? (value.clamp(0, 604800) * 1000).round() : 0;
  }

  static String _utf8(ffi.Pointer<ffi.Uint8> text) {
    var length = 0;
    while (text[length] != 0) {
      length++;
    }
    return utf8.decode(text.asTypedList(length));
  }
}

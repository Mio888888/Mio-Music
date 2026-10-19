import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:audio_service/audio_service.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

/// Android system presentation only. Rust remains the only player and queue owner.
/// This lives with audio_service's engine, independently of UI subscriptions.
class AndroidSystemMedia extends BaseAudioHandler {
  static const _host = MethodChannel('mio/system_media');
  static Future<void>? _initializing;
  static AndroidSystemMedia? _handler;
  static Future<void> initialize() async {
    if (!Platform.isAndroid) return;
    await _host.invokeMethod<void>('initialize');
    if (_initializing != null) return _initializing;
    final handler = _handler ??= AndroidSystemMedia._();
    _host.setMethodCallHandler(handler._receive);
    final attempt = _initializing = _start(handler);
    try {
      await attempt;
    } catch (_) {
      if (identical(_initializing, attempt)) _initializing = null;
      rethrow;
    }
  }

  static Future<void> _start(AndroidSystemMedia handler) async {
    await AudioService.init(
      builder: () => handler,
      config: const AudioServiceConfig(
        androidNotificationChannelId: 'mio_playback',
        androidNotificationChannelName: 'Mio Music',
        androidNotificationIcon: 'drawable/ic_notification_music',
        androidNotificationOngoing: false,
        androidStopForegroundOnPause: false,
        androidResumeOnClick: false,
        preloadArtwork: false,
        artDownscaleWidth: 256,
        artDownscaleHeight: 256,
      ),
    );
    handler._ready = true;
    handler._errors ??= AudioService.asyncError.listen((_) {
      // Native preparation times out too; revoke playback if presentation fails
      // later while running. Do not turn a failed stop into a fake idle state.
      unawaited(_host.invokeMethod<void>('failed').catchError((_) {}));
    });
  }

  AndroidSystemMedia._();
  bool _ready = false;
  StreamSubscription<Object>? _errors;
  int _runtime = 0, _artworkRevision = -1;
  int? _preparing;
  Timer? _prepareTimer;
  Map<String, dynamic>? _latest;
  Completer<void> _connected = Completer<void>();
  Uri? _artwork;
  Directory? _artDirectory;
  final List<File> _artFiles = [];
  bool get _pending => _preparing != null;

  Future<Object?> _receive(MethodCall call) async {
    switch (call.method) {
      case 'prepare':
        if (!_ready) return false;
        final args = Map<String, dynamic>.from(call.arguments as Map);
        final id = args['runtimeId'] as int, token = args['token'] as int;
        if (id <= 0 || (_runtime != 0 && id < _runtime)) return false;
        // Do not call Rust here: its owner is waiting for this foreground handshake.
        _preparing = token;
        _prepareTimer?.cancel();
        _prepareTimer = Timer(
          const Duration(seconds: 4),
          () => _cancelPrepare(token),
        );
        mediaItem.add(
          mediaItem.value ??
              const MediaItem(id: 'mio-preparing', title: 'Mio Music'),
        );
        playbackState.add(
          PlaybackState(
            controls: const [
              MediaControl.skipToPrevious,
              MediaControl.pause,
              MediaControl.skipToNext,
              MediaControl.stop,
            ],
            androidCompactActionIndices: const [0, 1, 2],
            processingState: AudioProcessingState.loading,
            playing: true,
            updatePosition: playbackState.value.updatePosition,
          ),
        );
        return true;
      case 'cancelPrepare':
        _cancelPrepare(call.arguments as int);
        return null;
      case 'runtimeClosed':
        if (_runtime == call.arguments) {
          _preparing = null;
          _prepareTimer?.cancel();
          _runtime = 0;
          _latest = null;
          _artwork = null;
          _artworkRevision = -1;
          _connected = Completer<void>();
          mediaItem.add(null);
          playbackState.add(
            PlaybackState(
              processingState: AudioProcessingState.idle,
              playing: false,
            ),
          );
        }
        return null;
      case 'snapshot':
        final snapshot = Map<String, dynamic>.from(
          jsonDecode(call.arguments as String) as Map,
        );
        await _accept(snapshot);
        return null;
      default:
        throw MissingPluginException(call.method);
    }
  }

  void _cancelPrepare(int token) {
    if (_preparing != token) return;
    _preparing = null;
    _prepareTimer?.cancel();
    final snapshot = _latest;
    if (snapshot != null) {
      _publish(snapshot);
    } else {
      playbackState.add(
        PlaybackState(
          processingState: AudioProcessingState.idle,
          playing: false,
        ),
      );
    }
  }

  Future<void> _accept(Map<String, dynamic> snapshot) async {
    final id = snapshot['runtime_id'] as int;
    if (id < _runtime || id <= 0) return;
    if (_runtime != id) {
      _runtime = id;
      _artworkRevision = -1;
      _artwork = null;
    }
    _latest = snapshot;
    if (!_connected.isCompleted) _connected.complete();
    final revision = snapshot['artwork_revision'] as int;
    if (revision != _artworkRevision) {
      _artwork = null;
      try {
        final encoded = snapshot['artwork_png'];
        if (encoded is String &&
            encoded.isNotEmpty &&
            encoded.length <= 700000) {
          final bytes = base64Decode(encoded);
          if (bytes.length <= 512 * 1024) {
            _artDirectory ??= Directory(
              '${(await getTemporaryDirectory()).path}/mio-system-media',
            );
            await _artDirectory!.create(recursive: true);
            final file = File('${_artDirectory!.path}/$id-$revision.png');
            await file.writeAsBytes(bytes, flush: false);
            if (_runtime != id) return;
            _artwork = file.uri;
            _artFiles.add(file);
            // Retain several revisions for audio_service's asynchronous artwork read.
            while (_artFiles.length > 8) {
              final old = _artFiles.removeAt(0);
              try {
                await old.delete();
              } on FileSystemException {
                /* cache eviction is best effort */
              }
            }
          }
        }
      } catch (_) {
        // Artwork is optional; cache failures must never block playback state.
        _artwork = null;
      }
      _artworkRevision = revision;
    }
    if (_runtime != id) return;
    final current = snapshot['current'];
    if (current is Map) {
      final item = MediaItem(
        id: '$id:${current['id']}',
        title: current['title'] as String? ?? 'Mio Music',
        artist: current['artist'] as String?,
        album: current['album'] as String?,
        duration: _duration(snapshot['duration']),
        artUri: _artwork,
      );
      if (item != mediaItem.value) mediaItem.add(item);
    } else if (!_pending) {
      mediaItem.add(null);
    }
    final status = snapshot['status'];
    if (_pending && status != 'playing' && status != 'loading') return;
    if (_pending) {
      _preparing = null;
      _prepareTimer?.cancel();
    }
    _publish(snapshot);
  }

  static Duration _duration(Object? seconds) {
    final value = (seconds as num?)?.toDouble() ?? 0;
    return Duration(
      milliseconds: value.isFinite
          ? (value.clamp(0, 604800) * 1000).round()
          : 0,
    );
  }

  void _publish(Map<String, dynamic> snapshot) {
    final status = snapshot['status'] as String? ?? 'idle';
    final playing = status == 'playing' || status == 'loading';
    final processing = switch (status) {
      'loading' => AudioProcessingState.buffering,
      'playing' || 'paused' => AudioProcessingState.ready,
      'ended' => AudioProcessingState.completed,
      'failed' => AudioProcessingState.error,
      _ => AudioProcessingState.idle,
    };
    playbackState.add(
      PlaybackState(
        controls: [
          MediaControl.skipToPrevious,
          playing ? MediaControl.pause : MediaControl.play,
          MediaControl.skipToNext,
          MediaControl.stop,
        ],
        androidCompactActionIndices: const [0, 1, 2],
        systemActions: const {
          MediaAction.seek,
          MediaAction.seekForward,
          MediaAction.seekBackward,
        },
        processingState: processing,
        playing: playing,
        updatePosition: _duration(snapshot['position']),
        bufferedPosition: _duration(snapshot['position']),
        speed: 1,
        errorCode: status == 'failed' ? 1 : null,
        errorMessage: status == 'failed' ? 'Playback failed' : null,
      ),
    );
  }

  Future<void> _command(int action, [double value = 0]) async {
    if (_runtime == 0) {
      await _connected.future.timeout(const Duration(seconds: 3));
    }
    final runtime = _runtime;
    if (runtime == 0) throw StateError('Media runtime unavailable');
    await _host.invokeMethod<void>('command', {
      'runtimeId': runtime,
      'command': action,
      'value': value,
    });
    // Only native snapshots publish the actual result of this command.
  }

  @override
  Future<void> play() => _command(1);
  @override
  Future<void> pause() => _command(2);
  @override
  Future<void> skipToNext() => _command(3);
  @override
  Future<void> skipToPrevious() => _command(4);
  @override
  Future<void> stop() => _command(5);
  @override
  Future<void> seek(Duration position) =>
      _command(6, position.inMilliseconds / 1000);
  @override
  Future<void> fastForward() => _command(12, 10);
  @override
  Future<void> rewind() => _command(12, -10);
  @override
  Future<void> onNotificationDeleted() => stop();
  @override
  Future<void> onTaskRemoved() async {
    /* Active playback belongs to the service. */
  }
}

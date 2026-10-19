import 'dart:convert';

import 'package:flutter/services.dart';

int _fileSequence = 0;

class PluginFileException implements Exception {
  final String code;
  const PluginFileException(this.code);
}

/// A request owns its cancellation; an old view cannot cancel a later chooser.
class PluginFileSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false;
  bool _finished = false;

  Future<String?> readCode() async {
    try {
      final bytes = await _channel.invokeMethod<Uint8List>(
        'pickPlugin',
        _token,
      );
      if (_cancelled || bytes == null) return null;
      if (bytes.isEmpty || bytes.length > 1024 * 1024) {
        throw const PluginFileException('file_too_large');
      }
      try {
        final code = utf8.decode(bytes, allowMalformed: false);
        return code.startsWith('\uFEFF') ? code.substring(1) : code;
      } on FormatException {
        throw const PluginFileException('file_invalid_utf8');
      }
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> cancel() async {
    if (_cancelled || _finished) return;
    _cancelled = true;
    try {
      await _channel.invokeMethod<void>('cancel', _token);
    } on PlatformException {
      // Native host may already be detached.
    } on MissingPluginException {
      // Unsupported platforms still offer URL installation.
    }
  }
}

/// Native audio stays in private staging; Dart only carries an opaque ticket.
class AudioFileSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false;
  bool _finished = false;
  String? _ticket;

  Future<String?> choose({bool single = false}) async {
    if (_cancelled) return null;
    try {
      final ticket = await _channel.invokeMethod<String>(
        single ? 'pickRecognitionAudio' : 'pickAudio',
        _token,
      );
      if (ticket == null) return null;
      if (!RegExp(r'^[0-9a-f]{32}-[0-9a-f]{32}$').hasMatch(ticket)) {
        throw const PluginFileException('file_unreadable');
      }
      _ticket = ticket;
      if (_cancelled) {
        await discard();
        return null;
      }
      return ticket;
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> discard() async {
    final ticket = _ticket;
    _ticket = null;
    if (ticket == null) return;
    try {
      await _channel.invokeMethod<void>('discardAudio', ticket);
    } on PlatformException {
      /* Detached host retains its own cleanup. */
    } on MissingPluginException {
      /* Unsupported host. */
    }
  }

  Future<void> cancel() async {
    if (_cancelled) return;
    _cancelled = true;
    if (!_finished) {
      try {
        await _channel.invokeMethod<void>('cancel', _token);
      } on PlatformException {
        /* Detached host. */
      } on MissingPluginException {
        /* Unsupported host. */
      }
    }
    // The import operation owns discard until Rust finishes reading its stage.
  }
}

/// EQ documents use scoped native choosers; no external path reaches Rust/Dart.
class EqDocumentSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false, _finished = false;

  Future<String?> read() async {
    if (_cancelled) return null;
    try {
      final bytes = await _channel.invokeMethod<Uint8List>('pickEq', _token);
      if (_cancelled || bytes == null) return null;
      if (bytes.isEmpty || bytes.length > 512 * 1024) {
        throw const PluginFileException('file_too_large');
      }
      try {
        final text = utf8.decode(bytes, allowMalformed: false);
        return text.startsWith('\uFEFF') ? text.substring(1) : text;
      } on FormatException {
        throw const PluginFileException('file_invalid_utf8');
      }
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<bool> save(String text) async {
    if (_cancelled) return false;
    final bytes = utf8.encode(text);
    if (bytes.isEmpty || bytes.length > 512 * 1024) {
      throw const PluginFileException('file_too_large');
    }
    try {
      final saved = await _channel.invokeMethod<bool>('saveEq', {
        'token': _token,
        'bytes': Uint8List.fromList(bytes),
      });
      return !_cancelled && saved == true;
    } on PlatformException catch (error) {
      if (_cancelled) return false;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return false;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> cancel() async {
    if (_cancelled || _finished) return;
    _cancelled = true;
    try {
      await _channel.invokeMethod<void>('cancel', _token);
    } on PlatformException {
      /* Host detached. */
    } on MissingPluginException {
      /* Host unsupported. */
    }
  }
}

/// The native host accepts a basename and independently checks its Downloads root.
Future<void> openCompletedDownload(String name) async {
  try {
    await const MethodChannel('mio/downloads')
        .invokeMethod<void>('openCompleted', name);
  } on PlatformException catch (error) {
    throw PluginFileException(error.code);
  } on MissingPluginException {
    throw const PluginFileException('open_unavailable');
  }
}

/// One selected plain playlist document; the OS path never leaves the host.
class PlaylistDocument {
  final String name;
  final Uint8List bytes;
  const PlaylistDocument(this.name, this.bytes);
}

class PlaylistDocumentSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false, _finished = false;
  Future<PlaylistDocument?> read() async {
    if (_cancelled) return null;
    try {
      final value = await _channel.invokeMapMethod<String, dynamic>(
        'pickPlaylist',
        _token,
      );
      if (_cancelled || value == null) return null;
      final name = value['name'], bytes = value['bytes'];
      if (name is! String ||
          name.isEmpty ||
          utf8.encode(name).length > 1024 ||
          name.contains(RegExp(r'[\x00-\x1f\x7f/\\]')) ||
          !RegExp(r'\.(json|cpl|cmpl)$', caseSensitive: false).hasMatch(name) ||
          bytes is! Uint8List ||
          bytes.isEmpty ||
          bytes.length > 32 * 1024 * 1024) {
        throw const PluginFileException('file_unreadable');
      }
      return PlaylistDocument(name, bytes);
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> cancel() async {
    if (_cancelled || _finished) return;
    _cancelled = true;
    try {
      await _channel.invokeMethod<void>('cancel', _token);
    } on PlatformException {
      /* Host detached. */
    } on MissingPluginException {
      /* Unsupported host. */
    }
  }
}

/// Scoped queue documents retain the existing encrypted .cpl/.cmpl wire format.
class QueueDocumentSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false, _finished = false;
  Future<PlaylistDocument?> read() async {
    if (_cancelled) return null;
    try {
      final value = await _channel.invokeMapMethod<String, dynamic>(
        'pickQueue',
        _token,
      );
      if (_cancelled || value == null) return null;
      final name = value['name'], bytes = value['bytes'];
      if (name is! String ||
          name.isEmpty ||
          utf8.encode(name).length > 1024 ||
          name.contains(RegExp(r'[\x00-\x1f\x7f/\\]')) ||
          !RegExp(r'\.(cpl|cmpl)$', caseSensitive: false).hasMatch(name) ||
          bytes is! Uint8List ||
          bytes.isEmpty ||
          bytes.length > 64 * 1024 * 1024) {
        throw const PluginFileException('file_unreadable');
      }
      return PlaylistDocument(name, bytes);
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<bool> save(String name, Uint8List bytes) async {
    if (_cancelled) return false;
    if (bytes.isEmpty ||
        bytes.length > 64 * 1024 * 1024 ||
        !RegExp(r'^cerumusic-playlist-[0-9]{4}-[0-9]{2}-[0-9]{2}\.cmpl$')
            .hasMatch(name)) {
      throw const PluginFileException('file_too_large');
    }
    try {
      final saved = await _channel.invokeMethod<bool>('saveQueue', {
        'token': _token,
        'name': name,
        'bytes': bytes,
      });
      return !_cancelled && saved == true;
    } on PlatformException catch (error) {
      if (_cancelled) return false;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return false;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> cancel() async {
    if (_cancelled || _finished) return;
    _cancelled = true;
    try {
      await _channel.invokeMethod<void>('cancel', _token);
    } on PlatformException {
      /* Detached host. */
    } on MissingPluginException {
      /* Unsupported host. */
    }
  }
}

/// A bounded image selection with original bytes; Rust validates the decoder limits.
class CoverDocumentSelection {
  static const _channel = MethodChannel('mio/files');
  final int _token = ++_fileSequence;
  bool _cancelled = false, _finished = false;
  Future<PlaylistDocument?> read() async {
    if (_cancelled) return null;
    try {
      final value = await _channel.invokeMapMethod<String, dynamic>(
        'pickCover',
        _token,
      );
      if (_cancelled || value == null) return null;
      final name = value['name'], bytes = value['bytes'];
      if (name is! String ||
          name.isEmpty ||
          utf8.encode(name).length > 1024 ||
          name.contains(RegExp(r'[\x00-\x1f\x7f/\\]')) ||
          !RegExp(
            r'\.(png|jpg|jpeg|gif|webp)$',
            caseSensitive: false,
          ).hasMatch(name) ||
          bytes is! Uint8List ||
          bytes.isEmpty ||
          bytes.length > 5 * 1024 * 1024) {
        throw const PluginFileException('file_unreadable');
      }
      return PlaylistDocument(name, bytes);
    } on PlatformException catch (error) {
      if (_cancelled) return null;
      throw PluginFileException(error.code);
    } on MissingPluginException {
      if (_cancelled) return null;
      throw const PluginFileException('picker_unavailable');
    } finally {
      _finished = true;
    }
  }

  Future<void> cancel() async {
    if (_cancelled || _finished) return;
    _cancelled = true;
    try {
      await _channel.invokeMethod<void>('cancel', _token);
    } on PlatformException {
      /* Host detached. */
    } on MissingPluginException {
      /* Unsupported host. */
    }
  }
}

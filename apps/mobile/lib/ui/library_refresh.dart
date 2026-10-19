import 'package:flutter/widgets.dart';

import '../core_controller.dart';

/// Coalesce Rust library invalidations. Owners flush again after in-flight
/// operations; a refresh never cancels a write or stores favorite truth here.
class LibraryRefresh {
  final CoreController core;
  final bool Function() ready;
  final VoidCallback refresh;
  int? _revision;
  bool _dirty = false, _scheduled = false, _disposed = false;
  LibraryRefresh({
    required this.core,
    required this.ready,
    required this.refresh,
  }) {
    _revision = core.player?.libraryRevision;
    core.addListener(_changed);
  }
  void _changed() {
    final next = core.player?.libraryRevision;
    if (next == _revision) return;
    _revision = next;
    _dirty = true;
    flush();
  }

  void flush() {
    if (_disposed || !_dirty || _scheduled) return;
    _scheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scheduled = false;
      if (_disposed || !_dirty || !ready()) return;
      _dirty = false;
      refresh();
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  void dispose() {
    _disposed = true;
    core.removeListener(_changed);
  }
}

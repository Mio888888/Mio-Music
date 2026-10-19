import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'cover.dart';

// Visible grid cells request images lazily. One job decodes at a time and the
// pending queue is bounded; disposed cells are skipped before any bridge call.
final _coverJobs = Queue<Future<void> Function()>();
bool _coverRunning = false;
void _pumpCovers() {
  if (_coverRunning) return;
  _coverRunning = true;
  unawaited(() async {
    try {
      while (_coverJobs.isNotEmpty) {
        await _coverJobs.removeFirst()();
      }
    } finally {
      _coverRunning = false;
    }
  }());
}

class MioSavedPlaylistCover extends StatefulWidget {
  final String id, source;
  final api.LibrarySession? session;
  final api.OnlineSaveSession? appendSession;
  final double size, radius;
  const MioSavedPlaylistCover({
    super.key,
    required this.id,
    required this.source,
    this.session,
    this.appendSession,
    required this.size,
    this.radius = 8,
  });
  @override
  State<MioSavedPlaylistCover> createState() => _MioSavedPlaylistCoverState();
}

class _MioSavedPlaylistCoverState extends State<MioSavedPlaylistCover> {
  Uint8List? _data;
  int _epoch = 0;
  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(MioSavedPlaylistCover old) {
    super.didUpdateWidget(old);
    if (old.id != widget.id ||
        old.source != widget.source ||
        !identical(old.session, widget.session) ||
        !identical(old.appendSession, widget.appendSession)) {
      _data = null;
      _load();
    }
  }

  void _load() {
    final epoch = ++_epoch,
        session = widget.session,
        appendSession = widget.appendSession,
        id = widget.id;
    if (widget.source != 'mio-saved-cover' ||
        (session == null && appendSession == null) ||
        _coverJobs.length >= 64) {
      return;
    }
    _coverJobs.add(() async {
      if (!mounted || epoch != _epoch) return;
      try {
        final data = session != null
            ? await session.cover(id: id)
            : await appendSession!.destinationCover(id: id);
        if (mounted && epoch == _epoch) setState(() => _data = data);
      } catch (_) {
        /* The old cursor may have closed or changed; show fallback. */
      }
    });
    _pumpCovers();
  }

  @override
  void dispose() {
    _epoch++;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => _data == null
      ? MioCover(widget.source, size: widget.size, radius: widget.radius)
      : ClipRRect(
          borderRadius: BorderRadius.circular(widget.radius),
          child: Image.memory(
            _data!,
            width: widget.size,
            height: widget.size,
            fit: BoxFit.cover,
            gaplessPlayback: false,
            errorBuilder: (_, _, _) =>
                MioCover(null, size: widget.size, radius: widget.radius),
          ),
        );
}

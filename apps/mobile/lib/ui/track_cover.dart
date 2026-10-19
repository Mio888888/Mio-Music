import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'cover.dart';

// One request at a time across visible rows and the mini player. Pending cells
// remove themselves on hide/dispose; no per-track global decoded image cache.
final _pending = Queue<_MioTrackCoverState>();
bool _running = false;
void _pump() {
  if (_running) return;
  _running = true;
  unawaited(() async {
    try {
      while (_pending.isNotEmpty) {
        await _pending.removeFirst()._load();
      }
    } finally {
      _running = false;
    }
  }());
}

class MioTrackCover extends StatefulWidget {
  final Object identity;
  final Future<Uint8List?> Function() load;
  final double size, radius;
  final bool active;
  const MioTrackCover({
    super.key,
    required this.identity,
    required this.load,
    required this.size,
    this.radius = 8,
    required this.active,
  });

  factory MioTrackCover.current({
    required api.PlayerSnapshot snapshot,
    required double size,
    required double radius,
    required bool active,
  }) => MioTrackCover(
    identity: (
      snapshot.current?.id,
      snapshot.playbackGeneration,
      snapshot.queueRevision,
      snapshot.sourceRevision,
      snapshot.localRevision,
      snapshot.current?.cover,
    ),
    size: size,
    radius: radius,
    active: active,
    load: () async {
      final song = snapshot.current;
      if (song == null) return null;
      final session = await api.beginPlayerDetails(
        songId: song.id,
        generation: snapshot.playbackGeneration,
        queueRevision: snapshot.queueRevision,
        sourceRevision: snapshot.sourceRevision,
      );
      try {
        return await session.cover();
      } finally {
        session.cancel();
        session.dispose();
      }
    },
  );

  @override
  State<MioTrackCover> createState() => _MioTrackCoverState();
}

class _MioTrackCoverState extends State<MioTrackCover>
    with WidgetsBindingObserver {
  Uint8List? _data;
  int _epoch = 0;
  bool _foreground = true;
  bool _enabled = false;
  bool _attempted = false;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
  }

  void _sync() {
    final enabled =
        widget.active && _foreground && TickerMode.valuesOf(context).enabled;
    if (_enabled != enabled) {
      _enabled = enabled;
      _epoch++;
      _pending.remove(this);
      if (enabled && _data == null) _attempted = false;
    }
    if (_enabled &&
        !_attempted &&
        !_pending.contains(this) &&
        _pending.length < 64) {
      _pending.add(this);
      _pump();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(MioTrackCover oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.identity != widget.identity) {
      _epoch++;
      _data = null;
      _attempted = false;
      _pending.remove(this);
    }
    _sync();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _sync();
  }

  Future<void> _load() async {
    if (!mounted || !_enabled) return;
    final epoch = _epoch;
    final load = widget.load;
    _attempted = true;
    try {
      final data = await load();
      if (mounted && _enabled && epoch == _epoch) {
        setState(() => _data = data);
      }
    } catch (_) {
      // Stale sessions, missing artwork and bounded-worker contention fall back.
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _epoch++;
    _pending.remove(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => _data == null
      ? MioCover(null, size: widget.size, radius: widget.radius)
      : ClipRRect(
          borderRadius: BorderRadius.circular(widget.radius),
          child: Image.memory(
            _data!,
            width: widget.size,
            height: widget.size,
            fit: BoxFit.cover,
            cacheWidth: (widget.size * MediaQuery.devicePixelRatioOf(context))
                .ceil(),
            gaplessPlayback: false,
            errorBuilder: (_, _, _) =>
                MioCover(null, size: widget.size, radius: widget.radius),
          ),
        );
}

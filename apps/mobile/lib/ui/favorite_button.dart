import 'dart:async';
import 'dart:collection';

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'library_refresh.dart';
import 'song_target.dart';

/// Limit visible-row I/O across replacement lists. Stale queued work checks its
/// widget generation before creating a Rust handle; no handles sit in the queue.
class _FavoriteTasks {
  static final _queue = Queue<Future<void> Function()>();
  static int _running = 0;
  static Future<void> run(Future<void> Function() task) {
    if (_queue.length >= 64) {
      return Future.error(StateError('Favorite row queue occupied'));
    }
    final result = Completer<void>();
    _queue.add(() async {
      try {
        await task();
        result.complete();
      } catch (error, trace) {
        result.completeError(error, trace);
      }
    });
    _drain();
    return result.future;
  }

  static void _drain() {
    while (_running < 2 && _queue.isNotEmpty) {
      final task = _queue.removeFirst();
      _running++;
      unawaited(
        task().whenComplete(() {
          _running--;
          _drain();
        }),
      );
    }
  }
}

/// Desktop row heart behavior, backed by a fresh source-bound Rust intent for
/// each read/write. Unknown state is disabled while loading and retries on error.
class FavoriteButton extends StatefulWidget {
  final MioPalette palette;
  final CoreController core;
  final Object identity;
  final bool active;
  final Future<api.FavoriteIntent> Function() prepare;
  final VoidCallback? onSaved;
  final VoidCallback onReload;

  /// When set, tapping opens the playlist chooser (with "My favorites" on top)
  /// instead of toggling the favorite directly.
  final AddToPlaylist? addToPlaylist;
  final String songTitle;
  const FavoriteButton({
    super.key,
    required this.palette,
    required this.core,
    required this.identity,
    required this.active,
    required this.prepare,
    this.onSaved,
    required this.onReload,
    this.addToPlaylist,
    this.songTitle = '',
  });

  @override
  State<FavoriteButton> createState() => _FavoriteButtonState();
}

class _FavoriteButtonState extends State<FavoriteButton>
    with WidgetsBindingObserver {
  late final LibraryRefresh _favoriteRefresh;
  api.FavoriteIntent? _intent;
  bool? _liked;
  bool _busy = false, _failed = false;
  int _epoch = 0;
  bool _foreground = true;
  bool get _active => widget.active && _foreground;
  bool _valid(int epoch) => mounted && _active && _epoch == epoch;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _favoriteRefresh = LibraryRefresh(
      core: widget.core,
      ready: () => _active && !_busy,
      refresh: () => unawaited(_run()),
    );
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
    if (_active) unawaited(_run());
  }

  void _release() {
    _epoch++;
    // The owning future disposes after its pending FRB call returns.
    _intent?.close();
    _intent = null;
    _busy = false;
    _liked = null;
    _failed = false;
  }

  @override
  void didUpdateWidget(FavoriteButton old) {
    super.didUpdateWidget(old);
    if (old.identity != widget.identity || old.active != widget.active) {
      _release();
      if (_active) unawaited(_run());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      setState(() {
        _foreground = false;
        _release();
      });
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_active) unawaited(_run());
    }
  }

  @override
  void dispose() {
    _favoriteRefresh.dispose();
    _release();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _run({bool? desired}) async {
    if (!_active || _busy) return;
    final epoch = ++_epoch;
    final prepare = widget.prepare;
    setState(() {
      _busy = true;
      _failed = false;
    });
    try {
      await _FavoriteTasks.run(() async {
        if (!_valid(epoch)) return;
        final intent = await prepare();
        try {
          if (!_valid(epoch)) return;
          _intent = intent;
          bool? result;
          if (desired == null) {
            result = await intent.state();
          } else {
            await widget.core.command(() async {
              result = await intent.setFavorite(liked: desired);
            });
          }
          if (!_valid(epoch)) return;
          if (result == null) throw StateError('Favorite write failed');
          setState(() => _liked = result);
          if (desired != null) widget.onSaved?.call();
        } finally {
          if (identical(_intent, intent)) _intent = null;
          intent.close();
          intent.dispose();
        }
      });
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _liked = null;
          _failed = true;
        });
      }
    } finally {
      if (_valid(epoch)) {
        setState(() => _busy = false);
        _favoriteRefresh.flush();
      }
    }
  }

  Future<void> _choose() async {
    final add = widget.addToPlaylist;
    if (add == null || !_active || _busy) return;
    await showSongTarget(
      context: context,
      palette: widget.palette,
      core: widget.core,
      title: widget.songTitle,
      prepareFavorite: widget.prepare,
      addToPlaylist: add,
    );
    if (mounted && _active) {
      widget.onSaved?.call();
      unawaited(_run());
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final label = _failed
        ? (p.english ? 'Reload favorites' : '重新读取收藏')
        : _liked == null
        ? (p.english ? 'Loading favorites' : '正在读取收藏')
        : p.text(_liked! ? 'play.liked' : 'play.like');
    return SizedBox(
      width: 44,
      height: 44,
      child: Semantics(
        selected: _liked,
        child: MioButton(
          palette: p,
          label: label,
          radius: 8,
          onPressed: !_active || _busy
              ? null
              : _failed
              ? widget.onReload
              : widget.addToPlaylist != null
              ? () => unawaited(_choose())
              : () =>
                    unawaited(_run(desired: _liked == null ? null : !_liked!)),
          child: _busy
              ? Text('…', style: TextStyle(color: p.secondary))
              : MioIcon(
                  _liked == true ? 'TD-Heart-Filled' : 'TD-Heart',
                  color: _liked == true
                      ? p.color('--td-error-color')
                      : p.secondary,
                  size: 18,
                ),
        ),
      ),
    );
  }
}

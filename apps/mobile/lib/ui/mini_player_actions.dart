import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'download_quality.dart';
import 'resources.dart';
import 'library_refresh.dart';

/// Current-song actions use the same version-bound Rust session as FullPlayer.
class MiniPlayerActions extends StatefulWidget {
  final CoreController core;
  final api.PlayerSnapshot? snapshot;
  final MioPalette palette;
  final bool active, wide;
  const MiniPlayerActions({
    super.key,
    required this.core,
    required this.snapshot,
    required this.palette,
    required this.active,
    required this.wide,
  });
  @override
  State<MiniPlayerActions> createState() => _MiniPlayerActionsState();
}

class _MiniPlayerActionsState extends State<MiniPlayerActions>
    with WidgetsBindingObserver {
  final _favoriteFocus = FocusNode(debugLabel: 'Mini player favorite');
  final _downloadFocus = FocusNode(debugLabel: 'Mini player download');
  late final LibraryRefresh _favoriteRefresh;
  api.PlayerDetails? _session;
  api.DownloadChoice? _choice;
  bool? _liked;
  bool _loading = false, _failed = false, _liking = false, _downloading = false;
  bool _foreground = true;
  bool _restoreFavoriteFocus = false;
  int _epoch = 0;
  Object _identity(api.PlayerSnapshot? state) => (
    state?.current?.id,
    state?.playbackGeneration,
    state?.queueRevision,
    state?.sourceRevision,
    state?.localRevision,
  );
  bool get _active =>
      widget.active && _foreground && widget.snapshot?.current != null;
  bool _valid(int epoch) => mounted && _active && epoch == _epoch;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _favoriteRefresh = LibraryRefresh(
      core: widget.core,
      ready: () => _active && _session != null && !_loading && !_liking,
      refresh: () => unawaited(_refreshFavorite()),
    );
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
    if (_active) unawaited(_load());
  }

  void _release() {
    _epoch++;
    _session?.cancel();
    _session?.dispose();
    _session = null;
    // The operation owns disposal after any pending FRB call/route completes.
    _choice?.close();
    _choice = null;
    _liked = null;
    _restoreFavoriteFocus = false;
    _loading = _liking = _downloading = _failed = false;
  }

  @override
  void didUpdateWidget(MiniPlayerActions old) {
    super.didUpdateWidget(old);
    if (old.active != widget.active ||
        _identity(old.snapshot) != _identity(widget.snapshot)) {
      unawaited(_load());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _release();
      if (mounted) setState(() {});
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      unawaited(_load());
    }
  }

  @override
  void dispose() {
    _favoriteRefresh.dispose();
    _release();
    _favoriteFocus.dispose();
    _downloadFocus.dispose();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _load() async {
    _release();
    if (!_active) return;
    final state = widget.snapshot!;
    final epoch = _epoch;
    setState(() => _loading = true);
    api.PlayerDetails? session;
    var adopted = false;
    try {
      session = await api.beginPlayerDetails(
        songId: state.current!.id,
        generation: state.playbackGeneration,
        queueRevision: state.queueRevision,
        sourceRevision: state.sourceRevision,
      );
      if (!_valid(epoch)) return;
      _session = session;
      adopted = true;
      final liked = await session.favorite();
      if (_valid(epoch)) setState(() => _liked = liked);
    } catch (_) {
      if (_valid(epoch)) setState(() => _failed = true);
    } finally {
      if (!adopted) {
        session?.cancel();
        session?.dispose();
      }
      if (_valid(epoch)) {
        setState(() => _loading = false);
        _favoriteRefresh.flush();
      }
    }
  }

  Future<void> _refreshFavorite() async {
    final session = _session, epoch = _epoch;
    if (session == null || !_valid(epoch) || _loading || _liking) return;
    final wasFocused = _favoriteFocus.hasFocus;
    setState(() => _loading = true);
    try {
      final liked = await session.favorite();
      if (_valid(epoch)) {
        setState(() {
          _liked = liked;
          _failed = false;
        });
      }
    } catch (_) {
      if (_valid(epoch)) setState(() => _failed = true);
    } finally {
      if (_valid(epoch)) {
        setState(() => _loading = false);
        _favoriteRefresh.flush();
        _restoreFocus(
          _favoriteFocus,
          epoch,
          wasFocused || _restoreFavoriteFocus,
        );
      }
    }
  }

  void _restoreFocus(
    FocusNode node,
    int epoch,
    bool wasFocused, {
    bool wideOnly = false,
  }) {
    if (!wasFocused) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_valid(epoch) && (!wideOnly || widget.wide) && node.context != null) {
        if (identical(node, _favoriteFocus) && (_loading || _liking)) {
          _restoreFavoriteFocus = true;
          return;
        }
        if (!node.canRequestFocus) return;
        if (identical(node, _favoriteFocus)) _restoreFavoriteFocus = false;
        node.requestFocus();
      }
    });
  }

  Future<void> _favorite() async {
    if (!_active || _loading || _liking) return;
    final session = _session, liked = _liked, epoch = _epoch;
    if (session == null || liked == null || _failed) {
      await _load();
      return;
    }
    final wasFocused = _favoriteFocus.hasFocus;
    setState(() => _liking = true);
    bool? saved;
    await widget.core.command(() async {
      if (_valid(epoch)) saved = await session.setFavorite(liked: !liked);
    });
    if (_valid(epoch)) {
      setState(() {
        _liking = false;
        _failed = saved == null;
        if (saved != null) _liked = saved;
      });
      _favoriteRefresh.flush();
      _restoreFocus(_favoriteFocus, epoch, wasFocused);
    }
  }

  Future<void> _download() async {
    final session = _session, epoch = _epoch;
    if (!_active || session == null || _downloading) return;
    final wasFocused = _downloadFocus.hasFocus;
    setState(() => _downloading = true);
    api.DownloadChoice? choice;
    try {
      choice = await session.prepareDownload();
      if (!_valid(epoch)) return;
      _choice = choice;
      final info = await choice.info();
      if (!mounted || !_valid(epoch)) return;
      await showDownloadChoice(
        context: context,
        choice: choice,
        info: info,
        core: widget.core,
        palette: widget.palette,
      );
    } catch (error) {
      if (_valid(epoch)) {
        await widget.core.command(() => Future<void>.error(error));
      }
    } finally {
      if (identical(_choice, choice)) _choice = null;
      choice?.close();
      choice?.dispose();
      if (_valid(epoch)) {
        setState(() => _downloading = false);
        _restoreFocus(_downloadFocus, epoch, wasFocused, wideOnly: true);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    Widget button(
      FocusNode focusNode,
      String icon,
      String label,
      VoidCallback? action,
    ) => SizedBox(
      width: widget.wide ? 26 : 32,
      height: widget.wide ? 26 : 44,
      child: MioButton(
        palette: p,
        label: label,
        focusNode: focusNode,
        padding: EdgeInsets.zero,
        onPressed: action,
        child: Center(
          child: MioIcon(icon, size: 18, color: p.color('--player-text-idle')),
        ),
      ),
    );
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        button(
          _favoriteFocus,
          _failed
              ? 'Refresh'
              : _liked == true
              ? 'TD-Heart-Filled'
              : 'TD-Heart',
          _failed
              ? (p.english ? 'Reload favorites' : '重新读取收藏')
              : p.text(_liked == true ? 'play.liked' : 'play.like'),
          _active && !_loading && !_liking
              ? () => unawaited(_favorite())
              : null,
        ),
        if (widget.wide) ...[
          const SizedBox(width: 3),
          button(
            _downloadFocus,
            'Download',
            p.text('play.download'),
            _active &&
                    !_downloading &&
                    _session != null &&
                    (widget.snapshot?.current?.id.startsWith('mio-online:') ??
                        false)
                ? () => unawaited(_download())
                : null,
          ),
        ],
      ],
    );
  }
}

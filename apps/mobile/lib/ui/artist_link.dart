import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'cover.dart' show MioCover;
import 'device.dart';
import 'resources.dart';
import 'singer.dart';

/// A row-owned request transfers the artist handle only after a successful push.
class ArtistLink extends StatefulWidget {
  final String name;
  final bool available, active;
  final Object identity;
  final bool Function() isCurrent;
  final Future<api.SingerSession> Function() open;
  final ValueChanged<bool> onRouteChanged;
  final CoreController core;
  final MioPalette palette;
  final bool compact;
  const ArtistLink({
    super.key,
    required this.name,
    required this.available,
    required this.active,
    required this.identity,
    required this.isCurrent,
    required this.open,
    required this.onRouteChanged,
    required this.core,
    required this.palette,
    this.compact = false,
  });
  @override
  State<ArtistLink> createState() => _ArtistLinkState();
}

class _ArtistLinkState extends State<ArtistLink> {
  final _focus = FocusNode(debugLabel: 'Artist link');
  int _epoch = 0;
  bool _busy = false, _failed = false;
  @override
  void didUpdateWidget(ArtistLink old) {
    super.didUpdateWidget(old);
    if (old.identity != widget.identity || old.active && !widget.active) {
      _epoch++;
      _failed = false;
    }
  }

  @override
  void dispose() {
    _epoch++;
    _focus.dispose();
    super.dispose();
  }

  Future<void> _open() async {
    if (_busy || !widget.active || !widget.available) return;
    final epoch = _epoch, origin = widget.identity;
    final isCurrent = widget.isCurrent, onRouteChanged = widget.onRouteChanged;
    final open = widget.open,
        core = widget.core,
        palette = widget.palette,
        name = widget.name;
    final restoreFocus = MioDeviceScope.isTv(context);
    final navigator = Navigator.of(context);
    final host = SingerHost.of(context)?.open;
    final motion = MediaQuery.disableAnimationsOf(context)
        ? Duration.zero
        : const Duration(milliseconds: 200);
    api.SingerSession? candidate;
    bool presented = false;
    setState(() {
      _busy = true;
      _failed = false;
    });
    try {
      candidate = await open();
      if (!mounted ||
          epoch != _epoch ||
          origin != widget.identity ||
          !widget.active ||
          !isCurrent() ||
          !navigator.mounted) {
        return;
      }
      final artist = candidate;
      if (host != null) {
        final result = host(artist, name);
        candidate = null;
        presented = true;
        onRouteChanged(true);
        await result;
        return;
      }
      final route = PageRouteBuilder<void>(
        pageBuilder: (_, _, _) => SingerPage(
          session: artist,
          core: core,
          palette: palette,
          fallbackName: name,
        ),
        transitionDuration: motion,
        reverseTransitionDuration: motion,
        transitionsBuilder: (_, animation, _, child) =>
            FadeTransition(opacity: animation, child: child),
      );
      final result = navigator.push<void>(route);
      candidate = null;
      presented = true;
      onRouteChanged(true);
      await result;
      // Pop resolves before the reverse transition releases its focus scope.
      await route.completed;
    } catch (_) {
      if (mounted && epoch == _epoch && origin == widget.identity) {
        setState(() => _failed = true);
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
      if (presented) onRouteChanged(false);
      if (mounted) setState(() => _busy = false);
      if (presented && restoreFocus) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted &&
              origin == widget.identity &&
              widget.active &&
              widget.available &&
              widget.isCurrent() &&
              _focus.canRequestFocus) {
            _focus.requestFocus();
          }
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final label = _failed
        ? (p.english
              ? 'Artist page failed. Retry: ${widget.name}'
              : '歌手页打开失败，点击重试：${widget.name}')
        : (p.english ? 'Open artist: ${widget.name}' : '打开歌手页：${widget.name}');
    final text = Text(
      widget.name,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        fontSize: 12,
        height: widget.compact ? 1 : null,
        color: _failed ? p.color('--td-error-color') : p.secondary,
      ),
    );
    if (!widget.available || widget.name.isEmpty) return text;
    return MioButton(
      palette: p,
      focusNode: _focus,
      label: label,
      padding: EdgeInsets.zero,
      disabledOpacity: 1,
      onPressed: widget.active && !_busy ? () => unawaited(_open()) : null,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Flexible(child: text),
          if (_failed) ...[
            const SizedBox(width: 4),
            Text(
              p.english ? 'Retry' : '重试',
              style: TextStyle(fontSize: 12, color: p.brand),
            ),
          ],
        ],
      ),
    );
  }
}

/// Playback and artist navigation are separate focus and semantics targets.
class OnlineTrackRow extends StatelessWidget {
  final api.Track track;
  final MioPalette palette;
  final VoidCallback? onPlay;
  final Widget artist;
  final Widget? trailing;
  final int? number;
  final bool showAlbum;
  const OnlineTrackRow({
    super.key,
    required this.track,
    required this.palette,
    required this.onPlay,
    required this.artist,
    this.trailing,
    this.number,
    this.showAlbum = false,
  });
  @override
  Widget build(BuildContext context) => Row(
    children: [
      if (number != null)
        SizedBox(
          width: 32,
          child: Text('$number', style: TextStyle(color: palette.secondary)),
        ),
      MioCover(track.cover, size: 44, radius: 6),
      const SizedBox(width: 12),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            MioButton(
              palette: palette,
              label: '${palette.text('play.play')}: ${track.title}',
              padding: EdgeInsets.zero,
              onPressed: onPlay,
              child: SizedBox(
                width: double.infinity,
                child: Text(
                  track.title,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
            artist,
            if (showAlbum && track.album.isNotEmpty)
              Text(
                track.album,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 12, color: palette.secondary),
              ),
          ],
        ),
      ),
      const SizedBox(width: 8),
      trailing ??
          Text(
            '${track.duration ~/ 60}:${(track.duration.toInt() % 60).toString().padLeft(2, '0')}',
          ),
    ],
  );
}

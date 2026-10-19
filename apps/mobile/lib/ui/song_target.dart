import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'glass_surface.dart';
import 'play_choice.dart';
import 'resources.dart';

/// Adds the row's song to the saved playlist [destinationId]; returns the
/// number of songs added. The owner decides how the song is addressed.
typedef AddToPlaylist = Future<int> Function(String destinationId);

/// Online rows are saved from a fresh one-song intent per addition.
AddToPlaylist onlineSongTarget(
  Future<api.OnlineSaveSession> Function() prepare,
) => (destinationId) async {
  final intent = await prepare();
  try {
    return await intent.append(destinationId: destinationId);
  } finally {
    intent.close();
    intent.dispose();
  }
};

/// Picks where a song goes: "My favorites" (toggled in place) or one of the
/// user's saved playlists. Returns after the dialog closes.
Future<void> showSongTarget({
  required BuildContext context,
  required MioPalette palette,
  required CoreController core,
  required String title,
  required Future<api.FavoriteIntent> Function() prepareFavorite,
  required AddToPlaylist addToPlaylist,
}) async {
  final route = RawDialogRoute<void>(
    barrierDismissible: true,
    barrierLabel: palette.text('common.close'),
    barrierColor: const Color(0x66000000),
    transitionDuration: const Duration(milliseconds: 160),
    transitionBuilder: (context, animation, _, child) => FadeTransition(
      opacity: animation,
      child: ScaleTransition(
        scale: Tween<double>(begin: .96, end: 1).animate(animation),
        child: child,
      ),
    ),
    pageBuilder: (_, _, _) => _SongTargetDialog(
      palette: palette,
      core: core,
      title: title,
      prepareFavorite: prepareFavorite,
      addToPlaylist: addToPlaylist,
    ),
  );
  await Navigator.of(context).push<void>(route);
  await route.completed;
}

class _SongTargetDialog extends StatefulWidget {
  final MioPalette palette;
  final CoreController core;
  final String title;
  final Future<api.FavoriteIntent> Function() prepareFavorite;
  final AddToPlaylist addToPlaylist;
  const _SongTargetDialog({
    required this.palette,
    required this.core,
    required this.title,
    required this.prepareFavorite,
    required this.addToPlaylist,
  });
  @override
  State<_SongTargetDialog> createState() => _SongTargetDialogState();
}

class _SongTargetDialogState extends State<_SongTargetDialog> {
  static const _page = 20;
  api.FavoriteIntent? _intent;
  bool? _liked;
  bool _favoriteBusy = false;
  final _rows = <api.PlaylistInfo>[];
  int _offset = 0;
  bool _more = false, _loading = true, _adding = false, _closed = false;
  String? _error;
  MioPalette get p => widget.palette;

  @override
  void initState() {
    super.initState();
    unawaited(_loadFavorite());
    unawaited(_loadPlaylists());
  }

  @override
  void dispose() {
    _closed = true;
    _intent?.close();
    _intent?.dispose();
    super.dispose();
  }

  Future<void> _loadFavorite() async {
    try {
      final intent = await widget.prepareFavorite();
      if (_closed) {
        intent.close();
        intent.dispose();
        return;
      }
      _intent = intent;
      final liked = await intent.state();
      if (!_closed) setState(() => _liked = liked);
    } catch (_) {
      if (!_closed) setState(() => _liked = null);
    }
  }

  Future<void> _toggleFavorite() async {
    final intent = _intent, liked = _liked;
    if (intent == null || liked == null || _favoriteBusy) return;
    setState(() => _favoriteBusy = true);
    try {
      bool? result;
      await widget.core.command(() async {
        result = await intent.setFavorite(liked: !liked);
      });
      if (!_closed && result != null) setState(() => _liked = result);
    } finally {
      if (!_closed) setState(() => _favoriteBusy = false);
    }
  }

  Future<void> _loadPlaylists() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final page = await api.playlists(offset: _offset, limit: _page);
      if (_closed) return;
      setState(() {
        // "My favorites" is its own row above, not a duplicate destination.
        _rows.addAll(page.items.where((row) => !row.favorite));
        _offset += page.items.length;
        _more = page.hasMore && page.items.isNotEmpty;
      });
    } catch (_) {
      if (!_closed) {
        setState(
          () => _error = p.english ? 'Could not load playlists' : '无法读取歌单，请重试',
        );
      }
    } finally {
      if (!_closed) setState(() => _loading = false);
    }
  }

  Future<void> _pick(api.PlaylistInfo row) async {
    if (_adding) return;
    final navigator = Navigator.of(context);
    setState(() {
      _adding = true;
      _error = null;
    });
    try {
      final added = await widget.addToPlaylist(row.id);
      if (_closed) return;
      final message = p
          .text('common.addedSongsToPlaylist')
          .replaceAll('{count}', '$added')
          .replaceAll('{name}', row.title);
      navigator.pop();
      // The navigator outlives the dialog route, so its context stays valid.
      // ignore: use_build_context_synchronously
      showMioToast(navigator.context, p, message);
    } catch (_) {
      if (!_closed) {
        setState(() {
          _adding = false;
          _error = p.english
              ? 'Could not add the song. It may already be in this playlist.'
              : '无法添加，歌曲可能已在该歌单中。';
        });
      }
    }
  }

  Widget _row({
    required String label,
    String? detail,
    required String icon,
    required Color iconColor,
    Widget? trailing,
    required VoidCallback? onPressed,
  }) {
    final primary = p.color('--td-text-color-primary');
    return MioButton(
      palette: p,
      label: label,
      radius: 12,
      insetFocusBorder: false,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      onPressed: onPressed,
      child: SizedBox(
        height: 52,
        child: Row(
          children: [
            MioIcon(icon, size: 20, color: iconColor),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: primary,
                    ),
                  ),
                  if (detail != null)
                    Text(
                      detail,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                ],
              ),
            ),
            ?trailing,
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final primary = p.color('--td-text-color-primary');
    final liked = _liked == true;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            Navigator.of(context).pop(),
      },
      child: FocusScope(
        autofocus: true,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.text('common.addToPlaylistTitle'),
          child: DefaultTextStyle(
            style: TextStyle(
              color: primary,
              fontSize: 14,
              decoration: TextDecoration.none,
            ),
            child: Center(
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: 380,
                  maxHeight: MediaQuery.sizeOf(context).height * .72,
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(22),
                      boxShadow: const [
                        BoxShadow(
                          offset: Offset(0, 12),
                          color: Color(0x40000000),
                          blurRadius: 32,
                        ),
                      ],
                    ),
                    child: MioGlassSurface(
                      palette: p,
                      radius: 22,
                      tint: p.surface.withValues(alpha: p.dark ? .55 : .5),
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(12, 16, 12, 12),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 8,
                              ),
                              child: Row(
                                children: [
                                  Expanded(
                                    child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Text(
                                          p.english
                                              ? 'Choose a playlist'
                                              : '选择歌单',
                                          style: const TextStyle(
                                            fontSize: 17,
                                            fontWeight: FontWeight.w700,
                                          ),
                                        ),
                                        const SizedBox(height: 2),
                                        Text(
                                          widget.title,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                            fontSize: 12,
                                            color: p.secondary,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                  SizedBox.square(
                                    dimension: 36,
                                    child: MioButton(
                                      palette: p,
                                      label: p.text('common.close'),
                                      radius: 18,
                                      padding: EdgeInsets.zero,
                                      background: const Color(0x1f787880),
                                      onPressed: () =>
                                          Navigator.of(context).pop(),
                                      child: Center(
                                        child: MioIcon(
                                          'WindowClose',
                                          size: 16,
                                          color: p.secondary,
                                        ),
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            const SizedBox(height: 10),
                            _row(
                              label: p.text('music.songlist.myFavorite'),
                              detail: _liked == null
                                  ? (p.english ? 'Loading…' : '正在读取…')
                                  : (liked
                                        ? (p.english ? 'Added' : '已喜欢')
                                        : null),
                              icon: liked ? 'TD-Heart-Filled' : 'TD-Heart',
                              iconColor: liked
                                  ? p.color('--td-error-color')
                                  : p.secondary,
                              trailing: liked
                                  ? MioIcon(
                                      'TD-Check',
                                      size: 18,
                                      color: p.brand,
                                    )
                                  : null,
                              onPressed: _liked == null || _favoriteBusy
                                  ? null
                                  : () => unawaited(_toggleFavorite()),
                            ),
                            Padding(
                              padding: const EdgeInsets.symmetric(vertical: 6),
                              child: SizedBox(
                                height: 1,
                                child: ColoredBox(
                                  color: primary.withValues(alpha: .1),
                                ),
                              ),
                            ),
                            Flexible(
                              child: SingleChildScrollView(
                                child: Column(
                                  crossAxisAlignment:
                                      CrossAxisAlignment.stretch,
                                  children: [
                                    for (final row in _rows)
                                      _row(
                                        label: row.title,
                                        detail: p
                                            .text('common.songCount')
                                            .replaceAll(
                                              '{count}',
                                              '${row.count}',
                                            ),
                                        icon: 'nav-liebiao',
                                        iconColor: p.secondary,
                                        onPressed: _adding
                                            ? null
                                            : () => unawaited(_pick(row)),
                                      ),
                                    if (_loading)
                                      Padding(
                                        padding: const EdgeInsets.all(16),
                                        child: Center(
                                          child: Text(
                                            p.text('common.loading'),
                                            style: TextStyle(
                                              color: p.secondary,
                                            ),
                                          ),
                                        ),
                                      )
                                    else if (_rows.isEmpty && _error == null)
                                      Padding(
                                        padding: const EdgeInsets.all(16),
                                        child: Center(
                                          child: Text(
                                            p.english
                                                ? 'No playlists yet'
                                                : '还没有自己的歌单',
                                            style: TextStyle(
                                              color: p.secondary,
                                            ),
                                          ),
                                        ),
                                      ),
                                    if (_more && !_loading)
                                      MioButton(
                                        palette: p,
                                        label: p.english ? 'Load more' : '加载更多',
                                        radius: 12,
                                        onPressed: () =>
                                            unawaited(_loadPlaylists()),
                                        child: SizedBox(
                                          height: 44,
                                          child: Center(
                                            child: Text(
                                              p.english ? 'Load more' : '加载更多',
                                            ),
                                          ),
                                        ),
                                      ),
                                  ],
                                ),
                              ),
                            ),
                            if (_error != null)
                              Padding(
                                padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
                                child: Semantics(
                                  liveRegion: true,
                                  child: Text(
                                    _error!,
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: p.color('--td-error-color'),
                                    ),
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

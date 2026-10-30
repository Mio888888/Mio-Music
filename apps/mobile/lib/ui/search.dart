import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'artist_link.dart';
import 'controls.dart';
import 'context_menu.dart';
import 'device.dart';
import 'discovery.dart';
import 'cover.dart';
import 'resources.dart';
import 'online_append.dart';
import 'search_track_row.dart';
import 'play_choice.dart';
import 'tv/tv_widgets.dart';

part 'tv/tv_search.dart';

enum _SongAction { favorite, retryFavorite, select }

class SearchView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final String query;
  final int request;
  final bool active;
  final bool ready;
  final int sourceRevision;
  final double viewportWidth;
  final ValueChanged<bool>? onDetailsChanged;
  const SearchView({
    super.key,
    required this.core,
    required this.palette,
    required this.query,
    required this.request,
    required this.active,
    required this.ready,
    required this.sourceRevision,
    required this.viewportWidth,
    this.onDetailsChanged,
  });
  @override
  State<SearchView> createState() => SearchViewState();
}

class SearchViewState extends State<SearchView> with WidgetsBindingObserver {
  api.SearchSession? _session;
  api.PlaylistSearchSession? _playlistSession;
  List<api.PlaylistSearchItem> _playlists = [];
  api.PlaylistSearchItem? _selectedPlaylist;
  bool _playlistTab = false, _unsupported = false;
  final _detailsKey = GlobalKey<DiscoveryViewState>();

  bool handleBack() => _detailsKey.currentState?.handleBack() ?? false;
  List<api.SearchSource> _sources = [];
  List<api.Track> _items = [];
  Map<String, api.SearchRowLabel> _labels = {};
  Set<String> _singerIds = {};
  bool _artistRoute = false, _modalBusy = false, _selectingSongs = false;
  RawDialogRoute<_SongAction>? _songMenu;
  api.FavoriteIntent? _menuFavorite;
  Set<String> _selectedSongs = {};
  String? _actionNote;
  int? _routeRevision;
  List<api.SearchSourceState> _sourceStates = [];
  String? _source;
  String? _error;
  bool _loading = false;
  bool _noSources = false;
  bool _more = false;
  bool _foreground = true;
  int _epoch = 0;
  int _total = 0;
  final _scroll = ScrollController();
  final _tvCardFocus = <String, FocusNode>{};
  final _tvMenuFocus = <String, FocusNode>{};
  String? _tvReturnCard;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    _foreground =
        lifecycle != AppLifecycleState.paused &&
        lifecycle != AppLifecycleState.detached;
    if (_live) unawaited(_load(reset: true));
  }

  void _close() {
    _epoch++;
    final menu = _songMenu;
    _songMenu = null;
    if (menu != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (menu.isActive) menu.navigator?.removeRoute(menu);
      });
    }
    _menuFavorite?.close();
    _menuFavorite = null;
    _selectingSongs = false;
    _selectedSongs = {};
    _session?.cancel();
    _session?.dispose();
    _session = null;
    _playlistSession?.close();
    _playlistSession?.dispose();
    _playlistSession = null;
    _selectedPlaylist = null;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onDetailsChanged?.call(false);
    });
    _loading = false;
  }

  @override
  void didUpdateWidget(SearchView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.active && !widget.active ||
        oldWidget.ready && !widget.ready) {
      _close();
    }
    if (_modalBusy &&
        (oldWidget.request != widget.request ||
            oldWidget.sourceRevision != widget.sourceRevision)) {
      _close();
    }
    if (_live &&
        (oldWidget.request != widget.request ||
            oldWidget.sourceRevision != widget.sourceRevision ||
            !oldWidget.active ||
            !oldWidget.ready)) {
      unawaited(_load(reset: true));
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _close();
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live) unawaited(_load(reset: true));
    }
  }

  @override
  void dispose() {
    _close();
    _scroll.dispose();
    for (final node in _tvMenuFocus.values) {
      node.dispose();
    }
    for (final node in _tvCardFocus.values) {
      node.dispose();
    }
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _load({bool reset = false, bool retryFailed = false}) async {
    if (_playlistTab) {
      await _loadPlaylists(reset: reset, retryFailed: retryFailed);
      return;
    }
    if (!_live ||
        _artistRoute ||
        _modalBusy ||
        widget.query.isEmpty ||
        !reset && _loading) {
      return;
    }
    if (reset) _close();
    final epoch = ++_epoch;
    bool current() => _live && epoch == _epoch;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _noSources = false;
        _items = [];
        _labels = {};
        _singerIds = {};
        _sourceStates = [];
        _total = 0;
        _more = false;
        _actionNote = null;
      }
    });
    try {
      if (reset) {
        final sources = await api.searchSources();
        if (!current()) return;
        _sources = sources;
        if (_source != null && !sources.any((s) => s.id == _source)) {
          _source = null;
        }
        if (sources.isEmpty) {
          setState(() {
            _noSources = true;
            _error = p.text('music.search.onlineUnavailable');
          });
          return;
        }
        final session = await api.beginSearch(
          query: widget.query,
          source: _source ?? '',
        );
        if (!current()) {
          session.cancel();
          session.dispose();
          return;
        }
        _session = session;
      }
      final session = _session;
      if (session == null) return;
      final result = retryFailed
          ? await session.retryFailed()
          : await session.nextPage();
      if (!current()) return;
      setState(() {
        _items = result.items;
        _labels = {for (final label in result.labels) label.id: label};
        _selectedSongs.retainAll(result.items.map((song) => song.id));
        _singerIds = result.singerIds.toSet();
        _more = result.hasMore;
        _sourceStates = result.sources;
        _total = result.total;
      });
    } catch (_) {
      if (current()) {
        _session?.cancel();
        _session?.dispose();
        _session = null;
        setState(() {
          _more = false;
          _sourceStates = [];
          _error = p.text('music.search.searchFailed');
        });
      }
    } finally {
      if (mounted && epoch == _epoch) setState(() => _loading = false);
    }
  }

  Future<void> _appendSelected() async {
    final session = _session;
    if (!_live ||
        _modalBusy ||
        _artistRoute ||
        _loading ||
        session == null ||
        _selectedSongs.isEmpty) {
      return;
    }
    final epoch = _epoch,
        revision = widget.sourceRevision,
        request = widget.request;
    final ids = _items
        .where((song) => _selectedSongs.contains(song.id))
        .map((song) => song.id)
        .toList();
    setState(() {
      _modalBusy = true;
      _actionNote = null;
    });
    try {
      final saved = await showOnlineAppend(
        context: context,
        prepare: () => session.prepareSelectedSave(ids: ids),
        palette: p,
        isCurrent: () =>
            _live &&
            epoch == _epoch &&
            revision == widget.sourceRevision &&
            request == widget.request &&
            identical(session, _session),
      );
      if (mounted && saved != null) {
        setState(() {
          _actionNote = p.english
              ? 'Added ${saved.added} songs to ${saved.destination}'
              : '已向“${saved.destination}”添加 ${saved.added} 首歌曲';
          _selectedSongs = {};
          _selectingSongs = false;
        });
      }
    } catch (_) {
      if (mounted && epoch == _epoch) {
        setState(
          () => _actionNote = p.english
              ? 'Unable to prepare selected results. Reopen the search.'
              : '无法准备所选搜索结果，请重新搜索。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_live &&
            (_session == null ||
                revision != widget.sourceRevision ||
                request != widget.request)) {
          unawaited(_load(reset: true));
        }
      }
    }
  }

  Future<void> _loadPlaylists({
    bool reset = false,
    bool retryFailed = false,
  }) async {
    if (!_live ||
        _modalBusy ||
        _artistRoute ||
        widget.query.trim().isEmpty ||
        (!reset && _selectedPlaylist != null) ||
        (!reset && _loading)) {
      return;
    }
    if (reset) _close();
    final epoch = ++_epoch;
    bool current() => _live && _playlistTab && epoch == _epoch;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _unsupported = false;
        _noSources = false;
        _playlists = [];
        _sourceStates = [];
        _more = false;
        _total = 0;
      }
    });
    try {
      if (reset) {
        final sources = await api.playlistSearchSources();
        if (!current()) return;
        _sources = sources;
        if (_source != null && !sources.any((s) => s.id == _source)) {
          _source = null;
        }
        if (sources.isEmpty) {
          setState(() => _noSources = true);
          return;
        }
        final session = await api.beginPlaylistSearch(
          query: widget.query,
          source: _source ?? '',
        );
        if (!current()) {
          session.close();
          session.dispose();
          return;
        }
        _playlistSession = session;
      }
      final session = _playlistSession;
      if (session == null) return;
      final result = retryFailed
          ? await session.retryFailed()
          : await session.nextPage();
      if (!current()) return;
      setState(() {
        _playlists = result.items;
        _sourceStates = result.sources;
        _total = result.total;
        _more = result.hasMore;
      });
    } catch (error) {
      if (current()) {
        setState(() {
          _unsupported = error.toString().contains(
            'unsupported: playlist search',
          );
          _error = _unsupported
              ? p.text('music.search.playlistsUnavailable')
              : p.text('music.search.searchFailed');
        });
      }
    } finally {
      if (mounted && epoch == _epoch) setState(() => _loading = false);
    }
  }

  void _tab(bool playlists) {
    if (!_live || _modalBusy || _artistRoute || _playlistTab == playlists) {
      return;
    }
    setState(() => _playlistTab = playlists);
    unawaited(_load(reset: true));
  }

  void _select(String? source) {
    if (!_live || _modalBusy || _artistRoute || _source == source) return;
    setState(() => _source = source);
    unawaited(_load(reset: true));
  }

  /// [returnCard] names the TV card to focus again when the details close.
  void _openPlaylistResult(api.PlaylistSearchItem item, [String? returnCard]) {
    _tvReturnCard = returnCard;
    setState(() => _selectedPlaylist = item);
  }

  void _artistVisibility(bool visible) {
    if (!mounted) return;
    setState(() {
      _artistRoute = visible;
      if (visible) _routeRevision = widget.sourceRevision;
    });
    if (!visible &&
        _live &&
        (_session == null || _routeRevision != widget.sourceRevision)) {
      unawaited(_load(reset: true));
    }
  }

  /// Plays straight away when idle; otherwise asks whether to replace the
  /// queue or add the song behind it.
  Future<void> _tapSong(api.SearchSession session, api.Track song) async {
    final choice = await choosePlayChoice(
      context: context,
      palette: p,
      core: widget.core,
      title: song.title,
    );
    if (choice == null || !mounted || !_live) return;
    if (choice == PlayChoice.queue) {
      await widget.core.command(() => session.append(id: song.id));
      if (mounted && widget.core.error == null) {
        showMioToast(context, p, p.english ? 'Added to queue' : '已加入播放列表');
      }
    } else {
      await widget.core.command(() => session.play(id: song.id));
    }
  }

  Widget _songRow(api.Track song, int index, bool narrow) {
    final session = _session, epoch = _epoch;
    return SearchTrackRow(
      key: ValueKey((song.id, session)),
      track: song,
      labels: _labels[song.id],
      number: index + 1,
      narrow: narrow,
      palette: p,
      onMenu: _live && session != null && !_loading && !_artistRoute
          ? (anchor) => unawaited(_showSongMenu(song, anchor))
          : null,
      onPlay:
          !_live || session == null || _loading || _artistRoute || _modalBusy
          ? null
          : () => unawaited(_tapSong(session, song)),
      artist: ArtistLink(
        name: song.artist,
        compact: true,
        available: _singerIds.contains(song.id),
        active:
            _live &&
            !_loading &&
            !_artistRoute &&
            !_modalBusy &&
            session != null,
        identity: (song.id, session),
        core: widget.core,
        palette: p,
        isCurrent: () =>
            !_artistRoute &&
            _live &&
            epoch == _epoch &&
            identical(session, _session),
        open: () => session!.openSinger(id: song.id),
        onRouteChanged: _artistVisibility,
      ),
      trailing: _selectingSongs
          ? OnlineSelectionToggle(
              palette: p,
              track: song,
              selected: _selectedSongs.contains(song.id),
              onPressed:
                  _live &&
                      !_loading &&
                      !_artistRoute &&
                      !_modalBusy &&
                      session != null &&
                      (_selectedSongs.contains(song.id) ||
                          _selectedSongs.length < 200)
                  ? () => setState(() {
                      if (!_selectedSongs.remove(song.id)) {
                        _selectedSongs.add(song.id);
                      }
                    })
                  : null,
            )
          : MioDeviceScope.isTv(context)
          ? _tvRowMenu(
              song,
              _live && session != null && !_loading && !_artistRoute,
            )
          : null,
    );
  }

  Future<void> _showSongMenu(
    api.Track song,
    Offset anchor, {
    FocusNode? restoreFocus,
  }) async {
    final session = _session;
    if (!_live || session == null || _loading || _artistRoute || _modalBusy) {
      return;
    }
    final epoch = _epoch,
        request = widget.request,
        revision = widget.sourceRevision;
    bool current() =>
        _live &&
        epoch == _epoch &&
        request == widget.request &&
        revision == widget.sourceRevision &&
        identical(session, _session);
    final palette = p;
    final navigator = Navigator.of(context);
    final originFocus = restoreFocus ?? FocusManager.instance.primaryFocus;
    api.FavoriteIntent? intent;
    bool retry = false;
    setState(() => _modalBusy = true);
    try {
      bool? liked;
      try {
        intent = await session.prepareFavorite(id: song.id);
        if (!current()) return;
        _menuFavorite = intent;
        liked = await intent.state();
      } catch (_) {
        // Selection remains available when reading favorite state fails.
      }
      if (!current()) return;
      final menu = RawDialogRoute<_SongAction>(
        barrierDismissible: true,
        barrierLabel: palette.text('common.close'),
        barrierColor: const Color(0x00000000),
        transitionDuration: Duration.zero,
        pageBuilder: (_, _, _) => MioContextMenu<_SongAction>(
          palette: palette,
          anchor: anchor,
          label: song.title,
          entries: [
            MioMenuEntry(
              value: liked == null
                  ? _SongAction.retryFavorite
                  : _SongAction.favorite,
              label: liked == null
                  ? (palette.english ? 'Reload favorites' : '重新读取收藏')
                  : palette.text(liked ? 'play.liked' : 'play.like'),
              icon: liked == true ? 'TD-Heart-Filled' : 'TD-Heart',
            ),
            MioMenuEntry(
              value: _SongAction.select,
              label: palette.text('common.batchSelect'),
              icon: 'TD-Check',
            ),
          ],
        ),
      );
      _songMenu = menu;
      if (!navigator.mounted) return;
      final action = await navigator.push(menu);
      if (identical(_songMenu, menu)) _songMenu = null;
      if (!current()) return;
      switch (action) {
        case _SongAction.favorite:
          if (intent == null || liked == null) return;
          bool? saved;
          await widget.core.command(() async {
            if (current()) saved = await intent!.setFavorite(liked: !liked!);
          });
          if (current() && saved != null) {
            setState(
              () => _actionNote = palette.text(
                saved! ? 'play.liked' : 'play.like',
              ),
            );
          }
        case _SongAction.select:
          setState(() {
            _selectingSongs = true;
            _selectedSongs = {song.id};
          });
        case _SongAction.retryFavorite:
          retry = true;
        case null:
          break;
      }
    } catch (_) {
      if (current()) {
        setState(
          () => _actionNote = palette.english
              ? 'Unable to open song actions. Try again.'
              : '无法打开歌曲操作，请重试。',
        );
      }
    } finally {
      if (identical(_menuFavorite, intent)) _menuFavorite = null;
      intent?.close();
      intent?.dispose();
      if (mounted) {
        setState(() => _modalBusy = false);
        if (!retry && current()) {
          _restoreMenuFocus(song.id, originFocus, 4);
        }
        if (retry && current()) {
          unawaited(_showSongMenu(song, anchor, restoreFocus: originFocus));
        } else if (_live && _session == null && !_artistRoute) {
          unawaited(_load(reset: true));
        }
      }
    }
  }

  /// The rows are rebuilt by the action's note and the refreshed list, which
  /// can drop the control the menu was opened from; its row's own node (TV)
  /// survives, so fall back to that until the new row is mounted.
  void _restoreMenuFocus(String id, FocusNode? origin, int attempts) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _modalBusy) return;
      final fallback = _tvMenuFocus[id];
      final target = origin?.context != null && origin!.canRequestFocus
          ? origin
          : fallback;
      if (target?.context != null && target!.canRequestFocus) {
        target.requestFocus();
      } else if (attempts > 1) {
        _restoreMenuFocus(id, origin, attempts - 1);
      }
    });
  }

  Widget _sourceChip(String? source, String label, bool narrow) => Semantics(
    selected: _source == source,
    child: ConstrainedBox(
      constraints: BoxConstraints(minHeight: narrow ? 32 : 0),
      child: MioButton(
        key: ValueKey(source ?? ''),
        palette: p,
        label: label,
        radius: 16,
        // MioButton reserves a 2px focus border inside its box.
        padding: EdgeInsets.symmetric(horizontal: 12, vertical: narrow ? 4 : 2),
        background: _source == source ? p.brand : p.color('--theme-bg-primary'),
        onPressed: _live && !_loading && !_modalBusy && !_artistRoute
            ? () => _select(source)
            : null,
        child: Text(
          label,
          style: TextStyle(
            fontSize: narrow ? 12 : 13,
            height: 1,
            color: _source == source
                ? const Color(0xffffffff)
                : p.color('--theme-text-secondary'),
          ),
        ),
      ),
    ),
  );

  Widget _message(String title, [String? description]) => Center(
    child: Padding(
      padding: const EdgeInsets.all(20),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            title,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 16,
              color: p.color('--search-empty-title'),
            ),
          ),
          if (description != null) ...[
            const SizedBox(height: 8),
            Text(
              description,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 12,
                color: p.color('--search-empty-text'),
              ),
            ),
          ],
        ],
      ),
    ),
  );

  Widget _results(bool narrow, [EdgeInsets padding = EdgeInsets.zero]) {
    if (!_loading && _noSources) {
      return _message(p.text('music.search.noOnlineSources'), _error);
    }
    if (!_loading &&
        _error == null &&
        _items.isEmpty &&
        !_sourceStates.any((s) => s.failed)) {
      return _message(p.text('music.search.noSongResults'));
    }
    // Rows are built lazily: "load more" keeps appending pages to this list.
    final header = <Widget>[
      if (_sourceStates.any((s) => s.failed)) ...[
        for (final source in _sourceStates.where((s) => s.failed))
          Text(
            '${source.name}: ${p.text('music.search.searchFailed')}',
            style: TextStyle(color: p.secondary),
          ),
        MioButton(
          palette: p,
          label: p.text('common.retry'),
          onPressed: _loading || _modalBusy
              ? null
              : () => unawaited(_load(retryFailed: true)),
          child: Text(p.text('common.retry')),
        ),
      ],
      if (_error != null) ...[
        Text(_error!, style: TextStyle(color: p.secondary)),
        MioButton(
          palette: p,
          label: p.text('common.retry'),
          onPressed: _modalBusy ? null : () => unawaited(_load(reset: true)),
          child: Text(p.text('common.retry')),
        ),
      ],
      if (_items.isNotEmpty && _selectingSongs)
        OnlineSelectionToolbar(
          palette: p,
          tracks: _items,
          selected: _selectedSongs,
          selecting: _selectingSongs,
          enabled:
              _live &&
              !_loading &&
              !_modalBusy &&
              !_artistRoute &&
              _session != null,
          onChanged: (selecting, ids) => setState(() {
            _selectingSongs = selecting;
            _selectedSongs = ids;
          }),
          onAppend: () => unawaited(_appendSelected()),
        ),
      if (_actionNote != null)
        Semantics(liveRegion: true, child: Text(_actionNote!)),
    ];
    final footer = <Widget>[
      if (_loading)
        Padding(
          padding: const EdgeInsets.all(16),
          child: Text(p.text('common.loading')),
        ),
      if (_more && !_loading && _error == null)
        MioButton(
          palette: p,
          label: p.english ? 'Load more' : '加载更多',
          onPressed: _modalBusy ? null : () => unawaited(_load()),
          child: Text(p.english ? 'Load more' : '加载更多'),
        ),
    ];
    return ListView.builder(
      controller: _scroll,
      padding: padding,
      itemCount: header.length + _items.length + footer.length,
      itemBuilder: (context, index) {
        if (index < header.length) return header[index];
        final row = index - header.length;
        if (row < _items.length) return _songRow(_items[row], row, narrow);
        return footer[row - _items.length];
      },
    );
  }

  /// Failed sources and a retry above the playlist results.
  Widget _playlistNotices() => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      for (final source in _sourceStates.where((s) => s.failed))
        Text(
          '${source.name}: ${p.text('music.search.searchFailed')}',
          style: TextStyle(color: p.secondary),
        ),
      if (_sourceStates.any((s) => s.failed))
        MioButton(
          palette: p,
          label: p.text('common.retry'),
          onPressed: _loading
              ? null
              : () => unawaited(_load(retryFailed: true)),
          child: Text(p.text('common.retry')),
        ),
      if (_error != null) ...[
        Text(_error!, style: TextStyle(color: p.secondary)),
        MioButton(
          palette: p,
          label: p.text('common.retry'),
          onPressed: _loading
              ? null
              : () => unawaited(_load(reset: _playlistSession == null)),
          child: Text(p.text('common.retry')),
        ),
      ],
    ],
  );

  Widget _playlistFooter() => Column(
    children: [
      if (_loading)
        Padding(
          padding: const EdgeInsets.all(16),
          child: Text(p.text('common.loading')),
        ),
      if (_more && !_loading && _error == null)
        MioButton(
          palette: p,
          label: p.english ? 'Load more' : '加载更多',
          onPressed: () => unawaited(_load()),
          child: Text(p.english ? 'Load more' : '加载更多'),
        ),
    ],
  );

  Widget _playlistResults(bool narrow) {
    if (!_loading && _noSources) {
      return _message(
        p.text('music.search.noOnlineSources'),
        p.text('music.search.onlineUnavailable'),
      );
    }
    if (!_loading && _unsupported) {
      return _message(
        p.text('music.search.playlistsUnavailable'),
        p.english
            ? 'This source does not support playlist search.'
            : '该来源的歌单搜索尚未接入',
      );
    }
    if (!_loading &&
        _error == null &&
        _playlists.isEmpty &&
        !_sourceStates.any((s) => s.failed)) {
      return _message(
        p.text('music.search.noPlaylistResults'),
        p.text('music.search.tryOther'),
      );
    }
    if (MioDeviceScope.isTv(context)) return _tvPlaylistGrid();
    return LayoutBuilder(
      builder: (context, box) {
        const gap = 12.0;
        final padding = narrow ? 0.0 : 8.0;
        final content = math.max(1.0, box.maxWidth - padding * 2);
        final columns = narrow
            ? 2
            : math.max(1, ((content + gap) / (180 + gap)).floor());
        final cardWidth = math.max(
          1.0,
          (content - (columns - 1) * gap) / columns,
        );
        final scale = MediaQuery.textScalerOf(context);
        return CustomScrollView(
          controller: _scroll,
          slivers: [
            SliverToBoxAdapter(child: _playlistNotices()),
            SliverPadding(
              padding: EdgeInsets.all(padding),
              sliver: SliverGrid(
                gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                  crossAxisCount: columns,
                  mainAxisSpacing: gap,
                  crossAxisSpacing: gap,
                  mainAxisExtent:
                      cardWidth +
                      24 +
                      scale.scale(14) * 3 +
                      6 +
                      scale.scale(12) * 3 +
                      4,
                ),
                delegate: SliverChildBuilderDelegate((context, index) {
                  final item = _playlists[index], card = item.card;
                  final session = _playlistSession;
                  return Container(
                    clipBehavior: Clip.antiAlias,
                    decoration: BoxDecoration(
                      color: p.color('--td-bg-color-container'),
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(
                        color: p.color('--td-border-level-1-color'),
                      ),
                    ),
                    child: MioButton(
                      key: ValueKey((item.source, card.id)),
                      palette: p,
                      label: '${card.title} · ${card.author} · ${item.source}',
                      padding: EdgeInsets.zero,
                      radius: 12,
                      onPressed: !_live || _loading || session == null
                          ? null
                          : () => setState(() => _selectedPlaylist = item),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          MioCover(
                            card.cover,
                            size: math.max(1, cardWidth - 6),
                            radius: 0,
                          ),
                          Padding(
                            padding: const EdgeInsets.all(10),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  card.title,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 14,
                                    height: 1.5,
                                    fontWeight: FontWeight.w600,
                                    color: p.color('--td-text-color-primary'),
                                  ),
                                ),
                                const SizedBox(height: 6),
                                Text(
                                  card.description.isEmpty
                                      ? p.text('music.search.featuredPlaylist')
                                      : card.description,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 12,
                                    height: 1.5,
                                    color: p.secondary,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  );
                }, childCount: _playlists.length),
              ),
            ),
            SliverToBoxAdapter(child: _playlistFooter()),
          ],
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final selected = _selectedPlaylist, session = _playlistSession;
    if (selected != null && session != null) {
      return DiscoveryView.playlist(
        key: _detailsKey,
        onDetailsChanged: widget.onDetailsChanged,
        core: widget.core,
        palette: p,
        active: widget.active,
        ready: widget.ready,
        sourceRevision: widget.sourceRevision,
        viewportWidth: widget.viewportWidth,
        card: selected.card,
        open: () => session.open(source: selected.source, id: selected.card.id),
        back: () {
          if (mounted) {
            setState(() => _selectedPlaylist = null);
            widget.onDetailsChanged?.call(false);
            if (MioDeviceScope.isTv(context)) _restoreTvCardFocus();
          }
        },
      );
    }
    if (MioDeviceScope.isTv(context)) return _tvBody();
    final width = widget.viewportWidth;
    final narrow = !MioDeviceScope.wide(context, width);
    final chips = [
      _sourceChip(null, p.text('music.search.allSources'), narrow),
      for (final source in _sources)
        _sourceChip(source.id, source.name, narrow),
    ];
    return ColoredBox(
      color: p.color('--search-bg'),
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          narrow ? MioPalette.gutter(width) : 20,
          narrow ? (width * .035).clamp(14.0, 22.0) : 20,
          narrow ? MioPalette.gutter(width) : 20,
          narrow ? 0 : 20,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            LayoutBuilder(
              builder: (context, headerBox) => Row(
                children: [
                  Expanded(
                    child: Container(
                      padding: EdgeInsets.only(left: narrow ? 0 : 8),
                      decoration: narrow
                          ? null
                          : BoxDecoration(
                              border: Border(
                                left: BorderSide(color: p.brand, width: 4),
                              ),
                            ),
                      child: DefaultTextStyle(
                        style: TextStyle(
                          fontSize: narrow ? 18 : 24,
                          height: 1.6,
                          color: p.color('--search-title-color'),
                        ),
                        child: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text:
                                    '${p.text('music.search.searchKeyword')}"',
                              ),
                              TextSpan(
                                text: widget.query,
                                style: TextStyle(
                                  color: p.color('--search-keyword-color'),
                                ),
                              ),
                              const TextSpan(text: '"'),
                            ],
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ),
                  ),
                  if ((_playlistTab
                          ? _playlistSession != null
                          : _session != null) &&
                      _error == null &&
                      !_loading &&
                      ((_playlistTab
                              ? _playlists.isNotEmpty
                              : _items.isNotEmpty) ||
                          !_sourceStates.any((s) => s.failed))) ...[
                    const SizedBox(width: 10),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: headerBox.maxWidth * .45,
                      ),
                      child: Text(
                        _playlistTab
                            ? p
                                  .text(
                                    _source == null
                                        ? 'music.search.foundAggregatePlaylists'
                                        : 'music.search.foundPlaylists',
                                  )
                                  .replaceAll('{count}', '$_total')
                            : '$_total 首',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 12,
                          height: 1.6,
                          color: p.color('--search-info-color'),
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ),
            SizedBox(height: narrow ? 6 : 16),
            Container(
              decoration: BoxDecoration(
                border: Border(
                  bottom: BorderSide(color: p.color('--theme-border')),
                ),
              ),
              child: Row(
                children: [
                  for (final playlists in [false, true])
                    Padding(
                      padding: EdgeInsets.only(right: narrow ? 16 : 20),
                      child: Container(
                        decoration: BoxDecoration(
                          border: Border(
                            bottom: BorderSide(
                              width: 2,
                              color: _playlistTab == playlists
                                  ? p.brand
                                  : const Color(0x00000000),
                            ),
                          ),
                        ),
                        child: MioButton(
                          palette: p,
                          label: p.text(
                            playlists
                                ? 'music.search.tabPlaylists'
                                : 'music.search.tabSongs',
                          ),
                          padding: const EdgeInsets.symmetric(vertical: 8),
                          onPressed: _live && !_modalBusy && !_artistRoute
                              ? () => _tab(playlists)
                              : null,
                          child: Semantics(
                            selected: _playlistTab == playlists,
                            child: Text(
                              p.text(
                                playlists
                                    ? 'music.search.tabPlaylists'
                                    : 'music.search.tabSongs',
                              ),
                              style: TextStyle(
                                fontSize: narrow ? 15 : 14,
                                height: 1,
                                fontWeight: _playlistTab == playlists
                                    ? FontWeight.w500
                                    : FontWeight.normal,
                                color: _playlistTab == playlists
                                    ? p.brand
                                    : p.secondary,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            SizedBox(height: narrow ? 0 : 12),
            if (narrow)
              SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.only(top: 6, bottom: 2),
                child: Row(
                  children: [
                    for (var i = 0; i < chips.length; i++) ...[
                      if (i > 0) const SizedBox(width: 6),
                      chips[i],
                    ],
                  ],
                ),
              )
            else
              Wrap(spacing: 8, runSpacing: 8, children: chips),
            SizedBox(height: narrow ? 6 : 12),
            Expanded(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: ColoredBox(
                  color: p.color('--search-content-bg'),
                  child: _playlistTab
                      ? _playlistResults(narrow)
                      : _results(narrow),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

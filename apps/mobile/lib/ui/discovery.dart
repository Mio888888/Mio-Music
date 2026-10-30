import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'play_choice.dart';
import 'song_target.dart';
import 'artist_link.dart';
import 'cover.dart';
import 'online_save.dart';
import 'online_append.dart';
import 'online_downloads.dart';
import 'online_download_collection.dart';
import 'resources.dart';
import 'context_menu.dart';
import 'favorite_button.dart';
import 'online_playlist_header.dart';
import 'search_track_row.dart';
import 'tv/tv_widgets.dart';

part 'tv/tv_discovery.dart';

enum _PlaylistAction { save, download, downloadAll, select }

class DiscoveryView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final int sourceRevision;
  final int selectionRevision;
  final String selectedSource;
  final double viewportWidth;
  final Widget guide;
  final api.DiscoveryCard? initialCard;
  final Future<api.OnlinePlaylistSession> Function()? openDetails;
  final VoidCallback? onBack;
  final ValueChanged<bool>? onDetailsChanged;
  const DiscoveryView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.sourceRevision,
    this.selectionRevision = 0,
    this.selectedSource = '',
    required this.viewportWidth,
    required this.guide,
    this.onDetailsChanged,
  }) : initialCard = null,
       openDetails = null,
       onBack = null;
  const DiscoveryView.playlist({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.sourceRevision,
    this.selectionRevision = 0,
    this.selectedSource = '',
    required this.viewportWidth,
    required api.DiscoveryCard card,
    required Future<api.OnlinePlaylistSession> Function() open,
    required VoidCallback back,
    this.onDetailsChanged,
  }) : initialCard = card,
       openDetails = open,
       onBack = back,
       guide = const SizedBox.shrink();
  @override
  State<DiscoveryView> createState() => DiscoveryViewState();
}

class DiscoveryViewState extends State<DiscoveryView>
    with WidgetsBindingObserver {
  api.DiscoverySession? _session;
  api.SourceSelectionSession? _sourceSelection;
  bool _selectingSource = false, _selectionDirty = false;
  api.OnlinePlaylistSession? _details;
  List<api.DiscoverySource> _sources = [];
  List<api.DiscoveryCard> _cards = [];
  List<api.Track> _tracks = [];
  Map<String, api.SearchRowLabel> _trackLabels = {};
  String _description = '';
  bool _compactPlaylist = false, _shuffleRequested = false;

  /// Set when the header itself was swiped; list scrolling then leaves it be.
  bool _pinnedCompact = false;
  double _headerDrag = 0;
  int _favoriteRevision = 0;
  final _filter = TextEditingController();
  final _filterFocus = FocusNode(debugLabel: 'Playlist search');
  RawDialogRoute<_PlaylistAction>? _playlistMenu;
  Set<String> _singerIds = {};
  bool _artistRoute = false, _modalBusy = false, _confirm = false;
  String? _actionNote;
  bool _selectingSongs = false;
  Set<String> _selectedSongs = {};
  final _confirmationScope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  int? _routeRevision;
  api.DiscoveryCategories? _categories;
  api.DiscoveryCard? _selected;
  String? _source, _error, _categoryError;
  String _category = '', _categoryName = '';
  bool _charts = false, _loading = false, _more = true, _foreground = true;
  bool _categoriesOpen = false, _playing = false;
  final _categoryRowKey = GlobalKey();

  /// TV only: the card that opened the details gets focus back on return.
  final _tvCardFocus = <String, FocusNode>{};
  final _tvMoreFocus = FocusNode(debugLabel: 'More categories');
  String? _tvReturnCard;
  int _epoch = 0, _detailEpoch = 0, _group = 0, _total = 0;
  final _scroll = ScrollController(), _songScroll = ScrollController();
  MioPalette get p => widget.palette;
  bool get _wide => MioDeviceScope.wide(context, widget.viewportWidth);
  bool get _live => mounted && widget.active && widget.ready && _foreground;
  bool get _hasSession =>
      widget.openDetails == null ? _session != null : _details != null;
  bool get _canHandleBack =>
      _live && (_confirm || _modalBusy || _categoriesOpen || _selected != null);

  /// The shell dispatches back once, after its higher overlays have closed.
  bool handleBack() {
    if (!_live) return false;
    if (_confirm) {
      setState(() => _confirm = false);
      return true;
    }
    if (_modalBusy) return true;
    if (_categoriesOpen) {
      _closeCategories();
      return true;
    }
    if (_selected != null) {
      _back();
      return true;
    }
    return false;
  }

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    _scroll.addListener(_maybeMore);
    _songScroll.addListener(_onSongScroll);
    _filterFocus.addListener(_playlistFocusChanged);
    if (_live) unawaited(_open(refreshSources: true));
  }

  void _notifyDetails() {
    final visible = _selected != null;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onDetailsChanged?.call(visible);
    });
  }

  void _playlistFocusChanged() {
    if (mounted) setState(() {});
  }

  void _onSongScroll() {
    // Hysteresis: the header shrinking changes the viewport, so leaving the
    // compact state needs the list to be back near the top.
    final offset = _songScroll.hasClients ? _songScroll.offset : 0.0;
    // Scrolling the list takes over again once it has moved well past the top.
    if (_pinnedCompact && offset > 80) _pinnedCompact = false;
    final compact = _pinnedCompact
        ? true
        : (_compactPlaylist ? offset > 8 : offset > 80);
    if (mounted && compact != _compactPlaylist) {
      setState(() => _compactPlaylist = compact);
    }
    _maybeMore();
  }

  /// Wide headers also collapse (up) and expand (down) when swiped directly.
  Widget _swipeHeader(Widget child) => !_wide
      ? child
      : GestureDetector(
          behavior: HitTestBehavior.translucent,
          onVerticalDragStart: (_) => _headerDrag = 0,
          onVerticalDragUpdate: (details) {
            _headerDrag += details.delta.dy;
            if (_headerDrag < -24 && !_compactPlaylist) {
              _headerDrag = 0;
              setState(() => _pinnedCompact = _compactPlaylist = true);
            } else if (_headerDrag > 24 && _compactPlaylist) {
              _headerDrag = 0;
              setState(() {
                _pinnedCompact = _compactPlaylist = false;
                if (_songScroll.hasClients) _songScroll.jumpTo(0);
              });
            }
          },
          child: child,
        );

  void _closeDetails() {
    final menu = _playlistMenu;
    _playlistMenu = null;
    if (menu != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (menu.isActive) menu.navigator?.removeRoute(menu);
      });
    }
    _compactPlaylist = false;
    _pinnedCompact = false;
    _trackLabels = {};
    _description = '';
    _filter.clear();
    _detailEpoch++;
    _selectingSongs = false;
    _selectedSongs = {};
    _details?.close();
    _details?.dispose();
    _details = null;
    _selected = null;
    _notifyDetails();
    _tracks = [];
    _singerIds = {};
    _playing = false;
    _confirm = false;
    _actionNote = null;
  }

  void _close() {
    _epoch++;
    _sourceSelection?.close();
    _sourceSelection?.dispose();
    _sourceSelection = null;
    _selectingSource = false;
    _closeDetails();
    _session?.close();
    _session?.dispose();
    _session = null;
    _loading = false;
    _categoriesOpen = false;
  }

  @override
  void didUpdateWidget(DiscoveryView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.selectionRevision != widget.selectionRevision) {
      _selectionDirty = true;
    }
    if (!widget.active || !widget.ready) {
      if (oldWidget.active || oldWidget.ready) _close();
    }
    if (_live &&
        (!oldWidget.active ||
            !oldWidget.ready ||
            oldWidget.sourceRevision != widget.sourceRevision ||
            oldWidget.selectionRevision != widget.selectionRevision)) {
      unawaited(_open(refreshSources: true));
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live) unawaited(_open(refreshSources: true));
    } else {
      _foreground = false;
      _close();
    }
  }

  bool _valid(int epoch) => _live && epoch == _epoch;
  Future<void> _open({
    bool refreshSources = false,
    bool preserveCategories = false,
  }) async {
    if (!_live || _artistRoute || _modalBusy) return;
    refreshSources = refreshSources || _selectionDirty;
    // Category-only navigation keeps the strip and its scroll position stable.
    // Source refreshes must still invalidate the source-owned categories.
    final keepCategories =
        preserveCategories && !refreshSources && _categories != null;
    _selectionDirty = false;
    if (widget.openDetails != null) {
      await _openExternal();
      return;
    }
    _close();
    final epoch = _epoch;
    setState(() {
      _loading = true;
      _error = null;
      _categoryError = null;
      _cards = [];
      if (!keepCategories) _categories = null;
      _more = true;
    });
    api.DiscoverySession? candidate;
    try {
      if (refreshSources) {
        final sources = await api.discoverySources();
        if (!_valid(epoch)) return;
        _sources = sources;
        final source =
            _sources
                .where((s) => s.id == widget.selectedSource)
                .firstOrNull
                ?.id ??
            _sources.firstOrNull?.id;
        if (_source != source) {
          _source = source;
          _category = '';
          _categoryName = '';
        }
      }
      if (_source == null) {
        setState(() => _loading = false);
        return;
      }
      candidate = await api.beginDiscovery(
        source: _source!,
        category: _category,
        charts: _charts,
      );
      if (!_valid(epoch)) return;
      final session = candidate;
      _session = session;
      candidate = null;
      if (!_charts && !keepCategories) {
        unawaited(_loadCategories(session, epoch));
      }
      if (!_valid(epoch)) return;
      final page = await session.next();
      if (!_valid(epoch)) return;
      setState(() {
        _cards = page.cards;
        _more = page.hasMore;
        _loading = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_valid(epoch)) {
          if (_scroll.hasClients) _scroll.jumpTo(0);
          _maybeMore();
        }
      });
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Unable to load discovery. Retry or refresh the source.'
              : '发现内容加载失败，请重试或刷新来源。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _openExternal() async {
    _close();
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _selected = widget.initialCard;
      _notifyDetails();
      _loading = true;
      _error = null;
      _more = true;
      _total = 0;
    });
    api.OnlinePlaylistSession? candidate;
    try {
      candidate = await widget.openDetails!();
      if (!_valid(epoch) || detailEpoch != _detailEpoch) return;
      final details = candidate;
      _details = details;
      candidate = null;
      final page = await details.next();
      if (!_valid(epoch) || detailEpoch != _detailEpoch) return;
      setState(() {
        _tracks = page.tracks;
        _trackLabels = {for (final label in page.labels) label.id: label};
        _description = page.description;
        _singerIds = page.singerIds.toSet();
        _total = page.total;
        _more = page.hasMore;
        _loading = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_valid(epoch) && detailEpoch == _detailEpoch) _maybeMore();
      });
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Playlist unavailable. Retry or go back.'
              : '歌单加载失败，请重试或返回。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _loadCategories(api.DiscoverySession session, int epoch) async {
    try {
      final categories = await session.categories();
      if (!_valid(epoch) || !identical(session, _session)) return;
      setState(() => _categories = categories);
    } catch (_) {
      if (!_valid(epoch) || !identical(session, _session)) return;
      setState(() {
        _categoryError = p.english
            ? 'Categories unavailable. Refresh to retry.'
            : '分类暂不可用，可刷新重试。';
      });
    }
  }

  void _maybeMore() {
    if (!_live ||
        _artistRoute ||
        _modalBusy ||
        _confirm ||
        _loading ||
        _playing ||
        !_more ||
        _error != null ||
        _categoriesOpen) {
      return;
    }
    final controller = _selected == null ? _scroll : _songScroll;
    if (controller.hasClients && controller.position.extentAfter < 600) {
      unawaited(_next());
    }
  }

  Future<void> _next() async {
    if (!_live ||
        _loading ||
        _playing ||
        !_more ||
        _modalBusy ||
        _confirm ||
        _artistRoute) {
      return;
    }
    final session = _session, details = _details;
    if (_selected == null ? session == null : details == null) return;
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      if (_selected != null) {
        final page = await details!.next();
        if (!_valid(epoch) ||
            detailEpoch != _detailEpoch ||
            !identical(details, _details)) {
          return;
        }
        setState(() {
          _tracks = page.tracks;
          _trackLabels = {for (final label in page.labels) label.id: label};
          _description = page.description;
          _singerIds = page.singerIds.toSet();
          _total = page.total;
          _more = page.hasMore;
          _loading = false;
        });
      } else {
        final page = await session!.next();
        if (!_valid(epoch) ||
            detailEpoch != _detailEpoch ||
            !identical(session, _session)) {
          return;
        }
        setState(() {
          _cards = page.cards;
          _more = page.hasMore;
          _loading = false;
        });
      }
      WidgetsBinding.instance.addPostFrameCallback((_) => _maybeMore());
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Page failed. Retry keeps the current results.'
              : '分页加载失败，重试将保留当前结果。';
        });
      }
    }
  }

  Future<void> _openCard(api.DiscoveryCard card) async {
    final session = _session;
    if (!_live || _loading || session == null) return;
    if (MioDeviceScope.isTv(context)) _tvReturnCard = card.id;
    _closeDetails();
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _selected = card;
      _notifyDetails();
      _loading = true;
      _error = null;
      _more = true;
      _total = 0;
    });
    api.OnlinePlaylistSession? candidate;
    try {
      candidate = await session.open(id: card.id);
      if (!_valid(epoch) || detailEpoch != _detailEpoch) return;
      final details = candidate;
      _details = details;
      candidate = null;
      final page = await details.next();
      if (!_valid(epoch) || detailEpoch != _detailEpoch) return;
      setState(() {
        _tracks = page.tracks;
        _trackLabels = {for (final label in page.labels) label.id: label};
        _description = page.description;
        _singerIds = page.singerIds.toSet();
        _total = page.total;
        _more = page.hasMore;
        _loading = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_valid(epoch) && detailEpoch == _detailEpoch) {
          if (_songScroll.hasClients) _songScroll.jumpTo(0);
          _maybeMore();
        }
      });
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Playlist unavailable. Go back and retry.'
              : '歌单加载失败，请返回后重试。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  void _back() {
    if (_modalBusy) return;
    if (_confirm) {
      setState(() => _confirm = false);
      return;
    }
    if (widget.onBack != null) {
      _close();
      widget.onBack!();
      return;
    }
    setState(() {
      _closeDetails();
      _loading = false;
      _error = null;
      _more = _browseMore;
    });
    if (MioDeviceScope.isTv(context)) _restoreTvCardFocus();
  }

  bool _browseMore = true;
  Future<void> _selectSource(String source) async {
    if (!_live || _selectingSource || source == widget.selectedSource) return;
    final player = widget.core.player;
    if (player == null) return;
    final epoch = _epoch;
    setState(() => _selectingSource = true);
    api.SourceSelectionSession? candidate;
    var published = false;
    try {
      candidate = await api.beginSourceSelection(
        sourceRevision: player.sourceRevision,
        selectionRevision: player.sourceSelectionRevision,
      );
      if (!_valid(epoch)) return;
      _sourceSelection = candidate;
      published = true;
      await candidate.select(source: source);
      // The owner snapshot reopens discovery with the persisted preference.
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _error = p.english
              ? 'Unable to save source. Retry.'
              : '音源保存失败，请重试。',
        );
      }
    } finally {
      if (!published || identical(_sourceSelection, candidate)) {
        _sourceSelection = null;
        candidate?.close();
        candidate?.dispose();
      }
      if (_valid(epoch)) setState(() => _selectingSource = false);
    }
  }

  void _selectCategory(String id, String name) {
    _categoriesOpen = false;
    _category = id;
    _categoryName = name;
    _restoreMoreFocus();
    unawaited(_open(preserveCategories: !_wide));
  }

  Future<void> _play(api.Track? track) async {
    final details = _details;
    if (!_live ||
        _loading ||
        _playing ||
        _modalBusy ||
        _artistRoute ||
        details == null) {
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    if (track != null) {
      final choice = await choosePlayChoice(
        context: context,
        palette: p,
        core: widget.core,
        title: track.title,
      );
      if (choice == null ||
          !mounted ||
          !_valid(epoch) ||
          detailEpoch != _detailEpoch ||
          !identical(details, _details)) {
        return;
      }
      if (choice == PlayChoice.queue) {
        try {
          await details.append(id: track.id);
          if (mounted) {
            showMioToast(context, p, p.english ? 'Added to queue' : '已加入播放列表');
          }
        } catch (_) {
          if (mounted && _valid(epoch) && detailEpoch == _detailEpoch) {
            setState(
              () => _actionNote = p.english
                  ? 'Could not add the song. Retry or reopen this page.'
                  : '无法加入播放列表，请重试或重新打开页面。',
            );
          }
        }
        return;
      }
    }
    setState(() {
      _playing = true;
      _confirm = false;
      _actionNote = null;
    });
    try {
      if (track == null) {
        final query = _filter.text.trim();
        final shuffle = _shuffleRequested;
        // Keep partial collections out of the queue. The bounded Rust cursor
        // admits every page before the existing owner commits playback.
        while (_more) {
          final page = await details.next();
          if (!_valid(epoch) ||
              detailEpoch != _detailEpoch ||
              !identical(details, _details)) {
            return;
          }
          setState(() {
            _tracks = page.tracks;
            _trackLabels = {for (final label in page.labels) label.id: label};
            _description = page.description;
            _singerIds = page.singerIds.toSet();
            _total = page.total;
            _more = page.hasMore;
          });
        }
        if (!_valid(epoch) ||
            detailEpoch != _detailEpoch ||
            !identical(details, _details)) {
          return;
        }
        await details.playFiltered(query: query, shuffle: shuffle);
      } else {
        await details.play(id: track.id);
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Playback failed. Retry or reopen this page.'
              : '播放失败，请重试或重新打开页面。',
        );
      }
    } finally {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _playing = false);
      }
    }
  }

  Future<void> _appendSelected() async {
    final details = _details;
    if (!_live ||
        _modalBusy ||
        _loading ||
        _playing ||
        _artistRoute ||
        _confirm ||
        details == null ||
        _selectedSongs.isEmpty) {
      return;
    }
    final epoch = _epoch,
        detailEpoch = _detailEpoch,
        revision = widget.sourceRevision;
    final ids = _tracks
        .where((t) => _selectedSongs.contains(t.id))
        .map((t) => t.id)
        .toList();
    setState(() {
      _modalBusy = true;
      _actionNote = null;
    });
    try {
      final saved = await showOnlineAppend(
        context: context,
        prepare: () => details.prepareSelectedSave(ids: ids),
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(details, _details),
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
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Unable to prepare selected songs. Reopen this page.'
              : '无法准备所选歌曲，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_live &&
            (!_hasSession ||
                revision != widget.sourceRevision ||
                _selectionDirty)) {
          unawaited(_open(refreshSources: true));
        } else if (_live) {
          _maybeMore();
        }
      }
    }
  }

  Future<void> _saveLoaded() async {
    final details = _details, card = _selected;
    if (!_live ||
        _modalBusy ||
        _loading ||
        _playing ||
        _artistRoute ||
        _confirm ||
        details == null ||
        card == null ||
        _tracks.isEmpty) {
      return;
    }
    final epoch = _epoch,
        detailEpoch = _detailEpoch,
        revision = widget.sourceRevision;
    setState(() {
      _modalBusy = true;
      _actionNote = null;
    });
    try {
      final saved = await showOnlineSave(
        context: context,
        session: details,
        palette: p,
        name: card.title,
        cover: card.cover,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(details, _details),
      );
      if (saved != null && _valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Saved ${saved.count} songs to ${saved.title}'
              : '已将 ${saved.count} 首歌曲保存到 ${saved.title}',
        );
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Unable to prepare saved songs. Reopen this page.'
              : '无法准备歌曲保存，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_live &&
            (!_hasSession ||
                revision != widget.sourceRevision ||
                _selectionDirty)) {
          unawaited(_open(refreshSources: true));
        } else if (_live) {
          _maybeMore();
        }
      }
    }
  }

  Future<void> _downloadLoaded({api.Track? track}) async {
    final details = _details, card = _selected;
    if (!_live ||
        _modalBusy ||
        _loading ||
        _playing ||
        _artistRoute ||
        _confirm ||
        details == null ||
        card == null ||
        _tracks.isEmpty) {
      return;
    }
    final epoch = _epoch,
        detailEpoch = _detailEpoch,
        revision = widget.sourceRevision;
    setState(() {
      _modalBusy = true;
      _actionNote = null;
    });
    try {
      final report = await showOnlineDownloads(
        context: context,
        session: details,
        tracks: track == null ? _tracks : [track],
        core: widget.core,
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(details, _details),
      );
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _actionNote = report.message(p));
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Unable to prepare downloads. Reopen this page.'
              : '无法准备下载，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_live &&
            (!_hasSession ||
                revision != widget.sourceRevision ||
                _selectionDirty)) {
          unawaited(_open(refreshSources: true));
        } else if (_live) {
          _maybeMore();
        }
      }
    }
  }

  Future<void> _downloadComplete() async {
    final details = _details, card = _selected;
    if (!_live ||
        _modalBusy ||
        _loading ||
        _playing ||
        _artistRoute ||
        _confirm ||
        details == null ||
        card == null) {
      return;
    }
    final epoch = _epoch,
        detailEpoch = _detailEpoch,
        revision = widget.sourceRevision;
    setState(() {
      _modalBusy = true;
      _actionNote = null;
    });
    try {
      final report = await showCompleteOnlineDownloads(
        context: context,
        session: details,
        core: widget.core,
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(details, _details),
      );
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _actionNote = report?.message(p));
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionNote = p.english
              ? 'Unable to prepare downloads. Reopen this page.'
              : '无法准备下载，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_live &&
            (!_hasSession ||
                revision != widget.sourceRevision ||
                _selectionDirty)) {
          unawaited(_open(refreshSources: true));
        } else if (_live) {
          _maybeMore();
        }
      }
    }
  }

  Widget _button(String label, VoidCallback? action, {bool selected = false}) =>
      MioButton(
        palette: p,
        label: label,
        onPressed: action,
        background: selected ? Color.lerp(p.surface, p.brand, .18) : p.surface,
        child: Text(label),
      );
  Widget _errorBar() => Semantics(
    liveRegion: true,
    child: Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
        _button(
          p.english ? 'Retry' : '重试',
          _live && !_loading && (_selected == null || _details != null)
              ? () => unawaited(
                  !_hasSession || (_selected == null && _cards.isEmpty)
                      ? _open(refreshSources: true)
                      : _next(),
                )
              : null,
        ),
        _button(
          p.text('common.refresh'),
          _live ? () => unawaited(_open(refreshSources: true)) : null,
        ),
      ],
    ),
  );
  void _switchBrowseTab(bool charts) {
    if (_charts == charts) return;
    // Rebuild right away and drop the other tab's cards so a blocked reload
    // can never leave stale content under the new heading.
    setState(() {
      // The browse page is only tappable when no artist page or modal sheet
      // is above it, so these flags are stale here.
      _artistRoute = false;
      _modalBusy = false;
      _charts = charts;
      _cards = [];
    });
    unawaited(_open());
  }

  void _openCategories() => setState(() {
    _group = 0;
    _categoriesOpen = true;
  });
  void _closeCategories() {
    setState(() => _categoriesOpen = false);
    _restoreMoreFocus();
  }

  /// The picker's pills unmount with it, so hand focus back to its opener.
  void _restoreMoreFocus() {
    if (!MioDeviceScope.isTv(context)) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && !_categoriesOpen && _tvMoreFocus.context != null) {
        _tvMoreFocus.requestFocus();
      }
    });
  }

  void _showCategoryGroup(int index) => setState(() => _group = index);

  Widget _browseTab(bool charts) {
    final selected = _charts == charts;
    final label = p.text(
      charts ? 'music.find.tabLeaderboard' : 'music.find.tabPlaylist',
    );
    return Expanded(
      child: Semantics(
        selected: selected,
        child: MioButton(
          palette: p,
          label: label,
          radius: _wide ? 6 : 999,
          insetFocusBorder: false,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          background: selected ? p.surface : const Color(0x00000000),
          onPressed: _live ? () => _switchBrowseTab(charts) : null,
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: _wide ? 20 : 24),
            child: Center(
              child: Text(
                label,
                style: TextStyle(
                  fontSize: _wide ? 14 : 15,
                  height: 17 / (_wide ? 14 : 15),
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                  color: selected ? p.brand : p.secondary,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _categoryChip(
    String label,
    VoidCallback? action, {
    bool selected = false,
    Color? background,
  }) => Semantics(
    selected: selected,
    child: DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(_wide ? 6 : 999),
        border: _wide ? null : Border.all(color: p.border),
      ),
      child: MioButton(
        palette: p,
        label: label,
        onPressed: action,
        radius: _wide ? 6 : 999,
        insetFocusBorder: false,
        background: selected
            ? p.color('--td-brand-color-light')
            : (background ?? (_wide ? const Color(0x00000000) : p.surface)),
        padding: EdgeInsets.symmetric(horizontal: _wide ? 12 : 14),
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: _wide ? 30 : 44),
          child: Center(
            widthFactor: _wide ? 1 : null,
            child: Text(
              label,
              style: TextStyle(
                fontSize: _wide ? 13 : 14,
                color: selected ? p.brand : p.secondary,
              ),
            ),
          ),
        ),
      ),
    ),
  );

  Widget _browse() => MioDeviceScope.isTv(context)
      ? _browseTv()
      : _wide
      ? _browseWide()
      : _browseNarrow();

  // The page title and subtitle are gone: the shell header already names the
  // page, so the tabs start the content right away.
  Widget _browseNarrow() => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      const SizedBox(height: 4),
      // The shell search header owns source selection on touch/pointer layouts.
      if (MioDeviceScope.isTv(context)) ...[
        SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            children: [
              for (final source in _sources)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: _button(
                    source.name,
                    _live && !_selectingSource
                        ? () => unawaited(_selectSource(source.id))
                        : null,
                    selected: source.id == _source,
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 12),
      ],
      Container(
        padding: EdgeInsets.all(_wide ? 3 : 4),
        decoration: BoxDecoration(
          color: p.color(
            _wide
                ? '--td-bg-color-secondarycontainer'
                : '--mobile-glass-bg-strong',
          ),
          borderRadius: BorderRadius.circular(_wide ? 8 : 999),
          border: _wide
              ? null
              : Border.all(color: p.color('--mobile-glass-border'), width: .5),
        ),
        child: Row(children: [_browseTab(false), _browseTab(true)]),
      ),
      if (!_charts) ...[
        const SizedBox(height: 24),
        ShaderMask(
          blendMode: BlendMode.dstIn,
          shaderCallback: (bounds) => LinearGradient(
            colors: const [Color(0xffffffff), Color(0x00ffffff)],
            stops: [(1 - 32 / math.max(32, bounds.width)).clamp(0, 1), 1],
          ).createShader(bounds),
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.only(right: 32),
            child: Row(
              children: [
                _categoryChip(
                  p.text('music.playlistCategory.hot'),
                  _live ? () => _selectCategory('', '') : null,
                  selected: _category.isEmpty,
                ),
                for (final category
                    in _categories?.hot ?? <api.DiscoveryCategory>[])
                  Padding(
                    padding: const EdgeInsets.only(left: 8),
                    child: _categoryChip(
                      category.name,
                      _live
                          ? () => _selectCategory(category.id, category.name)
                          : null,
                      selected: category.id == _category,
                    ),
                  ),
                if (_categories?.groups.isNotEmpty == true)
                  Padding(
                    padding: const EdgeInsets.only(left: 8),
                    child: _categoryChip(
                      '${p.text('music.playlistCategory.moreCategory')} ▾',
                      _live
                          ? () => setState(() {
                              _group = 0;
                              _categoriesOpen = true;
                            })
                          : null,
                    ),
                  ),
              ],
            ),
          ),
        ),
        if (_categoryError != null)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(_categoryError!, style: TextStyle(color: p.secondary)),
          ),
        const SizedBox(height: 26),
      ] else
        const SizedBox(height: 16),
      Text(
        _charts
            ? p.text('music.leaderboard.title')
            : p
                  .text('music.playlistCategory.hotPlaylists')
                  .replaceAll(
                    '{name}',
                    _category.isEmpty
                        ? p.text('music.playlistCategory.hot')
                        : _categoryName,
                  ),
        style: TextStyle(
          fontSize: _wide ? 20 : 17,
          fontWeight: FontWeight.w700,
        ),
      ),
      const SizedBox(height: 12),
      if (_error != null) _errorBar(),
      Expanded(
        child: _loading && _cards.isEmpty
            ? _skeletons()
            : _cards.isEmpty
            ? Center(
                child: Text(
                  p.text(
                    _charts
                        ? 'music.leaderboard.noData'
                        : 'music.playlistCategory.noPlaylists',
                  ),
                ),
              )
            : _cardList(),
      ),
      if (_loading && _cards.isNotEmpty) Text(p.english ? 'Loading…' : '加载中…'),
    ],
  );

  Widget _skeletons() => LayoutBuilder(
    builder: (context, box) {
      final rem = MioPalette.rem(widget.viewportWidth);
      final gap = _wide ? (_charts ? 24.0 : 12.0) : 12.0;
      final columns = _wide
          ? math.max(
              1,
              ((box.maxWidth + gap) / ((_charts ? 160 : 136) + gap)).floor(),
            )
          : (_charts ? 2 : 1);
      final width = (box.maxWidth - (columns - 1) * gap) / columns;
      return GridView.builder(
        itemCount: columns * 3,
        gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: columns,
          mainAxisSpacing: gap,
          crossAxisSpacing: gap,
          mainAxisExtent: _charts
              ? width
              : (_wide
                    ? width +
                          _wideCardInfoHeight(
                            rem,
                            MediaQuery.textScalerOf(context),
                          )
                    : 104),
        ),
        itemBuilder: (context, index) => ExcludeSemantics(
          child: Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: p.surface,
              border: Border.all(color: p.border),
              borderRadius: BorderRadius.circular(16),
            ),
            child: _wide || _charts
                ? const SizedBox.shrink()
                : Row(
                    children: [
                      Container(
                        width: 84,
                        height: 84,
                        decoration: BoxDecoration(
                          color: p.color('--td-bg-color-secondarycontainer'),
                          borderRadius: BorderRadius.circular(12),
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Container(
                              height: 18,
                              color: p.color(
                                '--td-bg-color-secondarycontainer',
                              ),
                            ),
                            const SizedBox(height: 10),
                            FractionallySizedBox(
                              widthFactor: .6,
                              child: Container(
                                height: 12,
                                color: p.color(
                                  '--td-bg-color-secondarycontainer',
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
          ),
        ),
      );
    },
  );

  String _playCount(int count) => count >= 10000
      ? '${(count / 10000).toStringAsFixed(1)}${p.english ? 'w' : '万'}'
      : '$count';

  Widget _chartCard(api.DiscoveryCard card, double width) => ClipRRect(
    borderRadius: BorderRadius.circular(12),
    child: MioButton(
      palette: p,
      label: card.title,
      radius: 12,
      insetFocusBorder: false,
      padding: EdgeInsets.zero,
      onPressed: _live && !_loading
          ? () {
              _browseMore = _more;
              unawaited(_openCard(card));
            }
          : null,
      child: Stack(
        fit: StackFit.expand,
        children: [
          MioCover(card.cover, size: width, radius: 12),
          Positioned(
            left: 0,
            right: 0,
            bottom: 0,
            child: Container(
              padding: const EdgeInsets.fromLTRB(12, 40, 12, 12),
              decoration: const BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [Color(0x00000000), Color(0xb3000000)],
                ),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    card.title,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: Color(0xffffffff),
                    ),
                  ),
                  if (card.description.isNotEmpty) ...[
                    const SizedBox(height: 4),
                    Text(
                      card.description,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 12,
                        color: Color(0xffffffff),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    ),
  );

  /// Compact wide playlist card: square cover, a two-line title and one meta
  /// row. Descriptions stay on the detail page so more cards fit on screen.
  double _wideCardInfoHeight(double rem, TextScaler scale) =>
      8 + scale.scale(13) * 1.3 * 2 + 6 + scale.scale(11) * 1.4 + 8;

  Widget _wideDiscoveryCard(api.DiscoveryCard card, double width, double rem) {
    final placeholder = p.color('--td-text-color-placeholder');
    final songs = card.songCount ?? 0;
    Widget meta(Widget icon, String text) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        icon,
        const SizedBox(width: 3),
        Text(
          text,
          style: TextStyle(fontSize: 11, height: 1.4, color: placeholder),
        ),
      ],
    );
    return DecoratedBox(
      decoration: BoxDecoration(
        // Lift the card off the page: a faint tint of the text colour plus a
        // hairline border, so it reads on any cover-derived background.
        color: Color.alphaBlend(
          p.color('--td-text-color-primary').withValues(alpha: .07),
          p.color('--td-bg-color-page'),
        ),
        border: Border.all(
          color: p.color('--td-text-color-primary').withValues(alpha: .1),
        ),
        borderRadius: BorderRadius.circular(10),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(9),
        child: MioButton(
          palette: p,
          label: card.title,
          radius: 10,
          insetFocusBorder: false,
          padding: EdgeInsets.zero,
          onPressed: _live && !_loading
              ? () {
                  _browseMore = _more;
                  unawaited(_openCard(card));
                }
              : null,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              MioCover(card.cover, size: width, radius: 0),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        card.title,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 13,
                          height: 1.3,
                          fontWeight: FontWeight.w600,
                          color: p.color('--td-text-color-primary'),
                        ),
                      ),
                      const Spacer(),
                      if (card.playCount != null || songs > 0)
                        Row(
                          children: [
                            if (card.playCount != null)
                              meta(
                                MioIcon('Play', size: 11, color: placeholder),
                                _playCount(card.playCount!),
                              ),
                            const Spacer(),
                            if (songs > 0)
                              Text(
                                p.english ? '$songs songs' : '$songs 首',
                                style: TextStyle(
                                  fontSize: 11,
                                  height: 1.4,
                                  color: placeholder,
                                ),
                              ),
                          ],
                        ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _browseWide() {
    final rem = MioPalette.rem(widget.viewportWidth);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // The shell search header owns source selection on touch/pointer layouts.
        if (MioDeviceScope.isTv(context)) ...[
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                for (final source in _sources)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: _button(
                      source.name,
                      _live && !_selectingSource
                          ? () => unawaited(_selectSource(source.id))
                          : null,
                      selected: source.id == _source,
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 12),
        ],
        Container(
          padding: const EdgeInsets.all(3),
          decoration: BoxDecoration(
            color: p.color('--td-bg-color-secondarycontainer'),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Row(
            children: [
              _browseTab(false),
              const SizedBox(width: 2),
              _browseTab(true),
            ],
          ),
        ),
        SizedBox(height: rem),
        if (!_charts) ...[
          // One row: the category chips scroll sideways and "more" stays pinned.
          Row(
            key: _categoryRowKey,
            children: [
              Expanded(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    children: [
                      _categoryChip(
                        p.text('music.playlistCategory.hot'),
                        _live ? () => _selectCategory('', '') : null,
                        selected: _category.isEmpty,
                      ),
                      for (final category
                          in _categories?.hot ?? <api.DiscoveryCategory>[]) ...[
                        const SizedBox(width: 8),
                        _categoryChip(
                          category.name,
                          _live
                              ? () =>
                                    _selectCategory(category.id, category.name)
                              : null,
                          selected: category.id == _category,
                        ),
                      ],
                    ],
                  ),
                ),
              ),
              if (_categories?.groups.isNotEmpty == true) ...[
                const SizedBox(width: 8),
                _categoryChip(
                  '${p.text('music.playlistCategory.moreCategory')} ▾',
                  _live
                      ? () => setState(() {
                          _group = 0;
                          _categoriesOpen = true;
                        })
                      : null,
                  background: p.color('--td-bg-color-secondarycontainer'),
                ),
              ],
            ],
          ),
          if (_categoryError != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                _categoryError!,
                style: TextStyle(color: p.secondary),
              ),
            ),
          SizedBox(height: rem),
        ],
        Padding(
          padding: EdgeInsets.only(bottom: rem),
          child: Text(
            _charts
                ? p.text('music.leaderboard.title')
                : p
                      .text('music.playlistCategory.hotPlaylists')
                      .replaceAll(
                        '{name}',
                        _category.isEmpty
                            ? p.text('music.playlistCategory.hot')
                            : _categoryName,
                      ),
            style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
          ),
        ),
        if (_error != null) _errorBar(),
        Expanded(
          child: _loading && _cards.isEmpty
              ? _skeletons()
              : _cards.isEmpty
              ? Center(
                  child: Text(
                    p.text(
                      _charts
                          ? 'music.leaderboard.noData'
                          : 'music.playlistCategory.noPlaylists',
                    ),
                  ),
                )
              : _cardList(),
        ),
        if (_loading && _cards.isNotEmpty)
          Text(p.english ? 'Loading…' : '加载中…'),
      ],
    );
  }

  Widget _discoveryCard(api.DiscoveryCard card, double width) {
    if (card.chart) return _chartCard(card, width);
    final info = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Text(
          card.title,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            fontSize: _wide ? 16 : 15,
            height: 1.4,
            fontWeight: FontWeight.w600,
          ),
        ),
        if (card.description.isNotEmpty) ...[
          const SizedBox(height: 6),
          Text(
            card.description,
            maxLines: _wide ? 2 : 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: _wide ? 14 : 13,
              height: 1.4,
              color: p.secondary,
            ),
          ),
        ],
        if (card.playCount != null || (card.songCount ?? 0) > 0) ...[
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              if (card.playCount != null)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    MioIcon(
                      'Play',
                      size: 14,
                      color: p.color('--td-text-color-placeholder'),
                    ),
                    const SizedBox(width: 4),
                    Text(
                      _playCount(card.playCount!),
                      style: TextStyle(
                        fontSize: 12,
                        color: p.color('--td-text-color-placeholder'),
                      ),
                    ),
                  ],
                ),
              if ((card.songCount ?? 0) > 0)
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: p.color('--td-bg-color-secondarycontainer'),
                    borderRadius: BorderRadius.circular(6),
                  ),
                  child: Text(
                    p.english
                        ? '${card.songCount} songs'
                        : '${card.songCount} 首',
                    style: TextStyle(fontSize: 12, color: p.secondary),
                  ),
                ),
            ],
          ),
        ],
      ],
    );
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 104),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.surface,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: p.border),
        ),
        child: MioButton(
          palette: p,
          label: card.title,
          radius: 16,
          insetFocusBorder: false,
          padding: const EdgeInsets.all(10),
          onPressed: _live && !_loading
              ? () {
                  _browseMore = _more;
                  unawaited(_openCard(card));
                }
              : null,
          child: _wide
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(
                      width: width - 20,
                      height: 180,
                      child: FittedBox(
                        fit: BoxFit.cover,
                        clipBehavior: Clip.hardEdge,
                        child: MioCover(
                          card.cover,
                          size: width - 20,
                          radius: 12,
                        ),
                      ),
                    ),
                    const SizedBox(height: 12),
                    Expanded(child: info),
                  ],
                )
              : Row(
                  children: [
                    MioCover(card.cover, size: 84, radius: 12),
                    const SizedBox(width: 12),
                    Expanded(child: info),
                  ],
                ),
        ),
      ),
    );
  }

  Widget _cardList() => LayoutBuilder(
    builder: (context, box) {
      final rem = MioPalette.rem(widget.viewportWidth);
      final gap = _wide ? (_charts ? 24.0 : 12.0) : (_charts ? 10.0 : 12.0);
      final columns = _wide
          ? math.max(
              1,
              ((box.maxWidth + gap) / ((_charts ? 160 : 136) + gap)).floor(),
            )
          : (_charts ? 2 : 1);
      final width = (box.maxWidth - (columns - 1) * gap) / columns;
      final scale = MediaQuery.textScalerOf(context);
      final height = _charts
          ? width
          : _wide
          ? width + _wideCardInfoHeight(rem, scale)
          : 180.0 + 36 + math.max(130.0, scale.scale(16) * 7);
      final count = _cards.length + (_more ? 1 : 0);
      Widget item(BuildContext context, int index) {
        if (index == _cards.length) {
          return _button(
            p.english ? 'Load more' : '加载更多',
            _live && !_loading ? () => unawaited(_next()) : null,
          );
        }
        return _wide && !_cards[index].chart
            ? _wideDiscoveryCard(_cards[index], width, rem)
            : _discoveryCard(_cards[index], width);
      }

      return RawScrollbar(
        controller: _scroll,
        thickness: 8,
        radius: const Radius.circular(999),
        thumbColor: p
            .color('--td-text-color-placeholder')
            .withValues(alpha: .3),
        child: !_wide && !_charts
            ? ListView.separated(
                controller: _scroll,
                itemCount: count,
                separatorBuilder: (_, _) => SizedBox(height: gap),
                itemBuilder: item,
              )
            : GridView.builder(
                controller: _scroll,
                itemCount: count,
                gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                  crossAxisCount: columns,
                  mainAxisSpacing: gap,
                  crossAxisSpacing: gap,
                  mainAxisExtent: height,
                ),
                itemBuilder: item,
              ),
      );
    },
  );
  Widget _artistLink(api.Track track) {
    final session = _details, epoch = _epoch, detailEpoch = _detailEpoch;
    return ArtistLink(
      name: track.artist,
      available: _singerIds.contains(track.id),
      active:
          _live &&
          !_artistRoute &&
          !_modalBusy &&
          !_confirm &&
          !_loading &&
          !_playing &&
          session != null,
      identity: (track.id, session),
      core: widget.core,
      palette: p,
      isCurrent: () =>
          !_artistRoute &&
          _valid(epoch) &&
          detailEpoch == _detailEpoch &&
          identical(session, _details),
      open: () => session!.openSinger(id: track.id),
      onRouteChanged: _artistVisibility,
    );
  }

  void _artistVisibility(bool visible) {
    if (!mounted) return;
    setState(() {
      _artistRoute = visible;
      if (visible) _routeRevision = widget.sourceRevision;
    });
    if (!visible && _live) {
      if (!_hasSession ||
          _routeRevision != widget.sourceRevision ||
          _selectionDirty) {
        unawaited(_open(refreshSources: true));
      } else {
        _maybeMore();
      }
    }
  }

  bool get _playlistEnabled =>
      _live &&
      !_loading &&
      !_playing &&
      !_modalBusy &&
      !_confirm &&
      !_artistRoute &&
      _details != null;

  Future<void> _playlistActions(Offset anchor) async {
    final details = _details;
    if (!_playlistEnabled || details == null) return;
    final epoch = _epoch, detailEpoch = _detailEpoch;
    final sourceRevision = widget.sourceRevision,
        selectionRevision = widget.selectionRevision;
    final originFocus = FocusManager.instance.primaryFocus;
    final menu = RawDialogRoute<_PlaylistAction>(
      barrierDismissible: true,
      barrierLabel: p.text('common.close'),
      barrierColor: const Color(0x00000000),
      transitionDuration: Duration.zero,
      pageBuilder: (_, _, _) => MioContextMenu<_PlaylistAction>(
        palette: p,
        anchor: anchor,
        label: p.english ? 'Playlist actions' : '歌单操作',
        entries: [
          MioMenuEntry(
            value: _PlaylistAction.save,
            label: p.english ? 'Save loaded songs' : '保存已加载歌曲',
            icon: 'PlaylistAdd',
            enabled: _tracks.isNotEmpty,
          ),
          MioMenuEntry(
            value: _PlaylistAction.download,
            label: p.english ? 'Download loaded songs' : '下载已加载歌曲',
            icon: 'Download',
            enabled: _tracks.isNotEmpty,
          ),
          MioMenuEntry(
            value: _PlaylistAction.downloadAll,
            label: p.english ? 'Download all songs' : '下载全部歌曲',
            icon: 'Download',
            enabled: _tracks.isNotEmpty,
          ),
          MioMenuEntry(
            value: _PlaylistAction.select,
            label: p.text('common.batchSelect'),
            icon: 'TD-Check',
            enabled: _tracks.isNotEmpty,
          ),
        ],
      ),
    );
    _playlistMenu = menu;
    setState(() => _modalBusy = true);
    _PlaylistAction? action;
    try {
      action = await Navigator.of(context).push(menu);
      await menu.completed;
    } finally {
      if (identical(_playlistMenu, menu)) _playlistMenu = null;
      if (mounted) setState(() => _modalBusy = false);
      if (mounted &&
          MioDeviceScope.isTv(context) &&
          originFocus?.context != null) {
        originFocus?.requestFocus();
      }
    }
    if (_live &&
        (!_hasSession ||
            sourceRevision != widget.sourceRevision ||
            selectionRevision != widget.selectionRevision ||
            _selectionDirty)) {
      await _open(refreshSources: true);
      return;
    }
    if (!_valid(epoch) ||
        detailEpoch != _detailEpoch ||
        !identical(details, _details)) {
      return;
    }
    switch (action) {
      case _PlaylistAction.save:
        await _saveLoaded();
      case _PlaylistAction.download:
        await _downloadLoaded();
      case _PlaylistAction.downloadAll:
        await _downloadComplete();
      case _PlaylistAction.select:
        setState(() {
          _selectingSongs = true;
          _selectedSongs = {};
        });
      case null:
        break;
    }
    _maybeMore();
  }

  Widget _playlistSongSkeleton() {
    Widget block(double width, double height) => Container(
      width: width,
      height: height,
      decoration: BoxDecoration(
        color: p.color('--td-bg-color-secondarycontainer'),
        borderRadius: BorderRadius.circular(4),
      ),
    );
    return Semantics(
      label: p.english ? 'Loading songs' : '歌曲加载中',
      child: ExcludeSemantics(
        child: SizedBox(
          height: 72,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Row(
              children: [
                block(16, 12),
                const SizedBox(width: 10),
                block(40, 40),
                const SizedBox(width: 8),
                Expanded(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      FractionallySizedBox(
                        widthFactor: .8,
                        child: block(double.infinity, 15),
                      ),
                      const SizedBox(height: 5),
                      FractionallySizedBox(
                        widthFactor: .6,
                        child: block(double.infinity, 12),
                      ),
                      const SizedBox(height: 5),
                      FractionallySizedBox(
                        widthFactor: .5,
                        child: block(double.infinity, 14),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                block(28, 12),
              ],
            ),
          ),
        ),
      ),
    );
  }

  /// Desktop song_view header: `#`, title, album, liked, duration.
  Widget _wideSongHeader() {
    final muted = p.color('--song-list-header-text');
    Widget cell(
      String text,
      double? width, {
      EdgeInsets pad = EdgeInsets.zero,
    }) {
      final child = Padding(
        padding: pad,
        child: Text(
          text,
          maxLines: 1,
          style: TextStyle(fontSize: 12, color: muted),
        ),
      );
      return width == null
          ? Expanded(child: child)
          : SizedBox(width: width, child: child);
    }

    return Container(
      height: MioDeviceScope.isTv(context) ? 32 : 40,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(
        color: p.color('--song-list-header-bg'),
        border: Border(
          bottom: BorderSide(color: p.color('--song-list-header-border')),
        ),
      ),
      child: Row(
        children: [
          SizedBox(
            width: 50,
            child: Text(
              '#',
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 12, color: muted),
            ),
          ),
          cell(
            p.text('music.songList.title'),
            null,
            pad: const EdgeInsets.only(left: 10),
          ),
          cell(
            p.text('music.songList.album'),
            200,
            pad: const EdgeInsets.symmetric(horizontal: 10),
          ),
          SizedBox(
            width: 60,
            child: Text(
              p.text('music.songList.liked'),
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 12, color: muted),
            ),
          ),
          SizedBox(
            width: 80 + 36,
            child: Text(
              p.text('music.songList.duration'),
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 12, color: muted),
            ),
          ),
        ],
      ),
    );
  }

  /// Desktop song_view row (64 high): index, cover with title and artist,
  /// album, liked and duration columns.
  Widget _wideSongRow({
    required Key key,
    required api.Track track,
    required int number,
    required bool playing,
    required bool active,
    required api.OnlinePlaylistSession? session,
  }) {
    final labels = _trackLabels[track.id];
    final quality = labels?.quality.isNotEmpty == true
        ? p.text(labels!.quality)
        : '';
    final source = labels?.source ?? '';
    Widget badge(String label, String bg, String fg) => Container(
      margin: const EdgeInsets.only(right: 3),
      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
      decoration: BoxDecoration(
        color: p.color(bg),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        label,
        maxLines: 1,
        softWrap: false,
        style: TextStyle(
          fontSize: 10,
          height: 1,
          fontWeight: FontWeight.w500,
          color: p.color(fg),
        ),
      ),
    );
    final duration = track.duration.isFinite && track.duration > 0
        ? '${(track.duration ~/ 60).toString().padLeft(2, '0')}:${(track.duration.toInt() % 60).toString().padLeft(2, '0')}'
        : '--:--';
    final tv = MioDeviceScope.isTv(context);
    final title = Text(
      track.title,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        fontSize: tv ? 16 : 14,
        height: 16.8 / 14,
        fontWeight: FontWeight.w500,
        color: playing ? p.brand : p.color('--song-list-title-color'),
      ),
    );
    return Container(
      key: key,
      // TV packs two lines per row (tags join the title) to show more songs.
      height: tv ? 60 : 76,
      padding: EdgeInsets.symmetric(horizontal: 10, vertical: tv ? 4 : 6),
      decoration: BoxDecoration(
        color: playing ? p.color('--song-list-item-playing') : null,
        border: Border(
          bottom: BorderSide(color: p.color('--song-list-item-border')),
        ),
      ),
      child: Row(
        children: [
          SizedBox(
            width: 50,
            child: _selectingSongs
                ? Center(
                    child: OnlineSelectionToggle(
                      palette: p,
                      track: track,
                      selected: _selectedSongs.contains(track.id),
                      onPressed:
                          active &&
                              (_selectedSongs.contains(track.id) ||
                                  _selectedSongs.length < 200)
                          ? () => setState(() {
                              if (!_selectedSongs.remove(track.id)) {
                                _selectedSongs.add(track.id);
                              }
                            })
                          : null,
                    ),
                  )
                : Text(
                    '$number',
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 14,
                      color: playing
                          ? p.brand
                          : p.color('--song-list-track-number'),
                    ),
                  ),
          ),
          Expanded(
            child: MioButton(
              palette: p,
              label: '${track.title} ${track.artist}',
              radius: 4,
              insetFocusBorder: false,
              padding: const EdgeInsets.only(left: 10),
              onPressed: active
                  ? () {
                      if (_selectingSongs) {
                        setState(() {
                          if (!_selectedSongs.remove(track.id) &&
                              _selectedSongs.length < 200) {
                            _selectedSongs.add(track.id);
                          }
                        });
                      } else {
                        unawaited(_play(track));
                      }
                    }
                  : null,
              child: Row(
                children: [
                  MioCover(track.cover, size: 40, radius: 4),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        if (tv)
                          Row(
                            children: [
                              Flexible(child: title),
                              const SizedBox(width: 8),
                              if (quality.isNotEmpty)
                                badge(
                                  quality,
                                  '--song-list-quality-bg',
                                  '--song-list-quality-color',
                                ),
                              if (source.isNotEmpty)
                                badge(
                                  source,
                                  '--song-list-source-bg',
                                  '--song-list-source-color',
                                ),
                            ],
                          )
                        else
                          title,
                        const SizedBox(height: 1),
                        SizedBox(
                          height: 18,
                          child: OverflowBox(
                            alignment: Alignment.centerLeft,
                            minHeight: 0,
                            maxHeight: 44,
                            child: _artistLink(track),
                          ),
                        ),
                        if (!tv &&
                            (quality.isNotEmpty || source.isNotEmpty)) ...[
                          const SizedBox(height: 2),
                          // Both tags stay whole; a narrow title column scales
                          // them down instead of truncating them.
                          Align(
                            alignment: Alignment.centerLeft,
                            child: FittedBox(
                              fit: BoxFit.scaleDown,
                              alignment: Alignment.centerLeft,
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (quality.isNotEmpty)
                                    badge(
                                      quality,
                                      '--song-list-quality-bg',
                                      '--song-list-quality-color',
                                    ),
                                  if (source.isNotEmpty)
                                    badge(
                                      source,
                                      '--song-list-source-bg',
                                      '--song-list-source-color',
                                    ),
                                ],
                              ),
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          SizedBox(
            width: 200,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: Text(
                track.album,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12,
                  color: p.color('--song-list-album-color'),
                ),
              ),
            ),
          ),
          SizedBox(
            width: 60,
            child: Center(
              child: FavoriteButton(
                palette: p,
                core: widget.core,
                identity: (session, track.id, _favoriteRevision),
                active: active,
                prepare: () => session!.prepareFavorite(id: track.id),
                songTitle: track.title,
                addToPlaylist: onlineSongTarget(
                  () => session!.prepareSelectedSave(ids: [track.id]),
                ),
                onReload: () => setState(() => _favoriteRevision++),
              ),
            ),
          ),
          SizedBox(
            width: 80,
            child: Center(
              child: Text(
                duration,
                style: TextStyle(
                  fontSize: 12,
                  color: p.color('--td-text-color-placeholder'),
                ),
              ),
            ),
          ),
          SizedBox(
            width: 36,
            height: 44,
            child: MioButton(
              palette: p,
              label: '${p.text('common.download')}: ${track.title}',
              radius: 8,
              padding: EdgeInsets.zero,
              onPressed: active
                  ? () => unawaited(_downloadLoaded(track: track))
                  : null,
              child: Center(
                child: MioIcon('Download', size: 16, color: p.secondary),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _playlist() {
    final card = _selected!, session = _details;
    final query = _filter.text.trim().toLowerCase();
    final rows = [
      for (var index = 0; index < _tracks.length; index++)
        if (query.isEmpty ||
            [
              _tracks[index].title,
              _tracks[index].artist,
              _tracks[index].album,
            ].any((value) => value.toLowerCase().contains(query)))
          (index: index, track: _tracks[index]),
    ];
    final header = OnlinePlaylistHeader(
      palette: p,
      title: card.title,
      description: _description.isNotEmpty
          ? _description
          : (card.description.isNotEmpty ? card.description : card.author),
      count: p.text('common.songCount').replaceAll('{count}', '$_total'),
      cover: card.cover,
      // TV never has room for the tall header above the song list.
      compact:
          MioDeviceScope.isTv(context) ||
          (_compactPlaylist && !_filterFocus.hasFocus),
      wide: _wide,
      busy: !_playlistEnabled,
      filter: _filter,
      filterFocus: _filterFocus,
      onQuery: (_) {
        setState(() {});
        if (_songScroll.hasClients) _songScroll.jumpTo(0);
        WidgetsBinding.instance.addPostFrameCallback((_) => _maybeMore());
      },
      onBack: _modalBusy || _playing ? null : _back,
      onPlay: _playlistEnabled && _tracks.isNotEmpty
          ? () => _askConfirmation(shuffle: false)
          : null,
      onShuffle: _playlistEnabled && _tracks.isNotEmpty
          ? () => _askConfirmation(shuffle: true)
          : null,
      onMore: _playlistEnabled
          ? (anchor) => unawaited(_playlistActions(anchor))
          : null,
    );
    return ColoredBox(
      color: _wide
          ? const Color(0x00000000)
          : p.color('--song-list-content-bg'),
      child: Padding(
        padding: EdgeInsets.all(
          MioDeviceScope.isTv(context) ? 10 : (_wide ? 20 : 0),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: math.max(68, MediaQuery.sizeOf(context).height * .5),
              ),
              child: _swipeHeader(SingleChildScrollView(child: header)),
            ),
            SizedBox(
              height: MioDeviceScope.isTv(context) ? 8 : (_wide ? 20 : 10),
            ),
            if (_selectingSongs)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: OnlineSelectionToolbar(
                  palette: p,
                  tracks: _tracks,
                  selected: _selectedSongs,
                  selecting: true,
                  enabled: _playlistEnabled,
                  onChanged: (selecting, ids) => setState(() {
                    _selectingSongs = selecting;
                    _selectedSongs = ids;
                  }),
                  onAppend: () => unawaited(_appendSelected()),
                ),
              ),
            if (_actionNote != null)
              Padding(
                padding: const EdgeInsets.all(12),
                child: Semantics(liveRegion: true, child: Text(_actionNote!)),
              ),
            if (_error != null)
              Padding(padding: const EdgeInsets.all(12), child: _errorBar()),
            if (_wide && (_loading || _playing) && _tracks.isEmpty)
              Semantics(
                liveRegion: true,
                child: Text(p.english ? 'Loading songs…' : '歌曲加载中…'),
              ),
            Expanded(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(_wide ? 12 : 0),
                child: ColoredBox(
                  color: _wide
                      ? p.color('--song-list-content-bg')
                      : const Color(0x00000000),
                  child: Column(
                    children: [
                      if (_wide) _swipeHeader(_wideSongHeader()),
                      Expanded(
                        child: RawScrollbar(
                          controller: _songScroll,
                          thickness: 8,
                          radius: const Radius.circular(999),
                          thumbColor: p
                              .color('--td-text-color-placeholder')
                              .withValues(alpha: .3),
                          child: ListView.builder(
                            controller: _songScroll,
                            itemCount:
                                !_wide &&
                                    (_loading || _playing) &&
                                    _tracks.isEmpty
                                ? 8
                                : rows.length + (_more ? 1 : 0),
                            itemBuilder: (context, index) {
                              if (!_wide &&
                                  (_loading || _playing) &&
                                  _tracks.isEmpty) {
                                return _playlistSongSkeleton();
                              }
                              if (index == rows.length) {
                                if (!_wide && (_loading || _playing)) {
                                  return _playlistSongSkeleton();
                                }
                                return _button(
                                  p.english ? 'Load more' : '加载更多',
                                  _playlistEnabled
                                      ? () => unawaited(_next())
                                      : null,
                                );
                              }
                              final row = rows[index],
                                  track = rows[index].track;
                              final active = _playlistEnabled;
                              final playing =
                                  widget.core.player?.current?.id == track.id;
                              if (_wide) {
                                return _wideSongRow(
                                  key: ValueKey(track.id),
                                  track: track,
                                  number: row.index + 1,
                                  playing: playing,
                                  active: active,
                                  session: session,
                                );
                              }
                              return DecoratedBox(
                                key: ValueKey(track.id),
                                decoration: BoxDecoration(
                                  color: playing
                                      ? p.color('--td-brand-color-light')
                                      : const Color(0x00000000),
                                  borderRadius: BorderRadius.circular(6),
                                ),
                                child: SearchTrackRow(
                                  track: track,
                                  labels: _trackLabels[track.id],
                                  number: row.index + 1,
                                  narrow: !_wide,
                                  palette: p,
                                  artist: _artistLink(track),
                                  durationAtEnd: true,
                                  onPlay: active
                                      ? () {
                                          if (_selectingSongs) {
                                            setState(() {
                                              if (!_selectedSongs.remove(
                                                    track.id,
                                                  ) &&
                                                  _selectedSongs.length < 200) {
                                                _selectedSongs.add(track.id);
                                              }
                                            });
                                          } else {
                                            unawaited(_play(track));
                                          }
                                        }
                                      : null,
                                  trailing: _selectingSongs
                                      ? OnlineSelectionToggle(
                                          palette: p,
                                          track: track,
                                          selected: _selectedSongs.contains(
                                            track.id,
                                          ),
                                          onPressed:
                                              active &&
                                                  (_selectedSongs.contains(
                                                        track.id,
                                                      ) ||
                                                      _selectedSongs.length <
                                                          200)
                                              ? () => setState(() {
                                                  if (!_selectedSongs.remove(
                                                    track.id,
                                                  )) {
                                                    _selectedSongs.add(
                                                      track.id,
                                                    );
                                                  }
                                                })
                                              : null,
                                        )
                                      : Row(
                                          mainAxisSize: MainAxisSize.min,
                                          children: [
                                            FavoriteButton(
                                              palette: p,
                                              core: widget.core,
                                              identity: (
                                                session,
                                                track.id,
                                                _favoriteRevision,
                                              ),
                                              active: active,
                                              prepare: () =>
                                                  session!.prepareFavorite(
                                                    id: track.id,
                                                  ),
                                              songTitle: track.title,
                                              addToPlaylist: onlineSongTarget(
                                                () => session!
                                                    .prepareSelectedSave(
                                                      ids: [track.id],
                                                    ),
                                              ),
                                              onReload: () => setState(
                                                () => _favoriteRevision++,
                                              ),
                                            ),
                                            SizedBox(
                                              width: 36,
                                              height: 44,
                                              child: MioButton(
                                                palette: p,
                                                label:
                                                    '${p.text('common.download')}: ${track.title}',
                                                radius: 8,
                                                padding: EdgeInsets.zero,
                                                onPressed: active
                                                    ? () => unawaited(
                                                        _downloadLoaded(
                                                          track: track,
                                                        ),
                                                      )
                                                    : null,
                                                child: Center(
                                                  child: MioIcon(
                                                    'Download',
                                                    size: 16,
                                                    color: p.secondary,
                                                  ),
                                                ),
                                              ),
                                            ),
                                          ],
                                        ),
                                ),
                              );
                            },
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            if (_wide && (_loading || _playing))
              Semantics(
                liveRegion: true,
                child: Padding(
                  padding: const EdgeInsets.all(8),
                  child: Text(p.english ? 'Loading…' : '加载中…'),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _categorySheetNarrow() {
    final groups = _categories!.groups;
    final group = groups[_group.clamp(0, groups.length - 1)];
    return Positioned.fill(
      child: BlockSemantics(
        child: CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.escape): () =>
                setState(() => _categoriesOpen = false),
          },
          child: FocusScope(
            autofocus: true,
            child: Stack(
              children: [
                Positioned.fill(
                  child: GestureDetector(
                    onTap: () => setState(() => _categoriesOpen = false),
                    child: const ColoredBox(color: Color(0x66000000)),
                  ),
                ),
                Center(
                  child: Container(
                    margin: const EdgeInsets.all(16),
                    padding: const EdgeInsets.all(16),
                    width: 700,
                    height: 460,
                    decoration: BoxDecoration(
                      color: p.surface,
                      border: Border.all(color: p.border),
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                p.english ? 'Categories' : '歌单分类',
                                style: const TextStyle(
                                  fontSize: 20,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                            _button(
                              p.text('common.close'),
                              () => setState(() => _categoriesOpen = false),
                            ),
                          ],
                        ),
                        const SizedBox(height: 12),
                        SingleChildScrollView(
                          scrollDirection: Axis.horizontal,
                          child: Row(
                            children: [
                              for (var i = 0; i < groups.length; i++)
                                Padding(
                                  padding: const EdgeInsets.only(right: 8),
                                  child: _button(
                                    groups[i].name,
                                    () => setState(() => _group = i),
                                    selected: i == _group,
                                  ),
                                ),
                            ],
                          ),
                        ),
                        const SizedBox(height: 12),
                        Expanded(
                          child: LayoutBuilder(
                            builder: (context, box) => GridView.builder(
                              itemCount: group.items.length,
                              gridDelegate:
                                  SliverGridDelegateWithFixedCrossAxisCount(
                                    crossAxisCount: math.max(
                                      1,
                                      (box.maxWidth / 140).floor(),
                                    ),
                                    mainAxisSpacing: 8,
                                    crossAxisSpacing: 8,
                                    mainAxisExtent: math.max(
                                      44,
                                      MediaQuery.textScalerOf(context)
                                              .scale(14) *
                                          3,
                                    ),
                                  ),
                              itemBuilder: (context, i) => _button(
                                group.items[i].name,
                                () => _selectCategory(
                                  group.items[i].id,
                                  group.items[i].name,
                                ),
                                selected: group.items[i].id == _category,
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  /// Wide layouts: a popover anchored under the category row. It floats over
  /// the cards without dimming them; tapping outside or pressing Esc closes it.
  Widget _categorySheet() {
    final groups = _categories!.groups;
    final group = groups[_group.clamp(0, groups.length - 1)];
    final primary = p.color('--td-text-color-primary');
    final secondary = p.color('--td-text-color-secondary');
    void close() => setState(() => _categoriesOpen = false);
    Widget text(String name, bool active, bool group, VoidCallback onTap) =>
        Semantics(
          selected: active,
          child: MioButton(
            palette: p,
            label: name,
            radius: 6,
            insetFocusBorder: false,
            background: active
                ? primary.withValues(alpha: .12)
                : const Color(0x00000000),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
            onPressed: onTap,
            child: Text(
              name,
              maxLines: 1,
              style: TextStyle(
                fontSize: 14,
                fontWeight: group ? FontWeight.w500 : FontWeight.w400,
                color: active && !group
                    ? p.brand
                    : (group ? primary : secondary),
              ),
            ),
          ),
        );
    return Positioned.fill(
      child: BlockSemantics(
        child: CallbackShortcuts(
          bindings: {const SingleActivator(LogicalKeyboardKey.escape): close},
          child: FocusScope(
            autofocus: true,
            child: LayoutBuilder(
              builder: (context, box) {
                // Anchor under the category row; fall back to the top edge.
                var left = 0.0, top = 0.0, width = box.maxWidth;
                final row = _categoryRowKey.currentContext?.findRenderObject();
                // The page's own box is laid out already; the builder's is not yet.
                final self = this.context.findRenderObject();
                if (row is RenderBox &&
                    row.hasSize &&
                    self is RenderBox &&
                    self.hasSize) {
                  final origin = self.globalToLocal(
                    row.localToGlobal(Offset.zero),
                  );
                  left = origin.dx;
                  top = origin.dy + row.size.height + 6;
                  width = row.size.width;
                }
                return Stack(
                  children: [
                    Positioned.fill(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: close,
                        child: const SizedBox.expand(),
                      ),
                    ),
                    Positioned(
                      left: left,
                      top: top,
                      width: width,
                      child: ConstrainedBox(
                        constraints: BoxConstraints(
                          maxHeight: math.max(120, box.maxHeight - top - 12),
                        ),
                        child: GestureDetector(
                          onTap: () {},
                          child: DecoratedBox(
                            decoration: BoxDecoration(
                              borderRadius: BorderRadius.circular(12),
                              boxShadow: const [
                                BoxShadow(
                                  color: Color(0x4d000000),
                                  blurRadius: 24,
                                  offset: Offset(0, 8),
                                ),
                              ],
                            ),
                            child: ClipRRect(
                              borderRadius: BorderRadius.circular(12),
                              child: BackdropFilter(
                                filter: ImageFilter.blur(
                                  sigmaX: 16,
                                  sigmaY: 16,
                                ),
                                child: DecoratedBox(
                                  decoration: BoxDecoration(
                                    color: p
                                        .color('--td-bg-color-container')
                                        .withValues(alpha: .86),
                                    border: Border.all(
                                      color: p.color(
                                        '--td-border-level-1-color',
                                      ),
                                    ),
                                    borderRadius: BorderRadius.circular(12),
                                  ),
                                  child: SingleChildScrollView(
                                    padding: const EdgeInsets.all(12),
                                    child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Wrap(
                                          spacing: 8,
                                          runSpacing: 4,
                                          children: [
                                            for (
                                              var i = 0;
                                              i < groups.length;
                                              i++
                                            )
                                              text(
                                                groups[i].name,
                                                i == _group,
                                                true,
                                                () =>
                                                    setState(() => _group = i),
                                              ),
                                          ],
                                        ),
                                        const SizedBox(height: 6),
                                        Wrap(
                                          spacing: 8,
                                          runSpacing: 4,
                                          children: [
                                            for (final item in group.items)
                                              text(
                                                item.name,
                                                item.id == _category,
                                                false,
                                                () => _selectCategory(
                                                  item.id,
                                                  item.name,
                                                ),
                                              ),
                                          ],
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
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }

  void _askConfirmation({required bool shuffle}) {
    setState(() {
      _shuffleRequested = shuffle;
      _confirm = true;
    });
    if (!MioDeviceScope.isTv(context)) return;
    // The page behind stops taking focus once the dialog is up. A remote has
    // no pointer, so move focus into the dialog or it would stay on the page.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _confirm) _confirmationScope.requestScopeFocus();
    });
  }

  Widget _confirmation() => Positioned.fill(
    child: CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            setState(() => _confirm = false),
      },
      child: BlockSemantics(
        child: Stack(
          children: [
            Positioned.fill(
              child: GestureDetector(
                onTap: () => setState(() => _confirm = false),
                child: ColoredBox(color: const Color(0x66000000)),
              ),
            ),
            Center(
              child: SingleChildScrollView(
                child: FocusScope(
                  autofocus: true,
                  node: _confirmationScope,
                  child: Container(
                    constraints: const BoxConstraints(maxWidth: 420),
                    margin: const EdgeInsets.all(24),
                    padding: const EdgeInsets.all(24),
                    decoration: BoxDecoration(
                      color: p.surface,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          p.english
                              ? 'Load the full playlist and replace the queue${_shuffleRequested ? ' in random order' : ''}${_filter.text.trim().isNotEmpty ? ' with matching songs' : ''}?'
                              : '加载完整歌单${_filter.text.trim().isNotEmpty ? '并筛选匹配歌曲' : ''}，替换播放队列${_shuffleRequested ? '并随机播放' : ''}？',
                        ),
                        const SizedBox(height: 16),
                        Wrap(
                          spacing: 12,
                          children: [
                            _button(
                              p.text('common.cancel'),
                              () => setState(() => _confirm = false),
                            ),
                            _button(
                              p.text('common.confirm'),
                              _live ? () => unawaited(_play(null)) : null,
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    if (widget.openDetails == null &&
        _sources.isEmpty &&
        !_loading &&
        _error == null) {
      return widget.guide;
    }
    final gutter = _wide
        ? 2 * MioPalette.rem(widget.viewportWidth)
        : MioPalette.gutter(widget.viewportWidth);
    return PopScope(
      // This page only vetoes the route pop. Shell owns the one back callback,
      // so a local detail/category close cannot also navigate shell history.
      canPop: !_canHandleBack,
      child: Stack(
        children: [
          ExcludeFocus(
            excluding: _categoriesOpen || _confirm,
            child: Padding(
              padding: _selected != null
                  ? EdgeInsets.zero
                  : EdgeInsets.fromLTRB(
                      gutter,
                      _selected == null && !_wide
                          ? (widget.viewportWidth * .035)
                                .clamp(14, 22)
                                .toDouble()
                          : (_wide
                                ? MioPalette.rem(widget.viewportWidth)
                                : gutter),
                      gutter,
                      _wide ? MioPalette.rem(widget.viewportWidth) : 16,
                    ),
              child: _selected == null ? _browse() : _playlist(),
            ),
          ),
          if (_confirm) _confirmation(),
          if (_categoriesOpen && _categories?.groups.isNotEmpty == true)
            (MioDeviceScope.isTv(context)
                ? _tvCategorySheet()
                : _wide
                ? _categorySheet()
                : _categorySheetNarrow()),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _close();
    WidgetsBinding.instance.removeObserver(this);
    _confirmationScope.dispose();
    _scroll.dispose();
    _songScroll.dispose();
    _filter.dispose();
    _filterFocus.dispose();
    _tvMoreFocus.dispose();
    for (final node in _tvCardFocus.values) {
      node.dispose();
    }
    super.dispose();
  }
}

import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

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
import 'favorite_button.dart';
import 'search_track_row.dart';
import 'context_menu.dart';

/// Phone artist navigation shares the existing shell's playback and navigation.
class SingerHost extends InheritedWidget {
  final Future<void> Function(api.SingerSession, String)? open;
  const SingerHost({super.key, required this.open, required super.child});
  static SingerHost? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<SingerHost>();
  @override
  bool updateShouldNotify(SingerHost oldWidget) => open != oldWidget.open;
}

/// Owns the transferred artist handle independently of the player that opened it.
class SingerPage extends StatefulWidget {
  final api.SingerSession session;
  final CoreController core;
  final MioPalette palette;
  final String fallbackName;
  final VoidCallback? onClose;
  const SingerPage({
    super.key,
    required this.session,
    required this.core,
    required this.palette,
    required this.fallbackName,
    this.onClose,
  });
  @override
  State<SingerPage> createState() => SingerPageState();
}

enum _SingerAction { save, download, downloadAll, select }

class SingerPageState extends State<SingerPage> with WidgetsBindingObserver {
  final _confirmationScope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  late api.SingerSession _session;
  api.OnlinePlaylistSession? _songs, _album;
  api.SingerProfile? _profile;
  api.DiscoveryCard? _selected;
  FocusNode? _albumOriginFocus;
  List<api.Track> _tracks = [], _albumTracks = [];
  Map<String, api.SearchRowLabel> _songLabels = {}, _albumLabels = {};
  int _favoriteRevision = 0;
  Set<String> _singerIds = {}, _albumSingerIds = {};
  bool _artistRoute = false, _suspended = false, _modalBusy = false;
  bool get _actionBusy => _playing || _modalBusy;
  List<api.DiscoveryCard> _albums = [];
  final _songScroll = ScrollController(),
      _albumScroll = ScrollController(),
      _detailScroll = ScrollController();
  bool _selectingSongs = false;
  Set<String> _selectedSongs = {};
  int _epoch = 0,
      _detailEpoch = 0,
      _songTotal = 0,
      _albumTotal = 0,
      _detailTotal = 0;
  late final int? _sourceRevision;
  bool _foreground = true,
      _invalid = false,
      _resuming = false,
      _profileLoading = false;
  bool _phoneHeaderCollapsed = false;
  double _phoneHeaderDrag = 0;
  bool _tabAlbums = false,
      _songLoading = false,
      _albumLoading = false,
      _detailLoading = false;
  bool _songMore = true,
      _albumMore = true,
      _detailMore = true,
      _playing = false,
      _confirm = false;
  String? _songError,
      _albumError,
      _detailError,
      _profileError,
      _actionError,
      _terminal;
  MioPalette get p => MioPalette(
    widget.palette.assets,
    coverTheme: widget.core.player?.coverTheme,
    dark: widget.core.player?.dark ?? widget.palette.dark,
    english: widget.core.player?.english ?? widget.palette.english,
  );
  bool get _live =>
      mounted &&
      _foreground &&
      !_invalid &&
      !_resuming &&
      !_suspended &&
      _terminal == null;
  bool _valid(int epoch) => _live && epoch == _epoch;
  bool get _detail => _selected != null;
  bool get _contentLoading => _detail ? _detailLoading : _songLoading;
  List<api.Track> get _contentTracks => _detail ? _albumTracks : _tracks;
  api.OnlinePlaylistSession? get _content => _detail ? _album : _songs;

  @override
  void initState() {
    super.initState();
    _session = widget.session;
    _sourceRevision = widget.core.player?.sourceRevision;
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    widget.core.addListener(_coreChanged);
    WidgetsBinding.instance.addObserver(this);
    for (final scroll in [_songScroll, _albumScroll, _detailScroll]) {
      scroll.addListener(_maybeMore);
    }
    if (_foreground) {
      _initial();
    } else {
      _suspend();
    }
  }

  void _coreChanged() {
    if (!mounted) return;
    if (!_invalid &&
        (widget.core.player == null ||
            widget.core.player?.sourceRevision != _sourceRevision)) {
      _invalid = true;
      _suspend();
      _terminal = p.english
          ? 'The source changed. Return to the player and reopen the artist.'
          : '来源已变更，请返回播放器重新打开歌手页。';
    }
    setState(() {});
  }

  void _closeContents() {
    _detailEpoch++;
    _albumOriginFocus = null;
    _selectingSongs = false;
    _selectedSongs = {};
    for (final session in [_songs, _album]) {
      session?.close();
      session?.dispose();
    }
    _songs = null;
    _album = null;
    _selected = null;
    _songLoading = false;
    _albumLoading = false;
    _detailLoading = false;
    _playing = false;
    _confirm = false;
  }

  void _suspend() {
    _epoch++;
    _suspended = true;
    _resuming = false;
    _profileLoading = false;
    _session.close();
    _closeContents();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (!_invalid && !_artistRoute && !_modalBusy) unawaited(_resume());
    } else {
      _foreground = false;
      _suspend();
    }
  }

  Future<void> _resume() async {
    if (!mounted || !_foreground || _invalid || _resuming) return;
    _resuming = true;
    final epoch = _epoch, original = _session;
    api.SingerSession? candidate;
    setState(() => _terminal = null);
    try {
      candidate = await original.reopen();
      if (!mounted || !_foreground || _invalid || epoch != _epoch) return;
      _session = candidate;
      candidate = null;
      _suspended = false;
      original.dispose();
      _resuming = false;
      _initial();
    } catch (_) {
      if (mounted && _foreground && epoch == _epoch) {
        setState(
          () => _terminal = p.english
              ? 'Unable to reconnect. Retry or reopen from the player.'
              : '重新连接失败，请重试或从播放器重新打开。',
        );
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
      if (epoch == _epoch) {
        _resuming = false;
        if (mounted) setState(() {});
      }
    }
  }

  void _initial() {
    _tracks = [];
    _songLabels = {};
    _albumLabels = {};
    _singerIds = {};
    _albumSingerIds = {};
    _albums = [];
    _albumTracks = [];
    _profile = null;
    _songTotal = 0;
    _albumTotal = 0;
    _detailTotal = 0;
    _songMore = true;
    _albumMore = true;
    _songError = null;
    _albumError = null;
    _profileError = null;
    _actionError = null;
    unawaited(_profileLoad());
    unawaited(_songsLoad());
    if (_tabAlbums) unawaited(_albumsLoad());
  }

  Future<void> _profileLoad() async {
    if (!_live || _profileLoading) return;
    _profileLoading = true;
    final epoch = _epoch, session = _session;
    try {
      final info = await session.info();
      if (!_valid(epoch) || !identical(session, _session)) return;
      setState(() => _profile ??= info);
      final profile = await session.profile();
      if (_valid(epoch) && identical(session, _session)) {
        setState(() {
          _profile = profile;
          _profileError = null;
        });
      }
    } catch (_) {
      if (_valid(epoch) && identical(session, _session)) {
        setState(
          () => _profileError = p.english ? 'Profile unavailable' : '歌手资料暂不可用',
        );
      }
    } finally {
      if (_valid(epoch)) setState(() => _profileLoading = false);
    }
  }

  Future<void> _songsLoad() async {
    if (!_live || _songLoading || !_songMore) return;
    final epoch = _epoch, parent = _session;
    setState(() {
      _songLoading = true;
      _songError = null;
    });
    api.OnlinePlaylistSession? candidate;
    try {
      if (_songs == null) {
        candidate = await parent.songs();
        if (!_valid(epoch) || !identical(parent, _session)) return;
        _songs = candidate;
        candidate = null;
      }
      final session = _songs!;
      final page = await session.next();
      if (!_valid(epoch) || !identical(session, _songs)) return;
      setState(() {
        _tracks = page.tracks;
        _songLabels = {for (final label in page.labels) label.id: label};
        _singerIds = page.singerIds.toSet();
        _songTotal = page.total;
        _songMore = page.hasMore;
      });
      _refill(epoch);
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _songError = p.english
              ? 'Unable to load songs. Retry.'
              : '歌曲加载失败，请重试。',
        );
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
      if (_valid(epoch)) {
        setState(() => _songLoading = false);
      }
    }
  }

  Future<void> _albumsLoad() async {
    if (!_live || _albumLoading || !_albumMore) return;
    final epoch = _epoch, session = _session;
    setState(() {
      _albumLoading = true;
      _albumError = null;
    });
    try {
      final page = await session.nextAlbums();
      if (!_valid(epoch) || !identical(session, _session)) return;
      setState(() {
        _albums = page.cards;
        _albumTotal = page.total;
        _albumMore = page.hasMore;
      });
      _refill(epoch);
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _albumError = p.english
              ? 'Unable to load albums. Retry.'
              : '专辑加载失败，请重试。',
        );
      }
    } finally {
      if (_valid(epoch)) {
        setState(() => _albumLoading = false);
      }
    }
  }

  Future<void> _openAlbum(api.DiscoveryCard card) async {
    if (!_live ||
        _playing ||
        _albumLoading ||
        _detail ||
        _profile?.albumDetails != true) {
      return;
    }
    final epoch = _epoch, detailEpoch = ++_detailEpoch, parent = _session;
    if (MioDeviceScope.isTv(context) && _albumOriginFocus == null) {
      _albumOriginFocus = FocusManager.instance.primaryFocus;
    }
    setState(() {
      _selected = card;
      _selectingSongs = false;
      _selectedSongs = {};
      _albumTracks = [];
      _detailMore = true;
      _detailTotal = 0;
      _detailError = null;
      _detailLoading = true;
      _actionError = null;
    });
    api.OnlinePlaylistSession? candidate;
    try {
      candidate = await parent.openAlbum(id: card.id);
      if (!_valid(epoch) || detailEpoch != _detailEpoch) return;
      _album = candidate;
      candidate = null;
      _detailLoading = false;
      await _detailLoad();
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() {
          _detailLoading = false;
          _detailError = p.english
              ? 'Unable to open album. Retry.'
              : '专辑打开失败，请重试。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _detailLoad() async {
    if (!_live || _detailLoading || !_detailMore) return;
    final session = _album;
    if (session == null) {
      final card = _selected;
      if (card != null) {
        _selected = null;
        await _openAlbum(card);
      }
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _detailLoading = true;
      _detailError = null;
    });
    try {
      final page = await session.next();
      if (!_valid(epoch) ||
          detailEpoch != _detailEpoch ||
          !identical(session, _album)) {
        return;
      }
      setState(() {
        _albumTracks = page.tracks;
        _albumLabels = {for (final label in page.labels) label.id: label};
        _albumSingerIds = page.singerIds.toSet();
        _detailTotal = page.total;
        _detailMore = page.hasMore;
      });
      _refill(epoch);
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _detailError = p.english
              ? 'Unable to load album songs. Retry.'
              : '专辑歌曲加载失败，请重试。',
        );
      }
    } finally {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _detailLoading = false);
      }
    }
  }

  void _refill(int epoch) => WidgetsBinding.instance.addPostFrameCallback((_) {
    if (_valid(epoch)) _maybeMore();
  });
  void _maybeMore() {
    if (!_live || _artistRoute || _confirm || _actionBusy) return;
    final scroll = _detail
        ? _detailScroll
        : (_tabAlbums ? _albumScroll : _songScroll);
    if (!scroll.hasClients || scroll.position.extentAfter >= 600) return;
    if (_detail) {
      if (_detailError == null) unawaited(_detailLoad());
    } else if (_tabAlbums) {
      if (_albumError == null) unawaited(_albumsLoad());
    } else {
      if (_songError == null) unawaited(_songsLoad());
    }
  }

  void handleBack() => _back();

  void _back() {
    if (_modalBusy) return;
    if (_confirm) {
      setState(() => _confirm = false);
      return;
    }
    if (_selectingSongs) {
      setState(() {
        _selectingSongs = false;
        _selectedSongs = {};
      });
      return;
    }
    if (_detail) {
      _detailEpoch++;
      _selectingSongs = false;
      _selectedSongs = {};
      _album?.close();
      _album?.dispose();
      _album = null;
      setState(() {
        _selected = null;
        _albumTracks = [];
        _detailLoading = false;
        _actionError = null;
        _playing = false;
      });
      _refill(_epoch);
      final origin = _albumOriginFocus, epoch = _epoch;
      _albumOriginFocus = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_valid(epoch) &&
            !_detail &&
            _tabAlbums &&
            origin?.context != null &&
            origin!.canRequestFocus) {
          origin.requestFocus();
        }
      });
    } else {
      if (widget.onClose != null) {
        widget.onClose!();
      } else {
        Navigator.of(context).pop();
      }
    }
  }

  Future<void> _confirmPlay() async {
    if (!_live || _actionBusy || _contentLoading || _confirm) return;
    setState(() => _confirm = true);
    if (widget.onClose == null) return;
    final epoch = _epoch, detailEpoch = _detailEpoch, session = _content;
    final route = RawDialogRoute<bool>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (dialogContext, _, _) => DefaultTextStyle(
        style: DefaultTextStyle.of(context).style,
        child: Stack(
          children: [
            _confirmation(
              dismiss: (value) => Navigator.of(dialogContext).pop(value),
            ),
          ],
        ),
      ),
    );
    final confirmed = await Navigator.of(context).push<bool>(route);
    await route.completed;
    if (!mounted) return;
    setState(() => _confirm = false);
    if (confirmed == true &&
        _valid(epoch) &&
        detailEpoch == _detailEpoch &&
        identical(session, _content)) {
      await _play();
    }
  }

  Future<void> _play({String? id}) async {
    final session = _content;
    if (!_live || _actionBusy || _contentLoading || session == null) return;
    final epoch = _epoch, detailEpoch = _detailEpoch;
    if (id != null) {
      final song = _contentTracks.where((t) => t.id == id).firstOrNull;
      final choice = await choosePlayChoice(
        context: context,
        palette: p,
        core: widget.core,
        title: song?.title ?? '',
      );
      if (choice == null ||
          !mounted ||
          !_valid(epoch) ||
          detailEpoch != _detailEpoch ||
          !identical(session, _content)) {
        return;
      }
      if (choice == PlayChoice.queue) {
        try {
          await session.append(id: id);
          if (mounted) {
            showMioToast(context, p, p.english ? 'Added to queue' : '已加入播放列表');
          }
        } catch (_) {
          if (mounted && _valid(epoch) && detailEpoch == _detailEpoch) {
            setState(
              () => _actionError = p.english
                  ? 'Could not add the song. Retry or reopen this page.'
                  : '无法加入播放列表，请重试或重新打开页面。',
            );
          }
        }
        return;
      }
    }
    setState(() {
      _confirm = false;
      _playing = true;
      _actionError = null;
    });
    try {
      if (id == null) {
        await session.playAll();
      } else {
        await session.play(id: id);
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Playback failed. Retry or reopen this page.'
              : '播放失败，请重试或重新打开页面。',
        );
      }
    } finally {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _playing = false);
        _refill(epoch);
      }
    }
  }

  Future<void> _appendSelected() async {
    final session = _content;
    if (!_live ||
        _actionBusy ||
        _contentLoading ||
        _artistRoute ||
        _confirm ||
        session == null ||
        _selectedSongs.isEmpty) {
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    final ids = _contentTracks
        .where((t) => _selectedSongs.contains(t.id))
        .map((t) => t.id)
        .toList();
    setState(() {
      _modalBusy = true;
      _actionError = null;
    });
    try {
      final saved = await showOnlineAppend(
        context: context,
        prepare: () => session.prepareSelectedSave(ids: ids),
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(session, _content),
      );
      if (mounted && saved != null) {
        setState(() {
          _actionError = p.english
              ? 'Added ${saved.added} songs to ${saved.destination}'
              : '已向“${saved.destination}”添加 ${saved.added} 首歌曲';
          _selectedSongs = {};
          _selectingSongs = false;
        });
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Unable to prepare selected songs. Reopen this page.'
              : '无法准备所选歌曲，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_foreground && !_invalid && _suspended) {
          unawaited(_resume());
        } else if (_live) {
          _refill(_epoch);
        }
      }
    }
  }

  Future<void> _saveLoaded() async {
    final session = _content;
    if (!_live ||
        _actionBusy ||
        _contentLoading ||
        _artistRoute ||
        _confirm ||
        session == null ||
        _contentTracks.isEmpty) {
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    final name = _selected?.title ?? _profile?.name ?? widget.fallbackName;
    final cover = _selected?.cover ?? _profile?.avatar;
    setState(() {
      _modalBusy = true;
      _actionError = null;
    });
    try {
      final saved = await showOnlineSave(
        context: context,
        session: session,
        palette: p,
        name: name,
        cover: cover,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(session, _content),
      );
      if (saved != null && _valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Saved ${saved.count} songs to ${saved.title}'
              : '已将 ${saved.count} 首歌曲保存到 ${saved.title}',
        );
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Unable to prepare saved songs. Reopen this page.'
              : '无法准备歌曲保存，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_foreground && !_invalid && _suspended) {
          unawaited(_resume());
        } else if (_live) {
          _refill(_epoch);
        }
      }
    }
  }

  Future<void> _downloadLoaded({api.Track? track}) async {
    final session = _content;
    if (!_live ||
        _actionBusy ||
        _contentLoading ||
        _artistRoute ||
        _confirm ||
        session == null ||
        _contentTracks.isEmpty) {
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _modalBusy = true;
      _actionError = null;
    });
    try {
      final report = await showOnlineDownloads(
        context: context,
        session: session,
        tracks: track == null ? _contentTracks : [track],
        core: widget.core,
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(session, _content),
      );
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _actionError = report.message(p));
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Unable to prepare downloads. Reopen this page.'
              : '无法准备下载，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_foreground && !_invalid && _suspended) {
          unawaited(_resume());
        } else if (_live) {
          _refill(_epoch);
        }
      }
    }
  }

  Future<void> _downloadComplete() async {
    final session = _content;
    if (!_live ||
        _actionBusy ||
        _contentLoading ||
        _artistRoute ||
        _confirm ||
        session == null) {
      return;
    }
    final epoch = _epoch, detailEpoch = _detailEpoch;
    setState(() {
      _modalBusy = true;
      _actionError = null;
    });
    try {
      final report = await showCompleteOnlineDownloads(
        context: context,
        session: session,
        core: widget.core,
        palette: p,
        isCurrent: () =>
            _valid(epoch) &&
            detailEpoch == _detailEpoch &&
            identical(session, _content),
      );
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(() => _actionError = report?.message(p));
      }
    } catch (_) {
      if (_valid(epoch) && detailEpoch == _detailEpoch) {
        setState(
          () => _actionError = p.english
              ? 'Unable to prepare downloads. Reopen this page.'
              : '无法准备下载，请重新打开页面。',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _modalBusy = false);
        if (_foreground && !_invalid && _suspended) {
          unawaited(_resume());
        } else if (_live) {
          _refill(_epoch);
        }
      }
    }
  }

  @override
  void dispose() {
    _confirmationScope.dispose();
    widget.core.removeListener(_coreChanged);
    WidgetsBinding.instance.removeObserver(this);
    _suspend();
    _session.dispose();
    for (final scroll in [_songScroll, _albumScroll, _detailScroll]) {
      scroll.dispose();
    }
    super.dispose();
  }

  Widget _button(String label, VoidCallback? action) => MioButton(
    palette: p,
    label: label,
    onPressed: action,
    child: Text(label),
  );
  Widget _tabButton(String label, bool selected, VoidCallback? action) =>
      Semantics(
        selected: selected,
        child: Container(
          decoration: BoxDecoration(
            border: Border(
              bottom: BorderSide(
                width: 2,
                color: selected ? p.brand : const Color(0x00000000),
              ),
            ),
          ),
          child: MioButton(
            palette: p,
            label: label,
            padding: EdgeInsets.zero,
            insetFocusBorder: false,
            onPressed: action,
            child: SizedBox(
              height: 42,
              child: Center(
                widthFactor: 1,
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 15,
                    color: selected ? p.brand : p.secondary,
                  ),
                ),
              ),
            ),
          ),
        ),
      );
  Widget _error(String message, VoidCallback? retry) => Semantics(
    liveRegion: true,
    child: Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      spacing: 8,
      children: [
        Text(message),
        if (retry != null) _button(p.english ? 'Retry' : '重试', retry),
      ],
    ),
  );
  Widget _header(bool wide, double width) {
    final card = _selected;
    final name = card?.title ?? _profile?.name ?? widget.fallbackName;
    final size = card != null ? (wide ? 160.0 : 88.0) : (wide ? 100.0 : 112.0);
    final cover = MioCover(
      card?.cover ?? _profile?.avatar,
      size: size,
      radius: card == null ? size / 2 : 10,
    );
    final info = Column(
      crossAxisAlignment: wide || card != null
          ? CrossAxisAlignment.start
          : CrossAxisAlignment.center,
      children: [
        Text(
          name,
          maxLines: wide ? 1 : 3,
          overflow: TextOverflow.ellipsis,
          textAlign: !wide && card == null ? TextAlign.center : TextAlign.start,
          style: TextStyle(
            fontSize: card != null
                ? 24
                : (wide ? 22 : (width * .09).clamp(32, 41.6)),
            fontWeight: FontWeight.bold,
            height: 1.1,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          card != null
              ? (p.english
                    ? '${_albumTracks.length} loaded · $_detailTotal songs'
                    : '已加载 ${_albumTracks.length} 首 · 共 $_detailTotal 首')
              : [
                  if (_profile?.songCount != null)
                    (p.english
                        ? '${_profile!.songCount} songs'
                        : '${_profile!.songCount} 首歌曲'),
                  if (_profile?.albumCount != null)
                    (p.english
                        ? '${_profile!.albumCount} albums'
                        : '${_profile!.albumCount} 张专辑'),
                ].join(' · '),
          style: TextStyle(fontSize: 13, height: 1.5, color: p.secondary),
        ),
        const SizedBox(height: 4),
        if ((card?.description ?? _profile?.description ?? '').isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              card?.description ?? _profile!.description,
              maxLines: wide ? 2 : 3,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: wide ? 12 : 13.6,
                color: p.color('--td-text-color-placeholder'),
              ),
            ),
          ),
        if (card == null && _profileError != null)
          _error(
            _profileError!,
            _live && !_profileLoading ? () => unawaited(_profileLoad()) : null,
          ),
        if (_contentTracks.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 16),
            child: MioButton(
              palette: p,
              label: p.text('music.singer.playAll'),
              background: p.brand,
              insetFocusBorder: false,
              radius: wide ? 20 : 12,
              padding: const EdgeInsets.symmetric(horizontal: 24),
              onPressed: _live && !_actionBusy && !_contentLoading
                  ? () => unawaited(_confirmPlay())
                  : null,
              child: SizedBox(
                height: wide ? 36 : 44,
                child: Center(
                  widthFactor: 1,
                  child: Text(
                    p.text('music.singer.playAll'),
                    style: TextStyle(
                      fontSize: 14,
                      color: p.color('--td-text-color-anti'),
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
    return Container(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [
            p.brand.withValues(alpha: .12),
            p.brand.withValues(alpha: 0),
          ],
        ),
      ),
      padding: EdgeInsets.fromLTRB(
        wide ? 24 : MioPalette.gutter(width),
        wide ? 16 : (width * .035).clamp(14, 22),
        wide ? 24 : MioPalette.gutter(width),
        20,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              SizedBox(
                width: 44,
                height: 44,
                child: MioButton(
                  palette: p,
                  label: p.text('common.back'),
                  radius: 22,
                  padding: EdgeInsets.zero,
                  onPressed: _back,
                  child: MioIcon(
                    'NavigateBefore',
                    size: 20,
                    color: p.secondary,
                  ),
                ),
              ),
              const Spacer(),
              if (_contentTracks.isNotEmpty && (!_tabAlbums || _detail))
                Builder(
                  builder: (context) => SizedBox(
                    width: 44,
                    height: 44,
                    child: MioButton(
                      palette: p,
                      label: p.english ? 'More actions' : '更多操作',
                      padding: EdgeInsets.zero,
                      onPressed: _live && !_actionBusy
                          ? () => unawaited(_showActions(context))
                          : null,
                      child: Text(
                        '···',
                        style: TextStyle(color: p.secondary, fontSize: 22),
                      ),
                    ),
                  ),
                ),
            ],
          ),
          SizedBox(height: wide ? 12 : 8),
          if (!wide && card == null)
            Column(children: [cover, const SizedBox(height: 12), info])
          else
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                cover,
                const SizedBox(width: 16),
                Expanded(child: info),
              ],
            ),
        ],
      ),
    );
  }

  void _setPhoneHeader(bool collapsed) {
    if (!mounted ||
        !_live ||
        _detail ||
        _confirm ||
        _actionBusy ||
        _phoneHeaderCollapsed == collapsed) {
      return;
    }
    setState(() => _phoneHeaderCollapsed = collapsed);
  }

  bool _phoneScroll(Notification notification) {
    if (_detail) return false;
    if (notification is ScrollMetricsNotification && notification.depth == 0) {
      _refill(_epoch);
    }
    if (notification is ScrollUpdateNotification && notification.depth == 0) {
      final delta = notification.scrollDelta ?? 0;
      final collapse = delta > 0 && notification.metrics.pixels > 12;
      final expand = delta < 0 && notification.metrics.pixels <= 0;
      if (collapse || expand) {
        // Scroll notifications can arrive during layout as the header resizes.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          _setPhoneHeader(collapse);
        });
      }
    }
    return false;
  }

  Widget _phoneBody(bool wide, Widget child) => wide
      ? child
      : NotificationListener<Notification>(
          onNotification: _phoneScroll,
          child: child,
        );

  Widget _phoneHeader(double width) => GestureDetector(
    behavior: HitTestBehavior.opaque,
    onVerticalDragStart: (_) => _phoneHeaderDrag = 0,
    onVerticalDragUpdate: (details) {
      _phoneHeaderDrag += details.primaryDelta ?? 0;
      if (_phoneHeaderDrag.abs() > 24) {
        _setPhoneHeader(_phoneHeaderDrag < 0);
      }
    },
    child: AnimatedCrossFade(
      duration: MediaQuery.disableAnimationsOf(context)
          ? Duration.zero
          : const Duration(milliseconds: 220),
      sizeCurve: Curves.easeInOutCubic,
      alignment: Alignment.topCenter,
      crossFadeState: _phoneHeaderCollapsed
          ? CrossFadeState.showSecond
          : CrossFadeState.showFirst,
      firstChild: _header(false, width),
      secondChild: Container(
        padding: EdgeInsets.fromLTRB(
          MioPalette.gutter(width),
          14,
          MioPalette.gutter(width),
          8,
        ),
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [
              p.brand.withValues(alpha: .12),
              p.brand.withValues(alpha: 0),
            ],
          ),
        ),
        child: Row(
          children: [
            SizedBox(
              width: 44,
              height: 44,
              child: MioButton(
                palette: p,
                label: p.text('common.back'),
                padding: EdgeInsets.zero,
                onPressed: _back,
                child: Center(
                  child: MioIcon(
                    'NavigateBefore',
                    size: 32,
                    color: p.secondary,
                  ),
                ),
              ),
            ),
            Expanded(
              child: SizedBox(
                height: 44,
                child: MioButton(
                  palette: p,
                  label: p.english ? 'Expand artist profile' : '展开歌手资料',
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  onPressed: () => _setPhoneHeader(false),
                  child: Row(
                    children: [
                      MioCover(_profile?.avatar, size: 32, radius: 16),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          _profile?.name ?? widget.fallbackName,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 18,
                            height: 1.2,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            if (_contentTracks.isNotEmpty)
              SizedBox(
                width: 44,
                height: 44,
                child: MioButton(
                  palette: p,
                  label: p.text('music.singer.playAll'),
                  padding: EdgeInsets.zero,
                  onPressed: _live && !_actionBusy && !_contentLoading
                      ? () => unawaited(_confirmPlay())
                      : null,
                  child: Center(
                    child: MioIcon('PlayFilled', size: 36, color: p.brand),
                  ),
                ),
              ),
            if (_contentTracks.isNotEmpty && !_tabAlbums)
              Builder(
                builder: (context) => SizedBox(
                  width: 44,
                  height: 44,
                  child: MioButton(
                    palette: p,
                    label: p.english ? 'More actions' : '更多操作',
                    padding: EdgeInsets.zero,
                    onPressed: _live && !_actionBusy
                        ? () => unawaited(_showActions(context))
                        : null,
                    child: Center(
                      child: MioIcon(
                        'TD-Ellipsis',
                        size: 22,
                        color: p.secondary,
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

  Future<void> _showActions(BuildContext anchorContext) async {
    final box = anchorContext.findRenderObject() as RenderBox;
    final overlay =
        Navigator.of(context).overlay!.context.findRenderObject() as RenderBox;
    final action = await Navigator.of(context).push<_SingerAction>(
      RawDialogRoute(
        barrierDismissible: true,
        barrierColor: const Color(0x00000000),
        pageBuilder: (_, _, _) => MioContextMenu<_SingerAction>(
          palette: p,
          anchor: overlay.globalToLocal(
            box.localToGlobal(Offset(0, box.size.height)),
          ),
          label: p.english ? 'Artist actions' : '歌手操作',
          entries: [
            MioMenuEntry(
              value: _SingerAction.save,
              label: p.english ? 'Save loaded songs' : '保存已加载歌曲',
              icon: 'Add',
            ),
            MioMenuEntry(
              value: _SingerAction.download,
              label: p.english ? 'Download loaded songs' : '下载已加载歌曲',
              icon: 'Download',
            ),
            MioMenuEntry(
              value: _SingerAction.downloadAll,
              label: p.english ? 'Download entire collection' : '下载全部歌曲',
              icon: 'Download',
            ),
            MioMenuEntry(
              value: _SingerAction.select,
              label: p.text('common.batchSelect'),
              icon: 'TD-Check',
            ),
          ],
        ),
      ),
    );
    if (!_live || _actionBusy) return;
    switch (action) {
      case _SingerAction.save:
        await _saveLoaded();
      case _SingerAction.download:
        await _downloadLoaded();
      case _SingerAction.downloadAll:
        await _downloadComplete();
      case _SingerAction.select:
        setState(() => _selectingSongs = true);
      case null:
        break;
    }
  }

  void _artistVisibility(bool visible) {
    if (!mounted) return;
    setState(() => _artistRoute = visible);
    if (!visible && _foreground && !_invalid) {
      if (_suspended) {
        unawaited(_resume());
      } else {
        _refill(_epoch);
      }
    }
  }

  Widget _artistLink(api.Track song, bool detail) {
    final session = detail ? _album : _songs,
        epoch = _epoch,
        detailEpoch = _detailEpoch;
    return ArtistLink(
      name: song.artist,
      available: (detail ? _albumSingerIds : _singerIds).contains(song.id),
      active:
          _live &&
          !_artistRoute &&
          !_actionBusy &&
          !(detail ? _detailLoading : _songLoading) &&
          session != null,
      identity: (song.id, session),
      core: widget.core,
      palette: p,
      isCurrent: () =>
          !_artistRoute &&
          _valid(epoch) &&
          (!detail || detailEpoch == _detailEpoch) &&
          identical(session, detail ? _album : _songs),
      open: () => session!.openSinger(id: song.id),
      onRouteChanged: _artistVisibility,
    );
  }

  Widget _songList(bool detail) {
    final tracks = detail ? _albumTracks : _tracks,
        more = detail ? _detailMore : _songMore,
        loading = detail ? _detailLoading : _songLoading;
    final error = detail ? _detailError : _songError;
    final load = detail ? _detailLoad : _songsLoad;
    return CustomScrollView(
      controller: detail ? _detailScroll : _songScroll,
      slivers: [
        if (error != null && tracks.isEmpty)
          SliverToBoxAdapter(
            child: _error(
              error,
              _live && !loading ? () => unawaited(load()) : null,
            ),
          ),
        if (tracks.isEmpty)
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Text(
                loading
                    ? p.text('common.loading')
                    : (error == null ? (p.english ? 'No songs' : '暂无歌曲') : ''),
              ),
            ),
          ),
        SliverList.builder(
          itemCount: tracks.length,
          itemBuilder: (context, i) {
            final song = tracks[i];
            final session = detail ? _album : _songs;
            final active =
                _live && !loading && !_actionBusy && !_artistRoute && !_confirm;
            return SearchTrackRow(
              track: song,
              labels: (detail ? _albumLabels : _songLabels)[song.id],
              palette: p,
              number: i + 1,
              narrow: !MioDeviceScope.wide(
                context,
                MediaQuery.sizeOf(context).width,
              ),
              durationAtEnd: true,
              // Judged on the window: the page is hosted in a content area
              // narrower than the window on wide layouts.
              threeLine: MioDeviceScope.wide(
                context,
                MediaQuery.sizeOf(context).width,
              ),
              onPlay: active ? () => unawaited(_play(id: song.id)) : null,
              artist: _artistLink(song, detail),
              trailing: _selectingSongs
                  ? OnlineSelectionToggle(
                      palette: p,
                      track: song,
                      selected: _selectedSongs.contains(song.id),
                      onPressed:
                          active &&
                              (_selectedSongs.contains(song.id) ||
                                  _selectedSongs.length < 200)
                          ? () => setState(() {
                              if (!_selectedSongs.remove(song.id)) {
                                _selectedSongs.add(song.id);
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
                          identity: (session, song.id, _favoriteRevision),
                          active: active,
                          prepare: () => session!.prepareFavorite(id: song.id),
                          songTitle: song.title,
                          addToPlaylist: onlineSongTarget(
                            () => session!.prepareSelectedSave(ids: [song.id]),
                          ),
                          onReload: () => setState(() => _favoriteRevision++),
                        ),
                        SizedBox(
                          width: 36,
                          height: 44,
                          child: MioButton(
                            palette: p,
                            label:
                                '${p.text('common.download')}: ${song.title}',
                            padding: EdgeInsets.zero,
                            onPressed: active
                                ? () => unawaited(_downloadLoaded(track: song))
                                : null,
                            child: MioIcon(
                              'Download',
                              size: 16,
                              color: p.secondary,
                            ),
                          ),
                        ),
                      ],
                    ),
            );
          },
        ),
        if (error != null && tracks.isNotEmpty)
          SliverToBoxAdapter(
            child: _error(
              error,
              _live && !loading ? () => unawaited(load()) : null,
            ),
          ),
        if (!more && tracks.isNotEmpty && error == null)
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: Center(
                child: Text(
                  p.text('music.singer.noMore'),
                  style: TextStyle(
                    fontSize: 13,
                    color: p.color('--td-text-color-placeholder'),
                  ),
                ),
              ),
            ),
          ),
        if (more && error == null)
          SliverToBoxAdapter(
            child: _button(
              loading
                  ? p.text('common.loading')
                  : (p.english ? 'Load more' : '加载更多'),
              _live && !loading && !_actionBusy
                  ? () => unawaited(load())
                  : null,
            ),
          ),
      ],
    );
  }

  Widget _albumList(bool wide) => LayoutBuilder(
    builder: (context, box) {
      final windowWide = MioDeviceScope.wide(
        context,
        MediaQuery.sizeOf(context).width,
      );
      final scale = MediaQuery.textScalerOf(context);
      final gap = windowWide ? 12.0 : (wide ? 16.0 : 12.0);
      final side = windowWide ? 16.0 : (wide ? 24.0 : 16.0);
      final columns = windowWide
          ? math.max(1, ((box.maxWidth - side * 2 + gap) / (128 + gap)).floor())
          : (wide
                ? math.max(1, ((box.maxWidth - 48 + gap) / (140 + gap)).floor())
                : 2);
      final coverSize =
          (box.maxWidth - side * 2 - (columns - 1) * gap) / columns;
      return CustomScrollView(
        controller: _albumScroll,
        slivers: [
          if (_albumError != null && _albums.isEmpty)
            SliverToBoxAdapter(
              child: _error(
                _albumError!,
                _live && !_albumLoading ? () => unawaited(_albumsLoad()) : null,
              ),
            ),
          if (_albums.isEmpty)
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text(
                  _albumLoading
                      ? p.text('common.loading')
                      : (_albumError == null
                            ? p.text('music.singer.noAlbums')
                            : ''),
                ),
              ),
            ),
          SliverPadding(
            padding: EdgeInsets.all(side),
            sliver: SliverGrid.builder(
              itemCount: _albums.length,
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: columns,
                mainAxisSpacing: gap,
                crossAxisSpacing: gap,
                mainAxisExtent: windowWide
                    ? math.max(0, coverSize) +
                          8 +
                          scale.scale(13) * 1.3 +
                          2 +
                          scale.scale(11) * 1.4 +
                          8
                    : math.max(0, coverSize) +
                          6 +
                          scale.scale(13) * 1.5 +
                          2 +
                          scale.scale(12) * 1.5,
              ),
              itemBuilder: (context, i) {
                final card = _albums[i];
                final primary = p.color('--td-text-color-primary');
                final open =
                    _live &&
                        !_actionBusy &&
                        !_albumLoading &&
                        _profile?.albumDetails == true
                    ? () => unawaited(_openAlbum(card))
                    : null;
                if (windowWide) {
                  // Compact card with a tinted fill and hairline border.
                  return DecoratedBox(
                    decoration: BoxDecoration(
                      color: primary.withValues(alpha: .07),
                      border: Border.all(color: primary.withValues(alpha: .1)),
                      borderRadius: BorderRadius.circular(10),
                    ),
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(9),
                      child: MioButton(
                        palette: p,
                        label: card.title,
                        onPressed: open,
                        padding: EdgeInsets.zero,
                        insetFocusBorder: false,
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            MioCover(
                              card.cover,
                              size: math.max(1, coverSize),
                              radius: 0,
                            ),
                            Padding(
                              padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
                              child: Text(
                                card.title,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 13,
                                  height: 1.3,
                                  fontWeight: FontWeight.w600,
                                  color: primary,
                                ),
                              ),
                            ),
                            if ((card.songCount ?? 0) > 0)
                              Padding(
                                padding: const EdgeInsets.fromLTRB(8, 2, 8, 0),
                                child: Text(
                                  p.english
                                      ? '${card.songCount} songs'
                                      : '${card.songCount} 首歌曲',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    color: p.color(
                                      '--td-text-color-placeholder',
                                    ),
                                    fontSize: 11,
                                    height: 1.4,
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  );
                }
                return MioButton(
                  palette: p,
                  label: card.title,
                  onPressed: open,
                  padding: EdgeInsets.zero,
                  insetFocusBorder: false,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      MioCover(
                        card.cover,
                        size: math.max(1, coverSize),
                        radius: wide ? 8 : 16,
                      ),
                      const SizedBox(height: 6),
                      Text(
                        card.title,
                        style: const TextStyle(fontSize: 13),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      if ((card.songCount ?? 0) > 0)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            card.songCount == null
                                ? card.author
                                : (p.english
                                      ? '${card.songCount} songs'
                                      : '${card.songCount} 首歌曲'),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              color: p.color('--td-text-color-placeholder'),
                              fontSize: 12,
                            ),
                          ),
                        ),
                    ],
                  ),
                );
              },
            ),
          ),
          if (_albumError != null && _albums.isNotEmpty)
            SliverToBoxAdapter(
              child: _error(
                _albumError!,
                _live && !_albumLoading ? () => unawaited(_albumsLoad()) : null,
              ),
            ),
          if (_albumMore && _albumError == null)
            SliverToBoxAdapter(
              child: _button(
                _albumLoading
                    ? p.text('common.loading')
                    : (p.english ? 'Load more' : '加载更多'),
                _live && !_albumLoading ? () => unawaited(_albumsLoad()) : null,
              ),
            ),
        ],
      );
    },
  );
  Widget _confirmation({ValueChanged<bool>? dismiss}) => Positioned.fill(
    child: BlockSemantics(
      child: Stack(
        children: [
          Positioned.fill(
            child: GestureDetector(
              onTap: () {
                if (dismiss != null) {
                  dismiss(false);
                } else {
                  setState(() => _confirm = false);
                }
              },
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
                            ? 'Replace the queue with ${_contentTracks.length} loaded songs?'
                            : '用已加载的 ${_contentTracks.length} 首歌曲替换播放队列？',
                      ),
                      const SizedBox(height: 16),
                      Wrap(
                        spacing: 12,
                        children: [
                          _button(p.text('common.cancel'), () {
                            if (dismiss != null) {
                              dismiss(false);
                            } else {
                              setState(() => _confirm = false);
                            }
                          }),
                          _button(
                            p.text('common.confirm'),
                            _live
                                ? () {
                                    if (dismiss != null) {
                                      dismiss(true);
                                    } else {
                                      unawaited(_play());
                                    }
                                  }
                                : null,
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
  );
  @override
  Widget build(BuildContext context) => PopScope(
    canPop: widget.onClose != null || (!_detail && !_confirm && !_modalBusy),
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop && widget.onClose == null) _back();
    },
    child: CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): _back},
      child: DefaultTextStyle(
        style: TextStyle(
          color: p.color('--td-text-color-primary'),
          fontFamily: 'PingFang SC',
          fontFamilyFallback: const ['sans-serif'],
          fontSize: 14,
          decoration: TextDecoration.none,
        ),
        child: ColoredBox(
          color: p.color('--search-bg'),
          child: SafeArea(
            top: widget.onClose == null,
            bottom: widget.onClose == null,
            child: LayoutBuilder(
              builder: (context, box) {
                final wide = MioDeviceScope.wide(context, box.maxWidth);
                return Stack(
                  children: [
                    ExcludeFocus(
                      excluding: _confirm,
                      child: ExcludeSemantics(
                        excluding: _confirm,
                        child: Column(
                          children: [
                            if (_terminal != null)
                              _error(
                                _terminal!,
                                !_invalid && !_resuming
                                    ? () => unawaited(_resume())
                                    : null,
                              ),
                            if (_resuming) Text(p.text('common.loading')),
                            Flexible(
                              flex: 0,
                              child: ConstrainedBox(
                                constraints: BoxConstraints(
                                  maxHeight: math.max(0, box.maxHeight - 100),
                                ),
                                child: SingleChildScrollView(
                                  child: wide || _detail
                                      ? _header(wide, box.maxWidth)
                                      : _phoneHeader(box.maxWidth),
                                ),
                              ),
                            ),
                            if (!_detail)
                              Container(
                                alignment: Alignment.centerLeft,
                                padding: EdgeInsets.symmetric(
                                  horizontal: wide
                                      ? 24
                                      : MioPalette.gutter(box.maxWidth),
                                ),
                                decoration: BoxDecoration(
                                  border: Border(
                                    bottom: BorderSide(color: p.border),
                                  ),
                                ),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  spacing: wide ? 24 : 12,
                                  children: [
                                    _tabButton(
                                      '${p.text('music.singer.tabSongs')}($_songTotal)',
                                      !_tabAlbums,
                                      _live && !_actionBusy
                                          ? () {
                                              setState(
                                                () => _tabAlbums = false,
                                              );
                                              _refill(_epoch);
                                            }
                                          : null,
                                    ),
                                    _tabButton(
                                      '${p.text('music.singer.tabAlbums')}(${_profile?.albumCount ?? _albumTotal})',
                                      _tabAlbums,
                                      _live && !_actionBusy
                                          ? () {
                                              setState(() => _tabAlbums = true);
                                              if (_albums.isEmpty &&
                                                  _albumMore) {
                                                unawaited(_albumsLoad());
                                              }
                                              _refill(_epoch);
                                            }
                                          : null,
                                    ),
                                  ],
                                ),
                              ),
                            if (_selectingSongs && (!_tabAlbums || _detail))
                              OnlineSelectionToolbar(
                                palette: p,
                                tracks: _contentTracks,
                                selected: _selectedSongs,
                                selecting: true,
                                enabled:
                                    _live && !_actionBusy && !_contentLoading,
                                onChanged: (selecting, ids) => setState(() {
                                  _selectingSongs = selecting;
                                  _selectedSongs = ids;
                                }),
                                onAppend: () => unawaited(_appendSelected()),
                              ),
                            if (_actionError != null)
                              Semantics(
                                liveRegion: true,
                                child: Text(_actionError!),
                              ),
                            if (_playing) Text(p.text('common.loading')),
                            Expanded(
                              child: _phoneBody(
                                wide,
                                Stack(
                                  fit: StackFit.expand,
                                  children: [
                                    if (_tabAlbums)
                                      ExcludeFocus(
                                        excluding: _detail,
                                        child: Offstage(
                                          offstage: _detail,
                                          child: _albumList(wide),
                                        ),
                                      ),
                                    if (_detail)
                                      _songList(true)
                                    else if (!_tabAlbums)
                                      _songList(false),
                                  ],
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    if (_confirm && widget.onClose == null) _confirmation(),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    ),
  );
}

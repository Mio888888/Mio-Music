import 'device.dart';

import 'dart:async';
import 'dart:convert' show utf8;
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'play_choice.dart';
import 'song_target.dart';
import 'cover.dart';
import 'track_cover.dart';

export 'cover.dart' show MioCover;
import 'context_menu.dart';
import 'favorite_button.dart';
import 'resources.dart';
import 'playlist_editor.dart';
import 'playlist_delete.dart';
import 'playlist_copy.dart';
import 'playlist_sort.dart';
import 'playlist_file_import.dart';
import 'playlist_network_import.dart';
import 'playlist_cover_editor.dart';
import 'saved_playlist_cover.dart';
import 'local_tag_editor.dart';
import 'download_toolbar.dart';
import 'online_playlist_header.dart';
import 'download_task_card.dart';

enum CatalogKind { local, playlists, downloads, savedPlaylist }

enum _LocalLibraryAction { playAll, import }

enum _PlaylistAction { file, network, queue }

enum _PlaylistRowAction { view, edit, delete }

enum _SavedPlaylistAction { edit, cover, delete }

enum _SongAction { play, remove, select, copy, editTags }

class CatalogView extends StatefulWidget {
  final CatalogKind kind;
  final CoreController core;
  final MioPalette palette;
  final bool active;
  final bool ready;
  final double viewportWidth;
  final String? playlistId;
  final ValueChanged<String>? onOpenPlaylist;
  final ValueChanged<String>? onPlaylistDeleted;
  final VoidCallback? onRecent;
  const CatalogView({
    super.key,
    required this.kind,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.playlistId,
    this.onOpenPlaylist,
    this.onPlaylistDeleted,
    this.onRecent,
  });
  @override
  State<CatalogView> createState() => _CatalogViewState();
}

/// View requests are revisioned independently of process-owned playback.
/// A download handle keeps the ordering immutable throughout one paging run.
class _CatalogViewState extends State<CatalogView> with WidgetsBindingObserver {
  final _scroll = ScrollController();
  final _filter = TextEditingController();
  final _filterFocus = FocusNode();
  String? _focusedDownloadTaskId;
  String? _downloadReturnTaskId;
  List<Object> _items = [];
  int _total = 0;
  List<int> _counts = [0, 0, 0];
  api.PlaylistInfo? _playlist;
  api.DownloadSnapshot? _downloadSnapshot;
  api.LocalTrackSession? _localSession;
  api.LibrarySession? _librarySession;
  api.PlaylistSort _sort = api.PlaylistSort.default_;
  api.DownloadBucket _bucket = api.DownloadBucket.active;
  api.AudioImport? _audioImport;
  AudioFileSelection? _audioSelection;
  bool _importBusy = false;
  bool _importCancelling = false;
  int _importEpoch = 0;
  String? _importMessage;
  bool _importFailed = false;
  bool _loading = false;
  bool _editing = false, _fileImportOpen = false, _coverOpen = false;
  bool _deleting = false;
  bool _localSearchOpen = false;
  String? _editingPlaylistId;
  api.LibrarySession? _cardSession;
  RawDialogRoute<dynamic>? _playlistRoute;
  bool _songActionsBusy = false, _songCommandPending = false;
  bool _tagEditorOpen = false;
  RawDialogRoute<_SongAction>? _songMenu;
  RawDialogRoute<PlaylistCopyResult>? _copyRoute;
  String? _songMessage;
  bool _selecting = false;
  final _selected = <String, int>{};
  int _selectionBytes = 0;
  String? _selectionError;
  bool _downloadBusy = false, _downloadSettingsOpen = false;
  bool _canStartDownloads = false, _canPauseDownloads = false;
  int _concurrent = 3;
  String? _downloadMessage;
  bool _failedReset = false;
  bool _sortCapacity = false;
  bool _loaded = false;
  bool _more = false;
  bool _foreground = true;
  String? _error;
  Timer? _debounce;
  Timer? _poll;
  int _epoch = 0;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;
  bool get _catalogBusy =>
      _deleting || _songActionsBusy || _fileImportOpen || _coverOpen;
  bool get _wide => MioDeviceScope.wide(context, widget.viewportWidth);

  /// Wide local music and playlists lay out their own title/toolbar rows.
  bool get _wideOwnHeader =>
      _wide &&
      (widget.kind == CatalogKind.local ||
          widget.kind == CatalogKind.playlists ||
          (widget.kind == CatalogKind.savedPlaylist && _playlist != null));

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.paused &&
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.detached;
    WidgetsBinding.instance.addObserver(this);
    FocusManager.instance.addListener(_rememberDownloadFocus);
    _scroll.addListener(_onScroll);
    if (_live) unawaited(_load(reset: true));
    _configurePoll();
  }

  void _rememberDownloadFocus() {
    if (!mounted || !widget.active || widget.kind != CatalogKind.downloads) {
      return;
    }
    final focusedContext = FocusManager.instance.primaryFocus?.context;
    if (focusedContext?.findAncestorStateOfType<_CatalogViewState>() == this) {
      _focusedDownloadTaskId = focusedContext
          ?.findAncestorWidgetOfExactType<DownloadTaskCard>()
          ?.task
          .id;
    }
  }

  @override
  void didUpdateWidget(CatalogView old) {
    super.didUpdateWidget(old);
    if (widget.kind == CatalogKind.savedPlaylist &&
        old.playlistId != widget.playlistId) {
      _releaseLibrary();
      _items = [];
      _loaded = false;
      _playlist = null;
      _editing = false;
      _sort = api.PlaylistSort.default_;
      _songMessage = null;
    }
    if ((!widget.active && old.active) || (!widget.ready && old.ready)) {
      if (widget.kind == CatalogKind.downloads &&
          MioDeviceScope.isTv(context)) {
        _downloadReturnTaskId = _focusedDownloadTaskId;
      }
      _cancelImport();
      _localSession?.close();
      _localSession?.dispose();
      _localSession = null;
      _releaseLibrary();
      _releaseDownloads();
      _editing = false;
      _epoch++;
      _loading = false;
      _debounce?.cancel();
    }
    if (_live &&
        ((!old.active && widget.active) ||
            (!old.ready && widget.ready) ||
            (widget.kind == CatalogKind.savedPlaylist &&
                old.playlistId != widget.playlistId))) {
      unawaited(_load(reset: true));
    }
    _configurePoll();
  }

  Future<void> _importPlaylistFile({
    bool fromQueue = false,
    bool fromNetwork = false,
  }) async {
    if (!_live ||
        _loading ||
        _editing ||
        _catalogBusy ||
        widget.kind != CatalogKind.playlists) {
      return;
    }
    if (fromQueue && widget.core.player == null) return;
    setState(() => _fileImportOpen = true);
    try {
      bool isCurrent() =>
          mounted &&
          widget.active &&
          widget.ready &&
          widget.kind == CatalogKind.playlists &&
          _fileImportOpen;
      final saved = fromNetwork
          ? await showPlaylistNetworkImport(
              context: context,
              palette: p,
              isCurrent: isCurrent,
            )
          : await showPlaylistFileImport(
              context: context,
              palette: p,
              queue: fromQueue ? widget.core.player : null,
              isCurrent: () =>
                  mounted &&
                  widget.active &&
                  widget.ready &&
                  widget.kind == CatalogKind.playlists &&
                  _fileImportOpen,
            );
      if (mounted && saved != null) {
        setState(
          () => _songMessage = p.english
              ? 'Imported ${saved.title}.'
              : '已导入 ${saved.title}。',
        );
      }
    } catch (error) {
      if (mounted) {
        final code = error.toString();
        setState(
          () => _songMessage = code.contains('queue_empty')
              ? p.text('music.songlist.playingListEmpty')
              : code.contains('queue_online_unsupported')
              ? (p.english
                    ? 'Saving a queue containing online songs is not supported. Save loaded songs from their source page.'
                    : '暂不支持将含在线歌曲的队列导入歌单，请在来源页面保存已加载歌曲。')
              : (p.english
                    ? 'Import unavailable or queue changed. Reopen to retry.'
                    : '无法导入或队列已变化，请重新打开后重试。'),
        );
      }
    } finally {
      if (mounted) setState(() => _fileImportOpen = false);
      if (_live) unawaited(_load(reset: true));
    }
  }

  Future<void> _editCover() async {
    final session = _librarySession, id = widget.playlistId;
    if (!_live ||
        _loading ||
        _editing ||
        _selecting ||
        _catalogBusy ||
        _error != null ||
        session == null ||
        id == null ||
        _playlist?.id != id ||
        widget.kind != CatalogKind.savedPlaylist) {
      return;
    }
    setState(() => _coverOpen = true);
    try {
      final saved = await showPlaylistCoverEditor(
        context: context,
        session: session,
        palette: p,
        isCurrent: () =>
            mounted &&
            widget.active &&
            widget.ready &&
            widget.playlistId == id &&
            widget.kind == CatalogKind.savedPlaylist &&
            _coverOpen &&
            identical(session, _librarySession),
      );
      if (mounted && saved != null) {
        setState(
          () =>
              _songMessage = p.english ? 'Playlist cover updated.' : '歌单封面已更新。',
        );
      }
    } catch (_) {
      if (mounted) {
        setState(
          () => _songMessage = p.english
              ? 'Cover editor unavailable. Refresh and reopen.'
              : '无法编辑封面，请刷新并重新打开。',
        );
      }
    } finally {
      if (mounted) setState(() => _coverOpen = false);
      if (_live) unawaited(_load(reset: true));
    }
  }

  Future<void> _deletePlaylist() async {
    final session = _librarySession;
    final playlist = _playlist;
    if (!_live ||
        _loading ||
        _editing ||
        _selecting ||
        _catalogBusy ||
        _error != null ||
        session == null ||
        playlist == null ||
        playlist.id != widget.playlistId) {
      return;
    }
    setState(() => _deleting = true);
    PlaylistDeleteResult? result;
    try {
      result = await Navigator.of(context).push<PlaylistDeleteResult>(
        RawDialogRoute<PlaylistDeleteResult>(
          barrierDismissible: false,
          barrierColor: const Color(0x00000000),
          pageBuilder: (context, animation, secondaryAnimation) =>
              PlaylistDeleteDialog(
                session: session,
                playlist: playlist,
                core: widget.core,
                palette: p,
              ),
        ),
      );
    } finally {
      if (mounted) setState(() => _deleting = false);
    }
    if (!mounted) return;
    if (result == PlaylistDeleteResult.deleted) {
      widget.onPlaylistDeleted?.call(playlist.id);
    } else if (_live &&
        (result == PlaylistDeleteResult.refresh || _librarySession == null)) {
      unawaited(_load(reset: true));
    }
  }

  void _releaseDownloads() {
    final snapshot = _downloadSnapshot;
    _downloadSnapshot = null;
    snapshot?.close();
    snapshot?.dispose();
  }

  Future<void> _downloadCommand(
    api.DownloadItem task,
    api.DownloadAction action,
  ) async {
    final snapshot = _downloadSnapshot;
    if (!_live ||
        _loading ||
        _downloadBusy ||
        snapshot == null ||
        _error != null) {
      return;
    }
    setState(() => _downloadBusy = true);
    try {
      await widget.core.command(
        () => snapshot.command(id: task.id, action: action),
      );
    } finally {
      if (mounted) setState(() => _downloadBusy = false);
    }
    if (_live) unawaited(_load(reset: true));
  }

  Future<void> _downloadBulk(api.DownloadBulkAction action) async {
    final snapshot = _downloadSnapshot;
    if (!_live ||
        _loading ||
        _downloadBusy ||
        snapshot == null ||
        _error != null) {
      return;
    }
    final origin = FocusManager.instance.primaryFocus;
    setState(() => _downloadBusy = true);
    try {
      await widget.core.command(() => snapshot.bulkCommand(action: action));
    } finally {
      if (mounted) setState(() => _downloadBusy = false);
    }
    if (!_live) return;
    await _load(reset: true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_live) return;
      final current = FocusManager.instance.primaryFocus;
      if (current != null && current != origin && current is! FocusScopeNode) {
        return;
      }
      if (origin?.context?.mounted == true &&
          origin!.parent != null &&
          origin.canRequestFocus) {
        origin.requestFocus();
      } else if (_filterFocus.context?.mounted == true &&
          _filterFocus.parent != null &&
          _filterFocus.canRequestFocus) {
        _filterFocus.requestFocus();
      }
    });
  }

  Future<void> _setConcurrent(int value) async {
    final snapshot = _downloadSnapshot;
    if (!_live ||
        _loading ||
        _downloadBusy ||
        _error != null ||
        snapshot == null) {
      return;
    }
    final epoch = _epoch;
    setState(() {
      _downloadBusy = true;
      _downloadMessage = null;
    });
    try {
      final actual = await snapshot.setConcurrent(next: value);
      if (_live && epoch == _epoch && identical(snapshot, _downloadSnapshot)) {
        setState(() => _concurrent = actual);
      }
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(
          () => _downloadMessage = p.english
              ? 'Unable to change concurrency. Refresh and retry.'
              : '无法修改并发数，请刷新后重试。',
        );
      }
    } finally {
      if (mounted) setState(() => _downloadBusy = false);
      if (_live) unawaited(_load(reset: true));
    }
  }

  Future<void> _openDownloaded(api.DownloadItem task) async {
    final snapshot = _downloadSnapshot;
    if (!_live ||
        _loading ||
        _downloadBusy ||
        _error != null ||
        snapshot == null ||
        task.state != api.DownloadState.done ||
        task.missing) {
      return;
    }
    final epoch = _epoch;
    setState(() {
      _downloadBusy = true;
      _downloadMessage = null;
    });
    try {
      final name = await snapshot.completedFileName(id: task.id);
      if (!_live ||
          epoch != _epoch ||
          !identical(snapshot, _downloadSnapshot)) {
        return;
      }
      await openCompletedDownload(name);
      if (_live && epoch == _epoch) {
        setState(
          () => _downloadMessage = p.english
              ? 'Requested system file opening.'
              : '已请求系统打开文件。',
        );
      }
    } catch (error) {
      if (_live && epoch == _epoch) {
        final noHandler =
            error is PluginFileException && error.code == 'no_handler';
        setState(
          () => _downloadMessage = noHandler
              ? (p.english
                    ? 'No application can open this audio file.'
                    : '没有可打开此音频文件的应用。')
              : (p.english
                    ? 'File opening unavailable. Refresh and retry.'
                    : '文件无法打开，请刷新后重试。'),
        );
      }
    } finally {
      if (mounted) setState(() => _downloadBusy = false);
    }
  }

  void _clearSelection() {
    _selecting = false;
    _selected.clear();
    _selectionBytes = 0;
    _selectionError = null;
  }

  bool get _canSelect =>
      _live &&
      !_loading &&
      !_editing &&
      !_catalogBusy &&
      _error == null &&
      widget.kind == CatalogKind.savedPlaylist &&
      _librarySession != null &&
      _playlist?.id == widget.playlistId;

  void _toggleSong(api.Track song) {
    if (!_canSelect || !_selecting) return;
    setState(() {
      _selectionError = null;
      final old = _selected.remove(song.id);
      if (old != null) {
        _selectionBytes -= old;
        return;
      }
      final bytes = utf8.encode(song.id).length;
      if (bytes == 0 || bytes > 4096 || song.id.contains('\u0000')) {
        _selectionError = p.english
            ? 'This song has an invalid identifier.'
            : '此歌曲标识无效。';
      } else if (_selected.length >= 200 ||
          _selectionBytes + bytes > 64 * 1024) {
        _selectionError = p.english
            ? 'Selection capacity reached (up to 200 songs). Complete an operation or deselect some songs to continue.'
            : '已达到选择容量（最多 200 首）。请先处理当前所选歌曲，或取消部分选择。';
      } else {
        _selected[song.id] = bytes;
        _selectionBytes += bytes;
      }
    });
  }

  Future<void> _removeSelected() async {
    final session = _librarySession;
    if (!_canSelect || !_selecting || _selected.isEmpty || session == null) {
      return;
    }
    // No await between checking the displayed cursor and capturing this batch.
    final ids = _selected.keys.toList(growable: false);
    setState(() {
      _songActionsBusy = true;
      _songCommandPending = true;
      _selectionError = null;
    });
    try {
      await widget.core.command(() => session.removeSongs(songIds: ids));
    } finally {
      if (mounted) {
        setState(() {
          _songActionsBusy = false;
          _songCommandPending = false;
        });
        // Success invalidates the cursor. Failure also requires a new view:
        // the original transaction may have rejected an external writer.
        if (_live) unawaited(_load(reset: true));
      }
    }
  }

  Future<bool> _openCopy(api.LibrarySession session, List<String> ids) async {
    final route = RawDialogRoute<PlaylistCopyResult>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (context, animation, secondaryAnimation) =>
          PlaylistCopyDialog(
            session: session,
            songIds: List.unmodifiable(ids),
            core: widget.core,
            palette: p,
          ),
    );
    _copyRoute = route;
    setState(() => _songMessage = null);
    try {
      final result = await Navigator.of(context).push(route);
      if (_live &&
          identical(session, _librarySession) &&
          result?.added != null) {
        setState(
          () => _songMessage = p
              .text('common.addedSongsToPlaylist')
              .replaceAll('{count}', '${result!.added}')
              .replaceAll('{name}', result.destination ?? ''),
        );
      }
      return result != null;
    } finally {
      if (identical(_copyRoute, route)) _copyRoute = null;
    }
  }

  Future<void> _playSelected() async {
    final session = _librarySession;
    if (!_canSelect || !_selecting || _selected.isEmpty || session == null) {
      return;
    }
    final ids = _selected.keys.toList(growable: false);
    final sort = _sort;
    setState(() {
      _songActionsBusy = true;
      _songCommandPending = true;
      _songMessage = null;
    });
    var started = false;
    try {
      await widget.core.command(() async {
        await session.playSongs(songIds: ids, sort: sort);
        started = true;
      });
    } finally {
      if (mounted) {
        setState(() {
          _songActionsBusy = false;
          _songCommandPending = false;
          if (_live && started && identical(session, _librarySession)) {
            _songMessage = p
                .text('music.list.replacedPlaylist')
                .replaceAll('{count}', '${ids.length}');
          }
        });
        if (_live && (!started || !identical(session, _librarySession))) {
          unawaited(_load(reset: true));
        }
      }
    }
  }

  Future<void> _copySelected() async {
    final session = _librarySession;
    if (!_canSelect || !_selecting || _selected.isEmpty || session == null) {
      return;
    }
    final ids = _selected.keys.toList(growable: false);
    setState(() => _songActionsBusy = true);
    var refresh = false;
    try {
      refresh = await _openCopy(session, ids);
    } finally {
      if (mounted) {
        setState(() => _songActionsBusy = false);
        if (_live && (refresh || !identical(session, _librarySession))) {
          unawaited(_load(reset: true));
        }
      }
    }
  }

  void _releaseLibrary() {
    _clearSelection();
    final routes = <Route<dynamic>?>[_songMenu, _copyRoute, _playlistRoute];
    _playlistRoute = null;
    _cardSession?.close();
    _cardSession = null;
    _songMenu = null;
    _copyRoute = null;
    for (final route in routes) {
      if (route != null) {
        // Also called from build/dispose: defer navigation, close Rust now.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (route.isActive) route.navigator?.removeRoute(route);
        });
      }
    }
    final session = _librarySession;
    _librarySession = null;
    session?.close();
    session?.dispose();
  }

  Future<void> _showLocalActions(
    api.Track song,
    api.LocalTrackSession session,
    Offset anchor,
  ) async {
    if (!_live ||
        _loading ||
        _editing ||
        _catalogBusy ||
        _error != null ||
        widget.kind != CatalogKind.local ||
        !identical(session, _localSession)) {
      return;
    }
    final epoch = _epoch, navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox || !overlay.hasSize) return;
    final route = RawDialogRoute<_SongAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (_, _, _) => MioContextMenu(
        palette: p,
        anchor: overlay.globalToLocal(anchor),
        label: p.english ? 'Song actions' : '歌曲操作',
        entries: [
          MioMenuEntry(
            value: _SongAction.play,
            label: p.text('music.songList.play'),
            icon: 'TD-PlayCircle',
          ),
          if (MioDeviceScope.of(context)?.tagEdit != false)
            MioMenuEntry(
              value: _SongAction.editTags,
              label: p.text('music.tagEditor.title'),
              icon: 'EditNote',
            ),
        ],
      ),
    );
    _songMenu = route;
    setState(() => _songActionsBusy = true);
    var attempted = false;
    try {
      final action = await navigator.push(route);
      await route.completed;
      bool isCurrent() =>
          mounted &&
          widget.active &&
          widget.ready &&
          widget.kind == CatalogKind.local &&
          epoch == _epoch &&
          identical(session, _localSession);
      if (!mounted || action == null || !_live || !isCurrent()) return;
      if (action == _SongAction.editTags) {
        attempted = true;
        setState(() => _tagEditorOpen = true);
        final saved = await showLocalTagEditor(
          context: context,
          session: session,
          id: song.id,
          palette: p,
          isCurrent: isCurrent,
        );
        if (mounted && saved != null) {
          setState(() => _songMessage = p.text('music.tagEditor.saveSuccess'));
        }
      } else if (action == _SongAction.play) {
        await widget.core.command(() => session.play(id: song.id));
      }
    } catch (_) {
      if (mounted) {
        setState(
          () => _songMessage = p.english
              ? 'Tag editor unavailable or local music changed. Refresh and reopen.'
              : '标签编辑不可用或本地音乐已变化，请刷新后重新打开。',
        );
      }
    } finally {
      if (identical(_songMenu, route)) _songMenu = null;
      if (mounted) {
        setState(() {
          _tagEditorOpen = false;
          _songActionsBusy = false;
        });
      }
      if (attempted && _live) unawaited(_load(reset: true));
    }
  }

  Future<void> _showSongActions(
    api.Track song,
    api.LibrarySession session,
    Offset anchor,
  ) async {
    if (!_live ||
        _loading ||
        _editing ||
        _selecting ||
        _catalogBusy ||
        _error != null ||
        widget.kind != CatalogKind.savedPlaylist ||
        !identical(session, _librarySession) ||
        _playlist?.id != widget.playlistId) {
      return;
    }
    final epoch = _epoch;
    final playlist = widget.playlistId;
    final navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox || !overlay.hasSize) return;
    final localAnchor = overlay.globalToLocal(anchor);
    final route = RawDialogRoute<_SongAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (context, animation, secondaryAnimation) => MioContextMenu(
        palette: p,
        anchor: localAnchor,
        label: p.english ? 'Song actions' : '歌曲操作',
        entries: [
          MioMenuEntry(
            value: _SongAction.play,
            label: p.text('music.songList.play'),
            icon: 'TD-PlayCircle',
          ),
          MioMenuEntry(
            value: _SongAction.copy,
            label: p.text('common.addToPlaylist'),
            icon: 'PlaylistAdd',
          ),
          MioMenuEntry(
            value: _SongAction.select,
            label: p.text('common.batchSelect'),
            icon: 'TD-Check',
          ),
          MioMenuEntry(
            value: _SongAction.remove,
            label: p.text('music.songList.removeFromPlaylist'),
            icon: 'TD-Delete',
            danger: true,
          ),
        ],
      ),
    );
    _songMenu = route;
    setState(() => _songActionsBusy = true);
    var attempted = false;
    try {
      final action = await navigator.push(route);
      if (action == null ||
          !_live ||
          epoch != _epoch ||
          playlist != widget.playlistId ||
          !identical(session, _librarySession)) {
        return;
      }
      if (action == _SongAction.select) {
        setState(() => _selecting = true);
        // The menu owns the busy flag until finally; select the first row
        // afterwards through the same bounded toggle used by checkboxes.
        return;
      }
      if (action == _SongAction.copy) {
        attempted = await _openCopy(session, [song.id]);
        return;
      }
      attempted = true;
      setState(() => _songCommandPending = true);
      await widget.core.command(
        () => action == _SongAction.remove
            ? session.removeSongs(songIds: [song.id])
            : session.play(songId: song.id),
      );
    } finally {
      if (identical(_songMenu, route)) _songMenu = null;
      if (mounted) {
        setState(() {
          _songActionsBusy = false;
          _songCommandPending = false;
        });
        if (_selecting &&
            identical(session, _librarySession) &&
            epoch == _epoch) {
          _toggleSong(song);
        }
        if (_live &&
            (attempted ||
                _librarySession == null ||
                playlist != widget.playlistId)) {
          unawaited(_load(reset: true));
        }
      }
    }
  }

  void _cancelImport() {
    _importEpoch++;
    _audioImport?.cancel();
    final selection = _audioSelection;
    if (selection != null) unawaited(selection.cancel());
  }

  Future<void> _importAudio() async {
    if (!_live || _importBusy) return;
    final epoch = ++_importEpoch;
    final selection = AudioFileSelection();
    _audioSelection = selection;
    setState(() {
      _importBusy = true;
      _importCancelling = false;
      _importMessage = null;
      _importFailed = false;
    });
    api.AudioImport? operation;
    try {
      operation = await api.beginAudioImport();
      if (!mounted ||
          epoch != _importEpoch ||
          !widget.active ||
          !widget.ready) {
        return;
      }
      _audioImport = operation;
      final ticket = await selection.choose();
      // System choosers may pause Flutter; route/runtime ownership remains the
      // admission boundary, independently of the catalog's paging lifecycle.
      if (ticket == null &&
          mounted &&
          epoch == _importEpoch &&
          _importCancelling) {
        setState(
          () => _importMessage = p.english ? 'Import cancelled.' : '已取消导入。',
        );
      }
      if (ticket == null ||
          !mounted ||
          epoch != _importEpoch ||
          !widget.active ||
          !widget.ready) {
        return;
      }
      final count = await operation.commit(ticket: ticket);
      if (!mounted || epoch != _importEpoch) return;
      setState(() {
        _importMessage = p.english
            ? 'Imported $count songs.'
            : '已导入 $count 首歌曲。';
      });
      if (_live) await _load(reset: true);
    } catch (error) {
      if (mounted && epoch == _importEpoch) {
        final code = error is PluginFileException ? error.code : '';
        setState(() {
          _importFailed = !_importCancelling;
          _importMessage = _importCancelling
              ? (p.english ? 'Import cancelled.' : '已取消导入。')
              : code == 'file_too_large'
              ? (p.english
                    ? 'Choose 1–100 files, up to 256 MiB each and 512 MiB total.'
                    : '请选择 1–100 个文件，每个不超过 256 MiB，总计不超过 512 MiB。')
              : code == 'picker_unavailable'
              ? (p.english
                    ? 'File picker unavailable on this device.'
                    : '此设备暂不可使用文件选择器。')
              : (p.english
                    ? 'Unable to import. Check that the files contain playable audio and try again.'
                    : '导入未完成，请确认文件是可播放的音频后重新选择。');
        });
      }
    } finally {
      if (identical(_audioImport, operation)) _audioImport = null;
      operation?.cancel();
      operation?.dispose();
      await selection.discard();
      if (identical(_audioSelection, selection)) {
        _audioSelection = null;
        if (mounted) setState(() => _importBusy = false);
      }
    }
  }

  void _configurePoll() {
    if (widget.kind == CatalogKind.downloads && _live) {
      _poll ??= Timer.periodic(const Duration(seconds: 2), (timer) {
        // Running tasks refresh every tick; an idle list only every fifth.
        if (!_canPauseDownloads && timer.tick % 5 != 0) return;
        if (!_loading && !_downloadBusy && _error == null && _live) {
          unawaited(_load(reset: true));
        }
      });
    } else {
      _poll?.cancel();
      _poll = null;
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live && !_coverOpen && !_tagEditorOpen) {
        unawaited(_load(reset: true));
      }
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      // The tag dialog owns picker pauses and invalidates ordinary background
      // edits itself. Keep its original local session until the dialog closes.
      if (_tagEditorOpen && state == AppLifecycleState.paused) {
        _configurePoll();
        return;
      }
      _localSession?.close();
      _localSession?.dispose();
      _localSession = null;
      // The cover dialog owns picker lifecycle and its original cursor.
      if (!_coverOpen) _releaseLibrary();
      _releaseDownloads();
      _editing = false;
      _epoch++;
      _loading = false;
      _debounce?.cancel();
    }
    _configurePoll();
  }

  void _onScroll() {
    if (_scroll.hasClients &&
        _scroll.position.extentAfter < 300 &&
        _more &&
        _error == null &&
        !_loading &&
        _live) {
      unawaited(_load());
    }
  }

  void _queryChanged(String _) {
    if (widget.kind == CatalogKind.local) {
      _localSession?.close();
      _localSession?.dispose();
      _localSession = null;
    }
    _debounce?.cancel();
    _epoch++;
    setState(() {
      _items = [];
      _loaded = false;
      _more = false;
      _loading = false;
      _error = null;
    });
    _debounce = Timer(const Duration(milliseconds: 250), () {
      if (_live) unawaited(_load(reset: true));
    });
  }

  void _selectBucket(api.DownloadBucket bucket) {
    if (_bucket == bucket || _downloadBusy) return;
    setState(() {
      _bucket = bucket;
      _items = [];
      _loaded = false;
      _more = false;
    });
    unawaited(_load(reset: true));
  }

  void _cycleSort(api.SortColumn column) {
    _changeSort(api.cyclePlaylistSort(sort: _sort, column: column));
  }

  void _changeSort(api.PlaylistSort sort) {
    if (!_live ||
        _loading ||
        _editing ||
        _catalogBusy ||
        _librarySession == null) {
      return;
    }
    setState(() {
      _sort = sort;
      _items = [];
      _loaded = false;
      _more = false;
    });
    if (_scroll.hasClients) _scroll.jumpTo(0);
    unawaited(_load(reset: true, keepLibrary: true));
  }

  Future<void> _load({bool reset = false, bool keepLibrary = false}) async {
    if (_coverOpen) return;
    if (!_live || _catalogBusy || _downloadBusy || (_loading && !reset)) return;
    final epoch = ++_epoch;
    final returnDownloadTask = _downloadReturnTaskId;
    _downloadReturnTaskId = null;
    final desired = reset && widget.kind == CatalogKind.downloads
        ? math.max(100, _items.length)
        : 100;
    var offset = reset ? 0 : _items.length;
    final incoming = <Object>[];
    var total = _total;
    var more = false;
    var counts = _counts;
    var concurrent = _concurrent;
    var canStart = _canStartDownloads, canPause = _canPauseDownloads;
    var playlist = _playlist;
    setState(() {
      _loading = true;
      _error = null;
      _sortCapacity = false;
    });
    try {
      if (widget.kind == CatalogKind.local &&
          (reset || _localSession == null)) {
        _localSession?.close();
        _localSession?.dispose();
        _localSession = null;
        final session = await api.beginLocalTracks(query: _filter.text);
        if (!_live || epoch != _epoch) {
          session.close();
          session.dispose();
          return;
        }
        _localSession = session;
      }
      if ((widget.kind == CatalogKind.playlists ||
              widget.kind == CatalogKind.savedPlaylist) &&
          ((reset && !keepLibrary) || _librarySession == null)) {
        _releaseLibrary();
        final session = await api.beginLibrary(
          id: widget.kind == CatalogKind.savedPlaylist
              ? widget.playlistId
              : null,
        );
        if (!_live || epoch != _epoch) {
          session.close();
          session.dispose();
          return;
        }
        _librarySession = session;
      }
      if (widget.kind == CatalogKind.downloads &&
          (reset || _downloadSnapshot == null)) {
        final handle = await api.downloadSnapshot(
          bucket: _bucket,
          query: _filter.text,
        );
        if (!_live || epoch != _epoch) {
          handle.close();
          handle.dispose();
          return;
        }
        final old = _downloadSnapshot;
        _downloadSnapshot = handle;
        old?.close();
        old?.dispose();
      }
      do {
        List<Object> rows;
        switch (widget.kind) {
          case CatalogKind.local:
            final page = await _localSession!.page(offset: offset, limit: 100);
            rows = page.items;
            total = page.total;
            more = page.hasMore;
          case CatalogKind.playlists:
            final page = await _librarySession!.playlists(
              offset: offset,
              limit: 100,
            );
            rows = page.items;
            total = page.total;
            more = page.hasMore;
          case CatalogKind.savedPlaylist:
            final page = await _librarySession!.sortedPlaylist(
              sort: _sort,
              offset: offset,
              limit: 100,
            );
            rows = page.tracks.items;
            total = page.tracks.total;
            more = page.tracks.hasMore;
            playlist = page.playlist;
          case CatalogKind.downloads:
            final page = await _downloadSnapshot!.page(
              offset: offset,
              limit: 100,
            );
            rows = page.items;
            total = page.total;
            more = page.hasMore;
            counts = page.counts;
            concurrent = page.concurrent;
            canStart = page.canStart;
            canPause = page.canPause;
        }
        if (!_live || epoch != _epoch) return;
        if (rows.isEmpty && more) {
          throw StateError('Catalog page did not advance');
        }
        incoming.addAll(rows);
        offset += rows.length;
      } while (reset &&
          widget.kind == CatalogKind.downloads &&
          more &&
          incoming.length < desired);
      if (mounted &&
          reset &&
          widget.kind == CatalogKind.downloads &&
          MioDeviceScope.isTv(context)) {
        final focusedTask =
            FocusManager.instance.primaryFocus?.context
                ?.findAncestorWidgetOfExactType<DownloadTaskCard>()
                ?.task
                .id ??
            returnDownloadTask;
        // Route restoration runs after the frame and can restore an old card.
        // Wait until that restoration and the refreshed list have both settled.
        if (focusedTask != null &&
            !incoming.any(
              (item) => (item as api.DownloadItem).id == focusedTask,
            )) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (_live &&
                epoch == _epoch &&
                _filterFocus.context?.mounted == true &&
                _filterFocus.parent != null &&
                _filterFocus.canRequestFocus) {
              _filterFocus.requestFocus();
            }
          });
        }
      }
      setState(() {
        _items = reset ? incoming : [..._items, ...incoming];
        _total = total;
        _more = more;
        _loaded = true;
        _counts = counts;
        _concurrent = concurrent;
        _canStartDownloads = canStart;
        _canPauseDownloads = canPause;
        _playlist = playlist;
      });
    } catch (error) {
      if (_live && epoch == _epoch) {
        _failedReset = reset || widget.kind != CatalogKind.downloads;
        setState(() {
          _clearSelection();
          _sortCapacity =
              widget.kind == CatalogKind.savedPlaylist &&
              error.toString().contains(
                'capacity_exceeded: playlist sort keys',
              );
          _error = _sortCapacity
              ? (p.english
                    ? 'This playlist exceeds sorting capacity. Use default order to continue.'
                    : '此歌单超出排序容量，请切回默认顺序继续浏览。')
              : (p.english ? 'Unable to load. Please retry.' : '加载失败，请重试');
        });
      }
    } finally {
      if (mounted && epoch == _epoch) setState(() => _loading = false);
    }
  }

  Widget _localTrackCover(api.Track song, api.LocalTrackSession session) =>
      MioTrackCover(
        identity: (session, song.id),
        load: () => session.cover(id: song.id),
        active: _live,
        size: _wide ? 44 : 48,
        radius: _wide ? 8 : 10,
      );

  @override
  void dispose() {
    _cancelImport();
    _epoch++;
    _debounce?.cancel();
    _poll?.cancel();
    _releaseDownloads();
    _localSession?.close();
    _localSession?.dispose();
    _releaseLibrary();
    WidgetsBinding.instance.removeObserver(this);
    FocusManager.instance.removeListener(_rememberDownloadFocus);
    _scroll.dispose();
    _filter.dispose();
    _filterFocus.dispose();
    super.dispose();
  }

  String get _title => switch (widget.kind) {
    CatalogKind.local => p.text('music.local.title'),
    CatalogKind.playlists => p.text('music.songlist.title'),
    CatalogKind.downloads => p.text('download.title'),
    CatalogKind.savedPlaylist =>
      _playlist?.title ?? p.text('music.songlist.title'),
  };
  @override
  Widget build(BuildContext context) {
    final width = widget.viewportWidth;
    final rem = MioPalette.rem(width);
    final downloads = widget.kind == CatalogKind.downloads;
    final gutter = _wide
        ? (downloads || widget.kind == CatalogKind.savedPlaylist
              ? 20.0
              : widget.kind == CatalogKind.playlists
              ? rem * 2
              : 24.0)
        : MioPalette.gutter(width);
    final headerTitle = Container(
      padding: EdgeInsets.only(left: _wide ? 12 : 0),
      decoration: _wide
          ? BoxDecoration(
              border: Border(
                left: BorderSide(
                  color: p.color('--td-brand-color-3'),
                  width: 8,
                ),
              ),
            )
          : null,
      child: Text(
        _title,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: _wide
              ? (widget.kind == CatalogKind.playlists ? rem * 1.875 : 28)
              : (width * .08).clamp(28, 38.4),
          height: 1.1,
          fontWeight: !_wide && widget.kind == CatalogKind.local
              ? FontWeight.w700
              : FontWeight.w600,
        ),
      ),
    );
    final headerActions = <Widget>[
      if (widget.kind == CatalogKind.local)
        MioButton(
          palette: p,
          label: p.text('music.local.importFiles'),
          onPressed: _live && !_importBusy
              ? () => unawaited(_importAudio())
              : null,
          child: MioIcon('FolderPlus', color: p.secondary),
        ),
      if (widget.kind == CatalogKind.playlists)
        SizedBox(
          width: 44,
          height: 44,
          child: MioButton(
            palette: p,
            label: p.text('music.recent.title'),
            onPressed: _live && !_editing && !_catalogBusy
                ? widget.onRecent
                : null,
            child: MioIcon('History', color: p.secondary),
          ),
        ),
      if (widget.kind == CatalogKind.savedPlaylist && _playlist != null)
        MioButton(
          palette: p,
          label: p.text('music.list.editCover'),
          padding: EdgeInsets.zero,
          onPressed:
              _live &&
                  !_loading &&
                  !_editing &&
                  !_selecting &&
                  !_catalogBusy &&
                  _error == null &&
                  _librarySession != null
              ? () => unawaited(_editCover())
              : null,
          child: MioSavedPlaylistCover(
            id: _playlist!.id,
            source: _playlist!.cover,
            session: _librarySession,
            size: 44,
          ),
        ),
      if (widget.kind == CatalogKind.playlists ||
          widget.kind == CatalogKind.savedPlaylist)
        MioButton(
          palette: p,
          label: p.text(
            widget.kind == CatalogKind.playlists
                ? 'music.songlist.newPlaylist'
                : 'music.songlist.editPlaylist',
          ),
          onPressed:
              _live &&
                  !_editing &&
                  !_selecting &&
                  !_catalogBusy &&
                  !_loading &&
                  _error == null &&
                  (widget.kind == CatalogKind.playlists ||
                      _playlist?.id == widget.playlistId)
              ? () => setState(() {
                  _editingPlaylistId = null;
                  _editing = true;
                })
              : null,
          child: Text(
            p.text(
              widget.kind == CatalogKind.playlists
                  ? 'music.songlist.newPlaylist'
                  : 'music.songlist.editPlaylist',
            ),
          ),
        ),
      if (widget.kind == CatalogKind.playlists)
        MioButton(
          palette: p,
          label: p.text('music.songlist.fromLocalFile'),
          onPressed: _live && !_loading && !_editing && !_catalogBusy
              ? () => unawaited(_importPlaylistFile())
              : null,
          child: MioIcon('PlaylistImport', color: p.secondary),
        ),
      if (widget.kind == CatalogKind.playlists)
        MioButton(
          palette: p,
          label: p.text('music.songlist.fromNetwork'),
          onPressed: _live && !_loading && !_editing && !_catalogBusy
              ? () => unawaited(_importPlaylistFile(fromNetwork: true))
              : null,
          child: MioIcon('PlaylistImport', color: p.secondary),
        ),
      if (widget.kind == CatalogKind.playlists)
        MioButton(
          palette: p,
          label: p.text('music.songlist.fromPlayingList'),
          onPressed: _live && !_loading && !_editing && !_catalogBusy
              ? () => unawaited(_importPlaylistFile(fromQueue: true))
              : null,
          child: MioIcon('nav-liebiao', color: p.secondary),
        ),
      if (widget.kind == CatalogKind.savedPlaylist)
        SizedBox(
          width: 44,
          height: 44,
          child: MioButton(
            palette: p,
            label: p.text('music.songlist.deletePlaylist'),
            onPressed:
                _live &&
                    !_loading &&
                    !_editing &&
                    !_selecting &&
                    !_catalogBusy &&
                    _error == null &&
                    _playlist?.id == widget.playlistId &&
                    _librarySession != null
                ? () => unawaited(_deletePlaylist())
                : null,
            child: MioIcon('Delete', color: p.secondary),
          ),
        ),
      if (widget.kind == CatalogKind.downloads)
        SizedBox(
          width: 44,
          height: 44,
          child: MioButton(
            palette: p,
            label: p.text('common.titleBar.settings'),
            background: _downloadSettingsOpen
                ? p.brand.withValues(alpha: .12)
                : null,
            onPressed: _live && !_downloadBusy
                ? () => setState(
                    () => _downloadSettingsOpen = !_downloadSettingsOpen,
                  )
                : null,
            child: MioIcon('nav-shezhi', color: p.secondary),
          ),
        ),
      MioButton(
        palette: p,
        label: p.text('common.refresh'),
        onPressed:
            widget.ready &&
                !_loading &&
                !_editing &&
                !_catalogBusy &&
                !_downloadBusy
            ? () => unawaited(_load(reset: true))
            : null,
        child: MioIcon('Refresh', color: p.secondary),
      ),
    ];
    final body = Padding(
      padding: EdgeInsets.fromLTRB(
        gutter,
        _wide
            ? (downloads || widget.kind == CatalogKind.savedPlaylist
                  ? 20
                  : widget.kind == CatalogKind.playlists
                  ? rem
                  : 24)
            : (width * .035).clamp(14, 22),
        gutter,
        _wide
            ? (downloads || widget.kind == CatalogKind.savedPlaylist ? 20 : 24)
            : 0,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (downloads)
            DownloadToolbar(
              palette: p,
              narrow: !_wide,
              rem: rem,
              enabled: _live && !_loading && !_downloadBusy && _error == null,
              searchEnabled: _live && !_downloadBusy,
              activeBucket: _bucket == api.DownloadBucket.active,
              canStart: _canStartDownloads,
              canPause: _canPauseDownloads,
              canClear: _items.isNotEmpty,
              settingsOpen: _downloadSettingsOpen,
              leading: DownloadTabs(
                palette: p,
                narrow: !_wide,
                inline: true,
                enabled: _live && !_downloadBusy,
                selected: _bucket,
                counts: _counts,
                onSelected: _selectBucket,
              ),
              filter: _filter,
              filterFocus: _filterFocus,
              onQuery: _queryChanged,
              onBulk: (action) => unawaited(_downloadBulk(action)),
              onSettings: () => setState(
                () => _downloadSettingsOpen = !_downloadSettingsOpen,
              ),
            )
          else if (_wide && widget.kind == CatalogKind.local) ...[
            // No page title on wide layouts: one toolbar row carries the
            // actions, the song count and the search box.
            _wideLocalToolbar(),
            const SizedBox(height: 12),
          ] else if (_wide &&
              widget.kind == CatalogKind.savedPlaylist &&
              _playlist != null) ...[
            _wideSavedHeader(),
            const SizedBox(height: 20),
          ] else if (_wide && widget.kind == CatalogKind.playlists) ...[
            // No page title on wide layouts: the count, the actions and an
            // icon-only refresh share a single row.
            Row(
              children: [
                Expanded(
                  child: Text(
                    p
                        .text('music.songlist.myPlaylists')
                        .replaceAll('{count}', '$_total'),
                    style: TextStyle(
                      fontSize: rem * 1.25,
                      height: 24 / (rem * 1.25),
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
                _widePlaylistActions(rem),
                SizedBox(width: rem * .75),
                SizedBox.square(
                  dimension: 32,
                  child: MioButton(
                    palette: p,
                    label: p.text('common.refresh'),
                    radius: 6,
                    padding: EdgeInsets.zero,
                    onPressed: _live && !_loading && !_editing && !_catalogBusy
                        ? () => unawaited(_load(reset: true))
                        : null,
                    child: Center(
                      child: MioIcon('Refresh', size: 18, color: p.secondary),
                    ),
                  ),
                ),
              ],
            ),
            SizedBox(height: rem * 1.5),
          ] else if (_wide)
            Row(
              children: [
                Expanded(child: headerTitle),
                ...headerActions,
              ],
            )
          else if (widget.kind == CatalogKind.local) ...[
            // No page title: import joins the toolbar and the count rides on
            // the play-all button.
            _narrowLocalToolbar(),
          ] else if (widget.kind == CatalogKind.playlists) ...[
            // No page title: the count and an icon refresh lead, the create and
            // import actions follow.
            Row(
              children: [
                Expanded(
                  child: Text(
                    p
                        .text('music.songlist.myPlaylists')
                        .replaceAll('{count}', '$_total'),
                    style: const TextStyle(
                      fontSize: 17.6,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
                SizedBox.square(
                  dimension: 44,
                  child: MioButton(
                    palette: p,
                    label: p.text('common.refresh'),
                    padding: EdgeInsets.zero,
                    radius: 999,
                    onPressed: _live && !_loading && !_editing && !_catalogBusy
                        ? () => unawaited(_load(reset: true))
                        : null,
                    child: Center(
                      child: MioIcon('Refresh', size: 20, color: p.secondary),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            _narrowPlaylistActions(),
          ] else ...[
            headerTitle,
            const SizedBox(height: 8),
            Wrap(spacing: 4, runSpacing: 4, children: headerActions),
          ],
          if (!downloads && !_wideOwnHeader) const SizedBox(height: 8),
          if (_loaded &&
              !downloads &&
              !_wideOwnHeader &&
              (_wide ||
                  (widget.kind != CatalogKind.playlists &&
                      widget.kind != CatalogKind.local)))
            Text(
              p
                  .text(
                    widget.kind == CatalogKind.playlists
                        ? 'music.songlist.myPlaylists'
                        : 'music.local.totalSongs',
                  )
                  .replaceAll('{count}', '$_total'),
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
          if (!downloads && !_wideOwnHeader) const SizedBox(height: 12),
          if (widget.kind == CatalogKind.local &&
              !_wide &&
              _localSearchOpen) ...[
            MioInput(
              palette: p,
              controller: _filter,
              focusNode: _filterFocus,
              placeholder: p.text(
                widget.kind == CatalogKind.local
                    ? 'music.local.searchPlaceholder'
                    : 'download.searchTask',
              ),
              onChanged: _queryChanged,
              readOnly: _downloadBusy,
              height: _wide ? 34 : 44,
            ),
            const SizedBox(height: 12),
          ],
          if (downloads && _downloadSettingsOpen)
            DownloadConcurrency(
              palette: p,
              narrow: !_wide,
              value: _concurrent,
              enabled:
                  _live &&
                  !_loading &&
                  !_downloadBusy &&
                  _error == null &&
                  _downloadSnapshot != null,
              onChanged: (value) => unawaited(_setConcurrent(value)),
            ),
          if (widget.kind == CatalogKind.downloads && _downloadMessage != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Semantics(
                liveRegion: true,
                child: Text(_downloadMessage!),
              ),
            ),
          if (widget.kind == CatalogKind.local &&
              (_importBusy || _importMessage != null))
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      _importBusy
                          ? (p.english
                                ? 'Selecting and importing audio…'
                                : '正在选择并导入音频…')
                          : _importMessage!,
                      style: TextStyle(
                        color: _importFailed
                            ? p.color('--td-error-color')
                            : p.secondary,
                      ),
                    ),
                  ),
                  if (_importBusy)
                    MioButton(
                      palette: p,
                      label: p.text('common.cancel'),
                      onPressed: () {
                        setState(() => _importCancelling = true);
                        _audioImport?.cancel();
                        final selection = _audioSelection;
                        if (selection != null) unawaited(selection.cancel());
                      },
                      child: Text(p.text('common.cancel')),
                    ),
                ],
              ),
            ),
          if (_songMessage != null)
            Semantics(
              liveRegion: true,
              child: Text(
                _songMessage!,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(color: p.secondary),
              ),
            ),
          if (_songCommandPending)
            Semantics(
              liveRegion: true,
              child: Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Text(
                  p.english ? 'Updating song…' : '正在处理歌曲操作…',
                  style: TextStyle(color: p.secondary),
                ),
              ),
            ),
          if (_downloadBusy)
            Semantics(
              liveRegion: true,
              child: Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Text(
                  p.english ? 'Updating download task…' : '正在处理下载任务…',
                  style: TextStyle(color: p.secondary),
                ),
              ),
            ),
          if (_error != null)
            Row(
              children: [
                Expanded(
                  child: Text(
                    _error!,
                    style: TextStyle(color: p.color('--td-error-color')),
                  ),
                ),
                MioButton(
                  palette: p,
                  label: _sortCapacity
                      ? p.text('music.songList.defaultSort')
                      : p.english
                      ? 'Retry'
                      : '重试',
                  onPressed: _loading
                      ? null
                      : _sortCapacity
                      ? () => _changeSort(api.PlaylistSort.default_)
                      : () => unawaited(_load(reset: _failedReset || !_loaded)),
                  child: Text(
                    _sortCapacity
                        ? p.text('music.songList.defaultSort')
                        : p.english
                        ? 'Retry'
                        : '重试',
                  ),
                ),
              ],
            ),
          if (widget.kind == CatalogKind.savedPlaylist && !_editing) ...[
            _selectionToolbar(),
            if (!_wide) _sortHeaders(),
          ],
          Expanded(
            child: _wrapWideSaved(
              _editing
                  ? PlaylistMetadataForm(
                      playlistId: widget.kind == CatalogKind.savedPlaylist
                          ? widget.playlistId
                          : _editingPlaylistId,
                      palette: p,
                      core: widget.core,
                      onClose: () => setState(() => _editing = false),
                      onSaved: () {
                        if (!_live) return;
                        setState(() => _editing = false);
                        unawaited(_load(reset: true));
                      },
                    )
                  : !_loaded && _items.isEmpty
                  ? Center(
                      child: Text(
                        _error != null ? '' : (p.english ? 'Loading…' : '加载中…'),
                        style: TextStyle(color: p.secondary),
                      ),
                    )
                  : _items.isEmpty && downloads
                  ? Align(
                      alignment: Alignment.topCenter,
                      child: DownloadEmpty(
                        palette: p,
                        narrow: !_wide,
                        searching: _filter.text.isNotEmpty,
                      ),
                    )
                  : _items.isEmpty && widget.kind == CatalogKind.local
                  ? _narrowLocalEmpty()
                  : _items.isEmpty &&
                        _wide &&
                        widget.kind == CatalogKind.playlists
                  ? _widePlaylistEmpty(rem)
                  : _items.isEmpty &&
                        !_wide &&
                        widget.kind == CatalogKind.playlists
                  ? _narrowPlaylistEmpty()
                  : _items.isEmpty
                  ? Center(
                      child: Text(
                        p.text(switch (widget.kind) {
                          CatalogKind.local =>
                            _filter.text.isEmpty
                                ? 'music.local.emptyTitle'
                                : 'music.local.noMatchMusic',
                          CatalogKind.playlists => 'music.songlist.emptyTitle',
                          CatalogKind.savedPlaylist =>
                            'music.songlist.emptyPlaylist',
                          CatalogKind.downloads =>
                            _filter.text.isEmpty
                                ? 'download.empty'
                                : 'download.noMatch',
                        }),
                        style: TextStyle(color: p.secondary),
                      ),
                    )
                  : _list(rem),
            ),
          ),
          if (_loading && _items.isNotEmpty)
            Padding(
              padding: const EdgeInsets.all(8),
              child: Text(
                p.english ? 'Loading…' : '加载中…',
                style: TextStyle(color: p.secondary, fontSize: 12),
              ),
            ),
        ],
      ),
    );
    return downloads ? ColoredBox(color: p.surface, child: body) : body;
  }

  Widget _list(double rem) {
    if (widget.kind == CatalogKind.playlists) {
      if (!_wide) {
        return ListView.separated(
          controller: _scroll,
          padding: const EdgeInsets.only(bottom: 16),
          itemCount: _items.length + (_more ? 1 : 0),
          separatorBuilder: (_, _) => const SizedBox(height: 8),
          itemBuilder: (_, i) => i == _items.length
              ? _loadMore()
              : _narrowPlaylistRow(_items[i] as api.PlaylistInfo),
        );
      }
      return LayoutBuilder(
        builder: (context, box) {
          final gap = _wide ? rem * 1.5 : 12.0;
          final columns = _wide
              ? math.max(1, ((box.maxWidth + gap) / (240 + gap)).floor())
              : 2;
          final cardWidth = (box.maxWidth - gap * (columns - 1)) / columns;
          final scale = MediaQuery.textScalerOf(context);
          final captionHeight = math.max(
            82.0,
            math.max(scale.scale(14) * 2.4, scale.scale(10) * 3.6 + 2) +
                scale.scale(12) * 1.2 +
                30,
          );
          final cardHeight =
              (_wide ? 180.0 : cardWidth) +
              (_wide ? captionHeight : math.max(168.0, captionHeight + 76));
          return GridView.builder(
            controller: _scroll,
            itemCount: _items.length + (_more ? 1 : 0),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: columns,
              crossAxisSpacing: gap,
              mainAxisSpacing: gap,
              mainAxisExtent: cardHeight,
            ),
            itemBuilder: (context, i) => i == _items.length
                ? _loadMore()
                : _playlistCard(_items[i] as api.PlaylistInfo, cardWidth),
          );
        },
      );
    }
    final rows = ListView.builder(
      controller: _scroll,
      itemCount: _items.length + (_more ? 1 : 0),
      findChildIndexCallback: widget.kind == CatalogKind.downloads
          ? (key) {
              if (key is! ValueKey<String>) return null;
              final index = _items.indexWhere(
                (item) => (item as api.DownloadItem).id == key.value,
              );
              return index < 0 ? null : index;
            }
          : null,
      itemBuilder: (context, i) => i == _items.length
          ? _loadMore()
          : widget.kind == CatalogKind.downloads
          ? _downloadRow(_items[i] as api.DownloadItem)
          : _trackRow(_items[i] as api.Track, i),
    );
    if (_wide && widget.kind == CatalogKind.local) {
      return Column(
        children: [
          _wideLocalHeader(),
          Expanded(child: rows),
        ],
      );
    }
    return rows;
  }

  /// Desktop local library column header: song, album, duration, actions.
  Widget _wideLocalHeader() {
    Widget label(String key, {double? width}) {
      final text = Text(
        p.text(key),
        style: TextStyle(fontSize: 12, color: p.secondary),
      );
      return width == null
          ? Expanded(child: text)
          : SizedBox(width: width, child: text);
    }

    return Container(
      height: 32,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      child: Row(
        children: [
          const SizedBox(width: 44),
          label('music.local.colSong'),
          label('music.local.colAlbum', width: 180),
          label('music.local.colDuration', width: 60),
          label('music.local.colActions', width: 60),
        ],
      ),
    );
  }

  Future<void> _editLocalTags(api.Track song) async {
    final session = _localSession;
    if (session == null || !_live || _catalogBusy || _loading) return;
    final epoch = _epoch;
    bool isCurrent() =>
        mounted &&
        widget.active &&
        widget.ready &&
        widget.kind == CatalogKind.local &&
        epoch == _epoch &&
        identical(session, _localSession);
    setState(() => _tagEditorOpen = true);
    try {
      final saved = await showLocalTagEditor(
        context: context,
        session: session,
        id: song.id,
        palette: p,
        isCurrent: isCurrent,
      );
      if (mounted && saved != null) {
        setState(() => _songMessage = p.text('music.tagEditor.saveSuccess'));
      }
    } catch (_) {
      if (mounted) {
        setState(
          () => _songMessage = p.english
              ? 'Tag editor unavailable or local music changed. Refresh and reopen.'
              : '标签编辑不可用或本地音乐已变化，请刷新后重新打开。',
        );
      }
    } finally {
      if (mounted) setState(() => _tagEditorOpen = false);
      if (_live) unawaited(_load(reset: true));
    }
  }

  Widget _loadMore() => Center(
    child: MioButton(
      palette: p,
      label: p.english ? 'Load more' : '加载更多',
      onPressed: _loading ? null : () => unawaited(_load()),
      child: Text(p.english ? 'Load more' : '加载更多'),
    ),
  );

  /// Where a row's heart sends the song: the local library adds it by its
  /// track id; a saved playlist copies the original row.
  AddToPlaylist _songTarget(
    api.Track song,
    api.LocalTrackSession? local,
    api.LibrarySession? library,
  ) => (destinationId) async {
    if (widget.kind == CatalogKind.local && local != null) {
      return local.addToPlaylist(id: song.id, destinationId: destinationId);
    }
    if (library == null) throw StateError('Playlist session unavailable');
    return library.copySongs(songIds: [song.id], destinationId: destinationId);
  };

  Future<void> _playTrack(api.Track song) async {
    if (!_live || _loading || _catalogBusy || _selecting || _error != null) {
      return;
    }
    final local = _localSession;
    if (widget.kind == CatalogKind.local && local == null) {
      return;
    }
    final library = _librarySession;
    if (widget.kind == CatalogKind.savedPlaylist &&
        (_playlist?.id != widget.playlistId || library == null)) {
      return;
    }
    final choice = await choosePlayChoice(
      context: context,
      palette: p,
      core: widget.core,
      title: song.title,
    );
    if (choice == null || !mounted || !_live) return;
    if (choice == PlayChoice.queue) {
      await widget.core.command(
        () => widget.kind == CatalogKind.local
            ? local!.append(id: song.id)
            : library!.appendSong(songId: song.id),
      );
      if (mounted && widget.core.error == null) {
        showMioToast(context, p, p.english ? 'Added to queue' : '已加入播放列表');
      }
      return;
    }
    await widget.core.command(
      () => widget.kind == CatalogKind.local
          ? local!.play(id: song.id)
          : library!.play(songId: song.id),
    );
  }

  Widget _selectionToolbar() {
    final count = p
        .text('music.songList.selectedCount')
        .replaceAll('{count}', '${_selected.length}');
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              if (!_selecting)
                MioButton(
                  palette: p,
                  label: p.text('common.batchSelect'),
                  onPressed: _canSelect
                      ? () => setState(() => _selecting = true)
                      : null,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 28),
                    child: Center(
                      widthFactor: 1,
                      child: Text(p.text('common.batchSelect')),
                    ),
                  ),
                )
              else ...[
                Semantics(liveRegion: true, child: Text(count)),
                MioButton(
                  palette: p,
                  label: p.text('music.songList.play'),
                  onPressed: _canSelect && _selected.isNotEmpty
                      ? () => unawaited(_playSelected())
                      : null,
                  child: SizedBox(
                    width: 28,
                    height: 28,
                    child: Center(
                      child: MioIcon('TD-PlayCircle', color: p.brand),
                    ),
                  ),
                ),
                MioButton(
                  palette: p,
                  label: p.text('common.addToPlaylist'),
                  onPressed: _canSelect && _selected.isNotEmpty
                      ? () => unawaited(_copySelected())
                      : null,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 28),
                    child: Center(
                      widthFactor: 1,
                      child: Text(p.text('common.addToPlaylist')),
                    ),
                  ),
                ),
                MioButton(
                  palette: p,
                  label: p.text('music.songList.removeFromPlaylist'),
                  onPressed: _canSelect && _selected.isNotEmpty
                      ? () => unawaited(_removeSelected())
                      : null,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 28),
                    child: Center(
                      widthFactor: 1,
                      child: Text(
                        p.text('music.songList.removeFromPlaylist'),
                        style: TextStyle(color: p.color('--td-error-color')),
                      ),
                    ),
                  ),
                ),
                MioButton(
                  palette: p,
                  label: p.text('music.songList.done'),
                  onPressed: !_catalogBusy
                      ? () => setState(_clearSelection)
                      : null,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 28),
                    child: Center(
                      widthFactor: 1,
                      child: Text(p.text('music.songList.done')),
                    ),
                  ),
                ),
              ],
            ],
          ),
          if (_selecting)
            Text(
              p.english
                  ? 'Up to 200 selected songs per operation; continue selecting across pages.'
                  : '每次最多选择 200 首，翻页可继续选择。',
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
          if (_selectionError != null)
            Semantics(
              liveRegion: true,
              child: Text(
                _selectionError!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
            ),
        ],
      ),
    );
  }

  /// Wide saved playlists show their rows in the rounded song-list panel.
  Widget _wrapWideSaved(Widget content) {
    if (!(_wide && widget.kind == CatalogKind.savedPlaylist && !_editing)) {
      return content;
    }
    return ClipRRect(
      borderRadius: BorderRadius.circular(12),
      child: ColoredBox(
        color: p.color('--song-list-content-bg'),
        child: Column(
          children: [
            _sortHeaders(),
            Expanded(child: content),
          ],
        ),
      ),
    );
  }

  Widget _wideSavedHeader() {
    final playlist = _playlist!;
    final enabled =
        _live &&
        !_loading &&
        !_editing &&
        !_selecting &&
        !_catalogBusy &&
        _error == null &&
        _librarySession != null &&
        playlist.id == widget.playlistId;
    return OnlinePlaylistHeader(
      palette: p,
      title: playlist.title,
      description: 'By local',
      count: p.text('common.songCount').replaceAll('{count}', '$_total'),
      cover: playlist.cover,
      compact: false,
      wide: true,
      busy: true,
      search: false,
      filter: _filter,
      filterFocus: _filterFocus,
      onBack: null,
      onPlay: enabled && _total > 0
          ? () =>
                unawaited(widget.core.command(() => _librarySession!.playAll()))
          : null,
      onShuffle: null,
      onQuery: (_) {},
      onMore: enabled
          ? (anchor) => unawaited(_savedPlaylistMenu(anchor))
          : null,
    );
  }

  Future<void> _savedPlaylistMenu(Offset anchor) async {
    if (!_live || _catalogBusy || _playlistRoute != null) return;
    final epoch = _epoch;
    final navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox) return;
    final route = RawDialogRoute<_SavedPlaylistAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (_, _, _) => MioContextMenu<_SavedPlaylistAction>(
        palette: p,
        anchor: overlay.globalToLocal(anchor),
        label: p.english ? 'More actions' : '更多操作',
        entries: [
          MioMenuEntry(
            value: _SavedPlaylistAction.edit,
            label: p.text('music.songlist.editPlaylist'),
            icon: 'TD-Edit2',
          ),
          MioMenuEntry(
            value: _SavedPlaylistAction.cover,
            label: p.text('music.list.editCover'),
            icon: 'Playlist-Edit-Cover',
          ),
          MioMenuEntry(
            value: _SavedPlaylistAction.delete,
            label: p.text('music.songlist.deletePlaylist'),
            icon: 'Delete',
          ),
        ],
      ),
    );
    _playlistRoute = route;
    final action = await navigator.push(route);
    await route.completed;
    if (identical(_playlistRoute, route)) _playlistRoute = null;
    if (!_live || epoch != _epoch) return;
    switch (action) {
      case _SavedPlaylistAction.edit:
        setState(() {
          _editingPlaylistId = null;
          _editing = true;
        });
      case _SavedPlaylistAction.cover:
        await _editCover();
      case _SavedPlaylistAction.delete:
        await _deletePlaylist();
      case null:
        break;
    }
  }

  /// Desktop song_view header: `#`, sortable title and album, liked, duration.
  Widget _wideSortHeaders() {
    final enabled =
        _live &&
        !_loading &&
        !_editing &&
        !_catalogBusy &&
        _librarySession != null;
    Widget header(api.SortColumn column) => PlaylistSortHeader(
      palette: p,
      sort: _sort,
      column: column,
      narrow: false,
      onPressed: enabled ? () => _cycleSort(column) : null,
    );
    final muted = p.color('--song-list-header-text');
    return Container(
      height: 40,
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
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(left: 10),
              child: header(api.SortColumn.title),
            ),
          ),
          SizedBox(
            width: 200,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: header(api.SortColumn.album),
            ),
          ),
          SizedBox(
            width: 60,
            child: Text(
              p.text('music.songList.liked'),
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 12, color: muted),
            ),
          ),
          SizedBox(width: 80, child: header(api.SortColumn.duration)),
        ],
      ),
    );
  }

  /// Desktop song_view row (64 high): index, cover and title, album, liked and
  /// duration columns.
  Widget _wideSavedRow(api.Track song, int index) {
    final library = _librarySession;
    final playing = widget.core.player?.current?.id == song.id;
    final busy =
        !_live || _loading || _catalogBusy || _editing || _error != null;
    return Container(
      height: 64,
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
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
            child: _selecting
                ? Center(
                    child: SizedBox(
                      width: 44,
                      height: 44,
                      child: Semantics(
                        checked: _selected.containsKey(song.id),
                        child: MioButton(
                          palette: p,
                          label:
                              '${p.text('common.batchSelect')} · ${song.title}',
                          onPressed: _canSelect
                              ? () => _toggleSong(song)
                              : null,
                          child: Center(child: _selectionMark(song.id)),
                        ),
                      ),
                    ),
                  )
                : Text(
                    '${index + 1}',
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
              label: '${song.title} ${song.artist}',
              radius: 4,
              insetFocusBorder: false,
              padding: const EdgeInsets.only(left: 10),
              onPressed: busy
                  ? null
                  : _selecting
                  ? () => _toggleSong(song)
                  : () => unawaited(_playTrack(song)),
              child: Row(
                children: [
                  MioCover(song.cover, size: 40, radius: 4),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          song.title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 14,
                            height: 16.8 / 14,
                            fontWeight: FontWeight.w500,
                            color: playing
                                ? p.brand
                                : p.color('--song-list-title-color'),
                          ),
                        ),
                        const SizedBox(height: 4),
                        Text(
                          song.artist.isEmpty
                              ? p.text('common.unknown')
                              : song.artist,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12,
                            height: 14.4 / 12,
                            color: p.color('--song-list-artist-color'),
                          ),
                        ),
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
                song.album.isEmpty ? p.text('common.unknownAlbum') : song.album,
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
              child:
                  (song.id.startsWith('mio-online:') ||
                      song.id.startsWith('mio-local:'))
                  ? FavoriteButton(
                      key: ValueKey(song.id),
                      palette: p,
                      core: widget.core,
                      identity: (song.id, _localSession, library),
                      active:
                          _live &&
                          !_loading &&
                          !_catalogBusy &&
                          !_editing &&
                          !_selecting &&
                          _error == null,
                      prepare: () => library!.prepareFavorite(songId: song.id),
                      songTitle: song.title,
                      addToPlaylist: _songTarget(song, _localSession, library),
                      onSaved: () => unawaited(_load(reset: true)),
                      onReload: () => unawaited(_load(reset: true)),
                    )
                  : const SizedBox(width: 32),
            ),
          ),
          SizedBox(
            width: 80,
            child: Center(
              child: Text(
                _time(song.duration),
                style: TextStyle(
                  fontSize: 12,
                  color: p.color('--td-text-color-placeholder'),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _sortHeaders() {
    if (_wide) return _wideSortHeaders();
    final enabled =
        _live &&
        !_loading &&
        !_editing &&
        !_catalogBusy &&
        _librarySession != null;
    Widget header(api.SortColumn column) => PlaylistSortHeader(
      palette: p,
      sort: _sort,
      column: column,
      narrow: !_wide,
      onPressed: enabled ? () => _cycleSort(column) : null,
    );
    return ColoredBox(
      color: p.color('--song-list-header-bg'),
      child: Row(
        children: [
          if (_selecting) const SizedBox(width: 44),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                children: [
                  Expanded(child: header(api.SortColumn.title)),
                  if (_wide)
                    SizedBox(width: 180, child: header(api.SortColumn.album)),
                ],
              ),
            ),
          ),
          SizedBox(
            width: 44,
            child: Text(
              p.text('music.songList.liked'),
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 12,
                color: p.color('--song-list-header-text'),
              ),
            ),
          ),
          SizedBox(
            width: _wide ? 60 : 48,
            child: header(api.SortColumn.duration),
          ),
        ],
      ),
    );
  }

  Widget _trackRow(api.Track song, [int index = 0]) {
    final local = _localSession;
    final library = _librarySession;
    return MioContextRegion(
      label: p.english ? 'Song actions' : '歌曲操作',
      onOpen:
          widget.kind == CatalogKind.savedPlaylist &&
              library != null &&
              _live &&
              !_loading &&
              !_editing &&
              !_selecting &&
              !_catalogBusy &&
              _error == null
          ? (anchor) => unawaited(_showSongActions(song, library, anchor))
          : widget.kind == CatalogKind.local &&
                local != null &&
                _live &&
                !_loading &&
                !_editing &&
                !_catalogBusy &&
                _error == null
          ? (anchor) => unawaited(_showLocalActions(song, local, anchor))
          : null,
      child: _wide && widget.kind == CatalogKind.savedPlaylist
          ? _wideSavedRow(song, index)
          : SizedBox(
              height: _wide ? 60 : 68,
              child: Row(
                children: [
                  if (_selecting)
                    SizedBox(
                      width: 44,
                      height: 44,
                      child: Semantics(
                        checked: _selected.containsKey(song.id),
                        child: MioButton(
                          palette: p,
                          label:
                              '${p.text('common.batchSelect')} · ${song.title}',
                          onPressed: _canSelect
                              ? () => _toggleSong(song)
                              : null,
                          child: _selectionMark(song.id),
                        ),
                      ),
                    ),
                  if (_wide &&
                      widget.kind == CatalogKind.local &&
                      (song.id.startsWith('mio-online:') ||
                          song.id.startsWith('mio-local:')))
                    FavoriteButton(
                      key: ValueKey(song.id),
                      palette: p,
                      core: widget.core,
                      identity: (song.id, local, library),
                      active:
                          _live &&
                          !_loading &&
                          !_catalogBusy &&
                          !_editing &&
                          !_selecting &&
                          _error == null,
                      prepare: () => local!.prepareFavorite(id: song.id),
                      songTitle: song.title,
                      addToPlaylist: _songTarget(song, local, library),
                      onReload: () => unawaited(_load(reset: true)),
                    ),
                  Expanded(
                    child: MioButton(
                      palette: p,
                      label: '${song.title} ${song.artist}',
                      radius: 12,
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      onPressed:
                          !_live ||
                              _loading ||
                              _catalogBusy ||
                              _editing ||
                              _error != null
                          ? null
                          : _selecting
                          ? () => _toggleSong(song)
                          : () => unawaited(_playTrack(song)),
                      child: Row(
                        children: [
                          if (widget.kind == CatalogKind.local &&
                              _localSession != null)
                            _localTrackCover(song, _localSession!)
                          else
                            MioCover(
                              song.cover,
                              size: _wide ? 44 : 48,
                              radius: _wide ? 8 : 10,
                            ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Column(
                              mainAxisAlignment: MainAxisAlignment.center,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  song.title,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 14,
                                    color:
                                        widget.core.player?.current?.id ==
                                            song.id
                                        ? p.brand
                                        : p.color('--td-text-color-primary'),
                                  ),
                                ),
                                const SizedBox(height: 4),
                                Text(
                                  song.artist.isEmpty
                                      ? p.text('common.unknown')
                                      : song.artist,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 13,
                                    color: p.color(
                                      '--td-text-color-placeholder',
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ),
                          if (_wide)
                            SizedBox(
                              width: 180,
                              child: Text(
                                song.album.isEmpty
                                    ? p.text('common.unknownAlbum')
                                    : song.album,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 13,
                                  color: p.secondary,
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                  if (!(_wide && widget.kind == CatalogKind.local) &&
                      (song.id.startsWith('mio-online:') ||
                          song.id.startsWith('mio-local:')))
                    FavoriteButton(
                      key: ValueKey(song.id),
                      palette: p,
                      core: widget.core,
                      identity: (song.id, local, library),
                      active:
                          _live &&
                          !_loading &&
                          !_catalogBusy &&
                          !_editing &&
                          !_selecting &&
                          _error == null,
                      prepare: () => widget.kind == CatalogKind.local
                          ? local!.prepareFavorite(id: song.id)
                          : library!.prepareFavorite(songId: song.id),
                      songTitle: song.title,
                      addToPlaylist: _songTarget(song, local, library),
                      onSaved: widget.kind == CatalogKind.savedPlaylist
                          ? () => unawaited(_load(reset: true))
                          : null,
                      onReload: () => unawaited(_load(reset: true)),
                    )
                  else if (widget.kind == CatalogKind.savedPlaylist)
                    const SizedBox(width: 44),
                  SizedBox(
                    width: _wide ? 60 : 48,
                    child: Text(
                      _time(song.duration),
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                  ),
                  if (_wide &&
                      widget.kind == CatalogKind.local &&
                      MioDeviceScope.of(context)?.tagEdit != false)
                    SizedBox(
                      width: 60,
                      child: MioButton(
                        palette: p,
                        label: p.text('music.local.edit'),
                        radius: 4,
                        padding: EdgeInsets.zero,
                        onPressed: _live && !_catalogBusy && !_importBusy
                            ? () => unawaited(_editLocalTags(song))
                            : null,
                        child: Center(
                          child: Text(
                            p.text('music.local.edit'),
                            style: const TextStyle(fontSize: 14),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
    );
  }

  Widget _selectionMark(String id) => Container(
    width: 18,
    height: 18,
    decoration: BoxDecoration(
      color: _selected.containsKey(id) ? p.brand : null,
      border: Border.all(color: _selected.containsKey(id) ? p.brand : p.border),
      borderRadius: BorderRadius.circular(3),
    ),
    child: _selected.containsKey(id)
        ? MioIcon('TD-Check', color: p.color('--td-text-color-anti'), size: 14)
        : null,
  );

  bool get _localActionsEnabled =>
      _live &&
      _loaded &&
      !_loading &&
      !_importBusy &&
      !_catalogBusy &&
      _error == null &&
      _localSession != null;

  Widget _localImportButton({bool pill = true}) {
    final enabled = _live && !_importBusy && !_catalogBusy;
    return MioButton(
      palette: p,
      label: p.text('music.local.importFiles'),
      radius: pill ? 999 : 6,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      background: enabled
          ? p.brand
          : p.color('--td-bg-color-component-disabled'),
      hoverBackground: Color.alphaBlend(
        p.onBrand.computeLuminance() > .5
            ? const Color(0x14000000)
            : const Color(0x14ffffff),
        p.brand,
      ),
      disabledOpacity: 1,
      onPressed: enabled ? () => unawaited(_importAudio()) : null,
      child: SizedBox(
        height: 32,
        child: Center(
          widthFactor: 1,
          child: Text(
            p.text('music.local.importFiles'),
            style: TextStyle(
              color: enabled ? p.onBrand : p.secondary,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _playAllLocal() async {
    final session = _localSession;
    if (!_localActionsEnabled || session == null || _total == 0) return;
    setState(() => _songActionsBusy = true);
    try {
      await widget.core.command(() => session.playAll());
    } finally {
      if (mounted) setState(() => _songActionsBusy = false);
      if (_live && !identical(_localSession, session)) {
        unawaited(_load(reset: true));
      }
    }
  }

  void _toggleLocalSearch() {
    setState(() => _localSearchOpen = !_localSearchOpen);
    if (_localSearchOpen) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _localSearchOpen && _filterFocus.context != null) {
          _filterFocus.requestFocus();
        }
      });
    } else {
      _filterFocus.unfocus();
      if (_filter.text.isNotEmpty) {
        _filter.clear();
        _queryChanged('');
      }
    }
  }

  Widget _narrowLocalToolbar() {
    final playEnabled = _localActionsEnabled && _total > 0;
    Widget circle(String icon, String label, VoidCallback? run) => SizedBox(
      width: 44,
      height: 44,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.surface,
          border: Border.all(color: p.border),
          borderRadius: BorderRadius.circular(999),
        ),
        child: MioButton(
          palette: p,
          label: label,
          radius: 999,
          padding: EdgeInsets.zero,
          onPressed: run,
          child: Center(
            child: MioIcon(
              icon,
              size: 20,
              color: p.color('--td-text-color-primary'),
            ),
          ),
        ),
      ),
    );
    return Row(
      children: [
        Expanded(
          child: MioButton(
            palette: p,
            label: p.text('common.playAll'),
            radius: 999,
            background: playEnabled
                ? p.brand
                : p.color('--td-bg-color-component-disabled'),
            hoverBackground: Color.alphaBlend(
              p.onBrand.computeLuminance() > .5
                  ? const Color(0x14000000)
                  : const Color(0x14ffffff),
              p.brand,
            ),
            disabledOpacity: 1,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            onPressed: playEnabled ? () => unawaited(_playAllLocal()) : null,
            child: SizedBox(
              height: 44,
              child: Center(
                child: Text(
                  '${p.text('common.playAll')} · $_total',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: playEnabled ? p.onBrand : p.secondary,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        circle(
          'FolderPlus',
          p.text('music.local.importFiles'),
          _live && !_importBusy && !_catalogBusy
              ? () => unawaited(_importAudio())
              : null,
        ),
        const SizedBox(width: 8),
        circle(
          'Refresh',
          p.text('common.refresh'),
          _live && !_loading && !_importBusy && !_catalogBusy
              ? () => unawaited(_load(reset: true))
              : null,
        ),
        const SizedBox(width: 8),
        Builder(
          builder: (buttonContext) => circle(
            'TD-Ellipsis',
            p.english ? 'More actions' : '更多操作',
            _live && !_catalogBusy && !_importBusy
                ? () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      unawaited(
                        _localLibraryMenu(
                          box.localToGlobal(Offset(0, box.size.height)),
                        ),
                      );
                    }
                  }
                : null,
          ),
        ),
        const SizedBox(width: 8),
        circle(
          'Search',
          p.text('common.search'),
          _live && !_catalogBusy ? _toggleLocalSearch : null,
        ),
      ],
    );
  }

  /// Desktop wide layout: Play all, Refresh and More on the left, filter box
  /// on the right.
  Widget _wideLocalToolbar() {
    final playEnabled = _localActionsEnabled && _total > 0;
    Widget outlined(Widget child, String label, VoidCallback? run) => SizedBox(
      height: 32,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.surface,
          border: Border.all(color: p.border),
          borderRadius: BorderRadius.circular(6),
        ),
        child: MioButton(
          palette: p,
          label: label,
          radius: 6,
          padding: const EdgeInsets.symmetric(horizontal: 8),
          onPressed: run,
          child: Center(widthFactor: 1, child: child),
        ),
      ),
    );
    return Row(
      children: [
        MioButton(
          palette: p,
          label: p.text('common.playAll'),
          radius: 6,
          background: playEnabled
              ? p.brand
              : p.color('--td-bg-color-component-disabled'),
          hoverBackground: Color.alphaBlend(
            p.onBrand.computeLuminance() > .5
                ? const Color(0x14000000)
                : const Color(0x14ffffff),
            p.brand,
          ),
          disabledOpacity: 1,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          onPressed: playEnabled ? () => unawaited(_playAllLocal()) : null,
          child: SizedBox(
            height: 32,
            child: Center(
              widthFactor: 1,
              child: Text(
                p.text('common.playAll'),
                style: TextStyle(
                  color: playEnabled ? p.onBrand : p.secondary,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        outlined(
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              MioIcon(
                'FolderPlus',
                size: 16,
                color: p.color('--td-text-color-primary'),
              ),
              const SizedBox(width: 6),
              Text(
                p.text('music.local.importFiles'),
                style: TextStyle(
                  fontSize: 14,
                  color: p.color('--td-text-color-primary'),
                ),
              ),
            ],
          ),
          p.text('music.local.importFiles'),
          _live && !_importBusy && !_catalogBusy
              ? () => unawaited(_importAudio())
              : null,
        ),
        const SizedBox(width: 8),
        outlined(
          MioIcon(
            'Refresh',
            size: 16,
            color: p.color('--td-text-color-primary'),
          ),
          p.text('common.refresh'),
          _live && !_loading && !_importBusy && !_catalogBusy
              ? () => unawaited(_load(reset: true))
              : null,
        ),
        const SizedBox(width: 8),
        Builder(
          builder: (buttonContext) => outlined(
            MioIcon(
              'TD-Ellipsis',
              size: 16,
              color: p.color('--td-text-color-primary'),
            ),
            p.english ? 'More actions' : '更多操作',
            _live && !_catalogBusy && !_importBusy
                ? () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      unawaited(
                        _localLibraryMenu(
                          box.localToGlobal(Offset(0, box.size.height)),
                        ),
                      );
                    }
                  }
                : null,
          ),
        ),
        const Spacer(),
        Text(
          p.text('music.local.totalSongs').replaceAll('{count}', '$_total'),
          style: TextStyle(fontSize: 12, color: p.secondary),
        ),
        const SizedBox(width: 12),
        SizedBox(
          width: 240,
          child: MioInput(
            palette: p,
            controller: _filter,
            focusNode: _filterFocus,
            placeholder: p.text('music.local.searchPlaceholder'),
            onChanged: _queryChanged,
            readOnly: _downloadBusy,
            height: 34,
          ),
        ),
      ],
    );
  }

  Future<void> _localLibraryMenu(Offset anchor) async {
    if (!_live || _catalogBusy || _playlistRoute != null) return;
    final epoch = _epoch;
    final navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox) return;
    final route = RawDialogRoute<_LocalLibraryAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (_, _, _) => MioContextMenu<_LocalLibraryAction>(
        palette: p,
        anchor: overlay.globalToLocal(anchor),
        label: p.english ? 'More actions' : '更多操作',
        entries: [
          MioMenuEntry(
            value: _LocalLibraryAction.playAll,
            label: p.text('common.playAll'),
            icon: 'TD-PlayCircle',
            enabled: _localActionsEnabled && _total > 0,
          ),
          MioMenuEntry(
            value: _LocalLibraryAction.import,
            label: p.text('music.local.importFiles'),
            icon: 'FolderPlus',
            enabled: !_importBusy,
          ),
        ],
      ),
    );
    _playlistRoute = route;
    final action = await navigator.push(route);
    await route.completed;
    if (identical(_playlistRoute, route)) _playlistRoute = null;
    if (!_live || epoch != _epoch) return;
    if (action == _LocalLibraryAction.playAll) {
      await _playAllLocal();
    }
    if (action == _LocalLibraryAction.import) {
      await _importAudio();
    }
  }

  Widget _narrowLocalEmpty() {
    final searching = _filter.text.trim().isNotEmpty;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            p.text(
              searching ? 'music.local.noMatchMusic' : 'music.local.emptyTitle',
            ),
            textAlign: TextAlign.center,
            style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 14),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 480),
            child: Text(
              searching
                  ? p.text('music.local.emptySearchHint')
                  : (p.english
                        ? 'Choose audio files to add them to your local music library.'
                        : '选择音频文件，导入后即可建立本地音乐库。'),
              style: TextStyle(fontSize: 14, color: p.secondary),
            ),
          ),
          if (!searching) ...[
            const SizedBox(height: 14),
            _localImportButton(pill: false),
            const SizedBox(height: 14),
            Text(
              p.english ? 'Imported music is available offline.' : '导入后可离线播放',
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
          ],
        ],
      ),
    );
  }

  bool get _playlistActionsEnabled =>
      _live && !_loading && !_editing && !_catalogBusy && _error == null;

  void _newPlaylist() => setState(() {
    _editingPlaylistId = null;
    _editing = true;
  });

  Widget _narrowPlaylistActions() => Row(
    children: [
      Expanded(
        child: DecoratedBox(
          decoration: BoxDecoration(
            border: Border.all(color: p.border),
            borderRadius: BorderRadius.circular(999),
            color: p.surface,
          ),
          child: MioButton(
            palette: p,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            label: p.text('music.songlist.newPlaylist'),
            radius: 999,
            onPressed: _playlistActionsEnabled ? _newPlaylist : null,
            child: SizedBox(
              height: 44,
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  MioIcon(
                    'nav-zengjia',
                    color: p.color('--td-text-color-primary'),
                    size: 16,
                  ),
                  const SizedBox(width: 8),
                  Flexible(
                    child: Text(
                      p.text('music.songlist.newPlaylist'),
                      maxLines: 1,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
      const SizedBox(width: 8),
      Expanded(
        child: Builder(
          builder: (buttonContext) => MioButton(
            palette: p,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            label: p.text('common.import'),
            radius: 999,
            background: p.brand,
            onPressed: _playlistActionsEnabled
                ? () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      unawaited(
                        _playlistImportMenu(
                          box.localToGlobal(Offset(0, box.size.height)),
                        ),
                      );
                    }
                  }
                : null,
            child: SizedBox(
              height: 44,
              child: Center(
                child: Text(
                  p.text('common.import'),
                  style: TextStyle(color: p.color('--td-text-color-anti')),
                ),
              ),
            ),
          ),
        ),
      ),
    ],
  );

  /// Desktop wide header actions: outlined New playlist and a primary Import
  /// that opens the same method menu as the narrow layout.
  Widget _widePlaylistActions(double rem) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      SizedBox(
        height: 32,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: p.surface,
            border: Border.all(color: p.border),
            borderRadius: BorderRadius.circular(6),
          ),
          child: MioButton(
            palette: p,
            label: p.text('music.songlist.newPlaylist'),
            radius: 6,
            padding: const EdgeInsets.symmetric(horizontal: 9),
            onPressed: _playlistActionsEnabled ? _newPlaylist : null,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                MioIcon(
                  'nav-zengjia',
                  size: 14,
                  color: p.color('--td-text-color-primary'),
                ),
                const SizedBox(width: 6),
                Text(
                  p.text('music.songlist.newPlaylist'),
                  maxLines: 1,
                  style: const TextStyle(fontSize: 14),
                ),
              ],
            ),
          ),
        ),
      ),
      SizedBox(width: rem * .75),
      Builder(
        builder: (buttonContext) => SizedBox(
          height: 32,
          child: MioButton(
            palette: p,
            label: p.text('common.import'),
            radius: 6,
            background: p.brand,
            padding: const EdgeInsets.symmetric(horizontal: 15),
            onPressed: _playlistActionsEnabled
                ? () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      unawaited(
                        _playlistImportMenu(
                          box.localToGlobal(Offset(0, box.size.height)),
                        ),
                      );
                    }
                  }
                : null,
            child: Center(
              widthFactor: 1,
              child: Text(
                p.text('common.import'),
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

  Widget _widePlaylistEmpty(double rem) => Align(
    alignment: Alignment.topCenter,
    child: Padding(
      padding: EdgeInsets.symmetric(vertical: rem * 4),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(height: rem * 1.5),
          Text(
            p.text('music.songlist.emptyTitle'),
            style: TextStyle(
              fontSize: rem * 1.125,
              height: 22 / (rem * 1.125),
              fontWeight: FontWeight.w600,
            ),
          ),
          SizedBox(height: rem * .5),
          Text(
            p.text('music.songlist.emptyDesc'),
            textAlign: TextAlign.center,
            style: TextStyle(color: p.secondary),
          ),
          SizedBox(height: rem * 2),
          SizedBox(
            height: 32,
            child: MioButton(
              palette: p,
              label: p.text('music.songlist.createPlaylist'),
              radius: 6,
              background: p.brand,
              padding: const EdgeInsets.symmetric(horizontal: 12),
              onPressed: _playlistActionsEnabled ? _newPlaylist : null,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  MioIcon(
                    'nav-zengjia',
                    size: 14,
                    color: p.color('--td-text-color-anti'),
                  ),
                  const SizedBox(width: 6),
                  Text(
                    p.text('music.songlist.createPlaylist'),
                    style: TextStyle(
                      fontSize: 14,
                      color: p.color('--td-text-color-anti'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    ),
  );

  Widget _narrowPlaylistEmpty() => Align(
    alignment: Alignment.topCenter,
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 48),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            p.text('music.songlist.emptyTitle'),
            style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          Text(
            p.text('music.songlist.emptyDesc'),
            textAlign: TextAlign.center,
            style: TextStyle(color: p.secondary),
          ),
          const SizedBox(height: 24),
          MioButton(
            palette: p,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            label: p.text('music.songlist.createPlaylist'),
            background: p.brand,
            onPressed: _playlistActionsEnabled ? _newPlaylist : null,
            child: SizedBox(
              height: 44,
              child: Center(
                widthFactor: 1,
                child: Text(
                  p.text('music.songlist.createPlaylist'),
                  style: TextStyle(color: p.color('--td-text-color-anti')),
                ),
              ),
            ),
          ),
        ],
      ),
    ),
  );

  Future<void> _playlistImportMenu(Offset anchor) async {
    if (!_playlistActionsEnabled) return;
    final epoch = _epoch;
    final navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox) return;
    final route = RawDialogRoute<_PlaylistAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (_, _, _) => MioContextMenu<_PlaylistAction>(
        palette: p,
        anchor: overlay.globalToLocal(anchor),
        label: p.text('common.import'),
        entries: [
          MioMenuEntry(
            value: _PlaylistAction.file,
            label: p.text('music.songlist.fromLocalFile'),
            icon: 'PlaylistImport',
          ),
          MioMenuEntry(
            value: _PlaylistAction.network,
            label: p.text('music.songlist.fromNetwork'),
            icon: 'PlaylistImport',
          ),
          MioMenuEntry(
            value: _PlaylistAction.queue,
            label: p.text('music.songlist.fromPlayingList'),
            icon: 'nav-liebiao',
            enabled: widget.core.player != null,
          ),
        ],
      ),
    );
    _playlistRoute = route;
    final action = await navigator.push(route);
    await route.completed;
    if (identical(_playlistRoute, route)) _playlistRoute = null;
    if (!_live || epoch != _epoch || action == null) return;
    await _importPlaylistFile(
      fromNetwork: action == _PlaylistAction.network,
      fromQueue: action == _PlaylistAction.queue,
    );
  }

  Future<void> _playlistCardAction(
    api.PlaylistInfo row, {
    bool delete = false,
  }) async {
    if (!_playlistActionsEnabled) return;
    final epoch = _epoch;
    final navigator = Navigator.of(context);
    setState(() => _songActionsBusy = true);
    api.LibrarySession? session;
    try {
      session = await api.beginLibrary(id: row.id);
      if (!_live || epoch != _epoch) return;
      _cardSession = session;
      final page = await session.playlist(offset: 0, limit: 1);
      if (!_live || epoch != _epoch) return;
      if (page.playlist != row) throw StateError('catalog_changed');
      if (delete) {
        final route = RawDialogRoute<PlaylistDeleteResult>(
          barrierDismissible: false,
          barrierColor: const Color(0x00000000),
          pageBuilder: (_, _, _) => PlaylistDeleteDialog(
            session: session!,
            playlist: page.playlist,
            core: widget.core,
            palette: p,
          ),
        );
        _playlistRoute = route;
        await navigator.push(route);
        await route.completed;
        if (identical(_playlistRoute, route)) _playlistRoute = null;
      } else {
        await widget.core.command(() => session!.playAll());
      }
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(
          () => _songMessage = p.english
              ? 'Playlist changed or unavailable. Refresh and retry.'
              : '歌单已变化或无法读取，请刷新后重试。',
        );
      }
    } finally {
      if (identical(_cardSession, session)) _cardSession = null;
      session?.close();
      session?.dispose();
      if (mounted) setState(() => _songActionsBusy = false);
      if (_live && epoch == _epoch) unawaited(_load(reset: true));
    }
  }

  Future<void> _playlistRowMenu(api.PlaylistInfo row, Offset anchor) async {
    if (!_playlistActionsEnabled || _playlistRoute != null) return;
    final epoch = _epoch;
    final navigator = Navigator.of(context);
    final overlay = navigator.overlay?.context.findRenderObject();
    if (overlay is! RenderBox) return;
    final route = RawDialogRoute<_PlaylistRowAction>(
      barrierDismissible: true,
      barrierColor: const Color(0x00000000),
      barrierLabel: p.text('common.close'),
      pageBuilder: (_, _, _) => MioContextMenu<_PlaylistRowAction>(
        palette: p,
        anchor: overlay.globalToLocal(anchor),
        label: row.title,
        entries: [
          MioMenuEntry(
            value: _PlaylistRowAction.view,
            label: p.text('music.songlist.viewDetail'),
            icon: 'TD-ViewList',
          ),
          MioMenuEntry(
            value: _PlaylistRowAction.edit,
            label: p.text('music.songlist.editPlaylist'),
            icon: 'TD-Edit2',
          ),
          MioMenuEntry(
            value: _PlaylistRowAction.delete,
            label: p.text('music.songlist.deletePlaylist'),
            icon: 'TD-Delete',
            danger: true,
          ),
        ],
      ),
    );
    _playlistRoute = route;
    final action = await navigator.push(route);
    await route.completed;
    if (identical(_playlistRoute, route)) _playlistRoute = null;
    if (!_live || epoch != _epoch || !_playlistActionsEnabled) return;
    switch (action) {
      case _PlaylistRowAction.view:
        widget.onOpenPlaylist?.call(row.id);
      case _PlaylistRowAction.edit:
        setState(() {
          _editingPlaylistId = row.id;
          _editing = true;
        });
      case _PlaylistRowAction.delete:
        await _playlistCardAction(row, delete: true);
      case null:
        break;
    }
  }

  Widget _narrowPlaylistRow(api.PlaylistInfo row) {
    final open = _playlistActionsEnabled
        ? () => widget.onOpenPlaylist?.call(row.id)
        : null;
    Widget action(String icon, String label, VoidCallback? run) => SizedBox(
      width: 44,
      height: 44,
      child: MioButton(
        palette: p,
        label: label,
        padding: EdgeInsets.zero,
        onPressed: run,
        child: Center(child: MioIcon(icon, size: 21, color: p.secondary)),
      ),
    );
    return Container(
      key: ValueKey('playlist-row-${row.id}'),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: p.surface,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        children: [
          Expanded(
            child: MioButton(
              palette: p,
              label: row.title,
              padding: EdgeInsets.zero,
              onPressed: open,
              child: Row(
                children: [
                  MioSavedPlaylistCover(
                    id: row.id,
                    source: row.cover,
                    session: _librarySession,
                    size: 60,
                    radius: 8,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          row.title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 15,
                            height: 1.3,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        const SizedBox(height: 5),
                        Text(
                          '${row.source.toUpperCase()} · ${p.text('music.local.totalSongs').replaceAll('{count}', '${row.count}')}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 11, color: p.secondary),
                        ),
                        if (row.favorite) ...[
                          const SizedBox(height: 4),
                          Text(
                            p.text('music.songlist.myFavorite'),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 10, color: p.brand),
                          ),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(width: 4),
          action(
            'TD-PlayCircle',
            p.text('music.songlist.playPlaylist'),
            _playlistActionsEnabled && row.count > 0
                ? () => unawaited(_playlistCardAction(row))
                : null,
          ),
          Builder(
            builder: (buttonContext) => action(
              'TD-Ellipsis',
              p.english ? 'More actions' : '更多操作',
              _playlistActionsEnabled
                  ? () {
                      final box = buttonContext.findRenderObject();
                      if (box is RenderBox) {
                        unawaited(
                          _playlistRowMenu(
                            row,
                            box.localToGlobal(Offset(0, box.size.height)),
                          ),
                        );
                      }
                    }
                  : null,
            ),
          ),
        ],
      ),
    );
  }

  Widget _playlistCard(api.PlaylistInfo row, double width) => MioButton(
    palette: p,
    label: row.favorite
        ? '${row.title}, ${p.text('music.songlist.myFavorite')}'
        : row.title,
    radius: 12,
    background: p.surface,
    padding: EdgeInsets.zero,
    onPressed: () => widget.onOpenPlaylist?.call(row.id),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: MioSavedPlaylistCover(
            id: row.id,
            source: row.cover,
            session: _librarySession,
            size: width,
            radius: 10,
          ),
        ),
        Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Text(
                      row.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 14,
                        height: 1.2,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  if (row.favorite) ...[
                    const SizedBox(width: 6),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: width * .4,
                        minHeight: 20,
                      ),
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 4),
                        decoration: BoxDecoration(
                          border: Border.all(
                            color: p.color('--td-error-color-3'),
                          ),
                          color: p.color('--td-error-color-light'),
                          borderRadius: BorderRadius.circular(3),
                        ),
                        child: Text(
                          p.text('music.songlist.myFavorite'),
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 10,
                            height: 1.8,
                            color: p.color('--td-error-color'),
                          ),
                        ),
                      ),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 6),
              Text(
                '${row.count} ${p.english ? 'tracks' : '首歌曲'}',
                style: TextStyle(fontSize: 12, height: 1.2, color: p.secondary),
              ),
            ],
          ),
        ),
      ],
    ),
  );
  Widget _downloadRow(api.DownloadItem task) => DownloadTaskCard(
    key: ValueKey(task.id),
    task: task,
    palette: p,
    narrow: !_wide,
    active: _live,
    enabled:
        _live &&
        !_loading &&
        !_downloadBusy &&
        _error == null &&
        _downloadSnapshot != null,
    onAction: (action) => unawaited(_downloadCommand(task, action)),
    onOpen: () => unawaited(_openDownloaded(task)),
  );

  String _time(double seconds) {
    final s = seconds.isFinite ? seconds.clamp(0, double.infinity).floor() : 0;
    return '${(s ~/ 60).toString().padLeft(2, '0')}:${(s % 60).toString().padLeft(2, '0')}';
  }
}

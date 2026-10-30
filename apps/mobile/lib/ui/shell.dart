import 'device.dart';
import 'mobile_settings.dart';
import 'glass_surface.dart';
import 'visual_policy.dart';
import 'track_cover.dart';
import 'workspace_background.dart';
import 'mini_player_actions.dart';
import 'mini_player_volume.dart';
import 'source_selector.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:liquid_glass_easy/liquid_glass_easy.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'catalog.dart';
import 'resources.dart';
import 'search.dart';
import 'plugins.dart';
import 'visual_settings.dart';
import 'tv/tv_settings.dart';
import 'full_player.dart';
import 'queue_drawer.dart';
import 'recent.dart';
import 'playback_settings.dart';
import 'equalizer.dart';
import 'audio_effects.dart';
import 'lyric_settings.dart';
import 'storage_settings.dart';
import 'backup_restore.dart';
import 'releases.dart';
import 'recognition.dart';
import 'discovery.dart';
import 'singer.dart';
import 'tv/tv_top_bar.dart';

part 'tv/tv_shortcuts.dart';

class MioShell extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  const MioShell({super.key, required this.core, required this.palette});
  @override
  State<MioShell> createState() => _MioShellState();
}

enum _Page {
  discover,
  appearance,
  queue,
  playlists,
  local,
  downloads,
  savedPlaylist,
  search,
  plugins,
  recent,
  playbackSettings,
  equalizer,
  audioEffects,
  lyricSettings,
  storageSettings,
  about,
  visualSettings,
  settings,
  musicSources,
  recognition,
}

typedef _Route = ({_Page page, String? playlistId});

class _MioShellState extends State<MioShell> {
  /// Times of recent Up presses that moved no focus (TV "raise the player").
  final _blockedUps = <DateTime>[];
  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_tvRemoteKey);
  }

  _Page _page = _Page.discover;
  bool _fullPlayer = false;
  bool _queueOpen = false;
  bool _artistReturnPlayer = false;
  final _artists =
      <
        ({
          GlobalKey<SingerPageState> key,
          api.SingerSession session,
          String name,
          Completer<void> done,
        })
      >[];

  Future<void> _showSinger(api.SingerSession session, String name) {
    final done = Completer<void>();
    setState(() {
      _artistReturnPlayer = _artistReturnPlayer || _fullPlayer;
      _fullPlayer = false;
      _artists.add((
        key: GlobalKey<SingerPageState>(),
        session: session,
        name: name,
        done: done,
      ));
    });
    return done.future;
  }

  void _closeSinger() {
    if (_artists.isEmpty) return;
    setState(() {
      _artists.removeLast().done.complete();
      if (_artists.isEmpty) {
        _fullPlayer = _artistReturnPlayer;
        _artistReturnPlayer = false;
      }
    });
  }

  void _leaveSingers() {
    for (final artist in _artists) {
      artist.done.complete();
    }
    _artists.clear();
    _artistReturnPlayer = false;
  }

  bool _discoveryDetails = false, _searchDetails = false;
  FocusNode? _queueReturnFocus, _playerReturnFocus;
  final _recognitionFocus = FocusNode(debugLabel: 'Header recognition');
  final _backupFocus = FocusNode(debugLabel: 'Header backup');
  final _guideFocus = FocusNode(debugLabel: 'Discover guide');

  void _restoreModalFocus(FocusNode? node, _Route route) {
    if (node == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted &&
          _page == route.page &&
          _selectedPlaylist == route.playlistId &&
          !_recognitionOpen &&
          !_backupOpen &&
          !_fullPlayer &&
          !_queueOpen &&
          node.context != null &&
          node.canRequestFocus) {
        node.requestFocus();
      }
    });
  }

  final bool _recognitionOpen = false;
  bool _backupOpen = false;
  Future<void> _restoreBackup() async {
    if (_backupOpen || core.player == null) return;
    // The S3 form is typed on the phone, in the settings web page.
    if (MioDeviceScope.isTv(context)) {
      _go(_Page.settings);
      return;
    }
    final returnFocus = _backupFocus.hasFocus ? _backupFocus : null;
    final route = (page: _page, playlistId: _selectedPlaylist);
    final revision = core.player!.sourceRevision;
    setState(() => _backupOpen = true);
    try {
      await showS3Restore(
        context: context,
        palette: p,
        isCurrent: () =>
            mounted &&
            core.player != null &&
            core.player!.sourceRevision == revision,
      );
    } finally {
      if (mounted) {
        setState(() => _backupOpen = false);
        _restoreModalFocus(returnFocus, route);
      }
    }
  }

  final _queueKey = GlobalKey<QueueDrawerState>();
  final _miniActionsKey = GlobalKey();
  final _discoveryKey = GlobalKey<DiscoveryViewState>();
  final _searchKey = GlobalKey<SearchViewState>();
  final _history = <_Route>[(page: _Page.discover, playlistId: null)];
  String? _selectedPlaylist;
  int _historyIndex = 0;
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
  String _searchQuery = '';
  int _searchRequest = 0;
  void _submitSearch() {
    final query = _search.text.trim();
    if (query.isEmpty) return;
    setState(() {
      _searchQuery = query;
      _searchRequest++;
    });
    _go(_Page.search);
  }

  final _pageScopes = {
    for (final page in _Page.values)
      page: FocusScopeNode(
        // Retain child focus history without making the page rectangle a stop.
        skipTraversal: true,
        traversalEdgeBehavior: TraversalEdgeBehavior.parentScope,
        directionalTraversalEdgeBehavior: TraversalEdgeBehavior.parentScope,
      ),
  };
  final _focusBookmarks = <_Route, FocusNode>{};
  void _rememberPageFocus() {
    if (!MioDeviceScope.isTv(context)) return;
    final node = FocusManager.instance.primaryFocus;
    if (node != null && node.ancestors.contains(_pageScopes[_page])) {
      _focusBookmarks[(page: _page, playlistId: _selectedPlaylist)] = node;
    }
  }

  void _restorePageFocus() {
    if (!MioDeviceScope.isTv(context)) return;
    final route = (page: _page, playlistId: _selectedPlaylist);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          _page != route.page ||
          _selectedPlaylist != route.playlistId ||
          _fullPlayer ||
          _queueOpen ||
          _backupOpen ||
          _recognitionOpen) {
        return;
      }
      final node = _focusBookmarks[route];
      if (node?.context != null && node!.canRequestFocus) {
        node.requestFocus();
      } else {
        _focusPageScope(route, 6);
      }
    });
  }

  /// A page that loads its rows after it appears has nothing to focus yet, and
  /// focus would otherwise stay wherever it was (the player bar's seek slider
  /// seeks on left/right). Try again until the page offers a control.
  void _focusPageScope(_Route route, int attempts) {
    final scope = _pageScopes[route.page]!;
    scope.nextFocus();
    final focus = FocusManager.instance.primaryFocus;
    if ((focus != null && focus.ancestors.contains(scope)) || attempts <= 1) {
      return;
    }
    Future<void>.delayed(const Duration(milliseconds: 200), () {
      if (!mounted ||
          _page != route.page ||
          _selectedPlaylist != route.playlistId ||
          _fullPlayer ||
          _queueOpen ||
          _backupOpen ||
          _recognitionOpen) {
        return;
      }
      _focusPageScope(route, attempts - 1);
    });
  }

  final _scroll = ScrollController();
  MioPalette get p => widget.palette;
  MioPalette? _navigationPaletteSource, _playerNavigationPalette;
  MioPalette get _rightNavigationPalette {
    if (!_fullPlayer || p.dark) return p;
    if (!identical(_navigationPaletteSource, p)) {
      _navigationPaletteSource = p;
      _playerNavigationPalette = MioPalette(
        p.assets,
        dark: true,
        english: p.english,
        coverTheme: core.player?.coverTheme,
      );
    }
    return _playerNavigationPalette!;
  }

  CoreController get core => widget.core;
  void _restoreOverlayFocus(FocusNode? node, {required bool playerOpen}) {
    if (node == null || !MioDeviceScope.isTv(context)) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          _queueOpen ||
          _backupOpen ||
          _recognitionOpen ||
          _fullPlayer != playerOpen) {
        return;
      }
      if (node.context?.mounted == true && node.canRequestFocus) {
        node.requestFocus();
      }
    });
  }

  void _openQueue() {
    if (_queueOpen || core.player == null) return;
    _queueReturnFocus = MioDeviceScope.isTv(context)
        ? FocusManager.instance.primaryFocus
        : null;
    setState(() => _queueOpen = true);
  }

  void _closeQueue() {
    final node = _queueReturnFocus;
    _queueReturnFocus = null;
    setState(() => _queueOpen = false);
    _restoreOverlayFocus(node, playerOpen: _fullPlayer);
  }

  void _openPlayer() {
    if (_fullPlayer || core.player?.current == null) return;
    _playerReturnFocus = MioDeviceScope.isTv(context)
        ? FocusManager.instance.primaryFocus
        : null;
    setState(() => _fullPlayer = true);
  }

  void _closePlayer() {
    final node = _playerReturnFocus;
    _playerReturnFocus = null;
    setState(() => _fullPlayer = false);
    _restoreOverlayFocus(node, playerOpen: false);
  }

  void _openPlayerAppearance() {
    // Navigation hands focus to the destination page, not the old launcher.
    _playerReturnFocus = null;
    setState(() => _fullPlayer = false);
    if (_page == _Page.appearance) {
      _restorePageFocus();
    } else {
      _go(_Page.appearance);
    }
  }

  void _goFromRightNavigation(_Page page) {
    if (page == _Page.recognition && !_canRecognize) return;
    if (_fullPlayer || _queueOpen) {
      setState(() {
        _fullPlayer = false;
        _queueOpen = false;
        _playerReturnFocus = null;
        _queueReturnFocus = null;
      });
    }
    _go(page);
  }

  void _go(_Page page, {String? playlistId, bool replace = false}) {
    if (page == _Page.recognition && !_canRecognize) return;
    if (page == _Page.queue) {
      if (core.player == null) return;
      _openQueue();
      return;
    }
    if (_artists.isNotEmpty) setState(_leaveSingers);
    if (_page == page && _selectedPlaylist == playlistId) return;
    _rememberPageFocus();
    setState(() {
      if (replace) {
        // Switching the settings category keeps one history entry, like the
        // desktop settings page.
        _history[_historyIndex] = (page: page, playlistId: playlistId);
      } else {
        _history.removeRange(_historyIndex + 1, _history.length);
        _history.add((page: page, playlistId: playlistId));
        _historyIndex++;
      }
      _selectedPlaylist = playlistId;
      _page = page;
    });
    _restorePageFocus();
  }

  void _playlistDeleted(String id) {
    if (_page == _Page.savedPlaylist && _selectedPlaylist == id) {
      _go(_Page.playlists);
    }
    setState(() {
      bool deleted(_Route route) =>
          route.page == _Page.savedPlaylist && route.playlistId == id;
      final removed = _history.take(_historyIndex + 1).where(deleted).length;
      _history.removeWhere(deleted);
      _historyIndex -= removed;
    });
  }

  /// Header Back: close an open detail first, then step through history.
  void _headerBack() {
    if (_artists.isNotEmpty) {
      _artists.last.key.currentState?.handleBack();
      return;
    }
    final handled = switch (_page) {
      _Page.discover => _discoveryKey.currentState?.handleBack() ?? false,
      _Page.search => _searchKey.currentState?.handleBack() ?? false,
      _ => false,
    };
    if (!handled) _move(-1);
  }

  void _move(int offset) {
    final next = _historyIndex + offset;
    if (next < 0 || next >= _history.length) return;
    _rememberPageFocus();
    setState(() {
      _historyIndex = next;
      _page = _history[next].page;
      _selectedPlaylist = _history[next].playlistId;
    });
    _restorePageFocus();
  }

  void _command(Future<void> Function() action) =>
      unawaited(core.command(action));
  void _toggle() {
    if (core.player?.current == null || core.player?.status == 'loading') {
      return;
    }
    _command(() => api.setPaused(paused: core.player?.status == 'playing'));
  }

  @override
  void didUpdateWidget(MioShell old) {
    super.didUpdateWidget(old);
    if (core.player?.current == null) _fullPlayer = false;
    if (core.player == null) _queueOpen = false;
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_tvRemoteKey);
    _leaveSingers();
    _search.dispose();
    _searchFocus.dispose();
    _recognitionFocus.dispose();
    _backupFocus.dispose();
    _guideFocus.dispose();
    _scroll.dispose();
    for (final scope in _pageScopes.values) {
      scope.dispose();
    }
    super.dispose();
  }

  Widget _iconButton(
    String name,
    String label,
    VoidCallback? action, {
    double size = 36,
    double iconSize = 18,
    double radius = 4,
    Color? iconColor,
    Color? background,
    double disabledOpacity = .4,
    FocusNode? focusNode,
  }) => SizedBox(
    width: size,
    height: size,
    child: MioButton(
      palette: p,
      focusNode: focusNode,
      padding: EdgeInsets.zero,
      label: label,
      radius: radius,
      background: background,
      disabledOpacity: disabledOpacity,
      onPressed: action,
      child: Center(
        child: MioIcon(name, color: iconColor ?? p.secondary, size: iconSize),
      ),
    ),
  );
  @override
  Widget build(BuildContext context) => Shortcuts(
    shortcuts: const {
      SingleActivator(LogicalKeyboardKey.space): _PlayIntent(),
      SingleActivator(LogicalKeyboardKey.keyF, meta: true): _SearchIntent(),
      SingleActivator(LogicalKeyboardKey.keyF, control: true): _SearchIntent(),
    },
    child: Actions(
      actions: {
        _PlayIntent: _PlaybackAction(_toggle),
        _SearchIntent: CallbackAction<_SearchIntent>(
          onInvoke: (_) {
            _searchFocus.requestFocus();
            return null;
          },
        ),
      },
      child: Focus(
        // The shell is a key-event ancestor, not a full-screen traversal stop.
        // Its autofocus otherwise wins before the TV navigation button can focus.
        canRequestFocus: false,
        skipTraversal: true,
        child: PopScope(
          canPop:
              _artists.isEmpty &&
              !_queueOpen &&
              !_fullPlayer &&
              _historyIndex == 0,
          onPopInvokedWithResult: (didPop, _) {
            if (!didPop) {
              if (_queueOpen) {
                if (!(_queueKey.currentState?.handleBack() ?? false)) {
                  _closeQueue();
                }
              } else if (_fullPlayer) {
                _closePlayer();
              } else if (_artists.isNotEmpty) {
                _artists.last.key.currentState?.handleBack();
              } else {
                final handled = switch (_page) {
                  _Page.discover =>
                    _discoveryKey.currentState?.handleBack() ?? false,
                  _Page.search =>
                    _searchKey.currentState?.handleBack() ?? false,
                  _ => false,
                };
                if (!handled) _move(-1);
              }
            }
          },
          child: LayoutBuilder(
            builder: (context, box) {
              final navigationInset = MioDeviceScope.rightNavigationInset(
                context,
              );
              final width = box.maxWidth - navigationInset;
              final wide = MioDeviceScope.wide(context, width);
              // Wide settings replace the whole window like the desktop page:
              // title bar, category list and content, with no sidebar/player.
              // TV hands every settings page to the phone (a QR code), so it has
              // no category list and keeps its sidebar and player bar.
              final settingsRoot =
                  wide &&
                  _artists.isEmpty &&
                  _settingsPages.contains(_page) &&
                  !MioDeviceScope.isTv(context);
              final bottomInset = math.max(
                MediaQuery.paddingOf(context).bottom,
                MioDeviceScope.isTv(context) ? 16.0 : 0.0,
              );
              final topInset = wide
                  ? 0.0
                  : View.of(context).viewPadding.top /
                        View.of(context).devicePixelRatio;
              // Embedded singer pages own the top background even when opened
              // from settings. Extend their first gradient color into the inset.
              final topBackground = _artists.isNotEmpty
                  ? Color.alphaBlend(
                      p.brand.withValues(alpha: .12),
                      p.color('--search-bg'),
                    )
                  : _compactHeaderBackground;
              final viewportHeight = box.maxHeight - bottomInset;
              final rem = MioPalette.rem(width);
              final textScale = MediaQuery.textScalerOf(context);
              final playerTextHeight = wide
                  ? textScale.scale(14) * 1.5 +
                        4 +
                        textScale.scale(12) * (17 / 12)
                  : textScale.scale(13) * (18 / 13) +
                        2 +
                        textScale.scale(11) * (16 / 11);
              // The progress hit area overlays the top edge; retain room for scaled text.
              final playerHeight = math.max(
                (viewportHeight * .1).clamp(70.0, 80.0),
                playerTextHeight + 28,
              );
              final navigationHeight = math
                  .max(64.0, 51 + textScale.scale(10))
                  .ceilToDouble();
              final workspace = SingerHost(
                // Singer pages open inside the shell (like the desktop content
                // area), so the player bar stays under them on TV too.
                open: _showSinger,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: navigationInset > 0
                        ? null
                        : p.color('--td-bg-color-page'),
                    // TV paints one backdrop behind the content and the
                    // floating player instead of a docked bar's own strip.
                    gradient: MioDeviceScope.isTv(context)
                        ? mioWorkspaceBackground(
                            p,
                            wide: true,
                            songTheme: core.player?.coverTheme != null,
                            viewportHeight: viewportHeight,
                            areaHeight: viewportHeight,
                          )
                        : null,
                  ),
                  child: Stack(
                    clipBehavior:
                        !wide &&
                            (_fullPlayer ||
                                _artists.isNotEmpty ||
                                _settingsPages.contains(_page))
                        ? Clip.none
                        : Clip.hardEdge,
                    children: [
                      ExcludeFocus(
                        excluding:
                            _fullPlayer ||
                            _queueOpen ||
                            _backupOpen ||
                            _recognitionOpen,
                        child: Column(
                          children: [
                            Expanded(
                              child: Row(
                                children: [
                                  if (wide &&
                                      navigationInset == 0 &&
                                      !MioDeviceScope.isTv(context))
                                    settingsRoot
                                        ? const SizedBox.shrink()
                                        : _sidebar(
                                            rem,
                                            viewportHeight,
                                            viewportHeight - playerHeight,
                                          ),
                                  Expanded(
                                    child: Stack(
                                      fit: StackFit.expand,
                                      children: [
                                        Offstage(
                                          offstage: _artists.isNotEmpty,
                                          child: DecoratedBox(
                                            decoration: BoxDecoration(
                                              color: settingsRoot
                                                  ? p.color(
                                                      '--settings-main-bg',
                                                    )
                                                  : null,
                                              gradient:
                                                  settingsRoot ||
                                                      navigationInset > 0 ||
                                                      MioDeviceScope.isTv(
                                                        context,
                                                      )
                                                  ? null
                                                  : mioWorkspaceBackground(
                                                      p,
                                                      wide: wide,
                                                      songTheme:
                                                          core
                                                              .player
                                                              ?.coverTheme !=
                                                          null,
                                                      viewportHeight:
                                                          viewportHeight,
                                                      areaHeight:
                                                          viewportHeight -
                                                          playerHeight,
                                                    ),
                                            ),
                                            child: Column(
                                              children: [
                                                if (settingsRoot)
                                                  _settingsHeader(rem)
                                                else if (wide ||
                                                    !((_page ==
                                                                _Page
                                                                    .discover &&
                                                            _discoveryDetails) ||
                                                        (_page ==
                                                                _Page.search &&
                                                            _searchDetails)))
                                                  !wide &&
                                                          _settingsPages
                                                              .contains(_page)
                                                      ? _compactSettingsHeader(
                                                          width,
                                                        )
                                                      : MioDeviceScope.isTv(
                                                          context,
                                                        )
                                                      ? _tvTopBar()
                                                      : _header(
                                                          width,
                                                          wide,
                                                          rem,
                                                          rightNavigation:
                                                              navigationInset >
                                                              0,
                                                        ),
                                                if (core.error != null)
                                                  _error(),
                                                Expanded(
                                                  child: Row(
                                                    children: [
                                                      settingsRoot
                                                          ? _settingsNav(rem)
                                                          : const SizedBox.shrink(),
                                                      Expanded(
                                                        child: Padding(
                                                          padding:
                                                              EdgeInsets.all(
                                                                settingsRoot
                                                                    ? 4
                                                                    : 0,
                                                              ),
                                                          child: _content(
                                                            width,
                                                            wide,
                                                            rem,
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
                                        for (final artist in _artists)
                                          Offstage(
                                            offstage: artist != _artists.last,
                                            child: SingerPage(
                                              key: artist.key,
                                              session: artist.session,
                                              core: core,
                                              palette: p,
                                              fallbackName: artist.name,
                                              onClose: _closeSinger,
                                            ),
                                          ),
                                      ],
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            if (!_fullPlayer &&
                                !_queueOpen &&
                                !_backupOpen &&
                                !settingsRoot)
                              ReleaseNotice(
                                updates: core.updates,
                                palette: p,
                                onDetails: () => _go(_Page.about),
                              ),
                            if (settingsRoot)
                              SizedBox(height: bottomInset)
                            else if (MioDeviceScope.isTv(context))
                              // TV floats the player as a rounded card above
                              // the screen edge instead of a docked bar.
                              Padding(
                                // Bottom 24 keeps the card inside the TV's overscan.
                                padding: const EdgeInsets.fromLTRB(
                                  28,
                                  4,
                                  28,
                                  24,
                                ),
                                child: ClipRRect(
                                  borderRadius: BorderRadius.circular(18),
                                  child: _player(
                                    width,
                                    wide,
                                    playerHeight,
                                    bottomInset,
                                  ),
                                ),
                              )
                            else
                              _player(
                                width,
                                wide,
                                playerHeight,
                                bottomInset +
                                    (wide || navigationInset > 0
                                        ? 0
                                        : navigationHeight + 10),
                                blendTrailingEdge: navigationInset > 0,
                              ),
                          ],
                        ),
                      ),
                      if (!wide && navigationInset == 0)
                        if (_glassNavigation)
                          Positioned.fill(
                            child: _glassBottomNavigation(
                              width,
                              navigationHeight,
                              bottomInset,
                            ),
                          )
                        else
                          Positioned(
                            bottom: bottomInset + 10,
                            left: 0,
                            right: 0,
                            child: ExcludeFocus(
                              excluding:
                                  _fullPlayer ||
                                  _queueOpen ||
                                  _backupOpen ||
                                  _recognitionOpen,
                              child: Center(
                                child: _compactNavigation(
                                  width,
                                  navigationHeight,
                                ),
                              ),
                            ),
                          ),
                      if (!wide &&
                          (_artists.isNotEmpty ||
                              _settingsPages.contains(_page)) &&
                          topInset > 0)
                        Positioned(
                          top: -topInset,
                          left: 0,
                          right: 0,
                          height: topInset,
                          child: AnnotatedRegion<SystemUiOverlayStyle>(
                            value: SystemUiOverlayStyle(
                              statusBarColor: const Color(0x00000000),
                              statusBarIconBrightness:
                                  topBackground.computeLuminance() > .179
                                  ? Brightness.dark
                                  : Brightness.light,
                              statusBarBrightness:
                                  topBackground.computeLuminance() > .179
                                  ? Brightness.light
                                  : Brightness.dark,
                              systemStatusBarContrastEnforced: false,
                            ),
                            child: ColoredBox(color: topBackground),
                          ),
                        ),
                    ],
                  ),
                ),
              );
              final layout = Stack(
                clipBehavior: Clip.none,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Expanded(
                        child: MediaQuery.removePadding(
                          context: context,
                          removeRight: navigationInset > 0,
                          child: workspace,
                        ),
                      ),
                      if (navigationInset > 0) SizedBox(width: navigationInset),
                    ],
                  ),
                  if ((_fullPlayer || _artistReturnPlayer) &&
                      core.player?.current != null)
                    Positioned.fill(
                      key: const ValueKey('full-player-overlay'),
                      top: -topInset,
                      child: MediaQuery.removePadding(
                        context: context,
                        removeRight: navigationInset > 0,
                        child: Offstage(
                          offstage: !_fullPlayer,
                          child: ExcludeFocus(
                            excluding: _queueOpen,
                            child: TickerMode(
                              enabled:
                                  _fullPlayer &&
                                  !_queueOpen &&
                                  !_backupOpen &&
                                  !_recognitionOpen,
                              child: MioFocusPanel(
                                child: FullPlayer(
                                  core: core,
                                  state: core.player!,
                                  palette: p,
                                  onClose: _closePlayer,
                                  systemInsets: wide
                                      ? EdgeInsets.only(bottom: bottomInset)
                                      : EdgeInsets.only(
                                          top: topInset,
                                          bottom: bottomInset,
                                        ),
                                  trailingInset: navigationInset,
                                  onQueue: _openQueue,
                                  onAppearance: _openPlayerAppearance,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  if (_queueOpen && core.player != null)
                    Positioned.fill(
                      key: const ValueKey('queue-overlay'),
                      right: navigationInset,
                      child: MediaQuery.removePadding(
                        context: context,
                        removeRight: navigationInset > 0,
                        child: QueueDrawer(
                          key: _queueKey,
                          state: core.player!,
                          palette: p,
                          onClose: _closeQueue,
                        ),
                      ),
                    ),
                  if (navigationInset > 0)
                    Positioned(
                      top: 0,
                      right: 0,
                      bottom: 0,
                      child: _rightNavigation(
                        navigationInset,
                        navigationHeight,
                        bottomInset,
                      ),
                    ),
                ],
              );
              if (navigationInset == 0) return layout;
              return DecoratedBox(
                decoration: BoxDecoration(
                  color: p.color('--td-bg-color-page'),
                  gradient: mioWorkspaceBackground(
                    p,
                    wide: wide,
                    songTheme: core.player?.coverTheme != null,
                    viewportHeight: viewportHeight,
                    areaHeight: box.maxHeight,
                  ),
                ),
                child: layout,
              );
            },
          ),
        ),
      ),
    ),
  );
  Widget _sidebar(double rem, double viewportHeight, double areaHeight) =>
      Container(
        width: rem * 15,
        padding: EdgeInsets.all(rem),
        decoration: BoxDecoration(
          gradient: mioWorkspaceBackground(
            p,
            wide: true,
            songTheme: core.player?.coverTheme != null,
            viewportHeight: viewportHeight,
            areaHeight: areaHeight,
            sidebar: true,
          ),
          border: Border(right: BorderSide(color: p.border)),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                ClipRRect(
                  borderRadius: BorderRadius.circular(rem * .625),
                  child: Image.asset(
                    'assets/images/icon.png',
                    width: rem * 2,
                    height: rem * 2,
                  ),
                ),
                SizedBox(width: rem * .5),
                Text(
                  'Mio Music',
                  style: TextStyle(
                    fontSize: rem * 1.125,
                    fontWeight: FontWeight.w800,
                  ),
                ),
              ],
            ),
            SizedBox(height: rem * 1.5),
            for (var i = 0; i < 4; i++)
              Padding(
                padding: EdgeInsets.only(bottom: rem * .25),
                child: SizedBox(
                  height: rem * 2.4,
                  child: MioButton(
                    palette: p,
                    autofocus: i == 0 && MioDeviceScope.isTv(context),
                    label: p.text(_navKeys[i]),
                    radius: 8,
                    padding: EdgeInsets.symmetric(horizontal: rem),
                    background: _navActive(i)
                        ? p.color('--td-brand-color-4')
                        : null,
                    onPressed: () => _go(_navPages[i]),
                    child: Row(
                      children: [
                        MioIcon(
                          'nav-${_navIcons[i]}',
                          size: rem,
                          color: _navActive(i)
                              ? p.color('--td-text-color-anti')
                              : p.secondary,
                        ),
                        SizedBox(width: rem * .75),
                        Text(
                          p.text(_navKeys[i]),
                          style: TextStyle(
                            fontSize: 14,
                            color: _navActive(i)
                                ? p.color('--td-text-color-anti')
                                : p.secondary,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        ),
      );

  /// Directional search from the TV top bar skips the Discover guide button
  /// for the player bar controls, so Down goes there directly.
  KeyEventResult _tvTopBarKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent ||
        event.logicalKey != LogicalKeyboardKey.arrowDown ||
        _page != _Page.discover ||
        _guideFocus.context == null ||
        !_guideFocus.canRequestFocus) {
      return KeyEventResult.ignored;
    }
    _guideFocus.requestFocus();
    return KeyEventResult.handled;
  }

  Widget _tvTopBar() => Focus(
    canRequestFocus: false,
    skipTraversal: true,
    onKeyEvent: _tvTopBarKey,
    child: TvTopBar(
      palette: p,
      autofocusFirst: true,
      tabs: [
        for (var i = 0; i < _navPages.length; i++)
          (
            icon: _navIcons[i],
            label: p.text(_navKeys[i]),
            active: _navActive(i),
            onPressed: () => _go(_navPages[i]),
          ),
      ],
      actions: [
        _iconButton(
          'nav-xiangzuo',
          p.text('common.back'),
          _historyIndex > 0 ||
                  _discoveryDetails ||
                  _searchDetails ||
                  _artists.isNotEmpty
              ? _headerBack
              : null,
          size: 40,
          iconSize: 20,
          radius: 999,
        ),
        _iconButton(
          'nav-xiangyou',
          p.english ? 'Forward' : '前进',
          _historyIndex + 1 < _history.length ? () => _move(1) : null,
          size: 40,
          iconSize: 20,
          radius: 999,
        ),
        SizedBox(width: 136, child: _searchBox(true)),
        if (_recognitionAvailable)
          _iconButton(
            'TD-Microphone',
            p.text('music.recognize.startRecognize'),
            !_canRecognize ? null : () => _go(_Page.recognition),
            focusNode: _recognitionFocus,
            size: 40,
            iconSize: 20,
            radius: 999,
          ),
        _iconButton(
          'BackupCloud',
          p.text('backup.backupAndRestore'),
          core.player == null || _backupOpen
              ? null
              : () => unawaited(_restoreBackup()),
          focusNode: _backupFocus,
          size: 40,
          iconSize: 20,
          radius: 999,
        ),
        _iconButton(
          'nav-shezhi',
          p.text('common.titleBar.settings'),
          () => _go(_Page.appearance),
          size: 40,
          iconSize: 20,
          radius: 999,
        ),
      ],
    ),
  );

  /// Settings title bar of the wide layout: Back, title and the settings gear.
  Widget _settingsHeader(double rem) => Container(
    padding: EdgeInsets.all(rem * 1.5),
    color: p.color('--settings-header-bg'),
    child: Row(
      children: [
        _iconButton(
          'nav-xiangzuo',
          p.text('common.back'),
          _historyIndex > 0 ? () => _move(-1) : null,
          size: rem * 2.25,
          iconSize: rem * 1.25,
        ),
        SizedBox(width: rem * .75),
        Expanded(
          child: Text(
            p.text('settings.title'),
            style: TextStyle(
              fontSize: rem * .875,
              height: 1.2,
              fontWeight: FontWeight.w500,
              color: p.color('--td-text-color-primary'),
            ),
          ),
        ),
        _iconButton(
          'nav-shezhi',
          p.text('common.titleBar.settings'),
          () => _go(_Page.appearance, replace: true),
          size: rem * 2.25,
          iconSize: rem * 1.25,
        ),
      ],
    ),
  );

  /// The settings category currently shown: sub pages highlight their parent.
  _Page get _settingsCategory => switch (_page) {
    _Page.equalizer => _Page.audioEffects,
    _Page.lyricSettings || _Page.visualSettings => _Page.appearance,
    _Page.settings => _Page.appearance,
    _ => _page,
  };

  Widget _settingsNav(double rem) {
    final active = _settingsCategory;
    return Container(
      width: 280,
      color: p.color('--settings-sidebar-bg'),
      child: ListView(
        key: const PageStorageKey('wide-settings-nav'),
        padding: const EdgeInsets.fromLTRB(5, 0, 5, 5),
        children: [
          for (final item in _settingsItems)
            Padding(
              padding: const EdgeInsets.only(top: 5),
              child: _settingsNavItem(item, item.page == active, rem),
            ),
        ],
      ),
    );
  }

  Widget _settingsNavItem(
    ({_Page page, String label, String description, String icon}) item,
    bool selected,
    double rem,
  ) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: selected ? p.color('--settings-nav-active-bg') : null,
        borderRadius: BorderRadius.circular(5),
        border: Border(
          left: BorderSide(
            color: selected
                ? p.color('--settings-nav-active-border')
                : const Color(0x00000000),
            width: 3,
          ),
        ),
      ),
      child: MioButton(
        palette: p,
        label: p.text(item.label),
        radius: 5,
        insetFocusBorder: false,
        padding: EdgeInsets.symmetric(
          horizontal: rem * 1.5,
          vertical: rem * .875,
        ),
        onPressed: () => _go(item.page, replace: true),
        child: Row(
          children: [
            MioIcon(
              item.icon,
              size: 20,
              color: p.color(
                selected
                    ? '--settings-nav-icon-active'
                    : '--settings-nav-icon-color',
              ),
            ),
            SizedBox(width: rem * .75),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    p.text(item.label),
                    style: TextStyle(
                      fontSize: rem * .875,
                      height: 1.6,
                      fontWeight: FontWeight.w500,
                      color: p.color('--settings-nav-label-color'),
                    ),
                  ),
                  SizedBox(height: rem * .125),
                  Text(
                    p.text(item.description),
                    style: TextStyle(
                      fontSize: rem * .75,
                      height: 1.3,
                      color: p.color('--settings-nav-desc-color'),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _header(
    double width,
    bool wide,
    double rem, {
    bool rightNavigation = false,
  }) => Padding(
    padding: wide
        ? EdgeInsets.all(rem * 1.5)
        : EdgeInsets.symmetric(
            horizontal: MioPalette.gutter(width),
            vertical: (width * .035).clamp(14, 22),
          ),
    child: SizedBox(
      height: wide ? 88.2 - rem * 3 : 44,
      child: Row(
        children: [
          // Detail/settings pages keep an explicit exit on platforms without
          // a system Back button. Search uses the persistent navigation below.
          if (!wide &&
              _historyIndex > 0 &&
              _page != _Page.discover &&
              _page != _Page.search &&
              _page != _Page.playlists &&
              _page != _Page.local &&
              _page != _Page.downloads) ...[
            _iconButton(
              'nav-xiangzuo',
              p.text('common.back'),
              () => _move(-1),
              size: 44,
            ),
            const SizedBox(width: 8),
          ],
          if (wide) ...[
            _iconButton(
              'nav-xiangzuo',
              p.text('common.back'),
              _historyIndex > 0 ||
                      _discoveryDetails ||
                      _searchDetails ||
                      _artists.isNotEmpty
                  ? _headerBack
                  : null,
              size: 32,
              iconSize: rem,
              radius: 999,
            ),
            SizedBox(width: rem * .5),
            _iconButton(
              'nav-xiangyou',
              p.english ? 'Forward' : '前进',
              _historyIndex + 1 < _history.length ? () => _move(1) : null,
              size: 32,
              iconSize: rem,
              radius: 999,
            ),
            SizedBox(width: rem * .5),
          ],
          if (wide)
            SizedBox(width: 220, child: _searchBox(wide))
          else
            Expanded(child: _searchBox(wide)),
          if (wide) ...[
            if (!rightNavigation && _recognitionAvailable) ...[
              SizedBox(width: rem * .5),
              _iconButton(
                'TD-Microphone',
                p.text('music.recognize.startRecognize'),
                !_canRecognize ? null : () => _go(_Page.recognition),
                focusNode: _recognitionFocus,
                size: 32,
                iconSize: 16,
                radius: 999,
              ),
            ],
            const Spacer(),
          ] else if (!rightNavigation && _recognitionAvailable) ...[
            const SizedBox(width: 8),
            _iconButton(
              'TD-Microphone',
              p.text('music.recognize.startRecognize'),
              !_canRecognize ? null : () => _go(_Page.recognition),
              focusNode: _recognitionFocus,
              size: 36,
              iconSize: 18,
            ),
            const SizedBox(width: 6),
          ],
          if (rightNavigation) const SizedBox(width: 8),
          if (wide)
            _backupButton()
          else
            _iconButton(
              'BackupCloud',
              p.text('backup.backupAndRestore'),
              core.player == null || _backupOpen
                  ? null
                  : () => unawaited(_restoreBackup()),
              focusNode: _backupFocus,
              size: 36,
              iconSize: 18,
            ),
          if (!rightNavigation) ...[
            const SizedBox(width: 6),
            _iconButton(
              wide ? 'nav-shezhi' : 'nav-xiazai',
              p.text(
                wide ? 'common.titleBar.settings' : 'common.homeNav.download',
              ),
              () => _go(wide ? _Page.appearance : _Page.downloads),
              size: wide ? rem * 2.25 : 36,
              iconSize: wide ? rem : 18,
            ),
          ],
        ],
      ),
    ),
  );
  Widget _backupButton() => SizedBox(
    height: 30,
    child: MioButton(
      palette: p,
      label: p.text('backup.backupAndRestore'),
      focusNode: _backupFocus,
      radius: 999,
      insetFocusBorder: false,
      padding: const EdgeInsets.only(left: 8, right: 10),
      onPressed: core.player == null || _backupOpen
          ? null
          : () => unawaited(_restoreBackup()),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          MioIcon('BackupCloud', color: p.secondary, size: 15),
          const SizedBox(width: 5),
          Text(
            p.text('backup.backup'),
            style: TextStyle(
              color: p.secondary,
              fontSize: 12,
              height: 1,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
      ),
    ),
  );
  Widget _searchBox(bool wide) => Container(
    height: wide ? 32 : 44,
    padding: const EdgeInsets.only(left: 4, right: 4),
    decoration: BoxDecoration(
      color: p.surface,
      borderRadius: BorderRadius.circular(20),
    ),
    child: Row(
      children: [
        SourceSelector(
          player: core.player,
          palette: p,
          active:
              !_fullPlayer && !_queueOpen && !_backupOpen && !_recognitionOpen,
        ),
        Expanded(
          child: MioTvInputNavigation(
            focusNode: _searchFocus,
            controller: _search,
            radius: 16,
            // The topmost field has no directional candidate above it.
            upFallback: wide ? _recognitionFocus : null,
            child: Stack(
              alignment: Alignment.centerLeft,
              children: [
                ValueListenableBuilder(
                  valueListenable: _search,
                  builder: (context, value, _) => value.text.isEmpty
                      ? IgnorePointer(
                          child: Text(
                            p.text('common.searchMusicArtist'),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              color: p.color('--td-text-color-placeholder'),
                              fontSize: 14,
                            ),
                          ),
                        )
                      : const SizedBox.shrink(),
                ),
                EditableText(
                  controller: _search,
                  focusNode: _searchFocus,
                  style: DefaultTextStyle.of(context).style
                      .copyWith(fontSize: 14),
                  cursorColor: p.brand,
                  backgroundCursorColor: p.border,
                  textInputAction: TextInputAction.search,
                  onSubmitted: (_) => _submitSearch(),
                ),
              ],
            ),
          ),
        ),
        _iconButton(
          'Search',
          p.text('common.search'),
          _submitSearch,
          size: 28,
          iconSize: 16,
          radius: 999,
        ),
      ],
    ),
  );
  Widget _error() => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 24),
    child: Row(
      children: [
        Expanded(
          child: Text(
            core.error!,
            style: TextStyle(color: p.color('--td-error-color')),
          ),
        ),
        MioButton(
          palette: p,
          label: p.english ? 'Retry' : '重试',
          onPressed: () => unawaited(core.connect()),
          child: Text(p.english ? 'Retry' : '重试'),
        ),
      ],
    ),
  );
  bool _navActive(int index) => _navPageActive(_navPages[index]);
  bool _navPageActive(_Page page) =>
      _artists.isEmpty &&
      (_page == page ||
          (page == _Page.settings && _settingsPages.contains(_page)) ||
          (page == _Page.playlists &&
              (_page == _Page.savedPlaylist || _page == _Page.recent)));
  final Set<_Page> _visitedPages = {_Page.discover};
  Widget _retainedPage(_Page page, Widget child, {bool displayed = false}) {
    if (_page == page || displayed || core.player?.routePreload == true) {
      _visitedPages.add(page);
    }
    if (!_visitedPages.contains(page)) {
      return const SizedBox.shrink();
    }
    return ExcludeFocus(
      excluding:
          (_page != page && !displayed) ||
          _fullPlayer ||
          _queueOpen ||
          _backupOpen ||
          _recognitionOpen,
      child: TickerMode(
        enabled:
            (_page == page || displayed) &&
            !_fullPlayer &&
            !_queueOpen &&
            !_backupOpen &&
            !_recognitionOpen,
        child: FocusScope(node: _pageScopes[page], child: child),
      ),
    );
  }

  Widget _compactPage(_Page page, double width) => MioDeviceScope.isTv(context)
      // Remote-driven TV settings are a QR code to the phone-side web pages.
      ? TvSettingsQrPage(
          palette: p,
          ready:
              core.player != null &&
              // Wide screens show the Appearance page under the Settings route.
              (_page == page ||
                  (page == _Page.appearance && _page == _Page.settings)) &&
              !_fullPlayer &&
              !_queueOpen &&
              !_backupOpen &&
              !_recognitionOpen,
        )
      : _settingsPage(page, width);
  Widget _settingsPage(_Page page, double width) => MobileSettingsPage(
    key: ValueKey(page),
    core: core,
    palette: p,
    viewportWidth: width,
    active:
        _page == page &&
        !_fullPlayer &&
        !_queueOpen &&
        !_backupOpen &&
        !_recognitionOpen,
    category: switch (page) {
      _Page.appearance => CompactSettingsCategory.appearance,
      _Page.playbackSettings => CompactSettingsCategory.playback,
      _Page.audioEffects => CompactSettingsCategory.effects,
      _Page.plugins => CompactSettingsCategory.plugins,
      _Page.musicSources => CompactSettingsCategory.music,
      _Page.storageSettings => CompactSettingsCategory.storage,
      _Page.about => CompactSettingsCategory.about,
      _Page.equalizer => CompactSettingsCategory.equalizer,
      _Page.lyricSettings => CompactSettingsCategory.lyrics,
      _ => CompactSettingsCategory.visual,
    },
    onPlugins: () => _go(_Page.plugins),
    onLocal: () => _go(_Page.local),
    onBackup: () => unawaited(_restoreBackup()),
  );
  Widget _content(double width, bool wide, double rem) => IndexedStack(
    index: wide && _page == _Page.settings
        ? _Page.appearance.index
        : _page.index,
    children: [
      _retainedPage(
        _Page.discover,
        DiscoveryView(
          key: _discoveryKey,
          onDetailsChanged: (visible) {
            if (mounted && _discoveryDetails != visible) {
              setState(() => _discoveryDetails = visible);
            }
          },
          core: core,
          palette: p,
          // The queue and full player are overlays: closing the page's session
          // behind them would drop an open playlist detail.
          active: _page == _Page.discover && !_backupOpen,
          ready: core.player != null,
          sourceRevision: core.player?.sourceRevision ?? 0,
          selectionRevision: core.player?.sourceSelectionRevision ?? 0,
          selectedSource: core.player?.selectedSource ?? '',
          viewportWidth: width,
          guide: _guide(width, wide, rem),
        ),
      ),
      _retainedPage(
        _Page.appearance,
        _compactPage(_Page.appearance, width),
        displayed: wide && _page == _Page.settings,
      ),
      const SizedBox.shrink(), // _Page.queue is now an overlay; preserve route indexes.
      for (final entry in [
        (_Page.playlists, CatalogKind.playlists),
        (_Page.local, CatalogKind.local),
        (_Page.downloads, CatalogKind.downloads),
        (_Page.savedPlaylist, CatalogKind.savedPlaylist),
      ])
        _retainedPage(
          entry.$1,
          CatalogView(
            key: ValueKey(entry.$1),
            kind: entry.$2,
            core: core,
            palette: p,
            active: _page == entry.$1 && !_backupOpen,
            ready: core.player != null,
            viewportWidth: width,
            playlistId: _selectedPlaylist,
            onOpenPlaylist: (id) => _go(_Page.savedPlaylist, playlistId: id),
            onPlaylistDeleted: _playlistDeleted,
            onRecent: () => _go(_Page.recent),
          ),
        ),
      _retainedPage(
        _Page.search,
        SearchView(
          key: _searchKey,
          onDetailsChanged: (visible) {
            if (mounted && _searchDetails != visible) {
              setState(() => _searchDetails = visible);
            }
          },
          core: core,
          palette: p,
          viewportWidth: width,
          query: _searchQuery,
          request: _searchRequest,
          active: _page == _Page.search && !_backupOpen,
          ready: core.player != null,
          sourceRevision: core.player?.sourceRevision ?? 0,
        ),
      ),
      _retainedPage(
        _Page.plugins,
        (!wide || _cardSettings)
            ? _compactPage(_Page.plugins, width)
            : PluginsView(
                core: core,
                palette: p,
                active:
                    _page == _Page.plugins &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen,
                ready: core.player != null,
                sourceRevision: core.player?.sourceRevision ?? 0,
              ),
      ),
      _retainedPage(
        _Page.recent,
        RecentView(
          core: core,
          palette: p,
          state: core.player,
          viewportWidth: width,
          active:
              _page == _Page.recent &&
              !_fullPlayer &&
              !_queueOpen &&
              !_backupOpen,
        ),
      ),
      _retainedPage(
        _Page.playbackSettings,
        (!wide || _cardSettings)
            ? _compactPage(_Page.playbackSettings, width)
            : PlaybackSettingsView(
                core: core,
                palette: p,
                viewportWidth: width,
                ready: core.player != null,
                active:
                    _page == _Page.playbackSettings &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen,
              ),
      ),
      _retainedPage(
        _Page.equalizer,
        (!wide || _cardSettings)
            ? _compactPage(_Page.equalizer, width)
            : EqualizerView(
                core: core,
                palette: p,
                viewportWidth: width,
                ready: core.player != null,
                active:
                    _page == _Page.equalizer &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen &&
                    !_recognitionOpen,
              ),
      ),
      _retainedPage(
        _Page.audioEffects,
        (!wide || _cardSettings)
            ? _compactPage(_Page.audioEffects, width)
            : AudioEffectsView(
                onEqualizer: () => _go(_Page.equalizer),
                core: core,
                palette: p,
                viewportWidth: width,
                ready: core.player != null,
                active:
                    _page == _Page.audioEffects &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen,
              ),
      ),
      _retainedPage(
        _Page.lyricSettings,
        (!wide || _cardSettings)
            ? _compactPage(_Page.lyricSettings, width)
            : LyricSettingsView(
                core: core,
                palette: p,
                viewportWidth: width,
                ready: core.player != null,
                active:
                    _page == _Page.lyricSettings &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen,
              ),
      ),
      _retainedPage(
        _Page.storageSettings,
        (!wide || _cardSettings)
            ? _compactPage(_Page.storageSettings, width)
            : StorageSettingsView(
                core: core,
                palette: p,
                viewportWidth: width,
                ready: core.player != null,
                active:
                    _page == _Page.storageSettings &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen,
              ),
      ),
      _retainedPage(
        _Page.about,
        (!wide || _cardSettings)
            ? _compactPage(_Page.about, width)
            : ReleaseSettingsView(updates: core.updates, palette: p),
      ),
      _retainedPage(
        _Page.visualSettings,
        (!wide || _cardSettings)
            ? _compactPage(_Page.visualSettings, width)
            : VisualSettingsPage(
                palette: p,
                ready: core.player != null,
                active:
                    _page == _Page.visualSettings &&
                    !_fullPlayer &&
                    !_queueOpen &&
                    !_backupOpen &&
                    !_recognitionOpen,
              ),
      ),
      _retainedPage(
        _Page.settings,
        wide ? const SizedBox.shrink() : _settingsList(width),
      ),
      _retainedPage(
        _Page.musicSources,
        _compactPage(_Page.musicSources, width),
      ),
      if (_page == _Page.recognition &&
          !_fullPlayer &&
          !_queueOpen &&
          !_backupOpen &&
          _artists.isEmpty)
        RecognitionPage(
          core: core,
          palette: p,
          onSearch: (query) {
            _search.text = query;
            _submitSearch();
          },
        )
      else
        const SizedBox.shrink(),
    ],
  );

  Widget _guide(double width, bool wide, double rem) {
    final gap = wide ? 24.0 : (width * .035).clamp(12.0, 16.0);
    final card = _guideCard(wide, rem);
    final panel = Container(
      constraints: const BoxConstraints(maxWidth: 880),
      child: CustomPaint(
        painter: _GuideDecoration(p, wide ? 20 : 18),
        child: Padding(
          padding: EdgeInsets.all(wide ? 40 : (width * .05).clamp(16, 24)),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              ClipRRect(
                borderRadius: BorderRadius.circular(wide ? 22 : 18),
                child: Image.asset(
                  'assets/images/icon.png',
                  width: wide ? 72 : 60,
                  height: wide ? 72 : 60,
                ),
              ),
              SizedBox(height: gap),
              SizedBox(
                width: double.infinity,
                child: Column(
                  children: [
                    Text(
                      p.text('music.find.configGuide'),
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: rem * (wide ? 1.5 : 1.2),
                        fontWeight: FontWeight.w600,
                        height: 22 / (rem * (wide ? 1.5 : 1.2)),
                      ),
                    ),
                    const SizedBox(height: 10),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 580),
                      child: Text(
                        p.text('music.find.guideDescription'),
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          color: p.secondary,
                          fontSize: wide ? 14 : rem * .9,
                          height: (wide ? 23 : 24) / (wide ? 14 : rem * .9),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              SizedBox(height: gap),
              ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: wide ? 440 : double.infinity,
                ),
                child: card,
              ),
            ],
          ),
        ),
      ),
    );
    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        key: const PageStorageKey('discover'),
        controller: _scroll,
        padding: wide
            ? EdgeInsets.symmetric(horizontal: rem * 2, vertical: 24)
            : EdgeInsets.fromLTRB(
                MioPalette.gutter(width),
                (width * .035).clamp(14, 22) + 24,
                MioPalette.gutter(width),
                24,
              ),
        child: ConstrainedBox(
          constraints: BoxConstraints(
            minHeight: wide ? math.max(0, constraints.maxHeight - 48) : 0,
          ),
          child: Center(child: panel),
        ),
      ),
    );
  }

  Widget _guideCard(bool wide, double rem) {
    return Container(
      width: double.infinity,
      constraints: BoxConstraints(minHeight: wide ? 168 : 0),
      padding: EdgeInsets.all(wide ? 22 : 14),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: Color.lerp(p.border, p.brand, .26)!),
        color: Color.lerp(p.surface, p.brand, .08),
      ),
      child: Column(
        crossAxisAlignment: wide
            ? CrossAxisAlignment.start
            : CrossAxisAlignment.center,
        children: [
          Text(
            p.text('music.find.installListPlugin'),
            textAlign: wide ? TextAlign.start : TextAlign.center,
            style: TextStyle(
              fontWeight: FontWeight.w600,
              fontSize: rem * (wide ? 1 : .95),
              height: 1.4,
            ),
          ),
          const SizedBox(height: 8),
          Text(
            p.text('music.find.installListPluginDesc'),
            textAlign: wide ? TextAlign.start : TextAlign.center,
            style: TextStyle(
              color: p.secondary,
              fontSize: wide ? 14 : rem * .82,
              height: 1.6,
            ),
          ),
          SizedBox(height: wide ? 20 : 12),
          Align(
            alignment: Alignment.center,
            child: MioButton(
              palette: p,
              focusNode: _guideFocus,
              label: p.text('music.find.goToPluginManage'),
              background: p.brand,
              radius: 3,
              onPressed: core.player == null ? null : () => _go(_Page.plugins),
              child: Text(
                p.text('music.find.goToPluginManage'),
                style: TextStyle(color: p.color('--td-text-color-anti')),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Recognition is offered only while an installed, enabled plugin can identify songs.
  // A TV remote has no microphone to listen with, so it never offers this.
  bool get _recognitionAvailable =>
      !MioDeviceScope.isTv(context) && api.recognitionAvailable();

  bool get _canRecognize =>
      _recognitionAvailable && core.player != null && !_recognitionOpen;

  List<({String icon, String label, _Page page})> get _rightNavigationItems => [
    for (final item in _rightNavItems)
      if (item.page != _Page.recognition || _recognitionAvailable) item,
  ];

  // The live lens needs the foreground, a free screen and a capable device;
  // otherwise the opaque bar below keeps working (TV focus, reduced mode,
  // full-screen overlays).
  bool get _glassNavigation =>
      !_fullPlayer && !_queueOpen && _glassRightNavigationEnabled;
  bool get _glassRightNavigationEnabled =>
      !_backupOpen &&
      !_recognitionOpen &&
      !MioDeviceScope.isTv(context) &&
      !MioVisualPolicy.of(context).reduced &&
      !MediaQuery.disableAnimationsOf(context);

  Widget _glassBottomNavigation(
    double width,
    double height,
    double bottomInset,
  ) {
    final items = _compactNavItems;
    final selected = items.indexWhere((item) => _navPageActive(item.page));
    final double barWidth = math.min(
      width - 2 * MioPalette.gutter(width),
      520.0,
    );
    final bottomMargin =
        10 + bottomInset - MediaQuery.paddingOf(context).bottom;
    final bar = LiquidGlassTabBar(
      items: [
        for (final item in items)
          LiquidGlassTabBarItem(
            label: p.text(item.label),
            iconBuilder: (context, glyph) =>
                MioIcon(item.icon, size: glyph.size, color: glyph.color),
          ),
      ],
      // No tab is highlighted on pages outside the four (e.g. singer pages);
      // the bar still needs a valid index, so it rests on the first one.
      selectedIndex: selected < 0 ? 0 : selected,
      onChanged: (i) => _go(items[i].page),
      width: barWidth,
      height: height,
      margin: EdgeInsets.only(bottom: bottomMargin),
      itemStyle: LiquidGlassTabItemStyle(
        selectedColor: p.brand,
        unselectedColor: p.secondary,
        iconSize: 21,
        labelFontSize: 10,
        iconLabelGap: 3,
        selectedFontWeight: FontWeight.bold,
        unselectedFontWeight: FontWeight.w500,
      ),
      pillStyle: LiquidGlassTabPillStyle(
        color: p.brand.withValues(alpha: p.dark ? .18 : .12),
      ),
      style: LiquidGlassTabBar.defaultStyle.copyWith(
        color: p.surface.withValues(alpha: p.dark ? .5 : .4),
        shadow: const LiquidGlassShadow(blur: 18, opacity: .22),
      ),
    );
    // The bar only reports a change of selection. Off the four tabs (downloads,
    // singer pages) it rests on the first tab, so tapping that tab would never
    // fire; this transparent target over the first cell navigates there.
    if (selected >= 0) return bar;
    return Stack(
      fit: StackFit.expand,
      children: [
        bar,
        Align(
          alignment: Alignment.bottomCenter,
          child: Padding(
            padding: EdgeInsets.only(bottom: bottomMargin),
            child: SizedBox(
              width: barWidth,
              height: height,
              child: Align(
                alignment: Alignment.centerLeft,
                child: GestureDetector(
                  behavior: HitTestBehavior.translucent,
                  onTap: () => _go(items.first.page),
                  child: SizedBox(
                    width: barWidth / items.length,
                    height: height,
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _rightNavigation(
    double width,
    double itemHeight,
    double bottomInset,
  ) => SizedBox(
    width: width,
    child: Padding(
      padding: EdgeInsets.only(top: width * 2 + 12, bottom: bottomInset + 12),
      child: Align(
        alignment: Alignment.bottomCenter,
        child: SingleChildScrollView(
          reverse: true,
          clipBehavior: Clip.none,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (_glassRightNavigationEnabled &&
                  (!_recognitionAvailable || _canRecognize))
                _glassRightNavigation(width, itemHeight)
              else
                Padding(
                  padding: const EdgeInsets.fromLTRB(8, 12, 8, 12),
                  child: _compactNavigation(
                    width - 16,
                    itemHeight,
                    direction: Axis.vertical,
                  ),
                ),
              const SizedBox(height: 4),
              _rightSettingsNavigation(width - 16),
            ],
          ),
        ),
      ),
    ),
  );

  Widget _glassRightNavigation(double width, double itemHeight) {
    final p = _rightNavigationPalette;
    final items = _rightNavigationItems;
    final selected = _fullPlayer
        ? -1
        : items.indexWhere((item) => _navPageActive(item.page));
    final length = itemHeight * items.length + 12;
    final glyphSize = math.max(
      40.0,
      24 + MediaQuery.textScalerOf(context).scale(10),
    );
    final bar = MediaQuery.removePadding(
      context: context,
      removeLeft: true,
      removeTop: true,
      removeRight: true,
      removeBottom: true,
      child: RotatedBox(
        quarterTurns: 1,
        child: LiquidGlassTabBar(
          items: [
            for (final item in items)
              LiquidGlassTabBarItem(
                iconBuilder: (context, glyph) => RotatedBox(
                  quarterTurns: 3,
                  child: SizedBox.square(
                    dimension: glyphSize,
                    child: _compactNavGlyph(
                      item.icon,
                      item.label,
                      color: glyph.color,
                      selected: glyph.selected,
                    ),
                  ),
                ),
              ),
          ],
          selectedIndex: selected < 0 ? 0 : selected,
          onChanged: (index) => _goFromRightNavigation(items[index].page),
          width: length,
          height: width - 16,
          margin: EdgeInsets.zero,
          alignment: Alignment.center,
          itemStyle: LiquidGlassTabItemStyle(
            selectedColor: selected < 0 ? p.secondary : p.brand,
            unselectedColor: p.secondary,
            iconSize: glyphSize,
            underGlassIconSize: glyphSize * 1.08,
          ),
          pillStyle: LiquidGlassTabPillStyle(
            show: selected >= 0,
            animated: true,
            color: p.secondary.withValues(alpha: p.dark ? .12 : .09),
            refractionIndex: 1.06,
            magnification: 1.03,
          ),
          style: LiquidGlassTabBar.defaultStyle.copyWith(
            color: p.surface.withValues(alpha: p.dark ? .5 : .4),
            shadow: const LiquidGlassShadow(blur: 9, opacity: .13),
          ),
        ),
      ),
    );
    return SizedBox(
      width: width,
      height: length + 24,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned.fill(child: ExcludeSemantics(child: bar)),
          Positioned(
            top: 18,
            bottom: 18,
            left: 14,
            right: 14,
            child: Column(
              children: [
                for (var index = 0; index < items.length; index++)
                  Expanded(
                    child: Semantics(
                      button: true,
                      selected: selected == index,
                      label: p.text(items[index].label),
                      onTap: () => _goFromRightNavigation(items[index].page),
                      child: selected < 0 && index == 0
                          ? GestureDetector(
                              behavior: HitTestBehavior.translucent,
                              onTap: () =>
                                  _goFromRightNavigation(items.first.page),
                              child: const SizedBox.expand(),
                            )
                          : const SizedBox.expand(),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _rightSettingsNavigation(double size) {
    final p = _rightNavigationPalette;
    final item = _compactSettingsNavItem;
    final selected = !_fullPlayer && _navPageActive(item.page);
    final color = selected ? p.brand : p.secondary;
    void open() => _goFromRightNavigation(item.page);
    final glyph = _compactNavGlyph(
      item.icon,
      item.label,
      color: color,
      selected: selected,
    );
    return SizedBox.square(
      dimension: size,
      child: Semantics(
        button: true,
        selected: selected,
        label: p.text(item.label),
        onTap: open,
        excludeSemantics: true,
        child: _glassRightNavigationEnabled
            ? LiquidGlassTabBarAction(
                size: size,
                foregroundColor: color,
                onTap: open,
                style: LiquidGlassTabBarAction.defaultStyle.copyWith(
                  color: p.surface.withValues(alpha: p.dark ? .5 : .4),
                  blur: 6,
                  shadow: const LiquidGlassShadow(blur: 9, opacity: .13),
                ),
                child: glyph,
              )
            : MioGlassSurface(
                palette: p,
                radius: size / 2,
                enabled: false,
                tint: p.surface.withValues(alpha: p.dark ? .9 : 1),
                child: MioButton(
                  palette: p,
                  label: p.text(item.label),
                  radius: size / 2,
                  padding: EdgeInsets.zero,
                  onPressed: open,
                  child: Center(child: glyph),
                ),
              ),
      ),
    );
  }

  Widget _compactNavGlyph(
    String icon,
    String label, {
    required Color color,
    required bool selected,
  }) => Column(
    mainAxisSize: MainAxisSize.min,
    mainAxisAlignment: MainAxisAlignment.center,
    children: [
      MioIcon(icon, size: 21, color: color),
      const SizedBox(height: 3),
      Text(
        p.text(label),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: 10,
          height: 1,
          fontWeight: selected ? FontWeight.bold : FontWeight.w500,
          color: color,
        ),
      ),
    ],
  );

  Widget _compactNavigation(
    double width,
    double height, {
    Axis direction = Axis.horizontal,
  }) {
    final vertical = direction == Axis.vertical;
    final p = vertical ? _rightNavigationPalette : this.p;
    final items = vertical ? _rightNavigationItems : _compactNavItems;
    return Container(
      width: vertical
          ? width
          : math.min(width - 2 * MioPalette.gutter(width), 520),
      height: vertical ? height * items.length + 14 : height,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(32),
        boxShadow: const [
          BoxShadow(
            offset: Offset(0, 14),
            color: Color(0x38000000),
            blurRadius: 28,
          ),
        ],
      ),
      child: MioGlassSurface(
        palette: p,
        radius: 32,
        tint: p.surface.withValues(alpha: p.dark ? .9 : 1),
        enabled:
            (vertical || (!_fullPlayer && !_queueOpen)) &&
            !_backupOpen &&
            !_recognitionOpen,
        child: Stack(
          fit: StackFit.expand,
          children: [
            Padding(
              padding: EdgeInsets.symmetric(
                horizontal: vertical ? 6 : 8,
                vertical: 7,
              ),
              child: Flex(
                direction: direction,
                crossAxisAlignment: vertical
                    ? CrossAxisAlignment.stretch
                    : CrossAxisAlignment.center,
                children: [
                  for (final item in items)
                    Expanded(
                      child: Semantics(
                        selected: _navPageActive(item.page),
                        child: MioButton(
                          palette: p,
                          label: p.text(item.label),
                          focusNode: item.page == _Page.recognition
                              ? _recognitionFocus
                              : null,
                          radius: vertical ? 24 : 999,
                          padding: const EdgeInsets.fromLTRB(0, 5, 0, 4),
                          background: _navPageActive(item.page)
                              ? p.brand.withValues(alpha: p.dark ? .14 : .1)
                              : null,
                          onPressed:
                              item.page == _Page.recognition && !_canRecognize
                              ? null
                              : () => vertical
                                    ? _goFromRightNavigation(item.page)
                                    : _go(item.page),
                          child: _compactNavGlyph(
                            item.icon,
                            item.label,
                            color: _navPageActive(item.page)
                                ? p.brand
                                : p.secondary,
                            selected: _navPageActive(item.page),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Color get _compactHeaderBackground => mioCompactSettingsHeaderBackground(
    p,
    songTheme: core.player?.coverTheme != null,
  );

  Widget _compactSettingsHeader(double width) {
    final title =
        _settingsItems.where((item) => item.page == _page).firstOrNull?.label ??
        switch (_page) {
          _Page.equalizer => 'settings.equalizer.title',
          _Page.lyricSettings => 'settings.lyricFont.title',
          _Page.visualSettings => 'visuals.title',
          _ => 'settings.title',
        };
    return ColoredBox(
      color: _compactHeaderBackground,
      child: Padding(
        padding: EdgeInsets.symmetric(
          horizontal: MioPalette.gutter(width),
          vertical: 10,
        ),
        child: Row(
          children: [
            _iconButton(
              'nav-xiangzuo',
              p.text('common.back'),
              _historyIndex > 0 ? () => _move(-1) : null,
              size: 36,
              iconSize: 18,
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                p.text(title),
                style: const TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _settingsList(double width) => ListView(
    key: const PageStorageKey('settings-category-list'),
    padding: EdgeInsets.all(MioPalette.gutter(width)),
    children: [
      for (final item in _settingsItems)
        Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: p.color('--mobile-glass-bg'),
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: p.color('--mobile-glass-border')),
            ),
            child: MioButton(
              palette: p,
              label: p.text(item.label),
              radius: 16,
              insetFocusBorder: false,
              disabledOpacity: 1,
              padding: const EdgeInsets.all(14),
              onPressed: () => _go(item.page),
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 48),
                child: Row(
                  children: [
                    MioIcon(item.icon, color: p.secondary, size: 22),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            p.text(item.label),
                            style: const TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.w500,
                            ),
                          ),
                          if (item.description.isNotEmpty) ...[
                            const SizedBox(height: 4),
                            Text(
                              item.page == _Page.playbackSettings
                                  ? (p.english
                                        ? 'Playback queue and display settings'
                                        : '播放列表和播放显示设置')
                                  : p.text(item.description),
                              style: TextStyle(
                                fontSize: 12,
                                height: 1.4,
                                color: p.secondary,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    const SizedBox(width: 8),
                    MioIcon('nav-xiangyou', color: p.secondary, size: 16),
                  ],
                ),
              ),
            ),
          ),
        ),
    ],
  );

  Widget _player(
    double width,
    bool wide,
    double height,
    double bottomExtension, {
    bool blendTrailingEdge = false,
  }) {
    final state = core.player;
    final active = state?.current != null;
    final usable = active && state?.status != 'loading';
    final playing = state?.status == 'playing';
    final visible =
        !_fullPlayer && !_queueOpen && !_backupOpen && !_recognitionOpen;
    final textColor = p.color('--player-text-idle');
    final actions = MiniPlayerActions(
      key: _miniActionsKey,
      core: core,
      snapshot: state,
      palette: p,
      active: visible,
      wide: wide,
    );
    Widget transportButton(
      String icon,
      String label,
      VoidCallback? action, {
      bool play = false,
    }) => _iconButton(
      icon,
      label,
      action,
      size: wide ? (play ? 42 : 38) : 44,
      iconSize: wide ? 28 : (play ? 22 : 20),
      radius: play ? 100 : 4,
      iconColor: p.color(
        play ? '--player-text-hover-idle' : '--player-text-idle',
      ),
      background: play
          ? (state?.coverTheme != null
                ? const Color(0x33ffffff)
                : p.color('--player-btn-bg-idle'))
          : null,
      disabledOpacity: 1,
    );
    final controls = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        transportButton(
          'nav-shangyishou',
          p.text('common.previous'),
          active ? () => _command(() => api.step(forward: false)) : null,
        ),
        if (wide) const SizedBox(width: 16),
        transportButton(
          playing ? 'nav-zanting' : 'nav-bofang',
          p.text(playing ? 'common.pause' : 'common.play'),
          usable ? _toggle : null,
          play: true,
        ),
        if (wide) const SizedBox(width: 16),
        transportButton(
          'nav-xiayishou',
          p.text('common.next'),
          active ? () => _command(() => api.step(forward: true)) : null,
        ),
      ],
    );
    final information = Row(
      children: [
        if (active)
          Padding(
            padding: EdgeInsets.only(right: wide ? 12 : 10),
            child: SizedBox(
              width: wide ? 50 : 42,
              height: wide ? 50 : 42,
              child: MioButton(
                palette: p,
                label: p.english ? 'Open player artwork' : '打开播放器封面',
                padding: EdgeInsets.zero,
                insetFocusBorder: false,
                onPressed: _openPlayer,
                child: MioTrackCover.current(
                  snapshot: state!,
                  size: wide ? 50 : 42,
                  radius: wide ? 4 : 6,
                  active: visible,
                ),
              ),
            ),
          ),
        Flexible(
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxWidth: wide && active
                  ? 190
                  : width <= 576
                  ? 140
                  : double.infinity,
            ),
            child: MioButton(
              palette: p,
              label: p.english ? 'Open player' : '打开播放器',
              padding: EdgeInsets.zero,
              insetFocusBorder: false,
              disabledOpacity: 1,
              onPressed: active ? _openPlayer : null,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    state?.current?.title ?? p.text('play.welcomeSong'),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontWeight: FontWeight.bold,
                      color: p.color('--td-brand-color-active'),
                      fontSize: wide ? 14 : 13,
                      height: wide ? 21 / 14 : 18 / 13,
                    ),
                  ),
                  SizedBox(height: wide ? 4 : 2),
                  Text(
                    state?.error ??
                        state?.current?.artist ??
                        p.text('play.welcomeSinger'),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: textColor,
                      fontSize: wide ? 12 : 11,
                      height: wide ? 17 / 12 : 16 / 11,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        if (wide) ...[const SizedBox(width: 12), actions],
      ],
    );
    final content = Padding(
      padding: EdgeInsets.only(bottom: bottomExtension),
      child: Stack(
        fit: StackFit.expand,
        clipBehavior: Clip.none,
        children: [
          Padding(
            padding: EdgeInsets.symmetric(
              horizontal: wide ? 40 : MioPalette.gutter(width) - 6,
            ),
            child: Row(
              children: [
                Expanded(child: information),
                if (wide)
                  Expanded(child: Center(child: controls))
                else ...[
                  actions,
                  controls,
                ],
                if (wide)
                  Expanded(
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      children: [
                        Flexible(
                          child: ValueListenableBuilder(
                            valueListenable: core.playback,
                            builder: (context, clock, _) => Text(
                              '${_time(clock?.position ?? 0)} / ${_time(clock?.duration ?? 0)}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(fontSize: 12, color: textColor),
                            ),
                          ),
                        ),
                        const SizedBox(width: 12),
                        _iconButton(
                          playModeIcon(state?.playMode ?? api.PlayMode.list),
                          p.text(
                            playModeKey(state?.playMode ?? api.PlayMode.list),
                          ),
                          state == null
                              ? null
                              : () => _command(() => api.cyclePlayMode()),
                          size: 26,
                          iconSize: 18,
                          iconColor: textColor,
                          disabledOpacity: 1,
                        ),
                        const SizedBox(width: 12),
                        MiniPlayerVolume(
                          core: core,
                          palette: p,
                          active: visible && state != null,
                        ),
                        const SizedBox(width: 12),
                        _iconButton(
                          'nav-liebiao',
                          p.english ? 'Play queue' : '播放队列',
                          _openQueue,
                          size: 26,
                          iconSize: 18,
                          iconColor: textColor,
                          disabledOpacity: 1,
                        ),
                      ],
                    ),
                  )
                else
                  _iconButton(
                    'nav-liebiao',
                    p.english ? 'Play queue' : '播放队列',
                    _openQueue,
                    size: 44,
                    iconSize: 16,
                    iconColor: textColor,
                    disabledOpacity: 1,
                  ),
              ],
            ),
          ),
        ],
      ),
    );
    final tint = mioPlayerTint(
      p,
      wide: wide,
      active: active,
      coverTheme: state?.coverTheme,
    );
    final surface = MioGlassSurface(
      palette: p,
      radius: 0,
      tint: blendTrailingEdge ? const Color(0x00000000) : tint,
      enabled: visible,
      child: blendTrailingEdge
          ? DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment(1 - 2 * math.min(72.0, width) / width, 0),
                  end: Alignment.centerRight,
                  colors: [
                    tint,
                    tint.withValues(alpha: tint.a * .9),
                    tint.withValues(alpha: tint.a * .5),
                    tint.withValues(alpha: tint.a * .1),
                    tint.withValues(alpha: 0),
                  ],
                  stops: const [0, .2, .5, .8, 1],
                ),
              ),
              child: const SizedBox.expand(),
            )
          : content,
    );
    const shadow = BoxDecoration(
      boxShadow: [
        BoxShadow(
          offset: Offset(0, -1),
          color: Color(0x39000000),
          blurRadius: 8,
        ),
      ],
    );
    return Container(
      height: height + bottomExtension,
      decoration: blendTrailingEdge ? null : shadow,
      child: Stack(
        fit: StackFit.expand,
        clipBehavior: Clip.none,
        children: [
          if (blendTrailingEdge) IgnorePointer(child: surface) else surface,
          if (blendTrailingEdge) content,
          // The seek thumb extends above the glass surface's clipping bounds.
          if (active)
            Positioned(
              // On TV the bar sits inside the rounded card, clear of its
              // corners, instead of straddling the top edge.
              top: MioDeviceScope.isTv(context) ? -4 : -10,
              left: MioDeviceScope.isTv(context) ? 28 : 0,
              right: MioDeviceScope.isTv(context) ? 28 : 0,
              height: 24,
              child: ValueListenableBuilder(
                valueListenable: core.playback,
                builder: (context, clock, _) => MioSeekBar(
                  key: ValueKey((
                    state?.current?.id,
                    state?.playbackGeneration,
                    state?.queueRevision,
                    state?.sourceRevision,
                  )),
                  palette: p,
                  showThumb: true,
                  thumbOnlyWhenFocusedOnTv: true,
                  value: clock?.position ?? 0,
                  maximum: clock?.duration ?? 0,
                  onChanged: usable
                      ? (value) => _command(
                          () => api.seekCurrent(
                            songId: state!.current!.id,
                            generation: state.playbackGeneration,
                            queueRevision: state.queueRevision,
                            sourceRevision: state.sourceRevision,
                            seconds: value,
                          ),
                        )
                      : null,
                ),
              ),
            ),
        ],
      ),
    );
  }

  String _time(double seconds) {
    final s = seconds.isFinite ? seconds.clamp(0, double.infinity).floor() : 0;
    return '${s ~/ 60}:${(s % 60).toString().padLeft(2, '0')}';
  }

  static const _navPages = [
    _Page.discover,
    _Page.playlists,
    _Page.local,
    _Page.downloads,
  ];
  static const _navKeys = [
    'common.homeNav.discover',
    'common.homeNav.playlists',
    'common.homeNav.local',
    'common.homeNav.download',
  ];
  static const _navIcons = ['faxian', 'yanchu', 'shouye', 'xiazai'];
  static const _primaryCompactNavItems = [
    (
      page: _Page.discover,
      label: 'common.homeNav.discover',
      icon: 'nav-faxian',
    ),
    (
      page: _Page.playlists,
      label: 'common.homeNav.playlists',
      icon: 'nav-yanchu',
    ),
    (page: _Page.local, label: 'common.homeNav.local', icon: 'nav-shouye'),
  ];
  static const _rightNavItems = [
    ..._primaryCompactNavItems,
    (page: _Page.recognition, label: 'play.audioMatch', icon: 'TD-Microphone'),
    (
      page: _Page.downloads,
      label: 'common.homeNav.download',
      icon: 'nav-xiazai',
    ),
  ];
  static const _compactSettingsNavItem = (
    page: _Page.settings,
    label: 'common.titleBar.settings',
    icon: 'nav-shezhi',
  );
  static const _compactNavItems = [
    ..._primaryCompactNavItems,
    _compactSettingsNavItem,
  ];
  static const _settingsItems = [
    (
      page: _Page.appearance,
      label: 'settings.musicSource.navAppearance',
      description: 'settings.musicSource.navAppearanceDesc',
      icon: 'TD-Palette',
    ),
    (
      page: _Page.playbackSettings,
      label: 'settings.musicSource.navPlaylist',
      description: 'settings.musicSource.navPlaylistDesc',
      icon: 'TD-PlayCircle',
    ),
    (
      page: _Page.audioEffects,
      label: 'settings.musicSource.navEffects',
      description: 'settings.musicSource.navEffectsDesc',
      icon: 'TD-Audio',
    ),
    (
      page: _Page.plugins,
      label: 'settings.musicSource.navPlugins',
      description: 'settings.musicSource.navPluginsDesc',
      icon: 'TD-TreeRoundDot',
    ),
    (
      page: _Page.musicSources,
      label: 'settings.musicSource.navMusic',
      description: 'settings.musicSource.navMusicDesc',
      icon: 'TD-Music',
    ),
    (
      page: _Page.storageSettings,
      label: 'settings.musicSource.navStorage',
      description: 'settings.musicSource.navStorageDesc',
      icon: 'TD-Save',
    ),
    (
      page: _Page.about,
      label: 'settings.musicSource.navAbout',
      description: 'settings.musicSource.navAboutDesc',
      icon: 'TD-InfoCircle',
    ),
  ];

  /// Wide settings reuse the card pages (the desktop shares one set of
  /// cards between widths and only changes their metrics).
  static const _cardSettings = true;
  static const _settingsPages = {
    _Page.settings,
    _Page.musicSources,
    _Page.appearance,
    _Page.playbackSettings,
    _Page.audioEffects,
    _Page.equalizer,
    _Page.lyricSettings,
    _Page.visualSettings,
    _Page.plugins,
    _Page.storageSettings,
    _Page.about,
  };
}

class _PlayIntent extends Intent {
  const _PlayIntent();
}

class _SearchIntent extends Intent {
  const _SearchIntent();
}

class _GuideDecoration extends CustomPainter {
  final MioPalette palette;
  final double radius;
  _GuideDecoration(this.palette, this.radius);
  @override
  void paint(Canvas canvas, Size size) {
    final shape = RRect.fromRectAndRadius(
      Offset.zero & size,
      Radius.circular(radius),
    );
    canvas.drawRRect(shape, Paint()..color = palette.surface);
    canvas.save();
    canvas.clipRRect(shape);
    final glowRadius =
        math.sqrt(math.pow(size.width / 2, 2) + math.pow(size.height, 2)) * .42;
    canvas.drawRect(
      Offset.zero & size,
      Paint()
        ..shader =
            RadialGradient(
              colors: [
                palette.brand.withValues(alpha: .12),
                palette.brand.withValues(alpha: 0),
              ],
            ).createShader(
              Rect.fromCircle(
                center: Offset(size.width / 2, 0),
                radius: glowRadius,
              ),
            ),
    );
    canvas.restore();
    final path = Path()..addRRect(shape.deflate(.5));
    final paint = Paint()
      ..color = palette.border
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    for (final metric in path.computeMetrics()) {
      for (var start = 0.0; start < metric.length; start += 6) {
        canvas.drawPath(
          metric.extractPath(start, math.min(start + 3, metric.length)),
          paint,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_GuideDecoration old) =>
      old.palette.dark != palette.dark || old.radius != radius;
}

class _PlaybackAction extends Action<_PlayIntent> {
  final VoidCallback play;
  _PlaybackAction(this.play);
  @override
  bool isEnabled(_PlayIntent intent) {
    final context = FocusManager.instance.primaryFocus?.context;
    return context?.widget is! EditableText &&
        context?.findAncestorWidgetOfExactType<EditableText>() == null;
  }

  @override
  Object? invoke(_PlayIntent intent) {
    play();
    return null;
  }
}

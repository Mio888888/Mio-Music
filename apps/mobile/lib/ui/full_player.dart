import 'device.dart';
import 'visual_policy.dart';
import 'player_spectrum.dart';
import 'glass_surface.dart';
import 'package_lyrics.dart';
import 'player_background.dart';
import 'player_artwork.dart';
import 'player_title.dart';
import 'player_style_dialog.dart';
import 'player_idle.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' show ImageFilter;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart'
    show LogicalKeyboardKey, SystemUiOverlayStyle;

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'library_refresh.dart';
import 'download_quality.dart';
import 'singer.dart';
import 'wide_auto_hide.dart';
import 'comments.dart';
import 'queue_drawer.dart' show playModeKey, playModeIcon;

part 'tv/tv_full_player.dart';

class FullPlayer extends StatefulWidget {
  final CoreController core;
  final api.PlayerSnapshot state;
  final MioPalette palette;
  final VoidCallback onClose;
  final VoidCallback onQueue;
  final VoidCallback onAppearance;
  final EdgeInsets systemInsets;
  final double trailingInset;
  const FullPlayer({
    super.key,
    required this.core,
    required this.state,
    required this.palette,
    required this.onClose,
    required this.onQueue,
    required this.onAppearance,
    this.systemInsets = EdgeInsets.zero,
    this.trailingInset = 0,
  });
  @override
  State<FullPlayer> createState() => _FullPlayerState();
}

class _FullPlayerState extends State<FullPlayer> with WidgetsBindingObserver {
  final _headerNodes = [
    for (final label in [
      'Collapse',
      'Translation',
      'Romanization',
      'Follow',
      'Appearance',
    ])
      FocusNode(debugLabel: 'Player header $label'),
  ];
  void _moveHeaderFocus(int step) {
    final current = _headerNodes.indexWhere((node) => node.hasFocus);
    if (current < 0) return;
    for (var offset = 1; offset <= _headerNodes.length; offset++) {
      final node =
          _headerNodes[(current + step * offset) % _headerNodes.length];
      if (node.context != null && node.canRequestFocus) {
        node.requestFocus();
        return;
      }
    }
  }

  final _lyricKey = GlobalKey<MioPackageLyricsState>();
  final _styleLauncher = FocusNode(debugLabel: 'Player style');
  bool _presentingStyle = false;

  Future<void> _openStyle() async {
    if (!_foreground || _presentingStyle) return;
    setState(() => _presentingStyle = true);
    try {
      await Navigator.of(context).push<void>(
        RawDialogRoute<void>(
          barrierDismissible: true,
          barrierLabel: p.english ? 'Close player style' : '收起播放器样式',
          barrierColor: const Color(0x00000000),
          transitionDuration:
              (MediaQuery.disableAnimationsOf(context) ||
                  MioVisualPolicy.of(context).reduced)
              ? Duration.zero
              : const Duration(milliseconds: 250),
          pageBuilder: (_, _, _) =>
              PlayerStyleDialog(palette: p, core: widget.core),
          transitionBuilder: (_, animation, _, child) =>
              FadeTransition(opacity: animation, child: child),
        ),
      );
    } finally {
      if (mounted) setState(() => _presentingStyle = false);
      if (mounted && MioDeviceScope.isTv(context)) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted && _styleLauncher.canRequestFocus) {
            _styleLauncher.requestFocus();
          }
        });
      }
    }
  }

  late final LibraryRefresh _favoriteRefresh;
  api.PlayerDetails? _session;
  api.LyricDocument? _document;
  api.LyricClock? _clock;
  Uint8List? _cover;
  double _backgroundBass = 0;
  bool _backgroundFailed = false;
  bool? _favorite;
  bool _favoriteBusy = false;
  bool _favoriteFailed = false;
  bool _downloadBusy = false;
  bool _presentingComments = false;
  bool _singerAvailable = false,
      _openingSinger = false,
      _presentingSinger = false;
  String? _singerError;
  String? _downloadNote;
  bool _loading = true;
  bool _clockBusy = false;
  bool _foreground = true;
  bool _showLyrics = false;
  bool _translation = true;
  bool _romanization = false;
  void _toggleTranslation() {
    setState(() => _translation = !_translation);
    _follow(force: true);
  }

  void _toggleRomanization() {
    setState(() => _romanization = !_romanization);
    _follow(force: true);
  }

  bool _coverFailed = false;
  bool _coverBusy = false;
  String? _error;
  int _epoch = 0;
  MioPalette? _paletteSource, _darkPalette;
  MioPalette get p {
    if (widget.palette.dark) return widget.palette;
    if (!identical(_paletteSource, widget.palette)) {
      _paletteSource = widget.palette;
      _darkPalette = MioPalette(
        widget.palette.assets,
        dark: true,
        english: widget.palette.english,
        coverTheme: state.coverTheme,
      );
    }
    return _darkPalette!;
  }

  api.PlayerSnapshot get state => widget.state;
  Object _identity(api.PlayerSnapshot s) => (
    s.current?.id,
    s.playbackGeneration,
    s.queueRevision,
    s.sourceRevision,
    s.localRevision,
  );
  bool _valid(int epoch) => mounted && _foreground && epoch == _epoch;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unawaited(MioWideAutoHide.ensureLoaded());
    _favoriteRefresh = LibraryRefresh(
      core: widget.core,
      ready: () => _foreground && _session != null && !_favoriteBusy,
      refresh: () => unawaited(_loadFavorite(_session!, _epoch)),
    );
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    _foreground =
        lifecycle != AppLifecycleState.paused &&
        lifecycle != AppLifecycleState.detached;
    if (_foreground) unawaited(_load());
    widget.core.playback.addListener(_onPlayback);
  }

  // Position reports arrive several times a second without rebuilding the player;
  // each one refreshes the lyric clock.
  double? _clockPosition;
  void _onPlayback() {
    final clock = widget.core.playback.value;
    if (clock == null || _presentingSinger || !_foreground) return;
    if (clock.position == _clockPosition) return;
    _clockPosition = clock.position;
    if (_identity(clock) == _identity(state)) unawaited(_updateClock());
  }

  @override
  void didUpdateWidget(FullPlayer old) {
    super.didUpdateWidget(old);
    if (_presentingSinger) return;
    if (_identity(old.state) != _identity(state)) {
      if (_foreground) unawaited(_load());
    } else if (_foreground &&
        (old.state.position != state.position ||
            old.state.status != state.status)) {
      unawaited(_updateClock());
    }
  }

  void _release() {
    _backgroundBass = 0;
    _epoch++;
    _session?.cancel();
    _session?.dispose();
    _session = null;
    _clockBusy = false;
    _coverBusy = false;
    _favoriteBusy = false;
    _singerAvailable = false;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState lifecycle) {
    if (lifecycle == AppLifecycleState.resumed) {
      _foreground = true;
      if (!_presentingSinger) unawaited(_load());
    } else if (lifecycle == AppLifecycleState.paused ||
        lifecycle == AppLifecycleState.detached) {
      _foreground = false;
      _release();
    }
  }

  Future<void> _load() async {
    if (_presentingSinger) return;
    _release();
    final epoch = _epoch;
    final current = state.current;
    if (current == null || !_foreground) return;
    setState(() {
      _loading = true;
      _error = null;
      _document = null;
      _clock = null;
      _cover = null;
      _coverFailed = false;
      _favorite = null;
      _favoriteFailed = false;
      _singerError = null;
    });
    api.PlayerDetails? session;
    var adopted = false;
    try {
      session = await api.beginPlayerDetails(
        songId: current.id,
        generation: state.playbackGeneration,
        queueRevision: state.queueRevision,
        sourceRevision: state.sourceRevision,
      );
      if (!_valid(epoch)) {
        session.cancel();
        session.dispose();
        return;
      }
      _session = session;
      adopted = true;
      unawaited(_loadCover(session, epoch));
      unawaited(_loadFavorite(session, epoch));
      unawaited(_loadSingerAvailability(session, epoch));
      final document = await session.lyrics();
      if (!_valid(epoch)) return;
      setState(() {
        _document = document;
        _loading = false;
      });
      await _updateClock();
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _loading = false;
          _error = p.english ? 'Unable to load lyrics.' : '歌词加载失败';
        });
      }
      // Once adopted, the view owns release even if an old request later fails.
      if (!adopted) {
        session?.dispose();
      }
    }
  }

  Future<void> _download() async {
    final session = _session;
    if (_downloadBusy || !_foreground || session == null) return;
    final epoch = _epoch;
    setState(() {
      _downloadBusy = true;
      _downloadNote = null;
    });
    api.DownloadChoice? choice;
    try {
      choice = await session.prepareDownload();
      final info = await choice.info();
      if (!mounted || !_valid(epoch)) return;
      final result = await showDownloadChoice(
        context: context,
        choice: choice,
        info: info,
        core: widget.core,
        palette: p,
      );
      if (mounted && _foreground && result != null) {
        final key = switch (result) {
          api.DownloadEnqueueResult.added => 'download.addedToQueue',
          api.DownloadEnqueueResult.duplicate => 'download.duplicateDownload',
          api.DownloadEnqueueResult.alreadyDone => 'download.alreadyDownloaded',
        };
        // This is an event about the captured song, even if playback advanced.
        setState(() => _downloadNote = '${info.title}: ${p.text(key)}');
      }
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _downloadNote = p.english
              ? 'Download unavailable. Reopen the player and try again.'
              : '暂时无法下载，请重新打开播放器后重试。',
        );
      }
    } finally {
      choice?.close();
      choice?.dispose();
      if (mounted) setState(() => _downloadBusy = false);
    }
  }

  Future<void> _loadFavorite(api.PlayerDetails session, int epoch) async {
    if (_favoriteBusy) return;
    _favoriteBusy = true;
    try {
      final liked = await session.favorite();
      if (_valid(epoch)) {
        setState(() {
          _favorite = liked;
          _favoriteFailed = false;
        });
      }
    } catch (_) {
      if (_valid(epoch)) setState(() => _favoriteFailed = true);
    } finally {
      if (_valid(epoch)) {
        setState(() => _favoriteBusy = false);
        _favoriteRefresh.flush();
      }
    }
  }

  Future<void> _toggleFavorite() async {
    final session = _session;
    if (session == null || _favoriteBusy || !_foreground) return;
    if (_favorite == null) {
      await _loadFavorite(session, _epoch);
      return;
    }
    final epoch = _epoch;
    final liked = !_favorite!;
    setState(() => _favoriteBusy = true);
    try {
      final saved = await session.setFavorite(liked: liked);
      if (_valid(epoch)) {
        setState(() {
          _favorite = saved;
          _favoriteFailed = false;
        });
      }
    } catch (_) {
      if (_valid(epoch)) setState(() => _favoriteFailed = true);
    } finally {
      if (_valid(epoch)) {
        setState(() => _favoriteBusy = false);
        _favoriteRefresh.flush();
      }
    }
  }

  Future<void> _loadCover(api.PlayerDetails session, int epoch) async {
    if (_coverBusy) return;
    _coverBusy = true;
    try {
      final cover = await session.cover();
      if (_valid(epoch)) {
        setState(() {
          _cover = cover;
          _coverFailed = false;
        });
      }
    } catch (_) {
      if (_valid(epoch)) setState(() => _coverFailed = true);
    } finally {
      if (_valid(epoch)) _coverBusy = false;
    }
  }

  Future<void> _updateClock() async {
    final session = _session;
    if (session == null ||
        _clockBusy ||
        _document == null ||
        _error != null ||
        !_foreground) {
      return;
    }
    final epoch = _epoch;
    _clockBusy = true;
    try {
      final clock = await session.clock();
      if (!_valid(epoch)) return;
      setState(() => _clock = clock);
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _error = p.english
              ? 'Player changed. Reload lyrics.'
              : '播放状态已变更，请重新加载歌词',
        );
      }
    } finally {
      if (_valid(epoch)) _clockBusy = false;
    }
  }

  void _follow({bool force = false}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _foreground) _lyricKey.currentState?.resumeFollow();
    });
  }

  Future<void> _loadSingerAvailability(
    api.PlayerDetails session,
    int epoch,
  ) async {
    try {
      final available = await session.singerAvailable();
      if (_valid(epoch) && identical(session, _session)) {
        setState(() => _singerAvailable = available);
      }
    } catch (_) {
      // An unavailable source keeps the artist as plain text.
    }
  }

  Future<void> _openSinger() async {
    final session = _session;
    if (session == null ||
        !_foreground ||
        _openingSinger ||
        !_singerAvailable) {
      return;
    }
    final epoch = _epoch, name = state.current?.artist ?? '';
    setState(() {
      _openingSinger = true;
      _singerError = null;
    });
    api.SingerSession? candidate;
    try {
      candidate = await session.openSinger();
      if (!mounted || !_valid(epoch) || !identical(session, _session)) return;
      final artist = candidate;
      _presentingSinger = true;
      _release();
      final host = SingerHost.of(context)?.open;
      if (host != null) {
        final result = host(artist, name);
        candidate = null;
        await result;
        return;
      }
      final result = Navigator.of(context).push<void>(
        PageRouteBuilder(
          pageBuilder: (_, _, _) => SingerPage(
            session: artist,
            core: widget.core,
            palette: p,
            fallbackName: name,
          ),
          transitionDuration:
              (MediaQuery.disableAnimationsOf(context) ||
                  MioVisualPolicy.of(context).reduced)
              ? Duration.zero
              : const Duration(milliseconds: 200),
          reverseTransitionDuration: MediaQuery.disableAnimationsOf(context)
              ? Duration.zero
              : const Duration(milliseconds: 200),
          transitionsBuilder: (_, animation, _, child) =>
              FadeTransition(opacity: animation, child: child),
        ),
      );
      candidate = null; // The route owns disposal after a successful push.
      await result;
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _singerError = p.english
              ? 'Unable to open artist. Retry.'
              : '歌手页打开失败，请重试。',
        );
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
      final wasPresented = _presentingSinger;
      _presentingSinger = false;
      _openingSinger = false;
      if (mounted) {
        setState(() {});
        if (wasPresented && _foreground) unawaited(_load());
      }
    }
  }

  Future<void> _openComments() async {
    if (_presentingComments || !_foreground) return;
    setState(() => _presentingComments = true);
    try {
      await Navigator.of(context).push<void>(
        RawDialogRoute<void>(
          barrierDismissible: false,
          barrierColor: const Color(0x00000000),
          pageBuilder: (_, _, _) =>
              CommentsDialog(core: widget.core, palette: p),
        ),
      );
    } finally {
      if (mounted) setState(() => _presentingComments = false);
    }
  }

  void _command(Future<void> Function() action) =>
      unawaited(widget.core.command(action));
  @override
  void dispose() {
    widget.core.playback.removeListener(_onPlayback);
    _favoriteRefresh.dispose();
    _styleLauncher.dispose();
    for (final node in _headerNodes) {
      node.dispose();
    }
    _volumeLauncher.dispose();
    _volumeSlider.dispose();
    _release();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  String _time(double value) {
    final seconds = value.isFinite ? math.max(0, value.toInt()) : 0;
    return '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}';
  }

  Widget _button(
    String icon,
    String label,
    VoidCallback? action, {
    double size = 44,
    FocusNode? focusNode,
    double? iconSize,
    Color? iconColor,
    Color? background,
    bool outlined = false,
  }) => SizedBox(
    width: size,
    height: size,
    child: DecoratedBox(
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        border: outlined ? Border.all(color: const Color(0x33ffffff)) : null,
      ),
      child: MioButton(
        palette: p,
        label: label,
        radius: size / 2,
        padding: EdgeInsets.zero,
        onPressed: action,
        focusNode: focusNode,
        background: background,
        child: Center(
          child: MioIcon(
            icon,
            color: iconColor ?? const Color(0xffeeeeee),
            size: iconSize ?? size * .48,
          ),
        ),
      ),
    ),
  );
  Widget _art(double size, {bool reserveTonearm = true, bool? coverLayout}) =>
      MioPlayerArtwork(
        cover: _cover,
        coverLayout: coverLayout ?? state.visuals.coverLayout,
        playing: state.status == 'playing',
        size: size,
        reserveTonearm: reserveTonearm,
      );

  Widget _headerButton(bool wide, {bool switchMode = false}) {
    final size = wide ? 45.0 : 44.0;
    final radius = wide ? 10.0 : 22.0;
    final reduced = MioVisualPolicy.of(context).reduced;
    if (!wide) {
      // Narrow: a small liquid-glass lens over the artwork background.
      return DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(radius),
          boxShadow: const [
            BoxShadow(
              color: Color(0x33000000),
              offset: Offset(0, 6),
              blurRadius: 16,
            ),
          ],
        ),
        child: SizedBox.square(
          dimension: size,
          child: MioGlassSurface(
            palette: p,
            radius: radius,
            enabled: _foreground,
            tint: const Color(0x2e000000),
            child: MioButton(
              palette: p,
              label: switchMode
                  ? (_showLyrics
                        ? (p.english ? 'Switch to artwork' : '切换到封面模式')
                        : (p.english ? 'Switch to lyrics' : '切换到歌词模式'))
                  : (p.english ? 'Close player' : '关闭播放器'),
              focusNode: switchMode ? null : _headerNodes[0],
              onPressed: switchMode ? _toggleLyrics : widget.onClose,
              radius: radius,
              padding: EdgeInsets.zero,
              child: Center(
                child: MioIcon(
                  switchMode ? 'TD-Swap' : 'TD-Down',
                  color: const Color(0xf2ffffff),
                  size: 22,
                ),
              ),
            ),
          ),
        ),
      );
    }
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(radius),
        boxShadow: wide
            ? const []
            : const [
                BoxShadow(
                  color: Color(0x2e000000),
                  offset: Offset(0, 10),
                  blurRadius: 26,
                ),
              ],
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(radius),
        child: BackdropFilter(
          enabled: _foreground && !reduced,
          filter: ImageFilter.blur(
            sigmaX: wide ? 6 : 12,
            sigmaY: wide ? 6 : 12,
          ),
          child: DecoratedBox(
            position: DecorationPosition.foreground,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(radius),
              border: wide
                  ? null
                  : Border.all(color: const Color(0x33ffffff), width: .5),
            ),
            child: SizedBox.square(
              dimension: size,
              child: MioButton(
                palette: p,
                label: switchMode
                    ? (_showLyrics
                          ? (p.english ? 'Switch to artwork' : '切换到封面模式')
                          : (p.english ? 'Switch to lyrics' : '切换到歌词模式'))
                    : (p.english ? 'Close player' : '关闭播放器'),
                focusNode: switchMode ? null : _headerNodes[0],
                onPressed: switchMode ? _toggleLyrics : widget.onClose,
                radius: radius,
                padding: EdgeInsets.zero,
                background: !wide
                    ? const Color(0x33000000)
                    : state.coverTheme?.useBlackText == true
                    ? const Color(0x1a000000)
                    : const Color(0x1affffff),
                child: Center(
                  child: MioIcon(
                    switchMode ? 'TD-Swap' : 'TD-Down',
                    color: const Color(0xe6ffffff),
                    size: wide ? 25 : 22,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _album(
    double width,
    double height,
    bool wide, {
    double? contentWidth,
  }) => LayoutBuilder(
    builder: (context, constraints) {
      if (!wide) return _compactAlbum(constraints);
      final viewport = contentWidth ?? MediaQuery.sizeOf(context).width;
      final short = !wide && height <= 700;
      final bottom = (height * .1).clamp(70.0, 80.0);
      final cover = state.visuals.coverLayout;
      final mobileArt = math.min(
        viewport * (short ? .7 : .74),
        math.min(short ? 300.0 : 330.0, height * (short ? .36 : .4)),
      );
      final size = cover
          ? math.max(
              120.0,
              math.min(width - viewport * .06, math.min(480.0, height * .45)),
            )
          : math.max(
              200.0,
              math.min(viewport * .3, math.min(700.0, height - bottom - 250)),
            );
      final titleFont = short
          ? (viewport * .06).clamp(20.48, 28.0)
          : wide
          ? math.min(viewport * .03, 42.0)
          : (viewport * .066).clamp(23.2, 34.4);
      final infoWidth = cover ? width - viewport * .06 : math.min(width, 520.0);
      final metadata = TextStyle(
        fontSize: short
            ? 13.44
            : wide
            ? math.min(viewport * .015, 20)
            : (viewport * .038).clamp(13.76, 15.68),
        height: wide ? 1.5 : 1.35,
        fontWeight: wide ? FontWeight.w600 : FontWeight.normal,
        color: const Color(0x80ffffff),
      );
      final artist = Text(state.current?.artist ?? '', style: metadata);
      final album = Column(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          SizedBox(
            height: cover
                ? bottom * .5
                : wide
                ? 0
                : mobileArt * .08,
          ),
          _art(size, reserveTonearm: false, coverLayout: cover),
          SizedBox(
            height: cover
                ? 40
                : wide
                ? 36
                : 36 + (short ? 10.4 : (height * .026).clamp(16.0, 24.0)),
          ),
          SizedBox(
            width: infoWidth,
            child: MioPlayerTitle(
              title: state.current?.title ?? '',
              wide: wide,
              centered: true,
              fontSize: titleFont,
              gap: wide ? 2 * (viewport * .014).clamp(15.0, 18.0) : 32,
            ),
          ),
          const SizedBox(height: 12),
          SizedBox(
            width: infoWidth,
            child: Wrap(
              alignment: WrapAlignment.center,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 8,
              runSpacing: 8,
              children: [
                ConstrainedBox(
                  constraints: BoxConstraints(minHeight: wide ? 0 : 32),
                  child: _singerAvailable
                      ? MioButton(
                          palette: p,
                          label: p.english
                              ? 'Open artist: ${state.current?.artist ?? ""}'
                              : '打开歌手页：${state.current?.artist ?? ""}',
                          onPressed: !_openingSinger && _foreground
                              ? () => unawaited(_openSinger())
                              : null,
                          child: artist,
                        )
                      : Padding(
                          padding: EdgeInsets.symmetric(vertical: wide ? 0 : 6),
                          child: artist,
                        ),
                ),
                if (state.current?.album.isNotEmpty == true) ...[
                  if (state.current?.artist.isNotEmpty == true)
                    Text(
                      '/',
                      style: metadata.copyWith(color: const Color(0x22ffffff)),
                    ),
                  Text(
                    state.current!.album,
                    style: metadata.copyWith(color: const Color(0x54ffffff)),
                  ),
                ],
              ],
            ),
          ),
          if (_singerError != null)
            Text(
              _singerError!,
              style: const TextStyle(fontSize: 12, color: Color(0xffffb4b4)),
            ),
          if (_coverFailed)
            MioButton(
              palette: p,
              label: p.english ? 'Retry artwork' : '重试封面',
              onPressed: _session == null
                  ? null
                  : () => unawaited(_loadCover(_session!, _epoch)),
              child: Text(p.english ? 'Retry artwork' : '重试封面'),
            ),
        ],
      );
      if (!wide) {
        // Desktop centers the whole compact column, even when a wrapped title
        // makes it taller than the space between the header and transport.
        return OverflowBox(
          minHeight: 0,
          maxHeight: double.infinity,
          child: album,
        );
      }
      return SingleChildScrollView(
        clipBehavior: Clip.none,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: album,
        ),
      );
    },
  );

  Widget _compactAlbum(BoxConstraints box) {
    final width = box.maxWidth;
    final cover = state.visuals.coverLayout;
    final gutter = (width * .04).clamp(16.0, 24.0);
    final art = math.min(
      math.min(width * .68, cover ? 300.0 : 280.0),
      math.max(96.0, (box.maxHeight - 200) / (cover ? 1 : 1.24)),
    );
    final metadata = TextStyle(fontSize: 13, color: const Color(0x80ffffff));
    final artist = Text(
      state.current?.artist ?? '',
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      textAlign: TextAlign.right,
      style: metadata,
    );
    return SingleChildScrollView(
      padding: EdgeInsets.fromLTRB(gutter, 8, gutter, 16),
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: math.max(0, box.maxHeight - 24)),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            _art(art, reserveTonearm: true, coverLayout: cover),
            const SizedBox(height: 16),
            MioPlayerTitle(
              title: state.current?.title ?? '',
              wide: false,
              singleLine: true,
              centered: true,
              fontSize: (width * .066).clamp(23.2, 30.0),
              gap: 32,
            ),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Flexible(
                  child: _singerAvailable
                      ? MioButton(
                          palette: p,
                          label: p.english ? 'Open artist' : '打开歌手页',
                          padding: const EdgeInsets.symmetric(vertical: 10),
                          onPressed: !_openingSinger && _foreground
                              ? () => unawaited(_openSinger())
                              : null,
                          child: Align(
                            alignment: Alignment.centerRight,
                            widthFactor: 1,
                            child: artist,
                          ),
                        )
                      : Padding(
                          padding: const EdgeInsets.symmetric(vertical: 10),
                          child: artist,
                        ),
                ),
                const Padding(
                  padding: EdgeInsets.symmetric(horizontal: 10),
                  child: Text('/', style: TextStyle(color: Color(0x55ffffff))),
                ),
                Flexible(
                  child: Text(
                    state.current?.album ?? '',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: metadata,
                  ),
                ),
              ],
            ),
            if (_singerError != null)
              Text(
                _singerError!,
                style: const TextStyle(fontSize: 12, color: Color(0xffffb4b4)),
              ),
            if (_coverFailed)
              MioButton(
                palette: p,
                label: p.english ? 'Retry artwork' : '重试封面',
                onPressed: _session == null
                    ? null
                    : () => unawaited(_loadCover(_session!, _epoch)),
                child: Text(p.english ? 'Retry artwork' : '重试封面'),
              ),
            const SizedBox(height: 16),
            _compactLyricsPreview(),
          ],
        ),
      ),
    );
  }

  Widget _compactLyricsPreview() {
    final rows = _document?.rows ?? <api.LyricRow>[];
    final indices = [
      for (var i = 0; i < rows.length; i++)
        if (!rows[i].background && rows[i].text.trim().isNotEmpty) i,
    ];
    final active = _clock?.activeRow ?? 0;
    var center = indices.lastIndexWhere((i) => i <= active);
    if (center < 0) center = 0;
    final message = _loading
        ? (p.english ? 'Loading lyrics…' : '歌词加载中…')
        : _error ?? (p.english ? 'No lyrics' : '暂无歌词');
    final lines = [
      for (var offset = -1; offset <= 1; offset++)
        center + offset >= 0 && center + offset < indices.length
            ? rows[indices[center + offset]].text
            : (offset == 0 ? message : ' '),
    ];
    final reduced =
        MioVisualPolicy.of(context).reduced ||
        MediaQuery.disableAnimationsOf(context);
    return SizedBox(
      width: double.infinity,
      height: 100,
      child: MioButton(
        palette: p,
        label: p.english ? 'Expand lyrics' : '展开歌词',
        onPressed: _toggleLyrics,
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: ShaderMask(
          blendMode: BlendMode.dstIn,
          shaderCallback: (bounds) => const LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [
              Color(0x00ffffff),
              Color(0xffffffff),
              Color(0xffffffff),
              Color(0x00ffffff),
            ],
            stops: [0, .35, .65, 1],
          ).createShader(bounds),
          child: ClipRect(
            child: AnimatedSwitcher(
              duration: reduced
                  ? Duration.zero
                  : const Duration(milliseconds: 380),
              transitionBuilder: (child, animation) => FadeTransition(
                opacity: animation,
                child: SlideTransition(
                  position:
                      Tween<Offset>(
                        begin: const Offset(0, .16),
                        end: Offset.zero,
                      ).animate(
                        CurvedAnimation(
                          parent: animation,
                          curve: Curves.easeOutCubic,
                        ),
                      ),
                  child: child,
                ),
              ),
              child: SizedBox(
                key: ValueKey(
                  '${state.current?.id}:$center:${lines.join("|")}',
                ),
                height: 100,
                width: double.infinity,
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    for (var i = 0; i < 3; i++)
                      SizedBox(
                        height: 28,
                        child: Center(
                          child: Text(
                            lines[i],
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            textAlign: TextAlign.center,
                            style: TextStyle(
                              fontSize: 13,
                              height: 1.5,
                              color: i == 1
                                  ? _lyricColor.withValues(alpha: .9)
                                  : _lyricColor.withValues(alpha: .45),
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
    );
  }

  Color get _lyricColor {
    final theme = state.coverTheme;
    if (!state.visuals.immersiveLyricColor || theme == null) {
      return const Color(0xffffffff);
    }
    // Match desktop full_player.rs: 20% cover color mixed with80% white.
    // The snapshot's cover theme already obeys the shared dynamic-theme switch.
    int channel(int value) => (value * .2 + 255 * .8).round();
    return Color.fromARGB(
      255,
      channel(theme.red),
      channel(theme.green),
      channel(theme.blue),
    );
  }

  Widget _lyrics(bool wide, double font, double viewportWidth) {
    if (_loading ||
        _error != null ||
        _document == null ||
        _document!.rows.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              _loading
                  ? (p.english ? 'Loading lyrics…' : '歌词加载中…')
                  : _error ?? (p.english ? 'No lyrics' : '暂无歌词'),
            ),
            if (_error != null)
              MioButton(
                palette: p,
                label: p.english ? 'Retry' : '重试',
                onPressed: () => unawaited(_load()),
                child: Text(p.english ? 'Retry' : '重试'),
              ),
          ],
        ),
      );
    }
    return MioPackageLyrics(
      key: _lyricKey,
      document: _document!,
      clock: _clock,
      style: state.lyricStyle,
      fontSize: font,
      horizontalPadding: viewportWidth <= 500
          ? 20
          : (MioDeviceScope.wide(context, viewportWidth) ? font * .5 : font),
      color: _lyricColor,
      centered: !state.visuals.showLeftPanel,
      singleLine: MioDeviceScope.wide(context, viewportWidth),
      anchor: !state.visuals.coverLayout && state.visuals.showLeftPanel
          ? .5
          : .34,
      // Desktop always shows translation and romanization lines when present.
      translation: _translation,
      romanization:
          _romanization ||
          (MioDeviceScope.wide(context, viewportWidth) &&
              !MioDeviceScope.isTv(context)),
      onSeek: (index) {
        final session = _session;
        if (session != null && _foreground) {
          _command(() => session.seekLine(index: index));
        }
      },
    );
  }

  bool _volumeOpen = false;
  final _volumeLauncher = FocusNode(debugLabel: 'Player volume');
  final _volumeSlider = FocusNode(debugLabel: 'Player volume slider');
  void _toggleVolume() {
    if (_volumeOpen) {
      _closeVolume();
      return;
    }
    setState(() => _volumeOpen = true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _volumeOpen) _volumeSlider.requestFocus();
    });
  }

  void _closeVolume() {
    setState(() => _volumeOpen = false);
    _volumeLauncher.requestFocus();
  }

  Widget _transport(double height, {bool wide = true, double width = 1200}) {
    final usable = state.current != null && state.status != 'loading';
    final playing = state.status == 'playing';
    final policy = MioVisualPolicy.of(context);
    final backdrop = policy.background && _cover != null && !_backgroundFailed;
    final scaler = MediaQuery.textScalerOf(context);
    final titleLineHeight = scaler.scale(18);
    final artistLineHeight = scaler.scale(16);
    final titleLines =
        !wide && height - 4 >= titleLineHeight * 2 + artistLineHeight + 2
        ? 2
        : 1;
    final artistLines =
        !wide &&
            height - 4 >=
                titleLineHeight * titleLines + artistLineHeight * 2 + 2
        ? 2
        : 1;
    return SizedBox(
      height: height,
      child: ColoredBox(
        color: backdrop ? const Color(0x00000000) : const Color(0xff19232b),
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            Positioned.fill(
              child: ClipRect(
                child: BackdropFilter(
                  enabled: backdrop && _foreground && !policy.reduced,
                  filter: ImageFilter.blur(sigmaX: 8, sigmaY: 8),
                  child: const ColoredBox(color: Color(0x20000000)),
                ),
              ),
            ),
            Positioned.fill(
              top: 4,
              child: Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: wide ? 40 : (width * .04).clamp(16.0, 24.0) - 6,
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: Row(
                        children: [
                          ClipRRect(
                            borderRadius: BorderRadius.circular(wide ? 4 : 6),
                            child: SizedBox.square(
                              dimension: wide ? 50 : 42,
                              child: _cover == null
                                  ? Image.asset(
                                      'assets/images/default-cover.png',
                                      fit: BoxFit.cover,
                                    )
                                  : Image.memory(
                                      _cover!,
                                      fit: BoxFit.cover,
                                      // Decode at thumbnail size, not the cover's own.
                                      cacheWidth:
                                          ((wide ? 50 : 42) *
                                                  MediaQuery.devicePixelRatioOf(
                                                    context,
                                                  ))
                                              .ceil(),
                                    ),
                            ),
                          ),
                          SizedBox(width: wide ? 12 : 10),
                          Flexible(
                            child: ConstrainedBox(
                              constraints: BoxConstraints(
                                maxWidth: wide
                                    ? 190
                                    : width <= 576
                                    ? 140
                                    : double.infinity,
                              ),
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    state.current?.title ?? '',
                                    maxLines: titleLines,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: wide ? 14 : 13,
                                      height: wide ? 1.5 : 18 / 13,
                                      fontWeight: FontWeight.bold,
                                      color: p.brand,
                                    ),
                                  ),
                                  SizedBox(height: wide ? 4 : 2),
                                  Text(
                                    state.current?.artist ?? '',
                                    maxLines: artistLines,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: wide ? 12 : 11,
                                      height: wide ? 17 / 12 : 16 / 11,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                          if (wide)
                            _button(
                              _favorite == true
                                  ? 'TD-Heart-Filled'
                                  : 'TD-Heart',
                              _favoriteFailed
                                  ? (p.english
                                        ? 'Retry favorite operation'
                                        : '重试收藏操作')
                                  : p.text(
                                      _favorite == true
                                          ? 'play.liked'
                                          : 'play.like',
                                    ),
                              _favoriteBusy || _session == null
                                  ? null
                                  : () => unawaited(_toggleFavorite()),
                              size: 32,
                            ),
                          if (wide &&
                              (state.current?.id.startsWith('mio-online:') ??
                                  false))
                            _button(
                              'Download',
                              p.text('download.title'),
                              _downloadBusy || _session == null
                                  ? null
                                  : () => unawaited(_download()),
                              size: 32,
                            ),
                          if (wide) const SizedBox(width: 16),
                        ],
                      ),
                    ),
                    if (!wide) const SizedBox(width: 4),
                    if (!wide)
                      _button(
                        _favorite == true ? 'TD-Heart-Filled' : 'TD-Heart',
                        _favoriteFailed
                            ? (p.english
                                  ? 'Retry favorite operation'
                                  : '重试收藏操作')
                            : p.text(
                                _favorite == true ? 'play.liked' : 'play.like',
                              ),
                        _favoriteBusy || _session == null
                            ? null
                            : () => unawaited(_toggleFavorite()),
                        size: 32,
                      ),
                    _button(
                      'nav-shangyishou',
                      p.text('common.previous'),
                      () => _command(() => api.step(forward: false)),
                      size: wide ? 38 : 44,
                      iconSize: wide ? 28 : 20,
                      iconColor: p.color('--player-text-idle'),
                    ),
                    if (wide) const SizedBox(width: 16),
                    _button(
                      playing ? 'nav-zanting' : 'nav-bofang',
                      p.text(playing ? 'common.pause' : 'common.play'),
                      usable
                          ? () => _command(() => api.setPaused(paused: playing))
                          : null,
                      size: wide ? 42 : 44,
                      iconSize: wide ? 28 : 22,
                      iconColor: p.color('--player-text-hover-idle'),
                      background: state.coverTheme != null
                          ? const Color(0x33ffffff)
                          : p.color('--player-btn-bg-idle'),
                    ),
                    if (wide) const SizedBox(width: 16),
                    _button(
                      'nav-xiayishou',
                      p.text('common.next'),
                      () => _command(() => api.step(forward: true)),
                      size: wide ? 38 : 44,
                      iconSize: wide ? 28 : 20,
                      iconColor: p.color('--player-text-idle'),
                    ),
                    if (!wide) ...[
                      _button(
                        'Message',
                        p.english ? 'Comments' : '评论',
                        _foreground && !_presentingComments
                            ? () => unawaited(_openComments())
                            : null,
                        size: width < 360 ? 32 : 44,
                        iconSize: 18,
                        iconColor: p.color('--player-text-idle'),
                      ),
                      _button(
                        'nav-liebiao',
                        p.english ? 'Play queue' : '播放队列',
                        widget.onQueue,
                        size: 44,
                        iconSize: 16,
                        iconColor: p.color('--player-text-idle'),
                      ),
                    ],
                    if (wide)
                      Expanded(
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.end,
                          children: [
                            ValueListenableBuilder(
                              valueListenable: widget.core.playback,
                              builder: (context, clock, _) => Text(
                                '${_time(clock?.position ?? state.position)} / ${_time(clock?.duration ?? state.duration)}',
                                style: const TextStyle(fontSize: 12),
                              ),
                            ),
                            const SizedBox(width: 12),
                            _button(
                              playModeIcon(state.playMode),
                              p.text(playModeKey(state.playMode)),
                              () => _command(() => api.cyclePlayMode()),
                              size: 32,
                            ),
                            _button(
                              'nav-shengyin',
                              p.english ? 'Volume' : '音量',
                              _toggleVolume,
                              focusNode: _volumeLauncher,
                              size: 28,
                              iconSize: 20,
                              iconColor: p.color('--player-text-idle'),
                            ),
                            _button(
                              'Message',
                              p.english ? 'Comments' : '评论',
                              _foreground && !_presentingComments
                                  ? () => unawaited(_openComments())
                                  : null,
                              size: 32,
                            ),
                            _button(
                              'nav-liebiao',
                              p.english ? 'Play queue' : '播放队列',
                              widget.onQueue,
                              size: 32,
                              iconSize: 21,
                              iconColor: p.color('--player-text-idle'),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ),
            Positioned(
              left: 0,
              right: 0,
              top: -10,
              child: ValueListenableBuilder(
                valueListenable: widget.core.playback,
                builder: (context, clock, _) => MioSeekBar(
                  key: ValueKey(_identity(state)),
                  palette: p,
                  showThumb: true,
                  value: clock?.position ?? state.position,
                  maximum: clock?.duration ?? state.duration,
                  onChanged: usable && _session != null
                      ? (value) {
                          final session = _session!;
                          _command(() => session.seek(seconds: value));
                        }
                      : null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _idleChrome(
    Widget child,
    bool hidden,
    double progress, {
    Offset shift = Offset.zero,
    bool slide = false,
  }) => ExcludeSemantics(
    excluding: hidden,
    child: ExcludeFocus(
      excluding: hidden,
      child: IgnorePointer(
        ignoring: hidden,
        child: slide
            ? FractionalTranslation(
                translation: Offset(0, progress),
                child: child,
              )
            : Transform.translate(
                offset: shift * progress,
                child: Opacity(
                  opacity: (1 - progress).clamp(0.0, 1.0),
                  child: child,
                ),
              ),
      ),
    ),
  );

  Widget _wideLayout(
    BoxConstraints box,
    double gutter,
    double albumFraction,
    bool showAlbum,
    double lyricFont,
    Widget header,
    bool hidden,
    double idleProgress,
  ) {
    final transportHeight = (box.maxHeight * .1).clamp(70.0, 80.0);
    final bottom = hidden ? 0.0 : transportHeight;
    return Stack(
      children: [
        Positioned.fill(
          // Desktop centers the left column on the full height; only the
          // lyric column stops above the transport bar.
          child: Padding(
            // Single-line lyrics gain room by giving back most of the right gutter.
            padding: EdgeInsets.only(left: gutter, right: gutter * .35),
            child: Row(
              children: [
                if (showAlbum)
                  SizedBox(
                    width: (box.maxWidth - gutter * 2) * albumFraction,
                    child: _album(
                      (box.maxWidth - gutter * 2) * albumFraction,
                      box.maxHeight,
                      true,
                      contentWidth: box.maxWidth,
                    ),
                  ),
                Expanded(
                  child: Padding(
                    // Desktop overrides only the left inset in cover mode,
                    // and both insets when the artwork panel is hidden.
                    padding: EdgeInsets.only(
                      bottom: bottom,
                      left: !showAlbum
                          ? box.maxWidth * .1
                          : state.visuals.coverLayout
                          ? box.maxWidth * .03
                          : 20,
                      right: !showAlbum ? box.maxWidth * .04 : 20,
                    ),
                    child: _lyrics(true, lyricFont, box.maxWidth),
                  ),
                ),
              ],
            ),
          ),
        ),
        Positioned(
          left: 30,
          right: 30,
          top: 25,
          child: _idleChrome(
            header,
            hidden,
            idleProgress,
            shift: const Offset(0, -45),
          ),
        ),
        if (widget.core.error != null ||
            state.status == 'failed' ||
            _downloadNote != null)
          Positioned(
            left: 30,
            right: 30,
            bottom: bottom + 12,
            child: Semantics(
              liveRegion: true,
              child: Text(
                widget.core.error ??
                    (state.status == 'failed'
                        ? state.error ??
                              (p.english
                                  ? 'Playback failed. Please retry.'
                                  : '播放失败，请重试')
                        : _downloadNote!),
                textAlign: TextAlign.center,
                style: const TextStyle(fontSize: 13, color: Color(0xffffb4b4)),
              ),
            ),
          ),
        if (_volumeOpen)
          Positioned(
            key: const ValueKey('player-volume'),
            right: 72,
            bottom: bottom + 12,
            child: CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.arrowDown):
                    _closeVolume,
                const SingleActivator(LogicalKeyboardKey.escape): _closeVolume,
              },
              child: Container(
                width: 180,
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: const Color(0xff202a33),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: MioSeekBar(
                        focusNode: _volumeSlider,
                        palette: p,
                        value: state.volume,
                        maximum: 1,
                        semanticLabel: p.english ? 'Volume' : '音量',
                        onChanged: (value) =>
                            _command(() => api.setVolume(volume: value)),
                      ),
                    ),
                    _button(
                      'TD-Down',
                      p.english ? 'Close volume' : '收起音量',
                      _closeVolume,
                      size: 32,
                    ),
                  ],
                ),
              ),
            ),
          ),
        Positioned(
          key: const ValueKey('player-transport'),
          left: 0,
          right: 0,
          bottom: 0,
          child: _idleChrome(
            _transport(transportHeight),
            hidden,
            // The bar also leaves through the safe-area gap below it.
            idleProgress *
                (1 + (widget.systemInsets.bottom + 24) / transportHeight),
            slide: true,
          ),
        ),
      ],
    );
  }

  void _toggleLyrics() {
    setState(() => _showLyrics = !_showLyrics);
    _follow(force: true);
  }

  Widget _narrowLayout(
    BoxConstraints box,
    Widget header,
    double lyricFont,
    bool hidden,
    double idleProgress,
  ) {
    final width = box.maxWidth;
    final height = box.maxHeight;
    final transportHeight = (height * .1).clamp(70.0, 80.0);
    // With the bar auto-hidden the lyrics may use its space.
    final bottom = hidden ? 0.0 : transportHeight;
    final top = (width * .035).clamp(14.0, 22.0);
    final contentTop = top + 44 + (height <= 700 ? 8 : 12);
    final gutter = (width * .04).clamp(16.0, 24.0);
    return Stack(
      children: [
        Positioned.fill(
          top: contentTop,
          // Desktop keeps a height-minus-transport lyric viewport beneath
          // its floating header. The transport paints above the lower edge.
          bottom: _showLyrics ? bottom - contentTop : bottom + 18,
          child: _showLyrics
              ? Padding(
                  padding: EdgeInsets.only(
                    left:
                        gutter +
                        (!state.visuals.showLeftPanel
                            ? width * .1
                            : state.visuals.coverLayout
                            ? width * .03
                            : 20),
                    right:
                        gutter +
                        (!state.visuals.showLeftPanel ? width * .1 : 20),
                  ),
                  child: _lyrics(false, lyricFont, width),
                )
              : _album(width, height, false, contentWidth: width),
        ),
        Positioned(
          left: gutter,
          right: gutter,
          top: top,
          child: _idleChrome(
            header,
            hidden,
            idleProgress,
            shift: const Offset(0, -45),
          ),
        ),
        if (widget.core.error != null || state.status == 'failed')
          Positioned(
            left: gutter,
            // Keep recovery guidance clear of the lyric-style launcher.
            right: gutter + 52,
            bottom: bottom + 8,
            child: Text(
              widget.core.error ??
                  state.error ??
                  (p.english ? 'Playback failed. Please retry.' : '播放失败，请重试'),
              style: const TextStyle(fontSize: 13, color: Color(0xffffb4b4)),
            ),
          ),
        Positioned(
          key: const ValueKey('player-transport'),
          left: 0,
          right: 0,
          bottom: 0,
          child: _idleChrome(
            _transport(transportHeight, wide: false, width: width),
            hidden,
            // It also leaves through the safe-area gap below it.
            idleProgress *
                (1 + (widget.systemInsets.bottom + 24) / transportHeight),
            slide: true,
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => TickerMode(
    enabled: !_presentingComments && !_presentingSinger && !_downloadBusy,
    child: MioVisualMonitor(
      child: MioTvScreenLease(
        child: LayoutBuilder(
          builder: (context, constraints) {
            final box = constraints.deflate(
              EdgeInsets.only(right: widget.trailingInset),
            );
            final policy = MioVisualPolicy.of(context);
            final wide = MioDeviceScope.wide(context, box.maxWidth);
            final tv = MioDeviceScope.isTv(context);
            final insets = wide
                ? EdgeInsets.only(bottom: widget.systemInsets.bottom)
                : widget.systemInsets;
            final contentHeight = math.max(
              0.0,
              box.maxHeight - insets.vertical,
            );
            final lyricFont = wide
                ? math.min(
                        (box.maxWidth * .025).clamp(30.0, 50.0),
                        box.maxHeight * .05,
                      ) *
                      state.lyricStyle.rate
                : (box.maxWidth * .07).clamp(24.0, 34.0);
            final showAlbum = !wide || state.visuals.showLeftPanel;
            final albumFraction = state.visuals.coverLayout ? .35 : .4;
            final gutter = wide
                ? (state.visuals.coverLayout && showAlbum
                      ? math.min(box.maxWidth * .045, 100.0)
                      : box.maxWidth * .1)
                : (box.maxWidth * .04).clamp(16.0, 24.0);
            Widget header(double idleProgress) => CallbackShortcuts(
              bindings: MioDeviceScope.isTv(context)
                  ? {
                      const SingleActivator(LogicalKeyboardKey.arrowLeft): () =>
                          _moveHeaderFocus(-1),
                      const SingleActivator(
                        LogicalKeyboardKey.arrowRight,
                      ): () =>
                          _moveHeaderFocus(1),
                    }
                  : {},
              child: SizedBox(
                height: wide ? 45 : 44,
                child: Row(
                  children: [
                    _headerButton(wide),
                    const Spacer(),
                    if (!wide) _headerButton(false, switchMode: true),
                    if (wide && MioDeviceScope.isTv(context))
                      _tvHeaderActions(),
                  ],
                ),
              ),
            );
            final player = ValueListenableBuilder<bool>(
              valueListenable: MioWideAutoHide.value,
              builder: (context, autoHide, _) => PlayerIdleFrame(
                // Both layouts hide the bars, but only when the player style
                // panel turns it on (off by default).
                wide: true,
                autoHide: autoHide,
                reducedMotion:
                    policy.reduced || MediaQuery.disableAnimationsOf(context),
                suspended:
                    !_foreground ||
                    _presentingStyle ||
                    _presentingComments ||
                    _presentingSinger ||
                    _downloadBusy,
                onHide: () {
                  if (_volumeOpen) setState(() => _volumeOpen = false);
                },
                builder: (context, hidden, idleProgress) => DefaultTextStyle(
                  style: const TextStyle(
                    color: Color(0xffeeeeee),
                    fontSize: 16,
                  ),
                  child: ColoredBox(
                    color: const Color(0xff19232b),
                    child: Stack(
                      children: [
                        if (policy.background)
                          Positioned.fill(
                            child: PlayerBackground(
                              cover: _cover,
                              identity: _identity(state),
                              config: state.backgroundRendering,
                              lowFrequency: () => _backgroundBass,
                              onUnavailable: () {
                                if (mounted && !_backgroundFailed) {
                                  setState(() => _backgroundFailed = true);
                                }
                              },
                            ),
                          ),
                        const Positioned.fill(
                          child: IgnorePointer(
                            child: ColoredBox(color: Color(0x41000000)),
                          ),
                        ),
                        Positioned.fill(
                          right: widget.trailingInset,
                          child: Stack(
                            children: [
                              Positioned(
                                left: 0,
                                right: 0,
                                bottom:
                                    (tv
                                            ? _tvDeckHeight
                                            : (contentHeight * .1).clamp(
                                                70.0,
                                                80.0,
                                              )) *
                                        (1 - idleProgress) +
                                    // The safe-area gap is only kept while the bar is shown.
                                    insets.bottom * (1 - idleProgress) -
                                    (wide ? 10 : 0),
                                child: PlayerSpectrum(
                                  generation: state.playbackGeneration,
                                  playing: state.status == 'playing',
                                  wide: wide,
                                  // A lighter tint of the theme colour reads better over the blurred cover.
                                  accent: Color.lerp(
                                    p.brand,
                                    const Color(0xffffffff),
                                    .4,
                                  )!,
                                  backgroundResponse:
                                      policy.background &&
                                      !policy.reduced &&
                                      _cover != null &&
                                      !_backgroundFailed &&
                                      state.backgroundRendering.audioResponse &&
                                      !state.backgroundRendering.staticMode &&
                                      !MediaQuery.disableAnimationsOf(
                                        context,
                                      ) &&
                                      ImageFilter.isShaderFilterSupported,
                                  onLowFrequency: (value) =>
                                      _backgroundBass = value,
                                ),
                              ),
                              if (wide) ...[
                                if (insets.bottom > 0)
                                  Positioned(
                                    left: 0,
                                    right: 0,
                                    bottom: 0,
                                    height: insets.bottom,
                                    child: IgnorePointer(
                                      child: Opacity(
                                        opacity: (1 - idleProgress).clamp(
                                          0.0,
                                          1.0,
                                        ),
                                        // Same veil as the transport bar above it.
                                        child: const ColoredBox(
                                          color: Color(0x20000000),
                                        ),
                                      ),
                                    ),
                                  ),
                                Positioned.fill(
                                  bottom: insets.bottom,
                                  child: LayoutBuilder(
                                    builder: (context, inner) =>
                                        (tv ? _tvLayout : _wideLayout)(
                                          inner,
                                          gutter,
                                          albumFraction,
                                          showAlbum,
                                          lyricFont,
                                          header(idleProgress),
                                          hidden,
                                          idleProgress,
                                        ),
                                  ),
                                ),
                              ] else
                                Positioned.fill(
                                  top: insets.top,
                                  bottom: insets.bottom,
                                  child: LayoutBuilder(
                                    builder: (context, contentBox) =>
                                        _narrowLayout(
                                          contentBox,
                                          header(0),
                                          lyricFont,
                                          hidden,
                                          idleProgress,
                                        ),
                                  ),
                                ),
                              // TV carries the style button in its control deck.
                              if (!tv)
                                Positioned(
                                  right: wide
                                      ? 20
                                      : (box.maxWidth * .04).clamp(16.0, 24.0),
                                  bottom: wide
                                      ? 100 + insets.bottom
                                      : (contentHeight * .1).clamp(70.0, 80.0) +
                                            insets.bottom +
                                            12,
                                  child: _idleChrome(
                                    _button(
                                      'PenBall',
                                      p.text('play.playerTheme'),
                                      () => unawaited(_openStyle()),
                                      size: wide ? 50 : 44,
                                      iconSize: 20,
                                      focusNode: _styleLauncher,
                                      background: Color(
                                        wide ? 0x26ffffff : 0x38000000,
                                      ),
                                      outlined: !wide,
                                    ),
                                    hidden,
                                    idleProgress,
                                    shift: const Offset(0, 20),
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            );
            // This region covers the actual system insets, so removing the
            // player immediately restores the underlying page's system style.
            return wide
                ? player
                : AnnotatedRegion<SystemUiOverlayStyle>(
                    value: SystemUiOverlayStyle.light.copyWith(
                      statusBarColor: const Color(0x00000000),
                      systemNavigationBarColor: const Color(0x00000000),
                      systemNavigationBarDividerColor: const Color(0x00000000),
                      systemStatusBarContrastEnforced: false,
                      systemNavigationBarContrastEnforced: false,
                    ),
                    child: player,
                  );
          },
        ),
      ),
    ),
  );
}

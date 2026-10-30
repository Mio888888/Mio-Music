part of '../full_player.dart';

/// Height of the TV control deck at the bottom of the full player.
const _tvDeckHeight = 178.0;

/// Console-style full player layout for TV: the same artwork, lyrics, header,
/// volume panel and dialogs as the wide layout, with a larger control deck.
extension _TvFullPlayer on _FullPlayerState {
  Widget _tvLayout(
    BoxConstraints box,
    double gutter,
    double albumFraction,
    bool showAlbum,
    double lyricFont,
    Widget header,
    bool hidden,
    double idleProgress,
  ) {
    final bottom = hidden ? 0.0 : _tvDeckHeight;
    // `_album` reserves 70 at the bottom itself; give it the rest.
    final extra = _tvDeckHeight - 70;
    return Stack(
      children: [
        Positioned.fill(
          child: Padding(
            padding: EdgeInsets.only(left: gutter, right: gutter * .35),
            child: Row(
              children: [
                if (showAlbum)
                  SizedBox(
                    width: (box.maxWidth - gutter * 2) * albumFraction,
                    child: Padding(
                      padding: EdgeInsets.only(bottom: hidden ? 0 : extra),
                      child: _album(
                        (box.maxWidth - gutter * 2) * albumFraction,
                        box.maxHeight - (hidden ? 0 : extra),
                        true,
                        contentWidth: box.maxWidth,
                      ),
                    ),
                  ),
                Expanded(
                  child: Padding(
                    padding: EdgeInsets.only(
                      top: 56,
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
          top: 20,
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
            bottom: bottom + 6,
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
        Positioned(
          key: const ValueKey('player-transport'),
          left: 0,
          right: 0,
          bottom: 0,
          child: _idleChrome(
            _tvDeck(),
            hidden,
            idleProgress *
                (1 + (widget.systemInsets.bottom + 24) / _tvDeckHeight),
            slide: true,
          ),
        ),
        // Drawn after the deck so it covers the progress bar beneath it.
        if (_volumeOpen)
          Positioned(
            key: const ValueKey('player-volume'),
            // Above the volume button, which sits left of Previous.
            left: 200,
            bottom: 84,
            child: CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.arrowDown):
                    _closeVolume,
                const SingleActivator(LogicalKeyboardKey.escape): _closeVolume,
              },
              child: Container(
                width: 200,
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: const Color(0xff202a33),
                  borderRadius: BorderRadius.circular(14),
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
                      size: 36,
                    ),
                  ],
                ),
              ),
            ),
          ),
      ],
    );
  }

  Widget _tvDeck() {
    final usable = state.current != null && state.status != 'loading';
    final playing = state.status == 'playing';
    final online = state.current?.id.startsWith('mio-online:') ?? false;
    final iconColor = p.color('--player-text-idle');
    return SizedBox(
      height: _tvDeckHeight,
      child: DecoratedBox(
        decoration: const BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [Color(0x00000000), Color(0xb3000000)],
          ),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(32, 0, 32, 10),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Expanded(
                    // The artwork panel already names the song; the deck only
                    // does when that panel is hidden.
                    child: state.visuals.showLeftPanel
                        ? const SizedBox.shrink()
                        : Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                state.current?.title ?? '',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontSize: 22,
                                  height: 1.25,
                                  fontWeight: FontWeight.w700,
                                  color: Color(0xffffffff),
                                ),
                              ),
                              Text(
                                state.current?.artist ?? '',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontSize: 14,
                                  height: 1.4,
                                  color: Color(0xccffffff),
                                ),
                              ),
                            ],
                          ),
                  ),
                  ValueListenableBuilder(
                    valueListenable: widget.core.playback,
                    builder: (context, clock, _) => Text(
                      '${_time(clock?.position ?? state.position)} / ${_time(clock?.duration ?? state.duration)}',
                      style: const TextStyle(
                        fontSize: 14,
                        color: Color(0xccffffff),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              ValueListenableBuilder(
                valueListenable: widget.core.playback,
                builder: (context, clock, _) => MioSeekBar(
                  key: ValueKey(_identity(state)),
                  palette: p,
                  showThumb: true,
                  height: 22,
                  value: clock?.position ?? state.position,
                  maximum: clock?.duration ?? state.duration,
                  semanticLabel: p.english ? 'Progress' : '播放进度',
                  onChanged: usable && _session != null
                      ? (value) {
                          final session = _session!;
                          _command(() => session.seek(seconds: value));
                        }
                      : null,
                ),
              ),
              const SizedBox(height: 4),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      children: [
                        _TvPlayerButton(
                          palette: p,
                          icon: _favorite == true
                              ? 'TD-Heart-Filled'
                              : 'TD-Heart',
                          label: _favoriteFailed
                              ? (p.english
                                    ? 'Retry favorite operation'
                                    : '重试收藏操作')
                              : p.text(
                                  _favorite == true
                                      ? 'play.liked'
                                      : 'play.like',
                                ),
                          onPressed: _favoriteBusy || _session == null
                              ? null
                              : () => unawaited(_toggleFavorite()),
                        ),
                        if (online)
                          _TvPlayerButton(
                            palette: p,
                            icon: 'Download',
                            label: p.text('download.title'),
                            onPressed: _downloadBusy || _session == null
                                ? null
                                : () => unawaited(_download()),
                          ),
                        _TvPlayerButton(
                          palette: p,
                          icon: playModeIcon(state.playMode),
                          label: p.text(playModeKey(state.playMode)),
                          onPressed: () => _command(() => api.cyclePlayMode()),
                        ),
                        _TvPlayerButton(
                          palette: p,
                          icon: 'nav-shengyin',
                          label: p.english ? 'Volume' : '音量',
                          focusNode: _volumeLauncher,
                          iconColor: iconColor,
                          onPressed: _toggleVolume,
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 12),
                  _TvPlayerButton(
                    palette: p,
                    icon: 'nav-shangyishou',
                    label: p.text('common.previous'),
                    iconColor: iconColor,
                    onPressed: () => _command(() => api.step(forward: false)),
                  ),
                  _TvPlayerButton(
                    palette: p,
                    icon: playing ? 'nav-zanting' : 'nav-bofang',
                    label: p.text(playing ? 'common.pause' : 'common.play'),
                    big: true,
                    iconColor: p.color('--player-text-hover-idle'),
                    background: state.coverTheme != null
                        ? const Color(0x33ffffff)
                        : p.color('--player-btn-bg-idle'),
                    autofocus: true,
                    onPressed: usable
                        ? () => _command(() => api.setPaused(paused: playing))
                        : null,
                  ),
                  _TvPlayerButton(
                    palette: p,
                    icon: 'nav-xiayishou',
                    label: p.text('common.next'),
                    iconColor: iconColor,
                    onPressed: () => _command(() => api.step(forward: true)),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.start,
                      children: [
                        _TvPlayerButton(
                          palette: p,
                          icon: 'Message',
                          label: p.english ? 'Comments' : '评论',
                          onPressed: _foreground && !_presentingComments
                              ? () => unawaited(_openComments())
                              : null,
                        ),
                        _TvPlayerButton(
                          palette: p,
                          icon: 'nav-liebiao',
                          label: p.english ? 'Play queue' : '播放队列',
                          iconColor: iconColor,
                          onPressed: widget.onQueue,
                        ),
                        _TvPlayerButton(
                          palette: p,
                          icon: 'PenBall',
                          label: p.text('play.playerTheme'),
                          focusNode: _styleLauncher,
                          onPressed: () => unawaited(_openStyle()),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// Lyric display toggles and the style shortcut as one row of chips; a lit
  /// chip with a check is on, so the state reads at a glance.
  Widget _tvHeaderActions() {
    const white = Color(0xffffffff);
    Widget chip(
      String label,
      FocusNode node,
      VoidCallback action, {
      bool? on,
      String? icon,
    }) => Padding(
      padding: const EdgeInsets.only(left: 8),
      child: Semantics(
        toggled: on,
        child: MioButton(
          palette: p,
          label: label,
          focusNode: node,
          radius: 999,
          insetFocusBorder: false,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          background: on == true
              ? const Color(0x52ffffff)
              : const Color(0x24ffffff),
          onPressed: action,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (on == true || icon != null) ...[
                MioIcon(
                  on == true ? 'TD-Check' : icon!,
                  size: 16,
                  color: white,
                ),
                const SizedBox(width: 6),
              ],
              Text(
                label,
                style: TextStyle(
                  fontSize: 15,
                  height: 1.2,
                  fontWeight: on == true ? FontWeight.w700 : FontWeight.w500,
                  color: on == false ? const Color(0xccffffff) : white,
                ),
              ),
            ],
          ),
        ),
      ),
    );
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        chip(
          p.english ? 'Translation' : '翻译',
          _headerNodes[1],
          _toggleTranslation,
          on: _translation,
        ),
        chip(
          p.english ? 'Romanization' : '音译',
          _headerNodes[2],
          _toggleRomanization,
          on: _romanization,
        ),
        chip(
          p.english ? 'Follow' : '跟随',
          _headerNodes[3],
          () => _follow(force: true),
          icon: 'Lyrics',
        ),
        chip(
          p.english ? 'Style' : '外观',
          _headerNodes[4],
          widget.onAppearance,
          icon: 'Settings',
        ),
      ],
    );
  }
}

/// Round control of the TV deck; its name appears below while it has focus.
class _TvPlayerButton extends StatefulWidget {
  final MioPalette palette;
  final String icon, label;
  final VoidCallback? onPressed;
  final FocusNode? focusNode;
  final Color? iconColor, background;
  final bool big, autofocus;
  const _TvPlayerButton({
    required this.palette,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.focusNode,
    this.iconColor,
    this.background,
    this.big = false,
    this.autofocus = false,
  });
  @override
  State<_TvPlayerButton> createState() => _TvPlayerButtonState();
}

class _TvPlayerButtonState extends State<_TvPlayerButton> {
  FocusNode? _own;
  FocusNode get _node =>
      widget.focusNode ?? (_own ??= FocusNode(debugLabel: widget.label));
  @override
  void initState() {
    super.initState();
    _node.addListener(_changed);
  }

  @override
  void didUpdateWidget(_TvPlayerButton old) {
    super.didUpdateWidget(old);
    if (old.focusNode != widget.focusNode) {
      (old.focusNode ?? _own)?.removeListener(_changed);
      _node.addListener(_changed);
    }
  }

  @override
  void dispose() {
    _node.removeListener(_changed);
    _own?.dispose();
    super.dispose();
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final size = widget.big ? 58.0 : 42.0;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 3),
      child: SizedBox(
        width: 64,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(height: widget.big ? 0 : 8),
            SizedBox.square(
              dimension: size,
              child: MioButton(
                palette: widget.palette,
                label: widget.label,
                radius: size / 2,
                padding: EdgeInsets.zero,
                focusNode: _node,
                autofocus: widget.autofocus,
                background: widget.background ?? const Color(0x1affffff),
                onPressed: widget.onPressed,
                child: Center(
                  child: MioIcon(
                    widget.icon,
                    color: widget.iconColor ?? const Color(0xffeeeeee),
                    size: widget.big ? 28 : 20,
                  ),
                ),
              ),
            ),
            const SizedBox(height: 4),
            SizedBox(
              height: 16,
              child: AnimatedOpacity(
                opacity: _node.hasFocus ? 1 : 0,
                duration: const Duration(milliseconds: 120),
                child: Text(
                  widget.label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    fontSize: 11,
                    height: 1.3,
                    color: Color(0xffffffff),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

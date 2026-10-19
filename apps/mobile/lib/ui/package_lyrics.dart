import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_lyric/flutter_lyric.dart' as lyric;

import '../src/rust/api.dart' as api;
import 'lyric_style.dart';
import 'device.dart';
import 'visual_policy.dart';

/// The Rust clock already includes the lyric offset. Interpolate only a fresh
/// sample; never create a second audio clock or apply the offset a second time.
class _CoreProgress implements lyric.LyricPlaybackStatusSource {
  final _changes = StreamController<void>.broadcast(sync: true);
  final _age = Stopwatch();
  api.LyricClock? _clock;
  bool enabled = true;
  bool synced = true;
  void update(
    api.LyricClock? clock, {
    required bool enabled,
    required bool synced,
  }) {
    if (!identical(clock, _clock)) {
      _clock = clock;
      _age
        ..reset()
        ..start();
    }
    this.enabled = enabled;
    this.synced = synced;
    _changes.add(null);
  }

  @override
  Duration get position {
    if (!synced) return const Duration(microseconds: -1);
    final clock = _clock;
    if (clock == null) return Duration.zero;
    final elapsed = enabled && clock.playing
        ? _age.elapsedMilliseconds.clamp(0, 250)
        : 0;
    return Duration(
      microseconds: ((clock.positionMs + elapsed) * 1000).round(),
    );
  }

  @override
  bool get isPlaying => _clock?.playing == true;

  @override
  bool get isAdvancing =>
      enabled &&
      synced &&
      _clock?.playing == true &&
      _age.elapsedMilliseconds < 250;
  @override
  Stream<void> get changes => _changes.stream;
  void dispose() {
    _age.stop();
    unawaited(_changes.close());
  }
}

class MioPackageLyrics extends StatefulWidget {
  final api.LyricDocument document;
  final api.LyricClock? clock;
  final api.LyricStyleSettings style;
  final double fontSize;
  final double horizontalPadding;
  final Color color;
  final bool centered;
  final double anchor;
  final bool translation, romanization;

  /// Keeps each row on one line and scrolls the active row when it overflows.
  final bool singleLine;
  final ValueChanged<int> onSeek;
  const MioPackageLyrics({
    super.key,
    required this.document,
    required this.clock,
    required this.style,
    required this.fontSize,
    required this.horizontalPadding,
    required this.color,
    required this.centered,
    required this.anchor,
    required this.translation,
    required this.romanization,
    this.singleLine = false,
    required this.onSeek,
  });
  @override
  State<MioPackageLyrics> createState() => MioPackageLyricsState();
}

class MioPackageLyricsState extends State<MioPackageLyrics>
    with WidgetsBindingObserver {
  final _progress = _CoreProgress();
  late final _controller = lyric.LyricController(progressSource: _progress);
  final _rowIndices = <lyric.LyricLine, int>{};
  final _focus = FocusNode(debugLabel: 'Mio lyrics');
  bool _hasFocus = false;
  bool _foreground = true;
  int? _selected;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    _loadDocument();
  }

  Duration _time(double value) =>
      Duration(microseconds: (value * 1000).round());
  void _loadDocument() {
    _selected = null;
    _rowIndices.clear();
    final lines = <lyric.LyricLine>[];
    for (var i = 0; i < widget.document.rows.length; i++) {
      final row = widget.document.rows[i];
      final hasWordRoman = row.words.any((w) => w.romanization.isNotEmpty);
      final roman = row.romanization ?? '';
      final auxiliary = [
        if (widget.translation && row.translation?.isNotEmpty == true)
          row.translation!,
        if (widget.romanization && !hasWordRoman && roman.isNotEmpty) roman,
      ];
      final line = lyric.LyricLine(
        start: row.startMs == null
            ? Duration(microseconds: i)
            : _time(row.startMs!),
        end: row.endMs == null ? null : _time(row.endMs!),
        text: row.text,
        background: row.background,
        textAlign: row.duet ? TextAlign.right : null,
        translationBlocks: auxiliary.isEmpty ? null : auxiliary,
        words: row.words.isEmpty
            ? null
            : [
                for (final word in row.words)
                  lyric.LyricWord(
                    text: word.text,
                    start: _time(word.startMs),
                    end: _time(word.endMs),
                    romanization: widget.romanization ? word.romanization : '',
                  ),
              ],
      );
      lines.add(line);
      _rowIndices[line] = i;
    }
    _controller.loadDocument(
      lyric.LyricDocument(
        lines: lines,
        offset: Duration.zero,
        useLineIntervals: widget.document.synced,
      ),
    );
  }

  void _sync() => _progress.update(
    widget.clock,
    enabled: _foreground && TickerMode.valuesOf(context).enabled,
    synced: widget.document.synced,
  );
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(MioPackageLyrics oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.document, widget.document) ||
        oldWidget.translation != widget.translation ||
        oldWidget.romanization != widget.romanization) {
      _loadDocument();
    }
    _sync();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _sync();
    if (mounted) setState(() {});
  }

  void resumeFollow() {
    if (mounted && _foreground && widget.document.synced) {
      _selected = null;
      unawaited(
        _controller.resumeFollow(animate: MioVisualPolicy.of(context).animate),
      );
    }
  }

  void _seek(lyric.LyricLine line) {
    final index = _rowIndices[line];
    if (index != null &&
        widget.document.rows[index].seekSeconds != null &&
        _foreground) {
      widget.onSeek(index);
    }
  }

  bool _selectable(lyric.LyricLine line) {
    if (!line.background || !widget.document.synced || !_progress.isPlaying) {
      return true;
    }
    final now = _progress.position;
    return line.end == null
        ? identical(line, _controller.activeLine)
        : now >= line.start && now < line.end!;
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final lines = _controller.document!.lines;
    if (lines.isEmpty) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.arrowDown ||
        event.logicalKey == LogicalKeyboardKey.arrowUp) {
      final step = event.logicalKey == LogicalKeyboardKey.arrowDown ? 1 : -1;
      var next = (_selected ?? _controller.activeLineIndex ?? 0) + step;
      while (next >= 0 && next < lines.length && !_selectable(lines[next])) {
        next += step;
      }
      if (next < 0 || next >= lines.length) return KeyEventResult.handled;
      _selected = next;
      unawaited(
        _controller.scrollToLine(
          next,
          animate: MioVisualPolicy.of(context).animate,
        ),
      );
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.select) {
      final line =
          lines[(_selected ?? _controller.activeLineIndex ?? 0).clamp(
            0,
            lines.length - 1,
          )];
      if (_selectable(line)) _seek(line);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _controller.dispose();
    _progress.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final policy = MioVisualPolicy.of(context);
    final animate = policy.animate && widget.document.synced;
    final playing = widget.clock?.playing == true;
    final base = lyric.LyricPresets.appleMusic;
    final preset = base.copyWith(
      textStyle: lyricTextStyle(
        widget.style,
        widget.fontSize,
        color: widget.color.withValues(
          alpha: !widget.document.synced ? .85 : (playing ? .2 : .4),
        ),
        // macOS main.rs applies a fixed 0.28px advance per glyph at every size.
      ).copyWith(height: 1.2, letterSpacing: .28),
      translationTextStyle: lyricTextStyle(
        widget.style,
        widget.fontSize * .5,
        color: widget.color.withValues(alpha: .38),
      ).copyWith(height: 1.2, letterSpacing: .28),
      translationSpacing: 4,
      linePadding: widget.fontSize * .5,
      wrapAtWords: true,
      // Plain (untimed) lyrics have no progress to scroll with, so they wrap.
      singleLine: widget.singleLine && widget.document.synced,
      lineSpacing: 0,
      activeTranslationTextStyle: const TextStyle(),
      textAlign: widget.centered ? TextAlign.center : TextAlign.start,
      contentAlignment: widget.centered
          ? lyric.LyricContentAlignment.center
          : lyric.LyricContentAlignment.start,
      // Desktop buffers timed foreground text at .85 opacity. Its unfilled
      // mask is .4; inactive rows use .2 playing and .4 paused.
      activeTextStyle: TextStyle(
        color: widget.color.withValues(
          alpha: widget.document.synced ? .34 : .85,
        ),
      ),
      padding: EdgeInsets.symmetric(horizontal: widget.horizontalPadding),
      activeAnchor: lyric.LyricAnchor.fraction(
        widget.anchor,
        target: lyric.LyricAnchorTarget.groupCenter,
      ),
      selectionAnchor: lyric.LyricAnchor.fraction(widget.anchor),
      resumeDelay: const Duration(seconds: 5),
      resumeOnTap: false,
      countdown: widget.document.synced && !policy.reduced
          ? base.countdown.copyWith(color: widget.color)
          : const lyric.LyricCountdown.none(),
      highlight: widget.document.synced
          ? lyric.LyricHighlight.solid(
              widget.color.withValues(alpha: .85),
              tailFadeFontSizeFactor: .5,
              replaceColor: true,
            )
          : const lyric.LyricHighlight.none(),
      edgeFade: policy.reduced
          ? const lyric.LyricEdgeFade.none()
          : base.edgeFade,
      scrollAnimation: animate
          ? base.scrollAnimation
          : const lyric.LyricScrollAnimation.fixed(duration: Duration.zero),
      lineTransition: animate
          ? lyric.LyricLineTransition.effects([
              lyric.LyricLineEffect.scale(playing ? .97 : 1),
            ])
          : const lyric.LyricLineTransition.none(),
      lineMotion: animate ? base.lineMotion : const lyric.LyricLineMotion(),
      // Only real core word timing drives per-word effects; no simulated timing.
      wordEffects: animate
          ? lyric.LyricWordEffects(lift: 2.5, floatAmplitude: playing ? 1.4 : 0)
          : const lyric.LyricWordEffects(),
      distanceBlur: policy.blurLyrics
          ? base.distanceBlur
          : const lyric.LyricDistanceBlur(),
    );
    return TickerMode(
      enabled: _foreground && TickerMode.valuesOf(context).enabled,
      child: Focus(
        focusNode: _focus,
        onFocusChange: (value) {
          if (mounted) setState(() => _hasFocus = value);
        },
        onKeyEvent: _key,
        child: DecoratedBox(
          decoration: BoxDecoration(
            border: Border.all(
              width: 2,
              color: _hasFocus && MioDeviceScope.isTv(context)
                  ? widget.color.withValues(alpha: .6)
                  : const Color(0x00000000),
            ),
          ),
          child: lyric.LyricView(
            controller: _controller,
            preset: preset,
            onLineTap: (details) => _seek(details.line),
            onSelectionChanged: (details) => _selected = details?.index,
          ),
        ),
      ),
    );
  }
}

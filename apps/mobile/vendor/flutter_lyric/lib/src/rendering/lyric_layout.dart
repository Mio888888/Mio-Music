import 'dart:math' as math;
import 'package:flutter/rendering.dart';
import '../model/lyric_document.dart';
import '../preset/lyric_effects.dart';
import 'line_metrics.dart';
import 'resolved_preset.dart';
import 'lyric_word_layout.dart';
import 'lyric_ruby_painter.dart';
import 'lyric_auxiliary_painter.dart';

/// Geometry and paragraph ownership for one view. Active-only layouts allocate
/// one entry; scrollable layouts measure normal paragraphs and defer highlights.
class LyricLayout {
  final List<LineMetrics> metrics;
  ResolvedPreset style;
  final Size viewSize;
  final int firstIndex;
  final Map<int, Duration?> _ends;

  /// The visible countdown row before its next lyric. Hidden countdowns do not
  /// contribute to lyric positions or content height.
  List<int> countdownIndices = const [];
  double countdownExtent = 0;
  List<double>? _startsCache;
  List<double> get _starts => _startsCache ??= _computeStarts();
  Set<int> _hidden = {};
  int? _visibilityBucket;
  bool _playing = false;
  int _visibilityActive = -1;
  late final bool _openBackground =
      metrics.any((m) => m.line.background && m.line.end == null);
  late final List<Duration> _backgroundBoundaries = ([
    for (final metric in metrics)
      if (metric.line.background) ...[
        metric.line.start,
        if (metric.line.end != null) metric.line.end!,
      ],
  ]..sort());

  static TextStyle lineStyle(TextStyle style, LyricLine line) =>
      line.background ? style.apply(fontSizeFactor: .7) : style;

  int _bucket(Duration position) {
    var low = 0;
    var high = _backgroundBoundaries.length;
    while (low < high) {
      final mid = low + ((high - low) >> 1);
      if (_backgroundBoundaries[mid] <= position) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low;
  }

  bool visibilityNeedsUpdate(bool playing, Duration position, int active) =>
      _backgroundBoundaries.isNotEmpty &&
      (_visibilityBucket == null ||
          (playing && _openBackground && active != _visibilityActive) ||
          _playing != playing ||
          (playing && _visibilityBucket != _bucket(position)));

  /// Rebuild height prefixes only on transport/interval boundaries, never at
  /// every sampled frame. Hidden rows contribute neither height nor spacing.
  void updateVisibility(bool playing, Duration position, int active) {
    if (!visibilityNeedsUpdate(playing, position, active)) return;
    _playing = playing;
    _visibilityActive = active;
    _visibilityBucket = _bucket(position);
    _hidden = {
      for (var i = 0; i < metrics.length; i++)
        if (playing &&
            metrics[i].line.background &&
            !(metrics[i].line.end == null
                ? i + firstIndex == active
                : position >= metrics[i].line.start &&
                    position < metrics[i].line.end!))
          i + firstIndex,
    };
    _startsCache = null;
  }

  bool isVisible(int index) => !_hidden.contains(index);
  double spacingAfter(int index) => !isVisible(index)
      ? 0
      : style.preset.lineSpacing * (metric(index).line.background ? .7 : 1);

  LyricLayout._(
      this.metrics, this.style, this.viewSize, this.firstIndex, this._ends);

  factory LyricLayout.compute(
      LyricDocument document, ResolvedPreset style, Size viewSize,
      {int? onlyIndex}) {
    final lines =
        onlyIndex == null ? document.lines : [document.lines[onlyIndex]];
    final width = math.max(0.0, viewSize.width - style.padding.horizontal);
    // Single-line rows are measured at their natural width.
    final measureWidth = style.preset.singleLine ? double.infinity : width;
    final fixed = style.preset.layoutMode == LyricLayoutMode.fixed;
    TextPainter painter(String? text, TextStyle textStyle, LyricLine line,
        {bool primary = false}) {
      final span = text == null
          ? null
          : TextSpan(text: text, style: lineStyle(textStyle, line));
      final result = primary
          ? lyricTextPainter(
              words: line.words,
              wrapAtWords: style.preset.wrapAtWords,
              text: span,
              textAlign: line.textAlign ?? style.preset.textAlign,
              textDirection: style.direction,
              textScaleFactor: style.textScaleFactor,
              locale: style.locale,
            )
          : lyricAuxiliaryPainter(
              text: span,
              blocks: line.translationBlocks,
              wrapAtWords: style.preset.wrapAtWords,
              blockSpacing: style.preset.translationSpacing,
              textAlign: line.textAlign ?? style.preset.textAlign,
              textDirection: style.direction,
              textScaleFactor: style.textScaleFactor,
              locale: style.locale,
            );
      // Empty primary entries still occupy one line (e.g. an instrumental gap).
      if (text != null) {
        result.layout(maxWidth: measureWidth);
      }
      return result;
    }

    final ends = <int, Duration?>{};
    Duration? next;
    if (onlyIndex != null) {
      var low = onlyIndex + 1;
      var high = document.lines.length;
      while (low < high) {
        final middle = low + ((high - low) >> 1);
        if (document.lines[middle].start <= document.lines[onlyIndex].start) {
          low = middle + 1;
        } else {
          high = middle;
        }
      }
      if (low < document.lines.length) next = document.lines[low].start;
    }
    for (var i = onlyIndex ?? document.lines.length - 1;
        i >= (onlyIndex ?? 0);
        i--) {
      if (i + 1 < document.lines.length &&
          document.lines[i + 1].start > document.lines[i].start) {
        next = document.lines[i + 1].start;
      }
      final line = document.lines[i];
      ends[i] = line.end != null && line.end! > line.start ? line.end : next;
    }
    return LyricLayout._(
        lines
            .map((line) => LineMetrics.lazy(
                  line: line,
                  wrapAtWords: style.preset.wrapAtWords,
                  textPainter: painter(line.text,
                      fixed ? style.activeTextStyle : style.textStyle, line,
                      primary: true),
                  translationTextPainter: painter(
                      line.translation?.isEmpty == true
                          ? null
                          : line.translation,
                      fixed
                          ? style.activeTranslationTextStyle
                          : style.translationTextStyle,
                      line),
                  activeStyle: lineStyle(style.activeTextStyle, line),
                  activeTranslationStyle:
                      lineStyle(style.activeTranslationTextStyle, line),
                  translationBlockSpacing: style.preset.translationSpacing,
                  maxWidth: measureWidth,
                ))
            .toList(),
        style,
        viewSize,
        onlyIndex ?? 0,
        ends);
  }

  double get contentWidth =>
      math.max(0, viewSize.width - style.padding.horizontal);

  /// Width handed to paragraph layout; single-line rows keep natural width.
  double get measureWidth =>
      style.preset.singleLine ? double.infinity : contentWidth;

  /// Shared by paragraph painting, animated pivots and transformed hit regions.
  double contentOffsetX(int index, double width, {double? containerWidth}) {
    final available = (containerWidth ?? contentWidth) - width;
    // A row wider than the view starts at the leading edge and scrolls.
    if (style.preset.singleLine && available < 0) return 0;
    final align = metric(index).line.textAlign;
    if (align != null) {
      switch (align) {
        case TextAlign.left:
          return 0;
        case TextAlign.right:
          return available;
        case TextAlign.center:
          return available / 2;
        case TextAlign.start:
        case TextAlign.justify:
          return style.direction == TextDirection.ltr ? 0 : available;
        case TextAlign.end:
          return style.direction == TextDirection.ltr ? available : 0;
      }
    }
    switch (style.preset.contentAlignment) {
      case LyricContentAlignment.center:
        return available / 2;
      case LyricContentAlignment.start:
        return style.direction == TextDirection.ltr ? 0 : available;
      case LyricContentAlignment.end:
        return style.direction == TextDirection.ltr ? available : 0;
    }
  }

  /// Playback end of a row, used to pace single-line scrolling.
  Duration? endOf(int index) => _ends[index];
  int localIndex(int index) => index - firstIndex;
  bool contains(int index) =>
      index >= firstIndex && index < firstIndex + metrics.length;
  LineMetrics metric(int index) => metrics[localIndex(index)];
  List<double> _computeStarts() {
    final result = <double>[0];
    for (var i = 0; i < metrics.length; i++) {
      result.add(result.last +
          getLineHeight(false, i + firstIndex) +
          spacingAfter(i + firstIndex));
    }
    return result;
  }

  double _delta(int active) => !fixed && contains(active)
      ? getLineHeight(true, active) - getLineHeight(false, active)
      : 0;
  int _countdownsBefore(int index) {
    var low = 0;
    var high = countdownIndices.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (countdownIndices[middle] <= index) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  }

  double lineTop(int index, int active) =>
      _starts[localIndex(index)] +
      _countdownsBefore(index) * countdownExtent +
      (contains(active) && active < index ? _delta(active) : 0);

  double countdownTop(int nextIndex, int active) =>
      lineTop(nextIndex, active) - countdownExtent;
  late final double maxNormalHeight = [
    for (var i = firstIndex; i < firstIndex + metrics.length; i++)
      _measuredLineHeight(false, i),
  ].fold<double>(0, math.max);
  bool get fixed => style.preset.layoutMode == LyricLayoutMode.fixed;
  double get headroom => style.preset.wordEffects.extent;
  double textTop(int index) => style.preset.linePadding == null
      ? headroom
      : style.preset.linePadding! * (metric(index).line.background ? .7 : 1);
  double get translationMotionSpace => style.preset.linePadding == null
      ? style.preset.wordEffects.floatAmplitude
      : 0;
  LyricWordLine wordLine(int index, bool active) => metric(index).wordLine(
      ResolvedWordStyle(
          text: lineStyle(
              active || fixed ? style.activeTextStyle : style.textStyle,
              metric(index).line),
          translation: lineStyle(
              active || fixed
                  ? style.activeTranslationTextStyle
                  : style.translationTextStyle,
              metric(index).line),
          direction: style.direction,
          textAlign: metric(index).line.textAlign ?? style.preset.textAlign,
          textScale: style.textScaleFactor,
          wrapAtWords: style.preset.wrapAtWords,
          locale: style.locale,
          effects: style.preset.wordEffects,
          translationSpacing: style.preset.translationSpacing),
      _ends[index],
      active || fixed);

  double getLineHeight(bool active, int index) {
    if (!contains(index) || !isVisible(index)) return 0;
    return _measuredLineHeight(active, index);
  }

  double _measuredLineHeight(bool active, int index) {
    final line = metric(index);
    final main = active ? line.activeHeight : line.height;
    final translation =
        active ? line.activeTranslationHeight : line.translationHeight;
    return (style.preset.linePadding == null
            ? headroom + translationMotionSpace
            : textTop(index) * 2) +
        main +
        (translation == 0 ? 0 : translation + style.preset.translationSpacing);
  }

  double anchorOffsetY(int index, bool active, LyricAnchorTarget target) {
    if (!isVisible(index)) return 0;
    final line = metric(index);
    if (target == LyricAnchorTarget.groupCenter && !line.line.background) {
      var extent = getLineHeight(active, index);
      var previous = index;
      for (var next = index + 1;
          contains(next) && metric(next).line.background;
          next++) {
        if (!isVisible(next)) continue;
        extent += spacingAfter(previous) + getLineHeight(false, next);
        previous = next;
      }
      // Treat each row's spacing as equal top/bottom padding. Background
      // vocals use smaller spacing, so the group's outer halves differ.
      return (extent + spacingAfter(previous) - spacingAfter(index)) / 2;
    }
    final main = active ? line.activeHeight : line.height;
    final translation =
        active ? line.activeTranslationHeight : line.translationHeight;
    if (translation == 0 || target == LyricAnchorTarget.mainText) {
      return textTop(index) + main / 2;
    }
    if (target == LyricAnchorTarget.translationText) {
      return textTop(index) +
          translationMotionSpace +
          main +
          style.preset.translationSpacing +
          translation / 2;
    }
    return getLineHeight(active, index) / 2;
  }

  double lineOffsetY(int index, int active, LyricAnchor anchor) =>
      lineTop(index, active) +
      anchorOffsetY(index, index == active, anchor.target) -
      anchor.resolve(viewSize.height);
  double contentHeight(int active) =>
      _starts.last +
      countdownIndices.length * countdownExtent +
      _delta(active) -
      (metrics.isEmpty ? 0 : _lastSpacing);

  double get _lastSpacing {
    for (var i = firstIndex + metrics.length - 1; i >= firstIndex; i--) {
      if (isVisible(i)) return spacingAfter(i);
    }
    return 0;
  }

  /// First intersecting entry, using a binary search over normal line prefixes.
  int lineAtOffset(double offset, int active, {double bottomExtension = 0}) {
    var low = firstIndex;
    var high = firstIndex + metrics.length;
    while (low < high) {
      final mid = low + ((high - low) >> 1);
      if (lineTop(mid, active) +
              getLineHeight(mid == active, mid) +
              bottomExtension <
          offset) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low;
  }

  int closestToAnchor(double scrollY, int active, LyricAnchor anchor) {
    final point = scrollY + anchor.resolve(viewSize.height);
    var low = firstIndex;
    var high = firstIndex + metrics.length;
    double center(int i) =>
        lineTop(i, active) + anchorOffsetY(i, i == active, anchor.target);
    while (low < high) {
      final mid = low + ((high - low) >> 1);
      if (center(mid) < point) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    if (!isVisible(low.clamp(firstIndex, firstIndex + metrics.length - 1))) {
      var before = low - 1;
      while (before >= firstIndex && !isVisible(before)) {
        before--;
      }
      var after = low;
      while (after < firstIndex + metrics.length && !isVisible(after)) {
        after++;
      }
      if (before < firstIndex) {
        return after.clamp(firstIndex, firstIndex + metrics.length - 1);
      }
      if (after == firstIndex + metrics.length) return before;
      return (point - center(before)).abs() <= (center(after) - point).abs()
          ? before
          : after;
    }
    if (low == firstIndex) return low;
    if (low == firstIndex + metrics.length) return low - 1;
    var previous = low - 1;
    while (previous >= firstIndex && !isVisible(previous)) {
      previous--;
    }
    return previous >= firstIndex &&
            point - center(previous) <= center(low) - point
        ? previous
        : low;
  }

  LyricLayout restyle(ResolvedPreset next, Size size) {
    if (!next.sameTextPaint(style)) {
      for (final line in metrics) {
        line.updatePaintStyles(
            textStyle: lineStyle(
                fixed ? next.activeTextStyle : next.textStyle, line.line),
            activeStyle: lineStyle(next.activeTextStyle, line.line),
            translationStyle: lineStyle(
                fixed
                    ? next.activeTranslationTextStyle
                    : next.translationTextStyle,
                line.line),
            activeTranslationStyle:
                lineStyle(next.activeTranslationTextStyle, line.line));
      }
    }
    if (fixed &&
        size == viewSize &&
        style.padding == next.padding &&
        style.preset.lineSpacing == next.preset.lineSpacing &&
        style.preset.linePadding == next.preset.linePadding &&
        style.preset.translationSpacing == next.preset.translationSpacing) {
      style = next;
      return this;
    }
    return LyricLayout._(metrics, next, size, firstIndex, _ends);
  }

  void dispose() {
    for (final line in metrics) {
      line.dispose();
    }
  }
}

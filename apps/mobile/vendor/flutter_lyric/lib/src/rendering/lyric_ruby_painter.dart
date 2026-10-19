import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';

import '../model/lyric_document.dart';
import '../compat/painting.dart';
import 'lyric_token_painter.dart';

/// Main paragraphs and masks must reserve identical annotation bands.
TextPainter lyricTextPainter({
  required InlineSpan? text,
  required TextAlign textAlign,
  required TextDirection? textDirection,
  required double textScaleFactor,
  Locale? locale,
  List<LyricWord>? words,
  bool wrapAtWords = false,
}) {
  if (words?.any((word) => word.romanization.isNotEmpty) == true) {
    return LyricRubyPainter(
      text: text,
      textAlign: textAlign,
      textDirection: textDirection,
      textScaleFactor: textScaleFactor,
      locale: locale,
      words: words!,
      wrapAtWords: wrapAtWords,
    );
  }
  if (wrapAtWords) {
    return LyricTokenPainter(
      text: text,
      textAlign: textAlign,
      textDirection: textDirection,
      textScaleFactor: textScaleFactor,
      locale: locale,
      tokens: words,
      wrapAtWords: true,
    );
  }
  return TextPainter(
    text: text,
    textAlign: textAlign,
    textDirection: textDirection,
    // ignore: deprecated_member_use
    textScaleFactor: textScaleFactor,
    locale: locale,
  );
}

class _RubyRun {
  final TextPainter painter;
  final Offset offset;
  final Duration? start, end;
  _RubyRun(this.painter, this.offset, this.start, this.end);
}

/// Adds a half-font annotation band after each shaped visual row, without
/// changing horizontal advances or wrapping. Selection boxes, line metrics,
/// main painting and highlight masks all use the expanded coordinates.
class LyricRubyPainter extends LyricTokenPainter {
  final List<LyricWord> words;
  final List<_RubyRun> _ruby = [];
  List<ui.LineMetrics> _rows = [];
  bool _rubyDirty = true;
  double? _minWidth, _maxWidth;
  double _band = 0, _fallbackHeight = 0;

  LyricRubyPainter({
    required InlineSpan? text,
    required TextAlign textAlign,
    required TextDirection? textDirection,
    required double textScaleFactor,
    Locale? locale,
    required this.words,
    bool wrapAtWords = false,
  }) : super(
          text: text,
          textAlign: textAlign,
          textDirection: textDirection,
          // ignore: deprecated_member_use
          textScaleFactor: textScaleFactor,
          locale: locale,
          tokens: words,
          wrapAtWords: wrapAtWords,
        );

  @override
  void markNeedsLayout() {
    _rubyDirty = true;
    super.markNeedsLayout();
  }

  @override
  void layout({double minWidth = 0, double maxWidth = double.infinity}) {
    super.layout(minWidth: minWidth, maxWidth: maxWidth);
    if (!_rubyDirty && minWidth == _minWidth && maxWidth == _maxWidth) return;
    _minWidth = minWidth;
    _maxWidth = maxWidth;
    for (final run in _ruby) {
      run.painter.dispose();
    }
    _ruby.clear();
    _rows = super.computeLineMetrics();
    _fallbackHeight = 0;
    final mainStyle = text?.style ?? const TextStyle();
    final fontSize = mainStyle.fontSize ?? 14;
    // ignore: deprecated_member_use
    _band = fontSize * textScaleFactor * .5;
    final content = text?.toPlainText() ?? '';
    var offset = 0;
    final fallback = <String>[];
    TextPainter annotation(String value, double ratio) => TextPainter(
          text: TextSpan(
            text: value,
            style: TextStyle(
              fontFamily: mainStyle.fontFamily,
              fontFamilyFallback: mainStyle.fontFamilyFallback,
              fontWeight: mainStyle.fontWeight,
              fontStyle: mainStyle.fontStyle,
              fontVariations: mainStyle.fontVariations,
              letterSpacing: mainStyle.letterSpacing,
              fontSize: fontSize * .5 * ratio,
              height: 1,
              color: const Color(0xFFFFFFFF),
            ),
          ),
          textDirection: textDirection,
          textAlign: TextAlign.center,
          // ignore: deprecated_member_use
          textScaleFactor: textScaleFactor,
          locale: locale,
        );
    for (final word in words) {
      if (word.text.isEmpty) continue;
      final start = content.indexOf(word.text, offset);
      if (start < 0) continue;
      offset = start + word.text.length;
      if (word.romanization.isEmpty) continue;
      final boxes = super.getBoxesForSelection(
          TextSelection(baseOffset: start, extentOffset: offset));
      if (boxes.isEmpty || _rows.isEmpty) continue;
      final row = _rowFor(boxes.first);
      if (boxes.any((box) => _rowFor(box) != row)) {
        fallback.add(word.romanization);
        continue;
      }
      final left = boxes.map((box) => box.left).reduce(math.min);
      final right = boxes.map((box) => box.right).reduce(math.max);
      // Ruby never expands its word or overflows into the adjacent word.
      // ignore: deprecated_member_use
      final available = math.max(0.0, right - left - _band * .16);
      if (available <= 0) continue;
      var ruby = annotation(word.romanization, 1)..layout();
      if (ruby.width > available) {
        final ratio = available / ruby.width;
        ruby.dispose();
        ruby = annotation(word.romanization, ratio)..layout();
      }
      _ruby.add(_RubyRun(
        ruby,
        Offset(left + (right - left - ruby.width) / 2,
            _rows[row].baseline + _rows[row].descent + row * _band),
        word.start,
        word.end,
      ));
    }
    // Desktop renders annotations of split oversized tokens as static line
    // auxiliaries (player_lyrics.rs fallback_roman), without fabricated timing.
    if (fallback.isNotEmpty) {
      final ruby = annotation(fallback.join(' '), 1);
      ruby.textAlign = textAlign;
      ruby.layout(maxWidth: super.width);
      _ruby.add(_RubyRun(
          ruby, Offset(0, super.height + _rows.length * _band), null, null));
      _fallbackHeight = ruby.height;
    }
    _rubyDirty = false;
  }

  int _rowFor(ui.TextBox box) {
    var closest = 0;
    var distance = double.infinity;
    final middle = (box.top + box.bottom) / 2;
    for (var i = 0; i < _rows.length; i++) {
      final row = _rows[i];
      final next =
          (middle - (row.baseline + (row.descent - row.ascent) / 2)).abs();
      if (next < distance) {
        distance = next;
        closest = i;
      }
    }
    return closest;
  }

  @override
  double get height => super.height + _rows.length * _band + _fallbackHeight;
  @override
  Size get size => Size(width, height);

  @override
  List<ui.LineMetrics> computeLineMetrics() => [
        for (var i = 0; i < _rows.length; i++)
          ui.LineMetrics(
            hardBreak: _rows[i].hardBreak,
            ascent: _rows[i].ascent,
            descent: _rows[i].descent,
            unscaledAscent: _rows[i].unscaledAscent,
            height: _rows[i].height,
            width: _rows[i].width,
            left: _rows[i].left,
            baseline: _rows[i].baseline + i * _band,
            lineNumber: _rows[i].lineNumber,
          ),
      ];

  @override
  List<ui.TextBox> getBoxesForSelection(TextSelection selection,
          {ui.BoxHeightStyle boxHeightStyle = ui.BoxHeightStyle.tight,
          ui.BoxWidthStyle boxWidthStyle = ui.BoxWidthStyle.tight}) =>
      [
        for (final box in super.getBoxesForSelection(selection,
            boxHeightStyle: boxHeightStyle, boxWidthStyle: boxWidthStyle))
          ui.TextBox.fromLTRBD(box.left, box.top + _rowFor(box) * _band,
              box.right, box.bottom + _rowFor(box) * _band, box.direction),
      ];

  @override
  void paint(Canvas canvas, Offset offset) {
    if (_rows.isEmpty) return;
    final pad = preferredLineHeight;
    for (var i = 0; i < _rows.length; i++) {
      final top = i == 0 ? -pad : _rows[i].baseline - _rows[i].ascent;
      final bottom = i == _rows.length - 1
          ? super.height + pad
          : _rows[i + 1].baseline - _rows[i + 1].ascent;
      canvas.save();
      canvas.translate(0, i * _band);
      canvas.clipRect(
          Rect.fromLTRB(-pad, top, width + pad, bottom).shift(offset));
      super.paint(canvas, offset);
      canvas.restore();
    }
  }

  void paintRuby(Canvas canvas, Duration position, Color base, Color? highlight,
      double activity) {
    for (final run in _ruby) {
      final start = run.start, end = run.end;
      final progress = start == null || end == null || end <= start
          ? 0.0
          : ((position - start).inMicroseconds / (end - start).inMicroseconds)
              .clamp(0.0, 1.0);
      final color = Color.lerp(base, highlight ?? base, progress * activity)!;
      final bounds = (run.offset & run.painter.size).inflate(1);
      canvas.saveLayer(
          bounds,
          Paint()
            ..colorFilter = ColorFilter.mode(
                colorWithOpacity(color, colorOpacity(color) * .8),
                BlendMode.srcIn));
      run.painter.paint(canvas, run.offset);
      canvas.restore();
    }
  }

  @override
  void dispose() {
    for (final run in _ruby) {
      run.painter.dispose();
    }
    _ruby.clear();
    super.dispose();
  }
}

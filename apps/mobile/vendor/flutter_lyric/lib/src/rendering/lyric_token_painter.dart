import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';

import '../model/lyric_document.dart';

class _Token {
  final TextPainter painter;
  final int start, end;
  Offset offset = Offset.zero;
  _Token(this.painter, this.start, this.end);
}

class _TokenRow {
  final int first, end;
  final double top, width;
  final bool hardBreak;
  double left = 0;
  _TokenRow(this.first, this.end, this.top, this.width, this.hardBreak);
}

/// Optional desktop-style greedy wrapping. Timed words are indivisible until
/// one exceeds the viewport; untimed text splits after each whitespace. Each
/// token keeps its original UTF-16 range for masks, ruby and selection geometry.
class LyricTokenPainter extends TextPainter {
  final bool wrapAtWords;
  final List<LyricWord>? tokens;
  final List<_Token> _tokens = [];
  final List<_TokenRow> _rows = [];
  InlineSpan? _lastText;
  bool _dirty = true, _usesTokens = false;
  double _minWidth = 0, _maxWidth = double.infinity;
  double _width = 0, _height = 0, _rowHeight = 0;

  LyricTokenPainter({
    required InlineSpan? text,
    required TextAlign textAlign,
    required TextDirection? textDirection,
    required double textScaleFactor,
    Locale? locale,
    this.tokens,
    this.wrapAtWords = false,
  }) : super(
            text: text,
            textAlign: textAlign,
            textDirection: textDirection,
            // ignore: deprecated_member_use
            textScaleFactor: textScaleFactor,
            locale: locale);

  @override
  void markNeedsLayout() {
    _dirty = true;
    super.markNeedsLayout();
  }

  @override
  void layout({double minWidth = 0, double maxWidth = double.infinity}) {
    super.layout(minWidth: minWidth, maxWidth: maxWidth);
    if (!_dirty &&
        identical(text, _lastText) &&
        minWidth == _minWidth &&
        maxWidth == _maxWidth) {
      return;
    }
    _minWidth = minWidth;
    _maxWidth = maxWidth;
    for (final token in _tokens) {
      token.painter.dispose();
    }
    _tokens.clear();
    _rows.clear();
    final span = text;
    // Rich spans retain Flutter's paragraph shaping. The application supplies
    // a single style per lyric, including separate main/auxiliary paragraphs.
    _usesTokens = wrapAtWords &&
        maxWidth.isFinite &&
        span is TextSpan &&
        (span.children?.isEmpty ?? true);
    if (_usesTokens) {
      final simpleSpan = span as TextSpan;
      final content = simpleSpan.toPlainText();
      _width = maxWidth;
      final style = simpleSpan.style;
      // Desktop uses the explicit fractional line height. TextPainter rounds
      // preferredLineHeight, accumulating drift across wrapped lyric rows.
      final explicitHeight =
          // ignore: deprecated_member_use
          (style?.fontSize ?? 0) * (style?.height ?? 0) * textScaleFactor;
      _rowHeight = explicitHeight.isFinite && explicitHeight > 0
          ? explicitHeight
          : super.preferredLineHeight;
      final ranges = <TextRange>[];
      void untimed(int start, int end) {
        var first = start;
        for (var i = start; i < end;) {
          final rune = content.runeAt(i);
          final next = i + (rune > 0xffff ? 2 : 1);
          if (String.fromCharCode(rune).trim().isEmpty) {
            ranges.add(TextRange(start: first, end: next));
            first = next;
          }
          i = next;
        }
        if (first < end) ranges.add(TextRange(start: first, end: end));
      }

      var cursor = 0;
      for (final word in tokens ?? const <LyricWord>[]) {
        if (word.text.isEmpty) continue;
        final start = content.indexOf(word.text, cursor);
        if (start < 0) continue;
        untimed(cursor, start);
        cursor = start + word.text.length;
        ranges.add(TextRange(start: start, end: cursor));
      }
      untimed(cursor, content.length);
      TextPainter painter(int start, int end) => TextPainter(
            text: TextSpan(
                text: content.substring(start, end), style: simpleSpan.style),
            textDirection: textDirection,
            textAlign: TextAlign.left,
            // ignore: deprecated_member_use
            textScaleFactor: textScaleFactor,
            locale: locale,
          )..layout();
      var x = 0.0, y = 0.0, first = 0;
      void row(bool hardBreak) {
        _rows.add(_TokenRow(first, _tokens.length, y, x, hardBreak));
        y += _rowHeight;
        first = _tokens.length;
        x = 0;
      }

      void add(int start, int end, TextPainter p) {
        if (x > 0 && x + p.width > maxWidth) row(false);
        final token = _Token(p, start, end)..offset = Offset(x, y);
        _tokens.add(token);
        x += p.width;
      }

      for (final range in ranges) {
        final start = range.start, end = range.end;
        // Explicit line breaks retain their source offsets but have no glyphs.
        var segment = start;
        for (var i = start; i <= end; i++) {
          if (i != end && content.codeUnitAt(i) != 10) continue;
          if (segment < i) {
            final p = painter(segment, i);
            if (p.width > maxWidth) {
              p.dispose();
              for (var j = segment; j < i;) {
                final next = j + (content.runeAt(j) > 0xffff ? 2 : 1);
                add(j, next, painter(j, next));
                j = next;
              }
            } else {
              add(segment, i, p);
            }
          }
          if (i < end) row(true);
          segment = i + 1;
        }
      }
      row(true);
      _height = y;
      final align = textAlign == TextAlign.start
          ? (textDirection == TextDirection.rtl
              ? TextAlign.right
              : TextAlign.left)
          : textAlign == TextAlign.end
              ? (textDirection == TextDirection.rtl
                  ? TextAlign.left
                  : TextAlign.right)
              : textAlign;
      for (final row in _rows) {
        row.left = align == TextAlign.right
            ? _width - row.width
            : align == TextAlign.center
                ? (_width - row.width) / 2
                : 0;
        for (var i = row.first; i < row.end; i++) {
          _tokens[i].offset += Offset(row.left, 0);
        }
      }
    }
    _dirty = false;
    _lastText = text;
  }

  @override
  double get width => _usesTokens ? _width : super.width;
  @override
  double get height => _usesTokens ? _height : super.height;
  @override
  Size get size => Size(width, height);

  @override
  List<ui.LineMetrics> computeLineMetrics() {
    if (!_usesTokens) return super.computeLineMetrics();
    final template = _tokens.isEmpty
        ? null
        : _tokens.first.painter.computeLineMetrics().first;
    final ascent = template?.ascent ?? _rowHeight;
    final descent = template?.descent ?? 0;
    return [
      for (var i = 0; i < _rows.length; i++)
        ui.LineMetrics(
          hardBreak: _rows[i].hardBreak,
          ascent: ascent,
          descent: descent,
          unscaledAscent: template?.unscaledAscent ?? ascent,
          height: _rowHeight,
          width: _rows[i].width,
          left: _rows[i].left,
          baseline: _rows[i].top + ascent,
          lineNumber: i,
        )
    ];
  }

  @override
  List<ui.TextBox> getBoxesForSelection(
    TextSelection selection, {
    ui.BoxHeightStyle boxHeightStyle = ui.BoxHeightStyle.tight,
    ui.BoxWidthStyle boxWidthStyle = ui.BoxWidthStyle.tight,
  }) {
    if (!_usesTokens) {
      return super.getBoxesForSelection(selection,
          boxHeightStyle: boxHeightStyle, boxWidthStyle: boxWidthStyle);
    }
    return [
      for (final token in _tokens)
        if (selection.start < token.end && selection.end > token.start)
          for (final box in token.painter.getBoxesForSelection(
              TextSelection(
                baseOffset:
                    math.max(selection.start, token.start) - token.start,
                extentOffset: math.min(selection.end, token.end) - token.start,
              ),
              boxHeightStyle: boxHeightStyle,
              boxWidthStyle: boxWidthStyle))
            ui.TextBox.fromLTRBD(
                box.left + token.offset.dx,
                box.top + token.offset.dy,
                box.right + token.offset.dx,
                box.bottom + token.offset.dy,
                box.direction),
    ];
  }

  @override
  TextPosition getPositionForOffset(Offset offset) {
    if (!_usesTokens) return super.getPositionForOffset(offset);
    if (_tokens.isEmpty) return const TextPosition(offset: 0);
    final row =
        _rows[(offset.dy / _rowHeight).floor().clamp(0, _rows.length - 1)];
    if (row.first == row.end) {
      return TextPosition(
          offset: row.first < _tokens.length
              ? _tokens[row.first].start
              : text!.toPlainText().length);
    }
    var nearest = _tokens[row.first], distance = double.infinity;
    for (var i = row.first; i < row.end; i++) {
      final t = _tokens[i];
      final dx = offset.dx - t.offset.dx;
      final d = dx < 0 ? -dx : math.max(0.0, dx - t.painter.width);
      if (d < distance) {
        nearest = t;
        distance = d;
      }
    }
    final p = nearest.painter.getPositionForOffset(offset - nearest.offset);
    return TextPosition(offset: nearest.start + p.offset, affinity: p.affinity);
  }

  @override
  void paint(Canvas canvas, Offset offset) {
    if (_dirty || !identical(_lastText, text)) {
      layout(minWidth: _minWidth, maxWidth: _maxWidth);
    }
    if (!_usesTokens) {
      super.paint(canvas, offset);
      return;
    }
    for (final token in _tokens) {
      token.painter.paint(canvas, offset + token.offset);
    }
  }

  @override
  void dispose() {
    for (final token in _tokens) {
      token.painter.dispose();
    }
    _tokens.clear();
    super.dispose();
  }
}

extension on String {
  int runeAt(int index) {
    final first = codeUnitAt(index);
    if (first < 0xd800 || first > 0xdbff || index + 1 >= length) return first;
    final second = codeUnitAt(index + 1);
    if (second < 0xdc00 || second > 0xdfff) return first;
    return 0x10000 + ((first - 0xd800) << 10) + second - 0xdc00;
  }
}

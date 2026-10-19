import 'dart:math' as math;

import 'package:flutter/painting.dart';

import '../compat/painting.dart';
import 'lyric_token_painter.dart';

TextPainter lyricAuxiliaryPainter({
  required InlineSpan? text,
  required List<String>? blocks,
  required double blockSpacing,
  required TextAlign textAlign,
  required TextDirection? textDirection,
  required double textScaleFactor,
  Locale? locale,
  bool wrapAtWords = false,
}) =>
    blocks != null && blocks.length > 1
        ? LyricAuxiliaryPainter(
            blocks: blocks,
            wrapAtWords: wrapAtWords,
            blockSpacing: blockSpacing,
            text: text,
            textAlign: textAlign,
            textDirection: textDirection,
            textScaleFactor: textScaleFactor,
            locale: locale,
          )
        : LyricTokenPainter(
            wrapAtWords: wrapAtWords,
            text: text,
            textAlign: textAlign,
            textDirection: textDirection,
            // ignore: deprecated_member_use
            textScaleFactor: textScaleFactor,
            locale: locale,
          );

/// Static auxiliary paragraphs share a width/alignment, with spacing only
/// between blocks. Wrapped visual lines keep the paragraph's normal leading.
class LyricAuxiliaryPainter extends TextPainter {
  final List<String> blocks;
  final double blockSpacing;
  final bool wrapAtWords;
  final List<TextPainter> _painters = [];
  InlineSpan? _lastText;
  bool _dirty = true;
  double _minWidth = 0, _maxWidth = double.infinity;
  double _width = 0, _height = 0;

  LyricAuxiliaryPainter({
    required this.blocks,
    required this.blockSpacing,
    this.wrapAtWords = false,
    required InlineSpan? text,
    required TextAlign textAlign,
    required TextDirection? textDirection,
    required double textScaleFactor,
    Locale? locale,
  }) : super(
          text: text,
          textAlign: textAlign,
          textDirection: textDirection,
          // ignore: deprecated_member_use
          textScaleFactor: textScaleFactor,
          locale: locale,
        );

  @override
  void markNeedsLayout() {
    _dirty = true;
    super.markNeedsLayout();
  }

  @override
  void layout({double minWidth = 0, double maxWidth = double.infinity}) {
    super.layout(minWidth: minWidth, maxWidth: maxWidth);
    if (!_dirty &&
        identical(_lastText, text) &&
        minWidth == _minWidth &&
        maxWidth == _maxWidth) {
      return;
    }
    _minWidth = minWidth;
    _maxWidth = maxWidth;
    while (_painters.length < blocks.length) {
      _painters.add(LyricTokenPainter(
          wrapAtWords: wrapAtWords,
          text: null,
          textAlign: textAlign,
          textDirection: textDirection,
          // ignore: deprecated_member_use
          textScaleFactor: textScaleFactor,
          locale: locale));
    }
    _width = minWidth;
    _height = 0;
    for (var i = 0; i < blocks.length; i++) {
      final painter = _painters[i]
        ..text = TextSpan(text: blocks[i], style: text?.style)
        ..textDirection = textDirection
        ..textAlign = textAlign
        // ignore: deprecated_member_use
        ..textScaleFactor = textScaleFactor
        ..locale = locale;
      painter.layout(minWidth: minWidth, maxWidth: maxWidth);
      _width = math.max(_width, painter.width);
    }
    // Align short blocks within the widest block, including absolute duet
    // alignment and RTL start/end. This does not change their wrapping.
    for (var i = 0; i < _painters.length; i++) {
      final painter = _painters[i];
      painter.layout(minWidth: _width, maxWidth: maxWidth);
      _height += painter.height + (i == 0 ? 0 : blockSpacing);
    }
    _lastText = text;
    _dirty = false;
  }

  @override
  double get width => _width;
  @override
  double get height => _height;
  @override
  Size get size => Size(width, height);

  @override
  void paint(Canvas canvas, Offset offset) {
    // Modern Flutter can retain layout on color-only span changes. Propagate
    // those to the child paragraphs even when the caller skips layout().
    if (_dirty || !identical(_lastText, text)) {
      layout(minWidth: _minWidth, maxWidth: _maxWidth);
    }
    var y = offset.dy;
    for (final painter in _painters) {
      painter.paint(canvas, Offset(offset.dx, y));
      y += painter.height + blockSpacing;
    }
  }

  @override
  void dispose() {
    for (final painter in _painters) {
      disposeTextPainter(painter);
    }
    _painters.clear();
    super.dispose();
  }
}

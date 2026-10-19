import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;

/// Legacy desktop token uses the same bundled variable font on mobile.
TextStyle lyricTextStyle(
  api.LyricStyleSettings value,
  double size, {
  Color? color,
}) {
  final families = value.family
      .split(',')
      .map((v) => v.trim())
      .where((v) => v.isNotEmpty)
      .map((v) => v == 'lyricfont' ? 'miolyrics' : v)
      .toList(growable: false);
  return TextStyle(
    fontFamily: families.isEmpty ? 'miolyrics' : families.first,
    fontFamilyFallback: families.skip(1).toList(growable: false),
    fontSize: size,
    fontWeight: FontWeight.values[(value.weight / 100).round().clamp(1, 9) - 1],
    fontVariations: [FontVariation('wght', value.weight.toDouble())],
    height: 1.3,
    color: color,
  );
}

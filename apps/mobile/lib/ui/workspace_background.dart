import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'resources.dart';

Color mioCompactSettingsHeaderBackground(
  MioPalette palette, {
  required bool songTheme,
}) {
  final top = songTheme
      ? palette.color('--td-bg-color-secondarycontainer')
      : palette.dark
      ? const Color(0xf5181818)
      : const Color(0xf0ffffff);
  return Color.alphaBlend(
    palette.color('--mobile-glass-bg-strong'),
    Color.alphaBlend(top, palette.color('--td-bg-color-page')),
  );
}

Color mioPlayerTint(
  MioPalette palette, {
  required bool wide,
  required bool active,
  api.CoverTheme? coverTheme,
}) {
  if (coverTheme == null) {
    return palette.color(active ? '--player-bg-default' : '--player-bg-idle');
  }
  int channel(int value) => coverTheme.useBlackText
      ? (value + (255 - value) * .78).round()
      : (value * (wide ? .65 : .35)).round();
  return Color.fromARGB(
    224,
    channel(coverTheme.red),
    channel(coverTheme.green),
    channel(coverTheme.blue),
  );
}

/// HomeLayout uses viewport-relative stops, including a stop above the window.
/// Project the visible segment because Flutter requires stops within 0..1.
LinearGradient mioWorkspaceBackground(
  MioPalette palette, {
  required bool wide,
  required bool songTheme,
  required double viewportHeight,
  required double areaHeight,
  bool sidebar = false,
}) {
  if (!wide) {
    return LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: songTheme
          ? [
              palette.color('--td-bg-color-secondarycontainer'),
              palette.color('--td-bg-color-page'),
            ]
          : palette.dark
          ? [const Color(0xf5181818), const Color(0xff111111)]
          : [const Color(0xf0ffffff), const Color(0xfff5f7f8)],
    );
  }
  final start = sidebar ? -1.4 : -1.1;
  final end = sidebar ? .3 : .15;
  final ratio = math.max(1.0, viewportHeight) / math.max(1.0, areaHeight);
  final lastStop = math.min(1.0, end * ratio);
  final first = palette.color('--td-brand-color-4');
  final last = palette.surface;
  Color at(double position) =>
      Color.lerp(first, last, (position / ratio - start) / (end - start))!;
  return LinearGradient(
    begin: Alignment.topCenter,
    end: Alignment.bottomCenter,
    colors: [at(0), at(lastStop)],
    stops: [0, lastStop],
  );
}

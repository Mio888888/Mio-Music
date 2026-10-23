import 'dart:convert';

import '../src/rust/api.dart' as api;

import 'package:flutter/services.dart';

class MobileAssets {
  final Map<String, Map<String, String>> themes;
  final Map<String, dynamic> chinese;
  final Map<String, dynamic> english;
  final List<Offset> welcomePoints;
  api.CoverTheme? _cachedTheme;
  Map<String, Color> _cachedThemeColors = const {};
  Map<String, Color> songColors(api.CoverTheme? theme) {
    if (theme == null) return const {};
    if (theme != _cachedTheme) {
      _cachedThemeColors = Map.unmodifiable({
        for (final token in api.coverThemeColors(theme: theme))
          token.key: Color(token.argb),
      });
      _cachedTheme = theme;
    }
    return _cachedThemeColors;
  }

  MobileAssets(this.themes, this.chinese, this.english, this.welcomePoints);
  static Future<MobileAssets> load() async {
    final data = await Future.wait([
      rootBundle.loadString('assets/tokens.json'),
      rootBundle.loadString('assets/locales/zh-CN.json'),
      rootBundle.loadString('assets/locales/en-US.json'),
      rootBundle.loadString('assets/welcome/points.json'),
    ]);
    final tokens = jsonDecode(data[0]) as Map<String, dynamic>;
    return MobileAssets(
      (tokens['themes'] as Map<String, dynamic>).map(
        (key, value) => MapEntry(key, Map<String, String>.from(value as Map)),
      ),
      jsonDecode(data[1]) as Map<String, dynamic>,
      jsonDecode(data[2]) as Map<String, dynamic>,
      (jsonDecode(data[3]) as List)
          .map((value) {
            final pair = value as List;
            return Offset(
              (pair[0] as num).toDouble(),
              (pair[1] as num).toDouble(),
            );
          })
          .toList(growable: false),
    );
  }
}

class MioPalette {
  final MobileAssets assets;
  final bool dark;
  final bool english;
  final Map<String, Color> _colors = {};
  final Map<String, Color> _songColors;
  MioPalette(
    this.assets, {
    required this.dark,
    required this.english,
    api.CoverTheme? coverTheme,
  }) : _songColors = assets.songColors(coverTheme);
  final Map<String, String> _texts = {};
  String text(String key) => _texts.putIfAbsent(key, () {
    dynamic value = english ? assets.english : assets.chinese;
    for (final segment in key.split('.')) {
      if (value is! Map || !value.containsKey(segment)) return key;
      value = value[segment];
    }
    return value is String ? value : key;
  });

  String resolve(String key) {
    final visited = <String>{};
    final theme = assets.themes[dark ? 'dark' : 'light']!;
    while (visited.add(key)) {
      final value = theme[key];
      if (value == null) throw FormatException('Missing theme token: $key');
      if (value.startsWith('var(') && value.endsWith(')')) {
        final alias = value.substring(4, value.length - 1).trim();
        if (alias.startsWith('--') && !alias.contains(',')) {
          key = alias;
          continue;
        }
      }
      return value;
    }
    throw FormatException('Theme token cycle: $key');
  }

  Color? _dynamicColor(String key) {
    if (_songColors.isEmpty) return null;
    final theme = assets.themes[dark ? 'dark' : 'light']!;
    final visited = <String>{};
    while (visited.add(key)) {
      final color = _songColors[key];
      if (color != null) return color;
      final value = theme[key];
      if (value == null || !value.startsWith('var(') || !value.endsWith(')')) {
        return null;
      }
      final alias = value.substring(4, value.length - 1).trim();
      if (!alias.startsWith('--') || alias.contains(',')) return null;
      key = alias;
    }
    // Static resolution below retains the existing invalid-token diagnostics.
    return null;
  }

  /// Cover-derived accents and the label drawn on brand fills arrive from the
  /// shared core already adjusted for contrast (`song_theme.rs`), as on desktop.
  /// Both the cover-derived and the static answer are fixed for this palette.
  Color color(String key) =>
      _colors.putIfAbsent(key, () => _dynamicColor(key) ?? _staticColor(key));

  Color _staticColor(String key) {
    final value = resolve(key);
    if (value == 'transparent') return const Color(0x00000000);
    if (value.startsWith('#')) {
      var hex = value.substring(1);
      if (hex.length == 3 || hex.length == 4) {
        hex = hex.split('').map((c) => '$c$c').join();
      }
      if (hex.length == 6) {
        return Color(0xff000000 | int.parse(hex, radix: 16));
      }
      if (hex.length == 8) {
        final rgba = int.parse(hex, radix: 16);
        return Color((rgba >> 8) | ((rgba & 0xff) << 24));
      }
    }
    final match = RegExp(r'^rgba?\(([^)]+)\)$').firstMatch(value);
    if (match != null) {
      final parts = match[1]!.split(',').map((part) => part.trim()).toList();
      if (parts.length == 3 || parts.length == 4) {
        final alpha = parts.length == 4 ? parts[3] : '1';
        return Color.fromRGBO(
          int.parse(parts[0]),
          int.parse(parts[1]),
          int.parse(parts[2]),
          alpha.endsWith('%')
              ? double.parse(alpha.substring(0, alpha.length - 1)) / 100
              : double.parse(alpha),
        );
      }
    }
    throw FormatException('Unsupported color: $key = $value');
  }

  Color get brand => color('--td-brand-color');
  Color get onBrand {
    final background = Color.alphaBlend(brand, surface);
    final backgroundLuminance = background.computeLuminance();
    double contrast(Color foreground) {
      final luminance = Color.alphaBlend(
        foreground,
        background,
      ).computeLuminance();
      return luminance > backgroundLuminance
          ? (luminance + .05) / (backgroundLuminance + .05)
          : (backgroundLuminance + .05) / (luminance + .05);
    }

    final preferred = color('--td-text-color-anti');
    if (contrast(preferred) >= 4.5) return preferred;
    const dark = Color(0xff111111);
    const light = Color(0xffffffff);
    return contrast(dark) >= contrast(light) ? dark : light;
  }

  Color get surface => color('--td-bg-color-container');
  Color get secondary => color('--td-text-color-secondary');
  Color get border => color('--td-border-level-1-color');
  static double rem(double width) =>
      width <= 768 ? 16 : (width * .014).clamp(15, 18);
  static double gutter(double width) => (width * .04).clamp(16, 24);
}

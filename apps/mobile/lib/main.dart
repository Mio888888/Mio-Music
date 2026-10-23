import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'core_controller.dart';
import 'src/rust/api.dart' as api;
import 'ui/resources.dart';
import 'ui/device.dart';
import 'ui/visual_policy.dart';
import 'ui/shell.dart';
import 'ui/welcome.dart';
import 'ui/workspace_background.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const MioApp());
}

class MioApp extends StatefulWidget {
  const MioApp({super.key});
  @override
  State<MioApp> createState() => _MioAppState();
}

class _MioAppState extends State<MioApp> {
  final _core = CoreController();
  final _assets = MobileAssets.load();
  bool _welcome = true;
  @override
  void initState() {
    super.initState();
    unawaited(_core.connect());
  }

  @override
  void dispose() {
    _core.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Mio Music',
    // Raw dialog routes are siblings of home in the Navigator overlay. Keep
    // the application text style above that overlay so every route inherits it.
    builder: (context, child) => FutureBuilder<MobileAssets>(
      future: _assets,
      builder: (context, snapshot) => ListenableBuilder(
        listenable: _core,
        builder: (context, _) {
          final palette = snapshot.hasData
              ? _palette(context, snapshot.data!)
              : null;
          final background = _systemBackground(context, palette);
          final brightness = ThemeData.estimateBrightnessForColor(background);
          final footerBackground = palette == null || _welcome
              ? background
              : Color.alphaBlend(
                  mioPlayerTint(
                    palette,
                    wide:
                        _core.capabilities?.television == true ||
                        MediaQuery.sizeOf(context).width > 768,
                    active: _core.player?.current != null,
                    coverTheme: _core.player?.coverTheme,
                  ),
                  palette.color('--td-bg-color-page'),
                );
          final footerBrightness = ThemeData.estimateBrightnessForColor(
            footerBackground,
          );
          // Paint behind SafeArea too: Android edge-to-edge ignores the native
          // statusBarColor. Keep this above the Navigator for modal routes.
          return AnnotatedRegion<SystemUiOverlayStyle>(
            value: SystemUiOverlayStyle(
              statusBarColor: background,
              statusBarBrightness: brightness,
              statusBarIconBrightness: brightness == Brightness.dark
                  ? Brightness.light
                  : Brightness.dark,
              systemStatusBarContrastEnforced: false,
              systemNavigationBarColor: footerBackground,
              systemNavigationBarDividerColor: Colors.transparent,
              systemNavigationBarIconBrightness:
                  footerBrightness == Brightness.dark
                  ? Brightness.light
                  : Brightness.dark,
              systemNavigationBarContrastEnforced: false,
            ),
            child: ColoredBox(
              color: background,
              child: DefaultTextStyle(
                style: _textStyle(palette),
                child: MioDeviceScope(
                  capabilities: _core.capabilities,
                  child: MioOrientationPolicy(
                    child: MioVisualPolicy(
                      settings: _core.player?.visuals,
                      child: child ?? const SizedBox.shrink(),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    ),
    debugShowCheckedModeBanner: false,
    supportedLocales: const [Locale('zh', 'CN'), Locale('en', 'US')],
    home: FutureBuilder<MobileAssets>(
      future: _assets,
      builder: (context, snapshot) {
        if (!snapshot.hasData) {
          return ColoredBox(
            color: const Color(0xffeef3f0),
            child: Center(
              child: Text(snapshot.hasError ? '无法加载界面资源' : 'Mio Music'),
            ),
          );
        }
        return ListenableBuilder(
          listenable: _core,
          builder: (context, _) {
            final palette = _palette(context, snapshot.data!);
            return SafeArea(
              // The shell paints its footer through the bottom system inset.
              bottom: _welcome,
              right:
                  _welcome || MioDeviceScope.rightNavigationInset(context) == 0,
              minimum: _core.capabilities?.television == true
                  ? EdgeInsets.fromLTRB(24, 16, 24, _welcome ? 16 : 0)
                  : EdgeInsets.zero,
              child: _welcome
                  ? WelcomePage(
                      palette: palette,
                      onFinish: () => setState(() => _welcome = false),
                    )
                  : MioShell(core: _core, palette: palette),
            );
          },
        );
      },
    ),
  );

  Color _systemBackground(BuildContext context, MioPalette? palette) {
    if (palette == null) return const Color(0xffeef3f0);
    if (_welcome) {
      return Color(palette.dark ? 0xff0d0d0d : 0xffeef3f0);
    }
    final viewport = MediaQuery.sizeOf(context);
    final gradient = mioWorkspaceBackground(
      palette,
      wide: _core.capabilities?.television == true || viewport.width > 768,
      songTheme: _core.player?.coverTheme != null,
      viewportHeight: viewport.height,
      areaHeight: viewport.height,
    );
    return Color.alphaBlend(
      gradient.colors.first,
      palette.color('--td-bg-color-page'),
    );
  }

  // One palette per appearance: rebuilding it would drop its resolved-colour cache.
  (MobileAssets, api.CoverTheme?, bool, bool)? _paletteKey;
  MioPalette? _paletteValue;
  MioPalette _palette(BuildContext context, MobileAssets assets) {
    final key = (
      assets,
      _core.player?.coverTheme,
      _core.player?.dark ?? false,
      _core.player?.english ??
          Localizations.localeOf(context).languageCode != 'zh',
    );
    if (_paletteKey != key || _paletteValue == null) {
      _paletteKey = key;
      _paletteValue = MioPalette(
        assets,
        coverTheme: key.$2,
        dark: key.$3,
        english: key.$4,
      );
    }
    return _paletteValue!;
  }

  TextStyle _textStyle(MioPalette? palette) => TextStyle(
    color: palette?.color('--td-text-color-primary') ?? const Color(0xff181818),
    fontFamily: 'PingFang SC',
    fontFamilyFallback: const ['sans-serif'],
    fontSize: 14,
    decoration: TextDecoration.none,
  );
}

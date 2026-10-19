import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/widgets.dart';

import 'controls.dart';
import '../core_controller.dart';
import 'playback_settings.dart';
import 'device.dart';
import 'resources.dart';
import 'visual_policy.dart';
import 'visual_settings.dart';

class PlayerStyleDialog extends StatefulWidget {
  final MioPalette palette;
  final CoreController core;
  const PlayerStyleDialog({
    super.key,
    required this.palette,
    required this.core,
  });

  @override
  State<PlayerStyleDialog> createState() => _PlayerStyleDialogState();
}

class _PlayerStyleDialogState extends State<PlayerStyleDialog>
    with WidgetsBindingObserver {
  final _scroll = ScrollController();
  double? _resumeOffset;
  bool _visualLoaded = false, _playbackLoaded = false;
  MioPalette get palette => widget.palette;
  CoreController get core => widget.core;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if ((state == AppLifecycleState.inactive ||
            state == AppLifecycleState.paused) &&
        _resumeOffset == null &&
        _scroll.hasClients) {
      _resumeOffset = _scroll.offset;
      _visualLoaded = _playbackLoaded = false;
    }
  }

  void _loaded(bool visual) {
    if (visual) {
      _visualLoaded = true;
    } else {
      _playbackLoaded = true;
    }
    if (_resumeOffset == null || !_visualLoaded || !_playbackLoaded) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scroll.hasClients || _resumeOffset == null) return;
      final offset = _resumeOffset!;
      _resumeOffset = null;
      _scroll.jumpTo(offset.clamp(0.0, _scroll.position.maxScrollExtent));
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SafeArea(
    minimum: MioDeviceScope.isTv(context)
        ? const EdgeInsets.symmetric(horizontal: 24, vertical: 16)
        : EdgeInsets.zero,
    child: LayoutBuilder(
      builder: (context, box) {
        final wide = MioDeviceScope.wide(context, box.maxWidth);
        final footer = (box.maxHeight * .1).clamp(70.0, 80.0);
        final gutter = wide ? 20.0 : (box.maxWidth * .04).clamp(16.0, 24.0);
        final button = wide ? 50.0 : 44.0;
        // Matches the style button in the full player (raised 40px above desktop).
        final lift = wide ? 40.0 : 0.0;
        final bottom = (wide ? 60.0 : footer + 12) + lift;
        final gap = wide ? 70.0 : button + 10;
        final width = wide ? 340.0 : box.maxWidth - gutter * 2;
        final height = math.max(
          80.0,
          wide
              // Wide panels grow up to just below the top row of buttons.
              ? box.maxHeight - bottom - gap - 72
              : box.maxHeight - footer - 120,
        );
        final radius = wide ? 24.0 : 22.0;
        final reduced = MioVisualPolicy.of(context).reduced;
        return DefaultTextStyle(
          style: const TextStyle(color: Color(0xffeeeeee), fontSize: 14),
          child: MioFocusPanel(
            child: Stack(
              children: [
                Positioned(
                  right: gutter,
                  bottom: bottom + gap,
                  width: width,
                  height: height,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(radius),
                      boxShadow: const [
                        BoxShadow(
                          color: Color(0x66000000),
                          offset: Offset(0, 6),
                          blurRadius: 12,
                        ),
                        BoxShadow(color: Color(0x1affffff), spreadRadius: 1),
                      ],
                    ),
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(radius),
                      child: BackdropFilter(
                        enabled: !reduced,
                        filter: ImageFilter.blur(sigmaX: 6, sigmaY: 6),
                        child: ColoredBox(
                          color: Color(reduced ? 0xff1e1e1e : 0x4a1e1e1e),
                          child: Padding(
                            padding: EdgeInsets.all(wide ? 20 : 16),
                            child: SingleChildScrollView(
                              controller: _scroll,
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  VisualSettingsPage(
                                    palette: palette,
                                    active: true,
                                    ready: true,
                                    playerPanel: true,
                                    onPanelLoaded: () => _loaded(true),
                                  ),
                                  PlaybackSettingsView(
                                    core: core,
                                    palette: palette,
                                    active: true,
                                    ready: true,
                                    viewportWidth: box.maxWidth,
                                    playerPanel: true,
                                    onPanelLoaded: () => _loaded(false),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                Positioned(
                  right: gutter,
                  bottom: bottom,
                  width: button,
                  height: button,
                  child: MioButton(
                    palette: palette,
                    label: palette.english ? 'Close player style' : '收起播放器样式',
                    onPressed: () => Navigator.of(context).pop(),
                    radius: button / 2,
                    padding: EdgeInsets.zero,
                    background: Color(wide ? 0x26ffffff : 0x38000000),
                    child: const Center(
                      child: MioIcon(
                        'PenBall',
                        color: Color(0xe6ffffff),
                        size: 20,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    ),
  );
}

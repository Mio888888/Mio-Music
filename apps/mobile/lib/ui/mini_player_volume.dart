import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

class MiniPlayerVolume extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active;
  const MiniPlayerVolume({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
  });
  @override
  State<MiniPlayerVolume> createState() => _MiniPlayerVolumeState();
}

class _MiniPlayerVolumeState extends State<MiniPlayerVolume>
    with WidgetsBindingObserver {
  final _launcher = FocusNode(debugLabel: 'Mini player volume');
  RawDialogRoute<void>? _route;
  bool _foreground = true;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
  }

  void _remove() {
    final route = _route;
    _route = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
      WidgetsBinding.instance.ensureVisualUpdate();
    }
  }

  @override
  void didUpdateWidget(MiniPlayerVolume old) {
    super.didUpdateWidget(old);
    if (!widget.active) _remove();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _remove();
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
    }
  }

  Future<void> _open() async {
    if (!widget.active || !_foreground || _route != null) return;
    final box = context.findRenderObject() as RenderBox?;
    if (box == null || !box.hasSize) return;
    final origin = box.localToGlobal(Offset.zero);
    final p = widget.palette;
    final route = RawDialogRoute<void>(
      barrierDismissible: true,
      barrierLabel: p.english ? 'Close volume' : '收起音量',
      barrierColor: const Color(0x00000000),
      transitionDuration: Duration.zero,
      pageBuilder: (routeContext, _, _) => LayoutBuilder(
        builder: (_, constraints) {
          final left = (origin.dx - 132)
              .clamp(8.0, math.max(8.0, constraints.maxWidth - 208))
              .toDouble();
          final top = (origin.dy - 64)
              .clamp(8.0, math.max(8.0, constraints.maxHeight - 72))
              .toDouble();
          void close() => Navigator.of(routeContext).pop();
          return Stack(
            children: [
              Positioned(
                left: left,
                top: top,
                width: 200,
                child: _MiniVolumePanel(
                  core: widget.core,
                  palette: p,
                  onClose: close,
                  active: () =>
                      mounted &&
                      widget.active &&
                      _foreground &&
                      _route?.isActive == true,
                ),
              ),
            ],
          );
        },
      ),
    );
    _route = route;
    final future = Navigator.of(context).push(route);
    try {
      await future;
    } finally {
      if (identical(_route, route)) _route = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted &&
            widget.active &&
            _foreground &&
            _launcher.canRequestFocus) {
          _launcher.requestFocus();
        }
      });
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _remove();
    _launcher.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizedBox(
    width: 26,
    height: 26,
    child: MioButton(
      palette: widget.palette,
      focusNode: _launcher,
      padding: EdgeInsets.zero,
      label: widget.palette.english ? 'Volume' : '音量',
      disabledOpacity: 1,
      onPressed: widget.active ? () => unawaited(_open()) : null,
      child: Center(
        child: MioIcon(
          'nav-shengyin',
          size: 18,
          color: widget.palette.color('--player-text-idle'),
        ),
      ),
    ),
  );
}

// The route owns its slider node until Flutter actually unmounts the panel.
// Navigator's popped future can finish earlier than route.completed.
class _MiniVolumePanel extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final VoidCallback onClose;
  final bool Function() active;
  const _MiniVolumePanel({
    required this.core,
    required this.palette,
    required this.onClose,
    required this.active,
  });
  @override
  State<_MiniVolumePanel> createState() => _MiniVolumePanelState();
}

class _MiniVolumePanelState extends State<_MiniVolumePanel> {
  final _slider = FocusNode(debugLabel: 'Mini player volume slider');
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && widget.active() && _slider.canRequestFocus) {
        _slider.requestFocus();
      }
    });
  }

  @override
  void dispose() {
    _slider.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): widget.onClose,
        const SingleActivator(LogicalKeyboardKey.arrowDown): widget.onClose,
      },
      child: Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: p.surface,
          borderRadius: BorderRadius.circular(12),
          boxShadow: const [BoxShadow(color: Color(0x39000000), blurRadius: 8)],
        ),
        child: Row(
          children: [
            Expanded(
              child: AnimatedBuilder(
                animation: widget.core,
                builder: (_, _) => MioSeekBar(
                  focusNode: _slider,
                  palette: p,
                  value: widget.core.player?.volume ?? 0,
                  maximum: 1,
                  trackColor: p.secondary.withValues(alpha: .25),
                  semanticLabel: p.english ? 'Volume' : '音量',
                  semanticValue: (value) => '${(value * 100).round()}%',
                  onChanged: (value) {
                    if (widget.active()) {
                      unawaited(
                        widget.core.command(() => api.setVolume(volume: value)),
                      );
                    }
                  },
                ),
              ),
            ),
            SizedBox(
              width: 32,
              height: 32,
              child: MioButton(
                palette: p,
                label: p.english ? 'Close volume' : '收起音量',
                padding: EdgeInsets.zero,
                onPressed: widget.onClose,
                child: Center(
                  child: MioIcon('TD-Down', size: 18, color: p.secondary),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

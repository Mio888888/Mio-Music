import 'package:flutter/widgets.dart';
import 'package:liquid_glass_easy/liquid_glass_easy.dart';

import 'resources.dart';
import 'visual_policy.dart';

/// Uses the package's live Impeller lens and its frosted renderer fallback.
/// Hidden/background/reduced surfaces retain content without shader work.
class MioGlassSurface extends StatefulWidget {
  final MioPalette palette;
  final double radius;
  final Widget child;
  final bool enabled;
  final Color? tint;
  const MioGlassSurface({
    super.key,
    required this.palette,
    required this.radius,
    required this.child,
    this.enabled = true,
    this.tint,
  });
  @override
  State<MioGlassSurface> createState() => _MioGlassSurfaceState();
}

class _MioGlassSurfaceState extends State<MioGlassSurface>
    with WidgetsBindingObserver {
  // Switching the lens off must retain descendant focus/controllers. Replacing
  // the wrapper type otherwise destroys the entire content subtree.
  final _contentKey = GlobalKey();
  bool _foreground = true;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (mounted) {
      setState(() => _foreground = state == AppLifecycleState.resumed);
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final content = KeyedSubtree(key: _contentKey, child: widget.child);
    final enabled =
        widget.enabled &&
        _foreground &&
        TickerMode.valuesOf(context).enabled &&
        !MioVisualPolicy.of(context).reduced &&
        !MediaQuery.disableAnimationsOf(context);
    if (!enabled) {
      return DecoratedBox(
        decoration: BoxDecoration(
          color: widget.tint ?? p.surface,
          borderRadius: BorderRadius.circular(widget.radius),
        ),
        child: content,
      );
    }
    return LiquidGlassLens(
      style: LiquidGlassStyle(
        shape: LiquidGlassShape.roundedRectangle(cornerRadius: widget.radius),
        color: widget.tint ?? p.surface.withValues(alpha: p.dark ? .42 : .32),
        blur: 6,
        refractionIndex: 1.04,
        refractionWidth: 12,
      ),
      child: content,
    );
  }
}

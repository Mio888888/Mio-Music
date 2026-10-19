import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'device.dart';

class MioVisualPolicy extends StatefulWidget {
  final api.VisualSettings? settings;
  final Widget child;
  const MioVisualPolicy({
    super.key,
    required this.settings,
    required this.child,
  });
  static VisualPolicy of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_VisualScope>()!.value;
  @override
  State<MioVisualPolicy> createState() => _MioVisualPolicyState();
}

class _MioVisualPolicyState extends State<MioVisualPolicy> {
  bool _slow = false;
  void _degrade() {
    if (mounted && !_slow) setState(() => _slow = true);
  }

  @override
  Widget build(BuildContext context) {
    final settings = widget.settings;
    final device = MioDeviceScope.of(context);
    final mode = settings?.mode ?? 'auto';
    final reduced =
        mode == 'reduced' ||
        (mode == 'auto' &&
            (device?.limitedMemory == true ||
                (device?.television == true && _slow)));
    return _VisualScope(
      value: VisualPolicy(
        background: settings?.background ?? true,
        blurLyrics: (settings?.blurLyrics ?? false) && !reduced,
        spectrum: (settings?.spectrum ?? true) && !reduced,
        reduced: reduced,
        animate:
            (settings?.jumpLyrics ?? true) &&
            !reduced &&
            !MediaQuery.disableAnimationsOf(context),
        monitor: mode == 'auto' && device?.television == true && !reduced,
        slow: _slow,
        degrade: _degrade,
      ),
      child: widget.child,
    );
  }
}

class VisualPolicy {
  final bool background, blurLyrics, spectrum, reduced, animate, monitor, slow;
  final VoidCallback degrade;
  const VisualPolicy({
    required this.background,
    required this.blurLyrics,
    required this.spectrum,
    required this.reduced,
    required this.animate,
    required this.monitor,
    required this.slow,
    required this.degrade,
  });
}

class _VisualScope extends InheritedWidget {
  final VisualPolicy value;
  const _VisualScope({required this.value, required super.child});
  @override
  bool updateShouldNotify(_VisualScope old) =>
      (
        value.background,
        value.blurLyrics,
        value.spectrum,
        value.reduced,
        value.animate,
        value.monitor,
        value.slow,
      ) !=
      (
        old.value.background,
        old.value.blurLyrics,
        old.value.spectrum,
        old.value.reduced,
        old.value.animate,
        old.value.monitor,
        old.value.slow,
      );
}

/// Observe actual full-player frames, after warm-up. Reduction remains process-local.
class MioVisualMonitor extends StatefulWidget {
  final Widget child;
  const MioVisualMonitor({super.key, required this.child});
  @override
  State<MioVisualMonitor> createState() => _MioVisualMonitorState();
}

class _MioVisualMonitorState extends State<MioVisualMonitor>
    with WidgetsBindingObserver {
  final _warmup = Stopwatch()..start();
  int _frames = 0, _slow = 0;
  bool _foreground = true, _enabled = false;
  VoidCallback? _degrade;
  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    SchedulerBinding.instance.addTimingsCallback(_timings);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final policy = MioVisualPolicy.of(context);
    final enabled = policy.monitor && TickerMode.valuesOf(context).enabled;
    if (enabled != _enabled) {
      _frames = _slow = 0;
      _warmup.reset();
    }
    _enabled = enabled;
    _degrade = policy.degrade;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _warmup.reset();
    _frames = _slow = 0;
  }

  void _timings(List<FrameTiming> timings) {
    if (!_foreground || !_enabled || _warmup.elapsedMilliseconds < 2000) return;
    for (final timing in timings) {
      ++_frames;
      if (timing.buildDuration.inMicroseconds > 32000 ||
          timing.rasterDuration.inMicroseconds > 32000) {
        ++_slow;
      }
      if (_frames >= 60) {
        final reduce = _slow >= 20;
        _frames = _slow = 0;
        if (reduce) {
          _degrade?.call();
          return;
        }
      }
    }
  }

  @override
  void dispose() {
    SchedulerBinding.instance.removeTimingsCallback(_timings);
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

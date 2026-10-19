import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'visual_policy.dart';

class PlayerSpectrum extends StatefulWidget {
  final int generation;
  final bool playing, wide;
  final Color accent;
  final bool backgroundResponse;
  final ValueChanged<double> onLowFrequency;
  const PlayerSpectrum({
    super.key,
    required this.generation,
    required this.playing,
    required this.wide,
    required this.accent,
    required this.backgroundResponse,
    required this.onLowFrequency,
  });
  @override
  State<PlayerSpectrum> createState() => _PlayerSpectrumState();
}

class _PlayerSpectrumState extends State<PlayerSpectrum>
    with WidgetsBindingObserver {
  api.PlayerSpectrumSession? _session;
  Future<void>? _operation;
  Future<void> _cleanup = Future.value();
  Timer? _timer, _expiry;
  int _epoch = 0;
  bool _foreground = true, _enabled = false;

  /// The visualization switch itself. Rust drops its demand when it turns off,
  /// so the poll loop ends even if background response keeps `_enabled` true.
  bool _switchOn = false;
  final _frame = ValueNotifier<(List<double>, List<double>)>((
    List.filled(128, -80),
    List.filled(128, -80),
  ));
  bool get _live => mounted && _foreground && _enabled && widget.playing;
  bool _current(int epoch) => _live && epoch == _epoch;
  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
  }

  void _clear() {
    _expiry?.cancel();
    widget.onLowFrequency(0);
    _frame.value = (List.filled(128, -80), List.filled(128, -80));
  }

  void _stop() {
    ++_epoch;
    _timer?.cancel();
    _timer = null;
    _clear();
    _session?.close();
    final previous = _cleanup, pending = _operation;
    _cleanup = () async {
      await previous;
      await pending;
      final session = _session;
      _session = null;
      session?.close();
      session?.dispose();
    }();
  }

  void _restart() {
    _stop();
    if (!_live) return;
    final epoch = _epoch;
    unawaited(() async {
      await _cleanup;
      if (!_current(epoch)) return;
      final operation = _open(epoch);
      _operation = operation;
      await operation;
      if (_current(epoch) && _session != null) _schedule(epoch);
    }());
  }

  Future<void> _open(int epoch) async {
    try {
      final session = await api.beginPlayerSpectrum(
        generation: widget.generation,
      );
      _session = session;
      if (!_current(epoch)) session.close();
    } catch (_) {
      /* No fabricated signal on unavailable data. */
    }
  }

  void _schedule(int epoch) {
    _timer = Timer(
      const Duration(milliseconds: 34),
      () => unawaited(_poll(epoch)),
    );
  }

  Future<void> _poll(int epoch) async {
    final session = _session;
    if (!_current(epoch) || session == null) return;
    final operation = () async {
      final elapsed = Stopwatch()..start();
      try {
        final frame = await session.frame();
        if (!_current(epoch)) return;
        final age = frame.ageMs + elapsed.elapsedMilliseconds;
        if (!frame.playing || frame.generation != widget.generation) {
          _clear();
          return;
        }
        if (!frame.fresh || age > 200 || frame.levels.length != 128) return;
        final old = _frame.value;
        final shown = List<double>.generate(128, (i) {
          final target = frame.levels[i].isFinite
              ? frame.levels[i].clamp(-80.0, 10.0)
              : -80.0;
          final retain = target > old.$1[i] ? .3 : .85;
          return old.$1[i] * retain + target * (1 - retain);
        });
        final peaks = List<double>.generate(
          128,
          (i) => math.max(shown[i], old.$2[i] - .6),
        );
        _frame.value = (shown, peaks);
        if (widget.backgroundResponse) {
          // Same first-ten-band dB normalization as useBackgroundRender.ts.
          final bass =
              frame.levels
                  .take(10)
                  .fold(
                    0.0,
                    (sum, db) =>
                        sum + (db.isFinite ? db.clamp(-80.0, 10.0) : -80.0),
                  ) /
              10;
          widget.onLowFrequency(((bass + 80) / 80).clamp(0.0, 1.0));
        }
        _expiry?.cancel();
        _expiry = Timer(Duration(milliseconds: 200 - age), () {
          if (_current(epoch)) _clear();
        });
      } catch (error) {
        if (!_current(epoch)) return;
        if (!error.toString().contains('busy: spectrum')) _stop();
      }
    }();
    _operation = operation;
    await operation;
    if (_current(epoch)) _schedule(epoch);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final enabled =
        (MioVisualPolicy.of(context).spectrum ||
            (widget.backgroundResponse &&
                !MioVisualPolicy.of(context).reduced)) &&
        TickerMode.valuesOf(context).enabled;
    final switchOn = MioVisualPolicy.of(context).spectrum;
    if (_enabled != enabled || _switchOn != switchOn) {
      _enabled = enabled;
      _switchOn = switchOn;
      _restart();
    }
  }

  @override
  void didUpdateWidget(PlayerSpectrum old) {
    super.didUpdateWidget(old);
    if (old.generation != widget.generation ||
        old.playing != widget.playing ||
        old.backgroundResponse != widget.backgroundResponse) {
      final policy = MioVisualPolicy.of(context);
      _enabled =
          (policy.spectrum || (widget.backgroundResponse && !policy.reduced)) &&
          TickerMode.valuesOf(context).enabled;
      _restart();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _restart();
  }

  @override
  void dispose() {
    _stop();
    _frame.dispose();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => !MioVisualPolicy.of(context).spectrum
      ? const SizedBox.shrink()
      : IgnorePointer(
          child: ExcludeSemantics(
            child: RepaintBoundary(
              child: Opacity(
                opacity: widget.wide ? 1 : .42,
                child: SizedBox(
                  height: widget.wide ? 70 : 38,
                  width: double.infinity,
                  child: CustomPaint(
                    painter: _SpectrumPainter(_frame, widget.accent),
                  ),
                ),
              ),
            ),
          ),
        );
}

class _SpectrumPainter extends CustomPainter {
  final ValueNotifier<(List<double>, List<double>)> frame;
  final Color accent;
  _SpectrumPainter(this.frame, this.accent) : super(repaint: frame);
  @override
  void paint(Canvas canvas, Size size) {
    final slot = size.width / 80;
    double height(double db) =>
        math.pow((db.clamp(-80, 10) + 80) / 90, .55).toDouble() *
        size.height *
        .92;
    final levels = frame.value;
    for (var i = 0; i < 40; i++) {
      final h = height(levels.$1[i]), peak = height(levels.$2[i]);
      for (final x in [
        size.width * .5 - (i + 1) * slot,
        size.width * .5 + i * slot,
      ]) {
        final rect = Rect.fromLTWH(
          x,
          size.height - h,
          math.max(.5, slot - .5),
          h,
        );
        if (h >= 1) {
          final body = RRect.fromRectAndRadius(
            rect,
            Radius.circular(math.max(1, slot * .3)),
          );
          canvas.drawRRect(
            body,
            Paint()
              ..color = accent.withValues(alpha: .35)
              ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4),
          );
          canvas.drawRRect(
            body,
            Paint()
              ..shader = ui.Gradient.linear(rect.topCenter, rect.bottomCenter, [
                accent.withValues(alpha: .2),
                accent.withValues(alpha: .8),
              ]),
          );
        }
        if (peak > .5) {
          canvas.drawRect(
            Rect.fromLTWH(
              x,
              size.height - peak - 2,
              math.max(.5, slot - .5),
              2,
            ),
            Paint()..color = accent,
          );
        }
      }
    }
  }

  @override
  bool shouldRepaint(_SpectrumPainter old) =>
      old.accent != accent || old.frame != frame;
}

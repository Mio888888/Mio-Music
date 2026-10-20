import 'dart:math' as math;
import 'dart:async';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'resources.dart';
import 'visual_policy.dart';

/// A configured 44.1 kHz response, not a measurement of the playing source.
/// Curve samples come from the same Rust biquad calculation as desktop.
class EqResponsePlot extends StatefulWidget {
  final api.EqualizerSession? session;
  final bool active, playing;
  final int generation;
  final api.EqSnapshot value;
  final MioPalette palette;
  final bool wide;
  final bool compact;
  final int? selected;
  final ValueChanged<int>? onSelected;
  const EqResponsePlot({
    super.key,
    required this.session,
    required this.active,
    required this.playing,
    required this.generation,
    required this.value,
    required this.palette,
    required this.wide,
    this.compact = false,
    required this.selected,
    this.onSelected,
  });

  @override
  State<EqResponsePlot> createState() => _EqResponsePlotState();
}

class _EqResponsePlotState extends State<EqResponsePlot>
    with WidgetsBindingObserver {
  final _levels = ValueNotifier<List<double>>(List.filled(128, -80));
  List<double> _target = List.filled(128, -80);
  Timer? _timer, _expiry;
  api.EqualizerSession? _captureSession;
  Stopwatch? _freshSince;
  int _freshBudget = 0, _epoch = 0, _retryMs = 125;
  bool _inFlight = false, _foreground = true, _tickerEnabled = true;
  bool _reduceMotion = false, _failed = false, _visualEnabled = true;
  api.EqSnapshot get value => widget.value;
  MioPalette get palette => widget.palette;
  bool get wide => widget.wide;
  int? get selected => widget.selected;
  ValueChanged<int>? get onSelected => widget.onSelected;
  bool get _live =>
      mounted &&
      widget.active &&
      widget.playing &&
      widget.session != null &&
      _foreground &&
      _tickerEnabled &&
      _visualEnabled;
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
    _expiry = null;
    _target = List.filled(128, -80);
    _levels.value = List.filled(128, -80);
    _freshSince = null;
    _freshBudget = 0;
  }

  void _stop() {
    _epoch++;
    final session = _captureSession;
    _captureSession = null;
    try {
      session?.spectrumActive(active: false);
    } catch (_) {
      // Parent close/dispose already releases this original session's capture.
    }
    _timer?.cancel();
    _timer = null;
    _clear();
  }

  void _restart() {
    _stop();
    _failed = false;
    _retryMs = 125;
    if (_live) {
      try {
        widget.session!.spectrumActive(active: true);
        _captureSession = widget.session;
      } catch (_) {
        _failed = true;
      }
    }
    _schedule(Duration.zero);
  }

  void _schedule(Duration delay) {
    if (!_live || _failed || _inFlight || _timer != null) return;
    _timer = Timer(delay, () {
      _timer = null;
      unawaited(_poll());
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _visualEnabled = MioVisualPolicy.of(context).spectrum;
    _reduceMotion = !MioVisualPolicy.of(context).animate;
    _tickerEnabled = TickerMode.valuesOf(context).enabled;
    _restart();
  }

  @override
  void didUpdateWidget(EqResponsePlot old) {
    super.didUpdateWidget(old);
    if (!identical(old.session, widget.session) ||
        old.active != widget.active ||
        old.playing != widget.playing ||
        old.generation != widget.generation) {
      _restart();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) {
      _foreground = false;
      _stop();
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      _restart();
    }
  }

  Future<void> _poll() async {
    if (!_live || _inFlight || _failed) return;
    final session = widget.session!;
    final epoch = _epoch;
    _inFlight = true;
    final elapsed = Stopwatch()..start();
    var delay = Duration(milliseconds: _reduceMotion ? 100 : 34);
    bool current() =>
        _live && epoch == _epoch && identical(session, widget.session);
    try {
      final frame = await session.spectrum();
      if (!current()) return;
      _retryMs = 125;
      if (!frame.playing || frame.generation != widget.generation) {
        _clear();
        return;
      }
      final age = frame.ageMs + elapsed.elapsedMilliseconds;
      if (frame.fresh && age <= 200 && frame.levels.length == 128) {
        _target = frame.levels
            .map((v) => v.isFinite ? v.clamp(-80.0, 24.0) : -80.0)
            .toList(growable: false);
        _freshSince = Stopwatch()..start();
        _freshBudget = 200 - age;
        _expiry?.cancel();
        _expiry = Timer(Duration(milliseconds: _freshBudget), () {
          if (current()) _clear();
        });
      } else if (_freshSince == null ||
          _freshSince!.elapsedMilliseconds > _freshBudget) {
        _clear();
      }
      _levels.value = List.generate(128, (i) {
        if (_reduceMotion) return _target[i];
        final next = _levels.value[i] + (_target[i] - _levels.value[i]) * .18;
        return (next - _target[i]).abs() < .01 ? _target[i] : next;
      }, growable: false);
    } catch (error) {
      if (current()) {
        if (!error.toString().contains('busy:')) {
          _stop();
          setState(() => _failed = true);
        } else {
          delay = Duration(milliseconds: _retryMs);
          _retryMs = math.min(_retryMs * 2, 1000);
          if (_freshSince == null ||
              _freshSince!.elapsedMilliseconds > _freshBudget) {
            _clear();
          }
        }
      }
    } finally {
      _inFlight = false;
      if (mounted) _schedule(delay);
    }
  }

  @override
  void dispose() {
    _stop();
    WidgetsBinding.instance.removeObserver(this);
    _levels.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final rate = (value.referenceRate / 1000).toStringAsFixed(1);
    final caption = palette.english
        ? 'Configured response · $rate kHz reference · ±24 dB display'
        : '配置频响 · $rate kHz 参考 · 显示范围 ±24 dB';
    final textScale = MediaQuery.textScalerOf(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (!widget.compact) ...[
          Text(
            caption,
            style: TextStyle(fontSize: 12, color: palette.secondary),
          ),
          const SizedBox(height: 2),
          Wrap(
            spacing: 16,
            runSpacing: 2,
            children: [
              for (final text in [
                if (!value.enabled)
                  palette.english
                      ? 'EQ off · audio bypasses these filters'
                      : 'EQ 已关闭 · 音频旁路这些滤波器',
                !_visualEnabled
                    ? (palette.english
                          ? 'Spectrum disabled by visual settings'
                          : '频谱已按视觉设置关闭')
                    : _failed
                    ? (palette.english
                          ? 'Live spectrum unavailable. Reopen this page to retry.'
                          : '实时频谱暂不可用，重新进入此页可重试。')
                    : (palette.english
                          ? 'Audio spectrum · relative levels'
                          : '音频频谱 · 相对电平'),
              ])
                Text(
                  text,
                  style: TextStyle(fontSize: 12, color: palette.secondary),
                ),
            ],
          ),
          const SizedBox(height: 8),
        ],
        Semantics(
          image: true,
          label: palette.english
              ? '$caption. 20 Hz to 20 kHz. Select a band in the controls below to edit it.'
              : '$caption。20 Hz 到 20 kHz。可在下方频段控件选择并编辑。',
          child: ExcludeSemantics(
            child: Container(
              height:
                  (wide ? 250.0 : 180.0) +
                  math.max(0.0, textScale.scale(12) - 12) * 2,
              clipBehavior: Clip.antiAlias,
              decoration: BoxDecoration(
                color: palette.color('--td-bg-color-page'),
                border: Border.all(
                  color: Color.lerp(palette.border, palette.brand, .24)!,
                ),
                borderRadius: BorderRadius.circular(12),
              ),
              child: LayoutBuilder(
                builder: (context, box) {
                  final painter = _ResponsePainter(
                    value,
                    palette,
                    selected,
                    textScale,
                    Directionality.of(context),
                    _levels,
                    widget.compact,
                  );
                  return GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onTapUp: onSelected == null
                        ? null
                        : (event) {
                            final size = Size(box.maxWidth, box.maxHeight);
                            int? nearest;
                            var distance = 22.0;
                            for (var i = 0; i < value.bands.length; i++) {
                              final d =
                                  (event.localPosition -
                                          painter.marker(size, value.bands[i]))
                                      .distance;
                              if (d <= distance) {
                                distance = d;
                                nearest = i;
                              }
                            }
                            if (nearest != null) onSelected!(nearest);
                          },
                    child: CustomPaint(painter: painter, size: Size.infinite),
                  );
                },
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _ResponsePainter extends CustomPainter {
  final api.EqSnapshot value;
  final MioPalette palette;
  final int? selected;
  final bool compact;
  final TextScaler textScaler;
  final TextDirection direction;
  final ValueNotifier<List<double>> levels;
  _ResponsePainter(
    this.value,
    this.palette,
    this.selected,
    this.textScaler,
    this.direction,
    this.levels,
    this.compact,
  ) : super(repaint: levels);
  Rect frame(Size size) {
    final inset = math.max(16.0, textScaler.scale(10) * 1.6);
    return Rect.fromLTWH(
      inset,
      inset,
      math.max(1.0, size.width - inset * 2),
      math.max(1.0, size.height - inset * 2),
    );
  }

  double x(Rect r, double frequency) =>
      r.left +
      (math.log(frequency.clamp(20.0, 20000.0) / 20) / math.ln10 / 3) * r.width;
  double y(Rect r, double db) =>
      r.top + (24 - db.clamp(-24.0, 24.0)) / 48 * r.height;
  Offset marker(Size size, api.EqBand band) {
    final r = frame(size);
    return Offset(
      x(r, band.frequency),
      y(r, band.kind.index < 3 ? band.gain : 0),
    );
  }

  void label(
    Canvas canvas,
    Size size,
    String text,
    double x,
    double y, {
    bool center = true,
  }) {
    final painter = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(
          fontSize: 10,
          color: palette
              .color('--td-text-color-primary')
              .withValues(alpha: .38),
        ),
      ),
      textDirection: direction,
      textScaler: textScaler,
      maxLines: 1,
    )..layout(maxWidth: size.width);
    final left = (center ? x - painter.width / 2 : x).clamp(
      0.0,
      math.max(0.0, size.width - painter.width),
    );
    painter.paint(
      canvas,
      Offset(
        left.toDouble(),
        y.clamp(0.0, math.max(0.0, size.height - painter.height)).toDouble(),
      ),
    );
  }

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final r = frame(size);
    final brand = palette.brand;
    final text = palette.color('--td-text-color-primary');
    final fade = Rect.fromLTWH(
      0,
      size.height * .48,
      size.width,
      size.height * .52,
    );
    canvas.drawRect(
      fade,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [brand.withValues(alpha: 0), brand.withValues(alpha: .045)],
        ).createShader(fade),
    );
    const frequencies = [
      20.0,
      50.0,
      100.0,
      200.0,
      500.0,
      1000.0,
      2000.0,
      5000.0,
      10000.0,
      20000.0,
    ];
    // Keep the reference grid; omit labels that would overlap with large text.
    var lastRight = -double.infinity;
    final labelWidth = textScaler.scale(30);
    for (final frequency in frequencies) {
      final at = x(r, frequency);
      canvas.drawLine(
        Offset(at, r.top),
        Offset(at, r.bottom),
        Paint()
          ..color = text.withValues(
            alpha: frequency == 1000 || frequency == 10000 ? .055 : .035,
          )
          ..strokeWidth = .7,
      );
      final left = at - labelWidth / 2;
      if (left >= lastRight + 4) {
        final name = frequency >= 1000
            ? '${(frequency / 1000).toStringAsFixed(0)}k'
            : frequency.toStringAsFixed(0);
        label(canvas, size, name, at, size.height - textScaler.scale(12));
        lastRight = at + labelWidth / 2;
      }
    }
    final labelHeight = textScaler.scale(12);
    final dbLabelStep = r.height / 8 >= labelHeight
        ? 6
        : r.height / 4 >= labelHeight
        ? 12
        : r.height / 2 >= labelHeight
        ? 24
        : 48;
    for (var db = -24; db <= 24; db += 6) {
      final at = y(r, db.toDouble());
      if (db == 0) {
        final paint = Paint()
          ..color = brand.withValues(alpha: .34)
          ..strokeWidth = 1.15;
        for (var start = r.left; start < r.right; start += 13) {
          canvas.drawLine(
            Offset(start, at),
            Offset(math.min(start + 6, r.right), at),
            paint,
          );
        }
      } else {
        canvas.drawLine(
          Offset(r.left, at),
          Offset(r.right, at),
          Paint()
            ..color = text.withValues(alpha: db % 12 == 0 ? .055 : .035)
            ..strokeWidth = .7,
        );
      }
      if (db % dbLabelStep == 0) {
        label(
          canvas,
          size,
          db > 0 ? '+$db' : '$db',
          2,
          at - textScaler.scale(6),
          center: false,
        );
      }
    }
    final spectrum = levels.value;
    final slot = r.width / 128;
    final barWidth = math.max(1.0, slot * .55);
    final maxBar = r.height * .48;
    final hasSignal = spectrum.any((v) => v > -77);
    for (var i = 0; i < 128; i++) {
      final normalized = ((spectrum[i] + 80) / 80).clamp(0.0, 1.0);
      final floor = hasSignal ? 0.0 : .018 + (math.sin(i * 1.73) + 1) * .007;
      final barHeight = math
          .max(floor * maxBar, math.pow(normalized, .68) * maxBar)
          .toDouble();
      if (barHeight < .6) continue;
      final bar = Rect.fromLTWH(
        r.left + i * slot + (slot - barWidth) / 2,
        r.bottom - barHeight,
        barWidth,
        barHeight,
      );
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          bar,
          Radius.circular(math.min(barWidth / 2, 2.5)),
        ),
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.bottomCenter,
            end: Alignment.topCenter,
            colors: [
              palette
                  .color('--td-brand-color-5')
                  .withValues(alpha: hasSignal ? .02 : .025),
              palette
                  .color('--td-brand-color-7')
                  .withValues(alpha: hasSignal ? .28 : .07),
            ],
          ).createShader(bar),
      );
    }
    final points = value.response
        .take(300)
        .where((p) => p.frequency.isFinite && p.db.isFinite)
        .toList(growable: false);
    if (points.isNotEmpty) {
      final curve = Path();
      for (var i = 0; i < points.length; i++) {
        final at = Offset(x(r, points[i].frequency), y(r, points[i].db));
        if (i == 0) {
          curve.moveTo(at.dx, at.dy);
        } else {
          curve.lineTo(at.dx, at.dy);
        }
      }
      final area = Path()
        ..moveTo(x(r, points.first.frequency), y(r, 0))
        ..extendWithPath(curve, Offset.zero)
        ..lineTo(x(r, points.last.frequency), y(r, 0))
        ..close();
      canvas.save();
      canvas.clipRect(r);
      canvas.drawPath(area, Paint()..color = brand.withValues(alpha: .09));
      for (final stroke in [8.0, 2.2]) {
        canvas.drawPath(
          curve,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = stroke
            ..strokeJoin = StrokeJoin.round
            ..color = brand.withValues(alpha: stroke == 8 ? .12 : .92),
        );
      }
      canvas.restore();
    }
    for (var i = 0; i < value.bands.length; i++) {
      final at = marker(size, value.bands[i]);
      if (selected == i) {
        canvas.drawCircle(
          at,
          11,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 3
            ..color = brand.withValues(alpha: .34),
        );
      }
      if (compact && selected != i) {
        canvas.drawCircle(
          at,
          6,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.2
            ..color = brand.withValues(alpha: .28),
        );
        canvas.drawCircle(
          at,
          3.2,
          Paint()..color = brand.withValues(alpha: .42),
        );
        canvas.drawCircle(
          at,
          3.2,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = .8
            ..color = text.withValues(alpha: .18),
        );
        continue;
      }
      canvas.drawCircle(
        at,
        selected == i ? 6.2 : 4.2,
        Paint()
          ..color = brand.withValues(alpha: value.bands[i].enabled ? .86 : .28),
      );
      canvas.drawCircle(
        at,
        selected == i ? 6.2 : 4.2,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = selected == i ? 1.6 : 1
          ..color = text.withValues(alpha: .86),
      );
    }
  }

  @override
  bool shouldRepaint(_ResponsePainter old) =>
      !identical(old.value, value) ||
      !identical(old.palette, palette) ||
      old.selected != selected ||
      old.compact != compact ||
      old.textScaler != textScaler ||
      old.direction != direction ||
      !identical(old.levels, levels);
}

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'resources.dart';

class WelcomePage extends StatefulWidget {
  final MioPalette palette;
  final VoidCallback onFinish;
  const WelcomePage({super.key, required this.palette, required this.onFinish});
  @override
  State<WelcomePage> createState() => _WelcomePageState();
}

class _WelcomePageState extends State<WelcomePage>
    with SingleTickerProviderStateMixin {
  late final AnimationController _clock;
  bool _started = false;
  bool _finished = false;
  @override
  void initState() {
    super.initState();
    _clock =
        AnimationController(
          vsync: this,
          duration: const Duration(seconds: 5),
          animationBehavior: AnimationBehavior.preserve,
        )..addStatusListener((status) {
          if (status == AnimationStatus.completed) _finish();
        });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_started) {
      _started = true;
      _clock.forward(from: MediaQuery.disableAnimationsOf(context) ? .4 : 0);
    }
  }

  void _finish() {
    if (_finished) return;
    _finished = true;
    widget.onFinish();
  }

  @override
  void dispose() {
    _clock.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final palette = widget.palette;
    final reduced = MediaQuery.disableAnimationsOf(context);
    return ClipRRect(
      borderRadius: BorderRadius.circular(20),
      child: ColoredBox(
        color: Color(palette.dark ? 0xff0d0d0d : 0xffeef3f0),
        child: AnimatedBuilder(
          animation: _clock,
          builder: (context, _) {
            final elapsed = _clock.value * 5;
            return Stack(
              fit: StackFit.expand,
              children: [
                RepaintBoundary(
                  child: CustomPaint(
                    painter: _Particles(
                      palette.assets.welcomePoints,
                      palette.brand,
                      reduced ? 2 : elapsed,
                    ),
                  ),
                ),
                Positioned(
                  bottom: 110,
                  left: 0,
                  right: 0,
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: List.generate(
                      5,
                      (i) => Padding(
                        padding: EdgeInsets.only(right: i == 4 ? 0 : 5),
                        child: Container(
                          width: 4,
                          height: reduced
                              ? 14
                              : 12 + (math.sin(elapsed * 3 + i) + 1) * 10,
                          decoration: BoxDecoration(
                            color: palette.brand.withValues(alpha: .8),
                            borderRadius: BorderRadius.circular(2),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                if (elapsed >= 2)
                  Positioned(
                    bottom: 52,
                    left: 0,
                    right: 0,
                    child: Center(
                      child: MioButton(
                        palette: palette,
                        label: palette.text('music.welcome.skip'),
                        onPressed: _finish,
                        child: Text(
                          '${palette.text('music.welcome.skip')} ${(5 - elapsed).ceil()}s',
                        ),
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _Particles extends CustomPainter {
  final List<Offset> points;
  final Color brand;
  final double time;
  _Particles(this.points, this.brand, this.time);
  @override
  void paint(Canvas canvas, Size size) {
    final scale = math.min(math.min(size.width / 600, size.height / 500), 1.5);
    final convergence = (1 - math.exp(-1.5 * time)).clamp(0.0, 1.0);
    final center = Offset(size.width * .5, size.height * .44);
    final paint = Paint()..color = brand.withValues(alpha: .12);
    for (var i = 0; i < 160; i++) {
      final phase = i * 2.399963;
      canvas.drawCircle(
        center +
            Offset(
              math.sin(phase) * size.width * .55 +
                  math.sin(time * .22 + phase) * 14,
              math.cos(phase * 1.73) * size.height * .55 +
                  math.cos(time * .35 + phase) * 16,
            ),
        1.5 + (math.sin(phase) + 1) * 1.5,
        paint,
      );
    }
    paint.color = brand.withValues(alpha: .55 + convergence * .4);
    for (var i = 0; i < points.length; i++) {
      final phase = i * 2.399963;
      final drift = (1 - convergence) * 320 * scale;
      canvas.drawCircle(
        center +
            points[i] * scale +
            Offset(
              math.cos(phase) * drift,
              math.sin(phase) * drift + math.sin(time * 1.2) * 3,
            ),
        1.4 * math.max(scale, .65),
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_Particles old) => old.time != time || old.brand != brand;
}

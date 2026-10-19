import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import 'visual_policy.dart';

/// Shared desktop artwork assets and timings; animation is view-local.
class MioPlayerArtwork extends StatefulWidget {
  final Uint8List? cover;
  final bool coverLayout, playing;
  final double size;
  final bool reserveTonearm;
  const MioPlayerArtwork({
    super.key,
    required this.cover,
    required this.coverLayout,
    required this.playing,
    required this.size,
    this.reserveTonearm = true,
  });

  @override
  State<MioPlayerArtwork> createState() => _MioPlayerArtworkState();
}

class _MioPlayerArtworkState extends State<MioPlayerArtwork>
    with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  late final AnimationController _rotation;
  bool _foreground = true, _motion = false;

  @override
  void initState() {
    super.initState();
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    _rotation = AnimationController(
      vsync: this,
      // Common cycle of the desktop's 33s disc, 6s reflection and 8s label.
      duration: const Duration(seconds: 264),
    );
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _motion =
        TickerMode.valuesOf(context).enabled &&
        !MediaQuery.disableAnimationsOf(context) &&
        !MioVisualPolicy.of(context).reduced;
    _sync();
  }

  @override
  void didUpdateWidget(MioPlayerArtwork old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    final animate =
        _foreground && _motion && widget.playing && !widget.coverLayout;
    if (animate && !_rotation.isAnimating) {
      _rotation.repeat();
    } else if (!animate && _rotation.isAnimating) {
      _rotation.stop();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _sync();
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _rotation.dispose();
    super.dispose();
  }

  Widget _cover() => widget.cover == null
      ? Image.asset('assets/images/default-cover.png', fit: BoxFit.cover)
      : Image.memory(
          widget.cover!,
          fit: BoxFit.cover,
          gaplessPlayback: true,
          // Embedded artwork can be thousands of pixels; decode at the displayed size.
          cacheWidth: (widget.size * MediaQuery.devicePixelRatioOf(context))
              .ceil()
              .clamp(1, 4096),
        );
  Widget _svg(String name) => SvgPicture.asset('assets/images/$name.svg');

  Widget _transition({
    required double target,
    required Duration duration,
    required Curve curve,
    required Widget child,
    required ValueWidgetBuilder<double> builder,
  }) {
    // Dispose implicit controllers when motion is disabled, including mid-tween.
    if (!_motion || !_foreground) return builder(context, target, child);
    return TweenAnimationBuilder<double>(
      tween: Tween(end: target),
      duration: duration,
      curve: curve,
      builder: builder,
      child: child,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.coverLayout) {
      return SizedBox.square(
        dimension: widget.size,
        child: _transition(
          target: widget.playing ? 1 : .8,
          duration: const Duration(milliseconds: 440),
          curve: const Cubic(.34, 1.35, .64, 1),
          child: _cover(),
          builder: (_, scale, child) => Transform.scale(
            scale: scale,
            child: ClipRRect(
              // Desktop resizes the image before applying its 24px radius.
              borderRadius: BorderRadius.circular(24 / scale),
              child: child,
            ),
          ),
        ),
      );
    }
    final size = widget.size;
    final turns = Tween<double>(begin: 0, end: 8).animate(_rotation);
    return Padding(
      // Reserve the tonearm's overhang; scrolling must not crop it.
      padding: EdgeInsets.only(top: widget.reserveTonearm ? size * .24 : 0),
      child: RepaintBoundary(
        child: SizedBox.square(
          dimension: size,
          child: DecoratedBox(
            decoration: const BoxDecoration(
              shape: BoxShape.circle,
              boxShadow: [
                BoxShadow(
                  color: Color(0x33000000),
                  blurRadius: 15,
                  offset: Offset(0, 10),
                ),
                BoxShadow(
                  color: Color(0x1a000000),
                  blurRadius: 6,
                  offset: Offset(0, 4),
                ),
              ],
            ),
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Positioned.fill(child: _svg('vinyl')),
                Positioned.fill(child: CustomPaint(painter: const _Grooves())),
                Positioned.fill(
                  child: RotationTransition(
                    turns: turns,
                    child: _svg('vinyl-texture'),
                  ),
                ),
                Positioned.fill(
                  child: CustomPaint(painter: _RecordShine(_rotation)),
                ),
                Positioned(
                  left: size * .2,
                  top: size * .2,
                  width: size * .6,
                  height: size * .6,
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      RotationTransition(
                        turns: turns,
                        child: Stack(
                          fit: StackFit.expand,
                          children: [
                            _svg('vinyl-label'),
                            Padding(
                              padding: EdgeInsets.all(size * .015),
                              child: ClipOval(child: _cover()),
                            ),
                          ],
                        ),
                      ),
                      AnimatedBuilder(
                        animation: _rotation,
                        child: _svg('label-shine'),
                        builder: (_, child) {
                          final phase = (_rotation.value * 264 % 8) / 8;
                          const points = [.05, .15, .1, .15, .05];
                          final segment = (phase * 4).floor().clamp(0, 3);
                          final ease = Curves.easeInOut.transform(
                            phase * 4 - segment,
                          );
                          final opacity =
                              points[segment] +
                              (points[segment + 1] - points[segment]) * ease;
                          return Transform.rotate(
                            angle:
                                _rotation.value * 8 * math.pi * 2 +
                                _halfTurn(phase * 2) * math.pi,
                            child: Opacity(opacity: opacity, child: child),
                          );
                        },
                      ),
                    ],
                  ),
                ),
                Positioned(
                  left: size * .46,
                  top: size * .46,
                  width: size * .08,
                  height: size * .08,
                  child: _svg('vinyl-hole'),
                ),
                Positioned(
                  left: size * .52,
                  top: -size * .24,
                  width: size * .3,
                  child: _transition(
                    target: widget.playing ? 0 : -20 * math.pi / 180,
                    duration: const Duration(milliseconds: 280),
                    curve: const Cubic(.2, 0, 0, 1),
                    child: Image.asset('assets/images/pointer.png'),
                    builder: (_, angle, child) => Transform.rotate(
                      angle: angle,
                      alignment: const Alignment(-.68, -.68),
                      child: child,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

double _halfTurn(double phase) =>
    phase.floor() + Curves.easeInOut.transform(phase - phase.floor());

class _RecordShine extends CustomPainter {
  final Animation<double> clock;
  _RecordShine(this.clock) : super(repaint: clock);
  @override
  void paint(Canvas canvas, Size size) {
    final seconds = clock.value * 264;
    final phase = (seconds % 6) / 6;
    final ease = Curves.easeInOut.transform(
      phase <= .5 ? phase * 2 : (1 - phase) * 2,
    );
    final scale = 1 + .1 * ease;
    final opacity = .1 + .1 * ease;
    final side = size.shortestSide;
    final center = size.center(Offset.zero);
    canvas.save();
    canvas.translate(center.dx, center.dy);
    canvas.rotate(
      seconds / 33 * math.pi * 2 + _halfTurn(seconds % 6 / 3) * math.pi,
    );
    canvas.translate(-side / 2, -side / 2);
    final gradient = RadialGradient(
      center: Alignment(-.4 * scale, -.4 * scale),
      radius: math.sqrt(2) * .7 * scale,
      colors: [
        for (final alpha in [.08, .04, .02, .01, 0.0])
          Color.fromRGBO(255, 255, 255, alpha * opacity),
      ],
      stops: const [0, .25, .5, .75, 1],
    );
    canvas.drawCircle(
      Offset(side / 2, side / 2),
      side / 2,
      Paint()..shader = gradient.createShader(Rect.fromLTWH(0, 0, side, side)),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(_RecordShine old) => old.clock != clock;
}

class _Grooves extends CustomPainter {
  const _Grooves();
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = const Color(0x08ffffff);
    final center = size.center(Offset.zero);
    for (double radius = 8; radius < size.shortestSide / 2; radius += 8) {
      canvas.drawCircle(center, radius, paint);
    }
  }

  @override
  bool shouldRepaint(_Grooves old) => false;
}

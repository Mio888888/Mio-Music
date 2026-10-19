import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'visual_policy.dart';

/// Bounded, single-flight offscreen version of the desktop background pipeline.
/// It consumes the already loaded cover; it never fetches artwork or audio.
class PlayerBackground extends StatefulWidget {
  final Uint8List? cover;
  final Object identity;
  final api.BackgroundRendering config;
  final double Function() lowFrequency;
  final VoidCallback onUnavailable;
  const PlayerBackground({
    super.key,
    required this.cover,
    required this.identity,
    required this.config,
    required this.lowFrequency,
    required this.onUnavailable,
  });
  @override
  State<PlayerBackground> createState() => _PlayerBackgroundState();
}

class _Programs {
  final ui.FragmentProgram compose, filter;
  _Programs(this.compose, this.filter);
  static final Future<_Programs> shared = _load();
  static Future<_Programs> _load() async => _Programs(
    await ui.FragmentProgram.fromAsset('shaders/background_compose.frag'),
    await ui.FragmentProgram.fromAsset('shaders/background_filter.frag'),
  );
}

class _PlayerBackgroundState extends State<PlayerBackground>
    with WidgetsBindingObserver {
  final _frame = ValueNotifier<ui.Image?>(null);
  final _elapsed = Stopwatch()..start();
  final _random = math.Random();
  Timer? _timer;
  bool _foreground = true, _visible = false, _reduced = true, _motion = false;
  bool _busy = false, _again = false, _failed = false;
  int _epoch = 0, _width = 0, _height = 0, _lastMicros = 0;
  Uint8List? _loaded;
  ui.Image? _current, _previous;
  double _time = 0, _oldTime = 0, _alpha = 1, _oldAlpha = 0, _bass = 0;
  List<double> _angles = List.filled(4, 0), _oldAngles = List.filled(4, 0);
  _Filters? _filters;
  bool get _live => mounted && _foreground && _visible && !_reduced && !_failed;
  bool _valid(int epoch) => _live && epoch == _epoch;

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final policy = MioVisualPolicy.of(context);
    final visible = policy.background && TickerMode.valuesOf(context).enabled;
    final reduced = policy.reduced || !ui.ImageFilter.isShaderFilterSupported;
    // Lyric jump preference does not control the cover flow.
    final motion = !MediaQuery.disableAnimationsOf(context);
    if ((visible, reduced, motion) != (_visible, _reduced, _motion)) {
      _visible = visible;
      _reduced = reduced;
      _motion = motion;
      _invalidate();
    }
  }

  @override
  void didUpdateWidget(PlayerBackground old) {
    super.didUpdateWidget(old);
    final coverChanged =
        old.identity != widget.identity || !identical(old.cover, widget.cover);
    if (coverChanged) _clearFrame();
    if (coverChanged || old.config != widget.config) _invalidate();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _invalidate();
  }

  void _clearFrame() {
    final old = _frame.value;
    _frame.value = null;
    old?.dispose();
  }

  void _invalidate() {
    ++_epoch;
    _timer?.cancel();
    _timer = null;
    _lastMicros = _elapsed.elapsedMicroseconds;
    _request();
  }

  void _request() {
    if (!_live || widget.cover == null || _width < 2 || _height < 2) return;
    if (_busy) {
      _again = true;
      return;
    }
    _busy = true;
    unawaited(_render(_epoch));
  }

  Future<void> _render(int epoch) async {
    ui.Picture? picture;
    ui.FragmentShader? compose;
    ui.Image? result;
    try {
      final programs = await _Programs.shared;
      if (!_valid(epoch)) return;
      final bytes = widget.cover!;
      if (!identical(_loaded, bytes)) {
        // The source is already bounded/static PNG from the Rust cover loader.
        final codec = await ui.instantiateImageCodec(
          bytes,
          targetWidth: 512,
          targetHeight: 512,
          allowUpscaling: false,
        );
        ui.Image decoded;
        try {
          decoded = (await codec.getNextFrame()).image;
        } finally {
          codec.dispose();
        }
        if (!_valid(epoch)) {
          decoded.dispose();
          return;
        }
        _previous?.dispose();
        _previous = _current;
        _oldTime = _time;
        _oldAngles = _angles;
        _oldAlpha = _alpha;
        _current = decoded;
        _loaded = bytes;
        _time = 0;
        _alpha = _motion ? 0 : 1;
        _angles = List.generate(4, (_) => _random.nextDouble() * math.pi * 2);
      }
      final now = _elapsed.elapsedMicroseconds;
      final delta = ((now - _lastMicros) / 1000000).clamp(0.0, .1);
      _lastMicros = now;
      if (_motion) {
        if (!widget.config.staticMode) {
          _time += delta * 60 * widget.config.flowSpeed;
        }
        _alpha = math.min(1, _alpha + delta);
        _oldAlpha = math.max(0, _oldAlpha - delta);
      } else {
        _alpha = 1;
        _oldAlpha = 0;
      }
      if (_oldAlpha == 0) {
        _previous?.dispose();
        _previous = null;
      }
      final energy =
          _motion && !widget.config.staticMode && widget.config.audioResponse
          ? widget.lowFrequency().clamp(0.0, 1.0)
          : 0.0;
      if (energy == 0) {
        _bass = 0;
      } else {
        _bass += (energy - _bass) * (1 - math.exp(-12 * delta));
      }
      final width = _width, height = _height;
      if (_filters?.size != (width, height)) {
        _filters?.dispose();
        _filters = _Filters(programs.filter, width, height, _random.nextBool());
      }
      compose = programs.compose.fragmentShader();
      final params = <double>[
        width.toDouble(),
        height.toDouble(),
        _time,
        _oldTime,
        _alpha,
        _previous == null ? 0 : _oldAlpha,
        ..._angles,
        ..._oldAngles,
        _bass,
      ];
      for (var i = 0; i < params.length; ++i) {
        compose.setFloat(i, params[i]);
      }
      compose.setImageSampler(
        0,
        _current!,
        filterQuality: ui.FilterQuality.low,
      );
      compose.setImageSampler(
        1,
        _previous ?? _current!,
        filterQuality: ui.FilterQuality.low,
      );
      final bounds = Rect.fromLTWH(0, 0, width.toDouble(), height.toDouble());
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder, bounds)..clipRect(bounds);
      canvas.saveLayer(bounds, Paint()..imageFilter = _filters!.filter);
      canvas.drawRect(bounds, Paint()..shader = compose);
      canvas.restore();
      picture = recorder.endRecording();
      result = await picture.toImage(width, height);
      if (!_valid(epoch)) return;
      final prior = _frame.value;
      _frame.value = result;
      result = null;
      prior?.dispose();
    } catch (_) {
      // No retry loop on a backend/allocation failure. Static artwork remains usable.
      if (mounted && epoch == _epoch) {
        _failed = true;
        widget.onUnavailable();
        _clearFrame();
        setState(() {});
      }
    } finally {
      result?.dispose();
      picture?.dispose();
      compose?.dispose();
      _busy = false;
      if (!mounted || _failed) _disposeResources();
      if (mounted && _again) {
        _again = false;
        _request();
      } else if (_live &&
          widget.cover != null &&
          (_motion &&
              (!widget.config.staticMode || _alpha < 1 || _oldAlpha > 0))) {
        // Completion-paced: one raster operation in flight, never a queue of frames.
        _timer = Timer(
          Duration(
            microseconds: (1000000 / widget.config.fps.clamp(15, 60)).ceil(),
          ),
          _request,
        );
      }
    }
  }

  void _disposeResources() {
    _current?.dispose();
    _previous?.dispose();
    _current = _previous = null;
    _loaded = null;
    _filters?.dispose();
    _filters = null;
  }

  @override
  void dispose() {
    ++_epoch;
    _timer?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    if (!_busy) _disposeResources();
    _frame.value?.dispose();
    _frame.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: LayoutBuilder(
      builder: (context, box) {
        if (!box.maxWidth.isFinite ||
            !box.maxHeight.isFinite ||
            box.biggest.isEmpty) {
          return const SizedBox.shrink();
        }
        // Same requested scale as desktop, with mobile pixel/axis bounds.
        final scale = math.min(
          MediaQuery.devicePixelRatioOf(context) *
              widget.config.renderScale.clamp(.1, 1),
          math.min(
            2048 / math.max(box.maxWidth, box.maxHeight),
            math.sqrt(1024 * 1024 / (box.maxWidth * box.maxHeight)),
          ),
        );
        final width = math.max(2, (box.maxWidth * scale).floor() ~/ 2 * 2);
        final height = math.max(2, (box.maxHeight * scale).floor() ~/ 2 * 2);
        if ((width, height) != (_width, _height)) {
          _width = width;
          _height = height;
          _invalidate();
        }
        if (widget.cover == null) return const SizedBox.expand();
        if (_reduced || _failed) {
          return ImageFiltered(
            enabled: !_reduced,
            imageFilter: ui.ImageFilter.blur(sigmaX: 24, sigmaY: 24),
            child: Image.memory(
              widget.cover!,
              cacheWidth: 256,
              fit: BoxFit.cover,
              opacity: const AlwaysStoppedAnimation(.3),
              errorBuilder: (_, _, _) => const SizedBox.expand(),
            ),
          );
        }
        return RepaintBoundary(
          child: ColoredBox(
            color: const Color(0xff000000),
            child: CustomPaint(
              painter: _BackgroundPainter(_frame),
              size: Size.infinite,
            ),
          ),
        );
      },
    ),
  );
}

class _BackgroundPainter extends CustomPainter {
  final ValueNotifier<ui.Image?> frame;
  _BackgroundPainter(this.frame) : super(repaint: frame);
  @override
  void paint(Canvas canvas, Size size) {
    final image = frame.value;
    if (image == null) return;
    canvas.drawImageRect(
      image,
      Rect.fromLTWH(0, 0, image.width.toDouble(), image.height.toDouble()),
      Offset.zero & size,
      Paint()..filterQuality = FilterQuality.low,
    );
  }

  @override
  bool shouldRepaint(_BackgroundPainter old) => old.frame != frame;
}

class _Filters {
  final (int, int) size;
  final _shaders = <ui.FragmentShader>[];
  late final ui.ImageFilter filter;
  _Filters(ui.FragmentProgram program, int width, int height, bool mirrored)
    : size = (width, height) {
    ui.ImageFilter? chain;
    void add(double mode, double x, double y) {
      final shader = program.fragmentShader();
      _shaders.add(shader);
      shader.setFloat(2, mode);
      shader.setFloat(3, x);
      shader.setFloat(4, y);
      final next = ui.ImageFilter.shader(shader);
      chain = chain == null
          ? next
          : ui.ImageFilter.compose(outer: next, inner: chain!);
    }

    void blur(double strength, int quality) {
      for (var axis = 0; axis < 2; ++axis) {
        for (var i = 0; i < quality; ++i) {
          add(
            0,
            axis == 0 ? strength / quality : 0,
            axis == 1 ? strength / quality : 0,
          );
        }
      }
    }

    try {
      for (final pass in [
        (5.0, 1),
        (10.0, 1),
        (20.0, 2),
        (40.0, 2),
        (80.0, 2),
      ]) {
        blur(pass.$1, pass.$2);
      }
      if (math.min(width, height) > 768) blur(160, 4);
      if (math.min(width, height) > 1536) blur(320, 4);
      for (final mode in [1.0, 2.0, 3.0]) {
        add(mode, 0, 0);
      }
      blur(5, 1);
      add(4, mirrored ? .75 : .25, 1);
      add(4, mirrored ? .25 : .75, 0);
      filter = chain!;
    } catch (_) {
      dispose();
      rethrow;
    }
  }
  void dispose() {
    for (final shader in _shaders) {
      shader.dispose();
    }
    _shaders.clear();
  }
}

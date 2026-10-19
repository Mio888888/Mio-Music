import 'package:flutter/widgets.dart';

import 'visual_policy.dart';

/// Desktop's 15s title cycle: hold for 3.75s, then move one text stride.
/// Compact players can opt into the same single-line scrolling behavior.
class MioPlayerTitle extends StatefulWidget {
  final String title;
  final bool wide, centered;
  final bool singleLine;
  final double fontSize, gap;
  const MioPlayerTitle({
    super.key,
    required this.title,
    required this.wide,
    required this.centered,
    required this.fontSize,
    required this.gap,
    this.singleLine = false,
  });
  @override
  State<MioPlayerTitle> createState() => _MioPlayerTitleState();
}

class _MioPlayerTitleState extends State<MioPlayerTitle>
    with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  late final AnimationController _clock;
  bool _foreground = true, _motion = false, _overflow = false;
  double? _width;
  @override
  void initState() {
    super.initState();
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    _clock = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 15),
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
  void didUpdateWidget(MioPlayerTitle old) {
    super.didUpdateWidget(old);
    if (old.title != widget.title ||
        old.centered != widget.centered ||
        old.fontSize != widget.fontSize) {
      _clock.value = 0;
    }
    _sync();
  }

  void _sync() {
    final active =
        _foreground &&
        _motion &&
        (widget.wide || widget.singleLine) &&
        _overflow;
    if (active && !_clock.isAnimating) {
      _clock.repeat();
    } else if (!active && _clock.isAnimating) {
      _clock.stop();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _sync();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _clock.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style.merge(
      TextStyle(
        fontSize: widget.fontSize,
        fontWeight: FontWeight.w800,
        height: 1.2,
        color: const Color(0xf2ffffff),
      ),
    );
    if (!widget.wide && !widget.singleLine) {
      return Padding(
        padding: EdgeInsets.only(right: widget.gap),
        child: Text(widget.title, textAlign: TextAlign.center, style: style),
      );
    }
    return LayoutBuilder(
      builder: (context, box) {
        final measure = TextPainter(
          text: TextSpan(text: widget.title, style: style),
          textDirection: Directionality.of(context),
          textScaler: MediaQuery.textScalerOf(context),
          locale: Localizations.maybeLocaleOf(context),
          maxLines: 1,
        )..layout();
        final stride = measure.width + widget.gap;
        final height = measure.height;
        measure.dispose();
        if (_width != box.maxWidth) {
          _width = box.maxWidth;
          _clock.value = 0;
        }
        _overflow =
            (widget.singleLine ? stride - widget.gap : stride) > box.maxWidth;
        _sync();
        final label = Text(
          widget.title,
          style: style,
          maxLines: 1,
          softWrap: false,
        );
        return Semantics(
          label: widget.title,
          child: ExcludeSemantics(
            child: SizedBox(
              height: height,
              width: box.maxWidth,
              child: ClipRect(
                child: AnimatedBuilder(
                  animation: _clock,
                  builder: (_, child) {
                    final move = _overflow && _motion;
                    final offset = move
                        ? ((_clock.value - .25) / .75).clamp(0.0, 1.0) * stride
                        : 0.0;
                    return Transform.translate(
                      offset: Offset(-offset, 0),
                      child: OverflowBox(
                        alignment: widget.centered && !_overflow
                            ? Alignment.center
                            : Alignment.centerLeft,
                        minWidth: 0,
                        maxWidth: double.infinity,
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            label,
                            // The trailing gap only matters while the title scrolls;
                            // keeping it would pull a centered title off-centre.
                            if (_overflow) SizedBox(width: widget.gap),
                            if (move) label,
                          ],
                        ),
                      ),
                    );
                  },
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

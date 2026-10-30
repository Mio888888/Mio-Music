import 'device.dart';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart'
    show
        TextInputAction,
        LogicalKeyboardKey,
        LengthLimitingTextInputFormatter,
        KeyEvent,
        KeyDownEvent,
        KeyUpEvent;
import 'package:flutter_svg/flutter_svg.dart';

import 'resources.dart';

class MioIcon extends StatelessWidget {
  final String name;
  final Color color;
  final double size;
  const MioIcon(this.name, {super.key, required this.color, this.size = 18});
  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: SvgPicture.asset(
      'assets/icons/$name.svg',
      width: size,
      height: size,
      colorFilter: ColorFilter.mode(color, BlendMode.srcIn),
    ),
  );
}

class MioInput extends StatelessWidget {
  final MioPalette palette;
  final TextEditingController controller;
  final FocusNode focusNode;
  final String placeholder;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final double height;
  final bool obscureText;
  final bool readOnly;
  final int maxLines;
  final int? maxLength;
  final TextInputType keyboardType;
  final TextInputAction textInputAction;
  final String? leadingIcon;
  final double fontSize;
  final Color? background;
  final double radius;
  const MioInput({
    super.key,
    required this.palette,
    required this.controller,
    required this.focusNode,
    required this.placeholder,
    this.onChanged,
    this.onSubmitted,
    this.height = 34,
    this.obscureText = false,
    this.readOnly = false,
    this.maxLines = 1,
    this.maxLength,
    this.keyboardType = TextInputType.text,
    this.textInputAction = TextInputAction.search,
    this.leadingIcon,
    this.fontSize = 14,
    this.background,
    this.radius = 20,
  });

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: focusNode,
    builder: (context, _) => Semantics(
      textField: true,
      label: placeholder,
      child: MioTvInputNavigation(
        focusNode: focusNode,
        controller: controller,
        enabled: maxLines == 1,
        radius: radius,
        child: Container(
          height: height,
          padding: EdgeInsets.only(
            left: leadingIcon == null ? 14 : 34,
            right: 14,
          ),
          decoration: BoxDecoration(
            color: background ?? palette.surface,
            borderRadius: BorderRadius.circular(radius),
            border: Border.all(
              color: focusNode.hasFocus && MioDeviceScope.isTv(context)
                  ? palette.brand
                  : palette.border,
            ),
          ),
          child: Stack(
            clipBehavior: Clip.none,
            alignment: Alignment.centerLeft,
            children: [
              if (leadingIcon != null)
                Positioned(
                  left: -22,
                  child: MioIcon(
                    leadingIcon!,
                    size: 14,
                    color: palette.color('--td-text-color-placeholder'),
                  ),
                ),
              ValueListenableBuilder(
                valueListenable: controller,
                builder: (context, value, _) => value.text.isEmpty
                    ? IgnorePointer(
                        child: Text(
                          placeholder,
                          // TV fields are fixed-height: ellipsize, never wrap.
                          maxLines: MioDeviceScope.isTv(context) ? 1 : null,
                          overflow: MioDeviceScope.isTv(context)
                              ? TextOverflow.ellipsis
                              : null,
                          style: TextStyle(
                            color: palette.color('--td-text-color-placeholder'),
                            fontSize: fontSize,
                          ),
                        ),
                      )
                    : const SizedBox.shrink(),
              ),
              EditableText(
                controller: controller,
                focusNode: focusNode,
                style: DefaultTextStyle.of(context).style
                    .copyWith(fontSize: fontSize),
                cursorColor: palette.brand,
                backgroundCursorColor: palette.border,
                textInputAction: textInputAction,
                onChanged: onChanged,
                onSubmitted: onSubmitted,
                obscureText: obscureText,
                readOnly: readOnly,
                maxLines: maxLines,
                keyboardType: keyboardType,
                inputFormatters: maxLength == null
                    ? null
                    : [LengthLimitingTextInputFormatter(maxLength)],
                enableSuggestions: !obscureText,
                autocorrect: !obscureText,
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

/// One input path for touch, pointer, keyboard and directional focus.
class MioButton extends StatefulWidget {
  static const focusBorderWidth = 2.0;
  final MioPalette palette;
  final String label;
  final Widget child;
  final VoidCallback? onPressed;
  final FocusNode? focusNode;
  final bool autofocus;
  final Color? background;
  final Color? hoverBackground;
  final double radius;
  final double disabledOpacity;
  final bool insetFocusBorder;
  final bool preserveFocusWhenDisabled;
  final EdgeInsets padding;

  /// Scale of the TV focus lift; wide list rows use less so they stay inside
  /// their list.
  final double tvScale;
  const MioButton({
    super.key,
    required this.palette,
    required this.label,
    required this.child,
    this.onPressed,
    this.focusNode,
    this.autofocus = false,
    this.background,
    this.hoverBackground,
    this.radius = 4,
    this.disabledOpacity = .4,
    this.insetFocusBorder = true,
    this.preserveFocusWhenDisabled = false,
    this.padding = const EdgeInsets.all(8),
    this.tvScale = 1.05,
  });
  @override
  State<MioButton> createState() => _MioButtonState();
}

class _MioButtonState extends State<MioButton> {
  bool _hovered = false;
  bool _hasFocus = false;
  bool _focused = false;
  bool _pressed = false;
  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null;
    final tvFocus = _hasFocus && MioDeviceScope.isTv(context);
    final animate = !MediaQuery.disableAnimationsOf(context);
    final border = Border.all(
      width: MioButton.focusBorderWidth,
      color: tvFocus
          // TV rings use the text colour so they read on brand fills too.
          ? widget.palette.color('--td-text-color-primary')
          : _focused
          ? widget.palette.brand
          : const Color(0x00000000),
    );
    return Semantics(
      button: true,
      enabled: enabled,
      label: widget.label,
      child: MioRemoteActivation(
        activate: widget.onPressed,
        child: FocusableActionDetector(
          focusNode: widget.focusNode,
          autofocus: widget.autofocus,
          onFocusChange: (value) {
            setState(() => _hasFocus = value);
            if (value) revealTvFocus(context);
          },
          shortcuts: const {
            SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
          },
          enabled: enabled || (widget.preserveFocusWhenDisabled && _hasFocus),
          mouseCursor: enabled
              ? SystemMouseCursors.click
              : SystemMouseCursors.basic,
          onShowFocusHighlight: (value) => setState(() => _focused = value),
          onShowHoverHighlight: (value) => setState(() => _hovered = value),
          actions: {
            ActivateIntent: CallbackAction<ActivateIntent>(
              onInvoke: (_) {
                widget.onPressed?.call();
                return null;
              },
            ),
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: widget.onPressed,
            onTapDown: enabled ? (_) => setState(() => _pressed = true) : null,
            onTapUp: enabled ? (_) => setState(() => _pressed = false) : null,
            onTapCancel: () => setState(() => _pressed = false),
            // TV focus lifts and glows the control like a console menu.
            child: _tvLift(
              context,
              tvFocus,
              animate,
              AnimatedContainer(
                duration: animate
                    ? const Duration(milliseconds: 120)
                    : Duration.zero,
                padding: widget.padding,
                decoration: BoxDecoration(
                  color: enabled && (_hovered || _pressed)
                      ? widget.hoverBackground ??
                            widget.palette.color(
                              '--td-bg-color-component-hover',
                            )
                      : widget.background,
                  borderRadius: BorderRadius.circular(widget.radius),
                  border: widget.insetFocusBorder ? border : null,
                  boxShadow: tvFocus
                      ? [
                          BoxShadow(
                            color: widget.palette.brand.withValues(alpha: .45),
                            blurRadius: 22,
                            spreadRadius: 1,
                          ),
                        ]
                      : null,
                ),
                foregroundDecoration: widget.insetFocusBorder
                    ? null
                    : BoxDecoration(
                        borderRadius: BorderRadius.circular(widget.radius),
                        border: border,
                      ),
                child: Opacity(
                  opacity: enabled ? 1 : widget.disabledOpacity,
                  child: widget.child,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Only TV adds the scale layer, so other devices keep their widget tree.
  Widget _tvLift(
    BuildContext context,
    bool focused,
    bool animate,
    Widget child,
  ) => MioDeviceScope.isTv(context)
      ? AnimatedScale(
          scale: focused ? widget.tvScale : 1,
          duration: animate ? const Duration(milliseconds: 160) : Duration.zero,
          curve: Curves.easeOutCubic,
          child: child,
        )
      : child;
}

/// Touch drag commits once on release; keyboard and accessibility use bounded steps.
class MioSeekBar extends StatefulWidget {
  final FocusNode? focusNode;
  final MioPalette palette;
  final double value;
  final double maximum;
  final ValueChanged<double>? onChanged;
  final double? step;
  final String? semanticLabel;
  final String Function(double)? semanticValue;
  final double height;
  final Color? trackColor;
  final bool showThumb, hollowThumb;

  /// On TV the thumb only appears while the bar has focus, so a bar resting on
  /// a card edge does not read as a stray dot.
  final bool thumbOnlyWhenFocusedOnTv;
  const MioSeekBar({
    super.key,
    this.focusNode,
    required this.palette,
    required this.value,
    required this.maximum,
    this.onChanged,
    this.step,
    this.semanticLabel,
    this.semanticValue,
    this.height = 24,
    this.trackColor,
    this.showThumb = false,
    this.hollowThumb = false,
    this.thumbOnlyWhenFocusedOnTv = false,
  });
  @override
  State<MioSeekBar> createState() => _MioSeekBarState();
}

class _MioSeekBarState extends State<MioSeekBar> {
  double? _draft;
  bool _focused = false;
  bool _hasFocus = false;
  bool get _rangeValid => widget.maximum.isFinite && widget.maximum > 0;
  bool get _enabled => widget.onChanged != null && _rangeValid;
  double get _value => (_draft ?? (widget.value.isFinite ? widget.value : 0))
      .clamp(0.0, _rangeValid ? widget.maximum : 0.0);
  void _step(double direction) {
    if (_enabled) {
      widget.onChanged!(
        (_value + direction * (widget.step ?? (widget.maximum <= 1 ? .05 : 5)))
            .clamp(0.0, widget.maximum),
      );
    }
  }

  String _semanticText(double value) =>
      widget.semanticValue?.call(value) ??
      '${value.toStringAsFixed(1)} / ${widget.maximum.toStringAsFixed(1)}';
  double get _stepSize => widget.step ?? (widget.maximum <= 1 ? .05 : 5);

  @override
  Widget build(BuildContext context) => Semantics(
    slider: true,
    enabled: _enabled,
    label: widget.semanticLabel,
    value: _semanticText(_value),
    // The framework requires the neighbouring values alongside onIncrease.
    increasedValue: _semanticText(
      (_value + _stepSize).clamp(0.0, _rangeValid ? widget.maximum : 0.0),
    ),
    decreasedValue: _semanticText(
      (_value - _stepSize).clamp(0.0, _rangeValid ? widget.maximum : 0.0),
    ),
    onIncrease: _enabled ? () => _step(1) : null,
    onDecrease: _enabled ? () => _step(-1) : null,
    child: CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowLeft): () => _step(-1),
        const SingleActivator(LogicalKeyboardKey.arrowRight): () => _step(1),
        const SingleActivator(LogicalKeyboardKey.home): () {
          if (_enabled) widget.onChanged!(0);
        },
        const SingleActivator(LogicalKeyboardKey.end): () {
          if (_enabled) widget.onChanged!(widget.maximum);
        },
      },
      child: FocusableActionDetector(
        enabled: _enabled,
        focusNode: widget.focusNode,
        onFocusChange: (value) {
          setState(() => _hasFocus = value);
          if (value) revealTvFocus(context);
        },
        onShowFocusHighlight: (value) => setState(() => _focused = value),
        child: LayoutBuilder(
          builder: (context, box) {
            double position(double x) => box.maxWidth > 0
                ? (x / box.maxWidth).clamp(0.0, 1.0) * widget.maximum
                : 0;
            return GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTapUp: _enabled
                  ? (event) =>
                        widget.onChanged!(position(event.localPosition.dx))
                  : null,
              onHorizontalDragStart: _enabled
                  ? (event) => setState(
                      () => _draft = position(event.localPosition.dx),
                    )
                  : null,
              onHorizontalDragUpdate: _enabled
                  ? (event) => setState(
                      () => _draft = position(event.localPosition.dx),
                    )
                  : null,
              onHorizontalDragEnd: _enabled
                  ? (_) {
                      final value = _value;
                      setState(() => _draft = null);
                      widget.onChanged!(value);
                    }
                  : null,
              onHorizontalDragCancel: () => setState(() => _draft = null),
              child: SizedBox(
                height: widget.height,
                child: Stack(
                  clipBehavior: Clip.none,
                  children: [
                    Center(
                      child: Container(
                        height:
                            (_focused ||
                                (_hasFocus && MioDeviceScope.isTv(context)))
                            ? 6
                            : 4,
                        decoration: BoxDecoration(
                          color:
                              widget.trackColor ??
                              widget.palette.color('--player-bg-idle'),
                          borderRadius: BorderRadius.circular(3),
                        ),
                        child: Align(
                          alignment: Alignment.centerLeft,
                          child: FractionallySizedBox(
                            widthFactor: _rangeValid
                                ? (_value / widget.maximum).clamp(0.0, 1.0)
                                : 0,
                            child: DecoratedBox(
                              decoration: BoxDecoration(
                                color: widget.palette.brand,
                                borderRadius: BorderRadius.circular(3),
                              ),
                              child: const SizedBox.expand(),
                            ),
                          ),
                        ),
                      ),
                    ),
                    if (widget.showThumb &&
                        _rangeValid &&
                        box.maxWidth >= 16 &&
                        (!(widget.thumbOnlyWhenFocusedOnTv &&
                                MioDeviceScope.isTv(context)) ||
                            _hasFocus))
                      Positioned(
                        // Keep the whole thumb visible at both track ends.
                        left: (box.maxWidth * (_value / widget.maximum) - 8)
                            .clamp(0.0, box.maxWidth - 16),
                        top: (widget.height - 16) / 2,
                        width: 16,
                        height: 16,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: widget.hollowThumb
                                ? const Color(0x00000000)
                                : const Color(0xffffffff),
                            shape: BoxShape.circle,
                            border: Border.all(color: widget.palette.brand),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            );
          },
        ),
      ),
    ),
  );
}

/// A single-line field on TV is a stop in the focus order like any button:
/// moving onto it never raises the system keyboard, and OK starts editing. While
/// editing, left/right move the caret and leave the field at either end, and
/// up/down leave it at once.
class MioTvInputNavigation extends StatefulWidget {
  final FocusNode focusNode;
  final TextEditingController? controller;
  final FocusNode? upFallback;
  final Widget child;
  final bool enabled;
  final double radius;
  const MioTvInputNavigation({
    super.key,
    required this.focusNode,
    this.controller,
    this.upFallback,
    required this.child,
    this.enabled = true,
    this.radius = 8,
  });

  @override
  State<MioTvInputNavigation> createState() => _MioTvInputNavigationState();
}

class _MioTvInputNavigationState extends State<MioTvInputNavigation> {
  final _gate = FocusNode(debugLabel: 'Input gate');
  bool get _tv => widget.enabled && MioDeviceScope.isTv(context);

  @override
  void initState() {
    super.initState();
    _gate.addListener(_changed);
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // The field itself is only reached through its gate.
    widget.focusNode.skipTraversal = _tv;
    widget.focusNode.onKeyEvent = _tv ? _fieldKey : null;
  }

  @override
  void didUpdateWidget(MioTvInputNavigation old) {
    super.didUpdateWidget(old);
    if (!identical(old.focusNode, widget.focusNode)) {
      old.focusNode.onKeyEvent = null;
      old.focusNode.skipTraversal = false;
      didChangeDependencies();
    }
  }

  @override
  void dispose() {
    widget.focusNode.onKeyEvent = null;
    _gate.removeListener(_changed);
    _gate.dispose();
    super.dispose();
  }

  KeyEventResult _fieldKey(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.escape) {
      _gate.requestFocus();
      return KeyEventResult.handled;
    }
    final controller = widget.controller;
    final selection = controller?.selection;
    if (controller == null || selection == null || !selection.isCollapsed) {
      return KeyEventResult.ignored;
    }
    if (key == LogicalKeyboardKey.arrowRight &&
        selection.baseOffset >= controller.text.length) {
      return node.focusInDirection(TraversalDirection.right)
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    if (key == LogicalKeyboardKey.arrowLeft && selection.baseOffset <= 0) {
      return node.focusInDirection(TraversalDirection.left)
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    return KeyEventResult.ignored;
  }

  KeyEventResult _gateKey(FocusNode node, KeyEvent event) {
    final key = event.logicalKey;
    if (event is KeyDownEvent &&
        (key == LogicalKeyboardKey.select ||
            key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter)) {
      widget.focusNode.requestFocus();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    if (!_tv) return widget.child;
    final focused = _gate.hasPrimaryFocus;
    return Focus(
      focusNode: _gate,
      onKeyEvent: _gateKey,
      onFocusChange: (focused) {
        if (focused) revealTvFocus(context);
      },
      child: DecoratedBox(
        position: DecorationPosition.foreground,
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(widget.radius),
          border: Border.all(
            width: MioButton.focusBorderWidth,
            color: focused
                ? DefaultTextStyle.of(context).style.color ??
                      const Color(0xffffffff)
                : const Color(0x00000000),
          ),
        ),
        child: CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.arrowUp): () {
              if (!widget.focusNode.focusInDirection(TraversalDirection.up) &&
                  !_gate.focusInDirection(TraversalDirection.up)) {
                final fallback = widget.upFallback;
                if (fallback != null &&
                    fallback.context != null &&
                    fallback.canRequestFocus) {
                  fallback.requestFocus();
                }
              }
            },
            const SingleActivator(LogicalKeyboardKey.arrowDown): () {
              if (!widget.focusNode.focusInDirection(TraversalDirection.down)) {
                _gate.focusInDirection(TraversalDirection.down);
              }
            },
          },
          child: widget.child,
        ),
      ),
    );
  }
}

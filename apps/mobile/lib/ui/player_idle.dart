import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Ephemeral input/visibility state. Preferences remain in the Rust snapshot.
class PlayerIdleFrame extends StatefulWidget {
  final bool wide, autoHide, suspended, reducedMotion;
  final VoidCallback onHide;
  final Widget Function(BuildContext, bool hidden, double progress) builder;
  const PlayerIdleFrame({
    super.key,
    required this.wide,
    required this.autoHide,
    required this.suspended,
    required this.reducedMotion,
    required this.onHide,
    required this.builder,
  });

  @override
  State<PlayerIdleFrame> createState() => _PlayerIdleFrameState();
}

class _PlayerIdleFrameState extends State<PlayerIdleFrame>
    with WidgetsBindingObserver {
  final _focus = FocusNode(debugLabel: 'Player idle input');
  FocusNode? _restore;
  Timer? _timer;
  bool _hidden = false, _manual = false, _foreground = true;
  bool get _available =>
      mounted &&
      _foreground &&
      !widget.suspended &&
      TickerMode.valuesOf(context).enabled;
  bool get _canHide =>
      _available && widget.wide && !MediaQuery.accessibleNavigationOf(context);

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    HardwareKeyboard.instance.addHandler(_observeKey);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reset();
  }

  @override
  void didUpdateWidget(PlayerIdleFrame old) {
    super.didUpdateWidget(old);
    if (old.wide != widget.wide ||
        old.autoHide != widget.autoHide ||
        old.suspended != widget.suspended) {
      _reset();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _reset();
  }

  void _reset() {
    if (!_canHide) _manual = false;
    _wake(force: true);
  }

  void _arm() {
    _timer?.cancel();
    _timer = null;
    if (!_canHide || !widget.autoHide || _manual || _hidden) return;
    _timer = Timer(const Duration(seconds: 3), () {
      _timer = null;
      if (_canHide && widget.autoHide && !_manual) _hide();
    });
  }

  void _hide() {
    _timer?.cancel();
    _timer = null;
    if (_hidden) return;
    _restore = FocusManager.instance.primaryFocus;
    widget.onHide();
    setState(() => _hidden = true);
    _focus.requestFocus();
  }

  void _wake({bool force = false}) {
    if (_manual && !force) return;
    if (_hidden) {
      setState(() => _hidden = false);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || _hidden || !_available) return;
        final node = _restore;
        _restore = null;
        if (node != null &&
            node != _focus &&
            node.context != null &&
            node.canRequestFocus) {
          node.requestFocus();
        } else {
          _focus.nextFocus();
        }
      });
    }
    _arm();
  }

  // Observe handled child keys too, so D-pad navigation extends the deadline.
  bool _observeKey(KeyEvent event) {
    // Touch entry need not put focus inside the player. F1 is a player-wide
    // shortcut while its route is active, including before the first idle hide.
    if (_available &&
        event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.f1) {
      if (_canHide) {
        _manual = !_manual;
        if (_manual) {
          _hide();
        } else {
          _wake(force: true);
        }
      }
      return true;
    }
    if (!_hidden &&
        _focus.hasFocus &&
        _available &&
        (event is KeyDownEvent || event is KeyRepeatEvent)) {
      _wake();
    }
    return false;
  }

  KeyEventResult _key(FocusNode _, KeyEvent event) {
    if (!_available || event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.f1) {
      return KeyEventResult.ignored;
    }
    if (_hidden && !_manual) {
      _wake();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  void dispose() {
    _timer?.cancel();
    HardwareKeyboard.instance.removeHandler(_observeKey);
    WidgetsBinding.instance.removeObserver(this);
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Focus(
    focusNode: _focus,
    skipTraversal: true,
    onKeyEvent: _key,
    child: MouseRegion(
      onHover: (_) => _wake(),
      child: Listener(
        onPointerDown: (_) => _wake(),
        onPointerMove: (_) => _wake(),
        onPointerSignal: (_) => _wake(),
        child: Stack(
          fit: StackFit.expand,
          children: [
            TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: _hidden && widget.wide ? 1 : 0),
              duration: widget.reducedMotion || !widget.wide
                  ? Duration.zero
                  : const Duration(milliseconds: 440),
              curve: const Cubic(.34, 1.35, .64, 1),
              builder: (context, progress, _) => widget.builder(
                context,
                _hidden && widget.wide,
                widget.wide ? progress : 0,
              ),
            ),
            if (_hidden && !_manual)
              Positioned.fill(
                child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: _wake,
                  child: const SizedBox.expand(),
                ),
              ),
          ],
        ),
      ),
    ),
  );
}

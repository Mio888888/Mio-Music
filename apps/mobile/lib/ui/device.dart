import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;

/// Above the Navigator so dialogs use the same native device capabilities.
class MioDeviceScope extends InheritedWidget {
  final api.DeviceCapabilities? capabilities;
  const MioDeviceScope({
    super.key,
    required this.capabilities,
    required super.child,
  });
  static api.DeviceCapabilities? of(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<MioDeviceScope>()
      ?.capabilities;
  static bool isTv(BuildContext context) => of(context)?.television == true;
  static bool wide(BuildContext context, double width) =>
      isTv(context) || width > 768;
  static double rightNavigationInset(BuildContext context) {
    if (defaultTargetPlatform != TargetPlatform.iOS || isTv(context)) {
      return 0;
    }
    final inset = MediaQuery.viewPaddingOf(context).right;
    return inset >= 72 ? inset : 0;
  }

  @override
  bool updateShouldNotify(MioDeviceScope old) =>
      old.capabilities != capabilities;
}

class MioOrientationPolicy extends StatefulWidget {
  final Widget child;
  const MioOrientationPolicy({super.key, required this.child});
  @override
  State<MioOrientationPolicy> createState() => _MioOrientationPolicyState();
}

class _MioOrientationPolicyState extends State<MioOrientationPolicy>
    with WidgetsBindingObserver {
  static const _host = MethodChannel('mio/device');
  ui.FlutterView? _view;
  int _queryRevision = 0;
  bool? _portraitOnly;
  bool? _appliedPortraitOnly;
  bool _applying = false;

  Future<void> _update() async {
    if (!mounted ||
        kIsWeb ||
        (defaultTargetPlatform != TargetPlatform.iOS &&
            defaultTargetPlatform != TargetPlatform.android)) {
      return;
    }
    final revision = ++_queryRevision;
    final television = MioDeviceScope.isTv(context);
    bool? portraitOnly;
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      portraitOnly = await _host.invokeMethod<bool>('portraitOnly');
    } else {
      final display = _view?.display;
      if (display != null &&
          !display.size.isEmpty &&
          display.devicePixelRatio > 0) {
        portraitOnly =
            display.size.shortestSide / display.devicePixelRatio < 600;
      }
    }
    if (!mounted || revision != _queryRevision || portraitOnly == null) return;
    _portraitOnly = !television && portraitOnly;
    unawaited(_apply());
  }

  Future<void> _apply() async {
    if (_applying) return;
    _applying = true;
    try {
      while (mounted && _portraitOnly != _appliedPortraitOnly) {
        final portraitOnly = _portraitOnly;
        if (portraitOnly == null) return;
        await SystemChrome.setPreferredOrientations(
          portraitOnly ? const [DeviceOrientation.portraitUp] : const [],
        );
        _appliedPortraitOnly = portraitOnly;
      }
    } on PlatformException catch (error, stack) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: error,
          stack: stack,
          library: 'Mio device orientation',
        ),
      );
    } finally {
      _applying = false;
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (!kIsWeb && defaultTargetPlatform == TargetPlatform.iOS) {
      _host.setMethodCallHandler((call) async {
        if (call.method == 'orientationChanged') unawaited(_update());
      });
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _view = View.of(context);
    unawaited(_update());
  }

  @override
  void didChangeMetrics() => unawaited(_update());

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _appliedPortraitOnly = null;
      unawaited(_update());
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    ++_queryRevision;
    if (!kIsWeb && defaultTargetPlatform == TargetPlatform.iOS) {
      _host.setMethodCallHandler(null);
    }
    _view = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// A menu supplied by the row; long-confirm never also activates that row.
class MioRemoteMenu extends InheritedWidget {
  final VoidCallback? open;
  const MioRemoteMenu({super.key, required this.open, required super.child});
  static VoidCallback? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<MioRemoteMenu>()?.open;
  @override
  bool updateShouldNotify(MioRemoteMenu old) => old.open != open;
}

class MioRemoteActivation extends StatefulWidget {
  final Widget child;
  final VoidCallback? activate;
  const MioRemoteActivation({super.key, required this.child, this.activate});
  @override
  State<MioRemoteActivation> createState() => _MioRemoteActivationState();
}

class _MioRemoteActivationState extends State<MioRemoteActivation>
    with WidgetsBindingObserver {
  Timer? _timer;
  LogicalKeyboardKey? _held;
  VoidCallback? _activation;
  bool _opened = false;
  void _cancel() {
    _timer?.cancel();
    _timer = null;
    _held = null;
    _activation = null;
    _opened = false;
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) _cancel();
  }

  @override
  void didUpdateWidget(MioRemoteActivation old) {
    super.didUpdateWidget(old);
    if (widget.activate == null) _cancel();
  }

  @override
  void dispose() {
    _cancel();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (!MioDeviceScope.isTv(context) || widget.activate == null) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    final menu = MioRemoteMenu.of(context);
    if (key == LogicalKeyboardKey.contextMenu && menu != null) {
      if (event is KeyDownEvent) {
        _cancel();
        menu();
      }
      return KeyEventResult.handled;
    }
    if (key != LogicalKeyboardKey.select &&
        key != LogicalKeyboardKey.enter &&
        key != LogicalKeyboardKey.numpadEnter) {
      return KeyEventResult.ignored;
    }
    if (event is KeyDownEvent) {
      _cancel();
      if (menu == null) {
        widget.activate?.call();
        return KeyEventResult.handled;
      }
      _held = key;
      _activation = widget.activate;
      _timer = Timer(const Duration(milliseconds: 600), () {
        if (!mounted ||
            _held != key ||
            !node.hasFocus ||
            widget.activate == null) {
          return;
        }
        _opened = true;
        menu();
      });
    } else if (event is KeyUpEvent && _held == key) {
      final activate = !_opened ? _activation : null;
      _cancel();
      activate?.call();
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) => Focus(
    canRequestFocus: false,
    skipTraversal: true,
    onFocusChange: (focused) {
      if (!focused) _cancel();
    },
    onKeyEvent: _key,
    child: widget.child,
  );
}

void revealTvFocus(BuildContext context) {
  if (!MioDeviceScope.isTv(context)) return;
  WidgetsBinding.instance.addPostFrameCallback((_) {
    if (context.mounted) {
      unawaited(Scrollable.ensureVisible(context, alignment: .5));
    }
  });
}

/// Window ownership ends on page disposal and every foreground departure.
class MioTvScreenLease extends StatefulWidget {
  final Widget child;
  const MioTvScreenLease({super.key, required this.child});
  @override
  State<MioTvScreenLease> createState() => _MioTvScreenLeaseState();
}

class _MioTvScreenLeaseState extends State<MioTvScreenLease>
    with WidgetsBindingObserver {
  static const _host = MethodChannel('mio/device');
  static int _next = 0;
  late final int _token = ++_next;
  bool _tv = false;
  bool? _sent;
  int _revision = 0;
  void _update(bool active) {
    active = active && _tv;
    if (_sent == active) return;
    _sent = active;
    final revision = ++_revision;
    if (!_tv) return;
    unawaited(_send(active, revision));
  }

  Future<void> _send(bool active, int revision) async {
    try {
      int? epoch;
      if (active) {
        epoch = await _host.invokeMethod<int>('screenEpoch');
        if (!mounted || revision != _revision || _sent != true) return;
        if (epoch == null) {
          _sent = null;
          return;
        }
      }
      await _host.invokeMethod<void>('keepScreenOn', {
        'token': _token,
        'active': active,
        'epoch': epoch,
      });
    } on PlatformException {
      if (revision == _revision) _sent = null;
    } on MissingPluginException {
      if (revision == _revision) _sent = null;
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _tv = MioDeviceScope.isTv(context);
    _update(
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed,
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _sent = null;
    _update(state == AppLifecycleState.resumed);
  }

  @override
  void dispose() {
    _update(false);
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// A full-screen overlay owns directional focus until its explicit close/back.
class MioFocusPanel extends StatefulWidget {
  final Widget child;
  const MioFocusPanel({super.key, required this.child});
  @override
  State<MioFocusPanel> createState() => _MioFocusPanelState();
}

class _MioFocusPanelState extends State<MioFocusPanel> {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
    directionalTraversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _scheduled = false;
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_scheduled && MioDeviceScope.isTv(context)) {
      _scheduled = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _scope.nextFocus();
      });
    }
  }

  @override
  void dispose() {
    _scope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      FocusScope(node: _scope, autofocus: true, child: widget.child);
}

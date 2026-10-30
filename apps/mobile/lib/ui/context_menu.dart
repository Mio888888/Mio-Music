import 'device.dart';

import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/semantics.dart' show CustomSemanticsAction;
import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'glass_surface.dart';
import 'resources.dart';

/// A row's touch, secondary-pointer, keyboard and semantic menu entry points.
class MioContextRegion extends StatelessWidget {
  final Widget child;
  final ValueChanged<Offset>? onOpen;
  final String label;
  const MioContextRegion({
    super.key,
    required this.child,
    required this.label,
    this.onOpen,
  });
  @override
  Widget build(BuildContext context) {
    void keyboard() {
      final box = context.findRenderObject();
      if (box is RenderBox && box.hasSize) {
        onOpen?.call(box.localToGlobal(Offset(16, box.size.height)));
      }
    }

    return MioRemoteMenu(
      // Keep descendants mounted while a route temporarily disables its menu.
      open: onOpen == null ? null : keyboard,
      child: CallbackShortcuts(
        bindings: {
          if (onOpen != null) ...{
            const SingleActivator(LogicalKeyboardKey.f10, shift: true):
                keyboard,
            const SingleActivator(LogicalKeyboardKey.contextMenu): keyboard,
          },
        },
        child: Semantics(
          customSemanticsActions: onOpen == null
              ? null
              : {CustomSemanticsAction(label: label): keyboard},
          child: GestureDetector(
            behavior: HitTestBehavior.deferToChild,
            onLongPressStart: onOpen == null
                ? null
                : (details) => onOpen?.call(details.globalPosition),
            onSecondaryTapDown: onOpen == null
                ? null
                : (details) => onOpen?.call(details.globalPosition),
            child: child,
          ),
        ),
      ),
    );
  }
}

class MioMenuEntry<T> {
  final T value;
  final String label, icon;
  final bool danger, enabled;
  final Widget? leading;
  const MioMenuEntry({
    required this.value,
    required this.label,
    required this.icon,
    this.danger = false,
    this.enabled = true,
    this.leading,
  });
}

/// Custom menu based on desktop context_actions.rs, with touch-sized targets.
/// The caller owns route lifetime and binds commands to the original session.
class MioContextMenu<T> extends StatefulWidget {
  final MioPalette palette;
  final Offset anchor;
  final String label;
  final List<MioMenuEntry<T>> entries;
  final double? selectWidth;
  final T? selected;
  const MioContextMenu({
    super.key,
    required this.palette,
    required this.anchor,
    required this.label,
    required this.entries,
    this.selectWidth,
    this.selected,
  });
  @override
  State<MioContextMenu<T>> createState() => _MioContextMenuState<T>();
}

class _MioContextMenuState<T> extends State<MioContextMenu<T>> {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  late final _nodes = [for (final _ in widget.entries) FocusNode()];
  bool _closing = false;
  List<int> get _enabled => [
    for (var i = 0; i < widget.entries.length; i++)
      if (widget.entries[i].enabled) i,
  ];
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _enabled.isNotEmpty) _nodes[_enabled.first].requestFocus();
    });
  }

  void _dismiss([T? value]) {
    if (_closing || !mounted) return;
    _closing = true;
    Navigator.of(context).pop(value);
  }

  void _move(int delta) {
    final enabled = _enabled;
    if (enabled.isEmpty) return;
    final current = enabled.indexWhere((index) => _nodes[index].hasFocus);
    final next = current < 0
        ? (delta > 0 ? 0 : enabled.length - 1)
        : (current + delta) % enabled.length;
    _nodes[enabled[next]].requestFocus();
  }

  @override
  void dispose() {
    for (final node in _nodes) {
      node.dispose();
    }
    _scope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final media = MediaQuery.of(context);
    // A TV's overscan margin is not a system inset, so keep clear of it.
    final edge = MioDeviceScope.isTv(context) ? 28.0 : 8.0;
    final padding = EdgeInsets.fromLTRB(
      media.padding.left + edge,
      media.padding.top + edge,
      media.padding.right + edge,
      math.max(media.padding.bottom, media.viewInsets.bottom) + edge,
    );
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () => _dismiss(),
        const SingleActivator(LogicalKeyboardKey.arrowDown): () => _move(1),
        const SingleActivator(LogicalKeyboardKey.arrowUp): () => _move(-1),
        const SingleActivator(LogicalKeyboardKey.home): () {
          if (_enabled.isNotEmpty) _nodes[_enabled.first].requestFocus();
        },
        const SingleActivator(LogicalKeyboardKey.end): () {
          if (_enabled.isNotEmpty) _nodes[_enabled.last].requestFocus();
        },
      },
      child: FocusScope(
        node: _scope,
        autofocus: true,
        child: Semantics(
          scopesRoute: true,
          explicitChildNodes: true,
          namesRoute: true,
          label: widget.label,
          child: DefaultTextStyle(
            style: TextStyle(
              fontFamily: 'PingFang SC',
              fontFamilyFallback: const ['sans-serif'],
              fontSize: widget.selectWidth == null ? 13 : 14,
              height: 1.5,
              color: p.color('--td-text-color-primary'),
            ),
            child: CustomSingleChildLayout(
              delegate: _MenuPosition(
                widget.anchor,
                padding,
                widget.selectWidth ?? 180,
              ),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(
                    widget.selectWidth == null ? 10 : 6,
                  ),
                  boxShadow: const [
                    BoxShadow(
                      color: Color(0x33000000),
                      offset: Offset(0, 6),
                      blurRadius: 18,
                    ),
                  ],
                ),
                // Liquid glass: the menu refracts the page behind it.
                child: MioGlassSurface(
                  palette: p,
                  radius: (widget.selectWidth == null ? 10 : 6).toDouble(),
                  tint: p
                      .color('--td-bg-color-container')
                      .withValues(alpha: p.dark ? .55 : .5),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      border: Border.all(
                        color: p.color('--td-border-level-1-color'),
                      ),
                      borderRadius: BorderRadius.circular(
                        widget.selectWidth == null ? 10 : 6,
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(4),
                      child: SingleChildScrollView(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            for (var i = 0; i < widget.entries.length; i++)
                              ConstrainedBox(
                                constraints: BoxConstraints(
                                  minHeight: widget.selectWidth == null
                                      ? 44
                                      : 28,
                                ),
                                child: MioButton(
                                  palette: p,
                                  focusNode: _nodes[i],
                                  label: widget.entries[i].label,
                                  radius: widget.selectWidth == null ? 6 : 3,
                                  insetFocusBorder: widget.selectWidth == null,
                                  background:
                                      widget.selectWidth != null &&
                                          widget.selected ==
                                              widget.entries[i].value
                                      ? p.color('--td-bg-color-component')
                                      : null,
                                  onPressed: widget.entries[i].enabled
                                      ? () => _dismiss(widget.entries[i].value)
                                      : null,
                                  padding: EdgeInsets.symmetric(
                                    horizontal: widget.selectWidth == null
                                        ? 12
                                        : 8,
                                    vertical: widget.selectWidth == null
                                        ? 8
                                        : 4,
                                  ),
                                  child: Row(
                                    children: [
                                      if (widget.selectWidth == null)
                                        widget.entries[i].leading ??
                                            MioIcon(
                                              widget.entries[i].icon,
                                              size: 14,
                                              color: widget.entries[i].danger
                                                  ? p.color('--td-error-color')
                                                  : p.color(
                                                      '--td-text-color-primary',
                                                    ),
                                            ),
                                      if (widget.selectWidth == null)
                                        const SizedBox(width: 8),
                                      Expanded(
                                        child: Text(
                                          widget.entries[i].label,
                                          style: TextStyle(
                                            color: widget.entries[i].danger
                                                ? p.color('--td-error-color')
                                                : p.color(
                                                    '--td-text-color-primary',
                                                  ),
                                          ),
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _MenuPosition extends SingleChildLayoutDelegate {
  final Offset anchor;
  final EdgeInsets padding;
  final double width;
  const _MenuPosition(this.anchor, this.padding, this.width);
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    final width = math.max(
      0.0,
      math.min(this.width, constraints.maxWidth - padding.horizontal),
    );
    return BoxConstraints(
      minWidth: width,
      maxWidth: width,
      maxHeight: math.max(0.0, constraints.maxHeight - padding.vertical),
    );
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) => Offset(
    anchor.dx.clamp(
      padding.left,
      math.max(padding.left, size.width - padding.right - childSize.width),
    ),
    anchor.dy.clamp(
      padding.top,
      math.max(padding.top, size.height - padding.bottom - childSize.height),
    ),
  );
  @override
  bool shouldRelayout(_MenuPosition old) =>
      old.anchor != anchor || old.padding != padding || old.width != width;
}

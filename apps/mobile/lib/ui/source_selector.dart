import 'dart:async';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'context_menu.dart';
import 'controls.dart';
import 'resources.dart';
import 'source_icon.dart';

class SourceSelector extends StatefulWidget {
  final api.PlayerSnapshot? player;
  final MioPalette palette;
  final bool active;
  final bool expanded;
  const SourceSelector({
    super.key,
    required this.player,
    required this.palette,
    required this.active,
    this.expanded = false,
  });
  @override
  State<SourceSelector> createState() => _SourceSelectorState();
}

class _SourceSelectorState extends State<SourceSelector>
    with WidgetsBindingObserver {
  final _focus = FocusNode(debugLabel: 'Header source');
  api.SourceSelectionSession? _session;
  api.SourceSelectionSnapshot? _snapshot;
  RawDialogRoute<String>? _route;
  int _epoch = 0;
  bool _foreground = true, _busy = false, _failed = false;
  bool get _live =>
      mounted && widget.active && _foreground && widget.player != null;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    if (_live) unawaited(_load());
  }

  void _close() {
    _epoch++;
    _session?.close();
    _session?.dispose();
    _session = null;
    final route = _route;
    _route = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
      WidgetsBinding.instance.ensureVisualUpdate();
    }
    _busy = false;
  }

  @override
  void didUpdateWidget(SourceSelector old) {
    super.didUpdateWidget(old);
    if (!_live) {
      _close();
      return;
    }
    if (!old.active ||
        old.player == null ||
        old.player?.sourceRevision != widget.player?.sourceRevision ||
        old.player?.sourceSelectionRevision !=
            widget.player?.sourceSelectionRevision) {
      unawaited(_load());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live) unawaited(_load());
    } else {
      _foreground = false;
      _close();
    }
  }

  Future<void> _load() async {
    _close();
    if (!_live) return;
    final epoch = _epoch;
    final player = widget.player!;
    api.SourceSelectionSession? candidate;
    bool valid() => _live && epoch == _epoch;
    try {
      candidate = await api.beginSourceSelection(
        sourceRevision: player.sourceRevision,
        selectionRevision: player.sourceSelectionRevision,
      );
      if (!valid()) return;
      final snapshot = await candidate.snapshot();
      if (!valid()) return;
      setState(() {
        _snapshot = snapshot;
        _session = candidate;
        candidate = null;
        _failed = false;
      });
    } catch (_) {
      if (valid()) {
        setState(() {
          _snapshot = null;
          _failed = true;
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _open() async {
    if (!_live || _busy || _route != null) return;
    if (_session == null) await _load();
    final session = _session, snapshot = _snapshot;
    if (!mounted ||
        _busy ||
        _route != null ||
        !_live ||
        session == null ||
        snapshot == null ||
        snapshot.sources.isEmpty) {
      return;
    }
    final box = context.findRenderObject() as RenderBox?;
    if (box == null || !box.hasSize) return;
    final epoch = _epoch;
    final restore = _focus.hasFocus;
    final p = widget.palette;
    final selected = snapshot.sources.any((s) => s.id == snapshot.selected)
        ? snapshot.selected
        : snapshot.sources.first.id;
    final route = RawDialogRoute<String>(
      barrierDismissible: true,
      barrierLabel: p.text('common.close'),
      barrierColor: const Color(0x00000000),
      transitionDuration: Duration.zero,
      pageBuilder: (_, _, _) => MioContextMenu<String>(
        palette: p,
        anchor: box.localToGlobal(Offset(0, box.size.height)),
        label: p.english ? 'Source' : '音源',
        entries: [
          for (final source in snapshot.sources)
            MioMenuEntry(
              value: source.id,
              label: '${source.name}${source.id == selected ? ' ✓' : ''}',
              icon: 'nav-liebiao',
              leading: SourceIcon(
                value: source.icon,
                color: p.color('--td-text-color-primary'),
              ),
            ),
        ],
      ),
    );
    setState(() => _busy = true);
    _route = route;
    try {
      final source = await Navigator.of(context).push(route);
      if (identical(_route, route)) _route = null;
      if (!_live ||
          epoch != _epoch ||
          source == null ||
          source == snapshot.selected) {
        return;
      }
      await session.select(source: source);
      if (_live && epoch == _epoch) await _load();
    } catch (_) {
      if (_live && epoch == _epoch) setState(() => _failed = true);
    } finally {
      if (identical(_route, route)) _route = null;
      if (mounted && epoch == _epoch) setState(() => _busy = false);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_live &&
            !_busy &&
            restore &&
            _route == null &&
            _focus.canRequestFocus &&
            ModalRoute.of(context)?.isCurrent == true) {
          _focus.requestFocus();
        }
      });
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _close();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final sources = _snapshot?.sources ?? <api.SourceOption>[];
    final selected =
        sources.where((s) => s.id == _snapshot?.selected).firstOrNull ??
        sources.firstOrNull;
    // Nothing to choose until a list plugin provides a source; a failed read
    // keeps the button so it can retry.
    if (!_failed && sources.isEmpty) {
      return SizedBox(width: widget.expanded ? 0 : 12);
    }
    final label = _failed
        ? (p.english ? 'Reload sources' : '重新读取音源')
        : selected?.name ?? (p.english ? 'Source' : '音源');
    final button = SizedBox(
      width: widget.expanded ? double.infinity : 32,
      height: widget.expanded ? 48 : 28,
      child: MioButton(
        palette: p,
        focusNode: _focus,
        label: label,
        radius: widget.expanded ? 12 : 999,
        padding: widget.expanded
            ? const EdgeInsets.symmetric(horizontal: 14)
            : EdgeInsets.zero,
        onPressed: !_live || _busy ? null : () => unawaited(_open()),
        child: !widget.expanded
            ? SourceIcon(
                value: selected?.icon,
                color: p.color('--td-text-color-primary'),
              )
            : Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  SourceIcon(
                    value: selected?.icon,
                    color: p.color('--td-text-color-primary'),
                  ),
                  const SizedBox(width: 12),
                  Expanded(child: Text(label, overflow: TextOverflow.ellipsis)),
                  MioIcon('nav-xiangyou', color: p.secondary, size: 16),
                ],
              ),
      ),
    );
    return widget.expanded
        ? button
        : Padding(padding: const EdgeInsets.only(right: 4), child: button);
  }
}

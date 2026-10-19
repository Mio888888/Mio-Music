import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'glass_surface.dart';
import 'resources.dart';

String playModeKey(api.PlayMode mode) => switch (mode) {
  api.PlayMode.list => 'play.modeList',
  api.PlayMode.sequence => 'play.modeSequence',
  api.PlayMode.random => 'play.modeRandom',
  api.PlayMode.single => 'play.modeSingle',
};
String playModeIcon(api.PlayMode mode) => switch (mode) {
  api.PlayMode.random => 'Shuffle',
  api.PlayMode.sequence || api.PlayMode.single => 'RepeatSong',
  api.PlayMode.list => 'Repeat',
};

class QueueDrawer extends StatefulWidget {
  final api.PlayerSnapshot state;
  final MioPalette palette;
  final VoidCallback onClose;
  const QueueDrawer({
    super.key,
    required this.state,
    required this.palette,
    required this.onClose,
  });
  @override
  State<QueueDrawer> createState() => QueueDrawerState();
}

class QueueDrawerState extends State<QueueDrawer> with WidgetsBindingObserver {
  bool _focusScheduled = false;
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_focusScheduled && MioDeviceScope.isTv(context)) {
      _focusScheduled = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _scope.nextFocus();
      });
    }
  }

  final _scroll = ScrollController();
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  api.QueueSession? _session;
  // Four 64-row pages, each capped at 256 KiB by Rust. Queued requests
  // contain offsets only; a single reader prevents unbounded in-flight pages.
  final _pages = <int, List<api.Track>>{};
  final _requests = <int>{};
  (int, int)? _readingPage;
  bool _reading = false, _scheduled = false;
  api.QueueSession? _dragSession;
  int? _selected;
  bool _loading = true, _busy = false, _confirm = false, _foreground = true;
  int _epoch = 0;
  String? _error, _notice;
  MioPalette get p => widget.palette;
  Object _identity(api.PlayerSnapshot s) => (s.queueRevision, s.sourceRevision);
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
    if (_foreground) unawaited(_load());
  }

  @override
  void didUpdateWidget(QueueDrawer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_identity(oldWidget.state) != _identity(widget.state)) {
      _selected = null;
      _confirm = false;
      if (_foreground) unawaited(_load());
    }
  }

  void _release() {
    _epoch++;
    _session?.close();
    _session?.dispose();
    _session = null;
    _dragSession = null;
    _pages.clear();
    _requests.clear();
  }

  bool _valid(int epoch) => mounted && _foreground && epoch == _epoch;
  Future<void> _load() async {
    _release();
    final epoch = _epoch;
    setState(() {
      _loading = true;
      _error = null;
      _notice = null;
    });
    try {
      final session = await api.beginQueue(
        revision: widget.state.queueRevision,
        sourceRevision: widget.state.sourceRevision,
      );
      if (!_valid(epoch)) {
        session.close();
        session.dispose();
        return;
      }
      setState(() {
        _session = session;
        _loading = false;
      });
      // Rows request their own pages after layout, including restored offsets.
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _loading = false;
          _error = p.english
              ? 'Queue changed. Reload to continue.'
              : '队列已变更，请重新加载。';
        });
      }
    }
  }

  void _request(int offset) {
    if (!_enabled ||
        _pages.containsKey(offset) ||
        _readingPage == (_epoch, offset)) {
      return;
    }
    if (!_requests.contains(offset) && _requests.length >= 4) {
      _requests.remove(_requests.first);
    }
    _requests.add(offset);
    _scheduleRead();
  }

  void _scheduleRead() {
    if (_scheduled) return;
    _scheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scheduled = false;
      if (mounted) unawaited(_readPage());
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  Future<void> _readPage() async {
    final session = _session;
    if (!_enabled || _reading || session == null || _requests.isEmpty) return;
    final epoch = _epoch;
    final offset = _requests.first;
    _requests.remove(offset);
    _reading = true;
    _readingPage = (epoch, offset);
    try {
      final page = await session.page(offset: offset);
      if (!_valid(epoch) || !identical(session, _session)) return;
      if (page.offset != offset || page.total != widget.state.queueCount) {
        throw StateError('queue page changed');
      }
      setState(() {
        _pages[offset] = page.items;
        while (_pages.length > 4) {
          _pages.remove(_pages.keys.first);
        }
      });
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _requests.clear();
          _error = p.english
              ? 'Unable to load queue. Reload and try again.'
              : '队列加载失败，请重新加载后重试。';
        });
      }
    } finally {
      _reading = false;
      _readingPage = null;
      if (mounted && _requests.isNotEmpty) _scheduleRead();
    }
  }

  Future<void> _run(
    Future<void> Function(api.QueueSession) action, {
    api.QueueSession? expected,
  }) async {
    final session = _session;
    if (_busy ||
        _loading ||
        !_foreground ||
        session == null ||
        (expected != null && !identical(expected, session))) {
      return;
    }
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    try {
      await action(session);
    } catch (_) {
      if (_valid(epoch)) {
        setState(() {
          _pages.clear();
          _requests.clear();
          _error = p.english
              ? 'Unable to update queue. Reload and try again.'
              : '队列操作未完成，请重新加载后重试。';
        });
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        if (_requests.isNotEmpty) _scheduleRead();
      }
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      unawaited(_load());
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _release();
    }
  }

  @override
  void dispose() {
    _release();
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    _scroll.dispose();
    super.dispose();
  }

  bool get _enabled =>
      !_busy && !_loading && _foreground && _session != null && _error == null;
  Widget _icon(String icon, String label, VoidCallback? action) => SizedBox(
    width: 44,
    height: 44,
    child: MioButton(
      palette: p,
      label: label,
      onPressed: action,
      radius: 22,
      child: MioIcon(icon, color: p.secondary),
    ),
  );
  double _rowHeight(BuildContext context) {
    final scale = MediaQuery.textScalerOf(context);
    final narrow = !MioDeviceScope.wide(
      context,
      MediaQuery.sizeOf(context).width,
    );
    return math.max(
      narrow ? 68 : 66,
      scale.scale(14) * 1.6 + scale.scale(12) * 1.6 + 8,
    );
  }

  Future<void> _locate() => _run((session) async {
    final epoch = _epoch;
    final id = widget.state.current?.id;
    final rowHeight = _rowHeight(context);
    final index = id == null ? null : await session.locate(id: id);
    if (!_valid(epoch) ||
        !identical(session, _session) ||
        widget.state.current?.id != id) {
      return;
    }
    if (index == null) {
      setState(() => _notice = p.text('play.currentSongNotInPlaylist'));
      return;
    }
    if (_scroll.hasClients) {
      final target =
          index * rowHeight +
          (_selected != null && _selected! < index ? 56 : 0);
      // Fixed per-index extents let the viewport reach unloaded distant pages.
      _scroll.jumpTo(target.clamp(0.0, _scroll.position.maxScrollExtent));
    }
  });

  void _reorder(int from, int boundary) {
    final session = _dragSession;
    _dragSession = null;
    if (!_enabled || session == null || !identical(session, _session)) return;
    final to = boundary > from ? boundary - 1 : boundary;
    if (to == from) return;
    unawaited(
      _run(
        (s) => s.moveSong(from: from, boundary: boundary),
        expected: session,
      ),
    );
  }

  Widget _row(BuildContext context, int index) {
    final offset = index ~/ 64 * 64;
    final page = _pages.remove(offset);
    if (page != null) _pages[offset] = page;
    final position = index - offset;
    final song = page != null && position < page.length ? page[position] : null;
    final key = ValueKey((_identity(widget.state), index));
    if (song == null) {
      _request(offset);
      return SizedBox(
        key: key,
        height: _rowHeight(context) + (_selected == index ? 56 : 0),
        child: Center(child: Text(p.english ? 'Loading…' : '加载中…')),
      );
    }
    final selected = _selected == index;
    final active = song.id == widget.state.current?.id;
    final narrow = !MioDeviceScope.wide(
      context,
      MediaQuery.sizeOf(context).width,
    );
    final tv = MioDeviceScope.isTv(context);
    final error = p.color('--td-error-color');
    final play = ReorderableDelayedDragStartListener(
      index: index,
      enabled: _enabled && !tv,
      child: MioButton(
        palette: p,
        label: '${song.title} ${song.artist}',
        onPressed: _enabled
            ? () => unawaited(_run((s) => s.play(index: index, id: song.id)))
            : null,
        padding: EdgeInsets.zero,
        radius: narrow ? 16 : 10,
        child: Row(
          children: [
            SizedBox(
              width: 20,
              child: Text(
                '${index + 1}'.padLeft(2, '0'),
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: p.color('--td-text-color-placeholder'),
                ),
              ),
            ),
            SizedBox(width: narrow ? 12 : 15),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    song.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontSize: 14, height: 1.6),
                  ),
                  Text(
                    song.artist,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      height: 1.6,
                      color: p.secondary,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 6),
            SizedBox(
              width: 40,
              child: Text(
                _duration(song.duration),
                textAlign: TextAlign.right,
                style: TextStyle(
                  fontSize: 12,
                  color: p
                      .color('--td-text-color-placeholder')
                      .withValues(alpha: .72),
                ),
              ),
            ),
          ],
        ),
      ),
    );
    return Column(
      key: key,
      mainAxisSize: MainAxisSize.min,
      children: [
        SizedBox(
          height: _rowHeight(context),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(narrow ? 16 : 10),
              child: ColoredBox(
                color: active
                    ? p.brand.withValues(alpha: .12)
                    : const Color(0x00000000),
                child: Stack(
                  children: [
                    if (active)
                      Positioned(
                        left: 0,
                        top: 0,
                        bottom: 0,
                        width: 4,
                        child: ColoredBox(
                          color: p.brand.withValues(alpha: .58),
                        ),
                      ),
                    Padding(
                      padding: EdgeInsets.symmetric(
                        horizontal: narrow ? 12 : 16,
                      ),
                      child: Row(
                        children: [
                          Expanded(child: play),
                          const SizedBox(width: 6),
                          SizedBox(
                            width: 44,
                            height: 44,
                            child: MioButton(
                              palette: p,
                              label: p.english
                                  ? 'Remove ${song.title}'
                                  : '移除 ${song.title}',
                              onPressed: _enabled
                                  ? () => unawaited(
                                      _run((s) => s.remove(id: song.id)),
                                    )
                                  : null,
                              padding: EdgeInsets.zero,
                              radius: 22,
                              child: Center(
                                child: Container(
                                  width: 36,
                                  height: 36,
                                  decoration: BoxDecoration(
                                    shape: BoxShape.circle,
                                    color: error.withValues(alpha: .1),
                                  ),
                                  child: Center(
                                    child: MioIcon(
                                      'TD-Delete',
                                      color: error,
                                      size: 16,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                          if (tv)
                            _icon(
                              'Menu',
                              p.english ? 'Reorder' : '调整顺序',
                              _enabled
                                  ? () => setState(
                                      () => _selected = selected ? null : index,
                                    )
                                  : null,
                            ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
        if (selected)
          SizedBox(
            height: 56,
            child: Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                _icon(
                  'Up',
                  p.english ? 'Move up' : '上移',
                  _enabled && index > 0
                      ? () => unawaited(
                          _run(
                            (s) => s.moveSong(from: index, boundary: index - 1),
                          ),
                        )
                      : null,
                ),
                _icon(
                  'Down',
                  p.english ? 'Move down' : '下移',
                  _enabled && index + 1 < widget.state.queueCount
                      ? () => unawaited(
                          _run(
                            (s) => s.moveSong(from: index, boundary: index + 2),
                          ),
                        )
                      : null,
                ),
                _icon(
                  'TD-Close',
                  p.text('common.close'),
                  () => setState(() => _selected = null),
                ),
              ],
            ),
          ),
      ],
    );
  }

  /// System back and Escape dismiss the innermost queue surface first.
  bool handleBack() {
    if (_confirm) {
      setState(() => _confirm = false);
      return true;
    }
    if (_selected != null) {
      setState(() => _selected = null);
      return true;
    }
    return false;
  }

  Widget _body() {
    if (_confirm) {
      return SingleChildScrollView(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              p.text('play.clearPlaylistConfirmTitle'),
              style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 18),
            ),
            const SizedBox(height: 12),
            Text(p.text('play.clearPlaylistConfirmBody')),
            const SizedBox(height: 16),
            Wrap(
              spacing: 12,
              runSpacing: 8,
              children: [
                MioButton(
                  palette: p,
                  label: p.text('common.cancel'),
                  onPressed: () => setState(() => _confirm = false),
                  child: Text(p.text('common.cancel')),
                ),
                MioButton(
                  palette: p,
                  label: p.text('play.confirmClear'),
                  onPressed: _enabled
                      ? () {
                          setState(() => _confirm = false);
                          unawaited(_run((s) => s.clear()));
                        }
                      : null,
                  child: Text(
                    p.text('play.confirmClear'),
                    style: TextStyle(color: p.color('--td-error-color')),
                  ),
                ),
              ],
            ),
          ],
        ),
      );
    }
    if (widget.state.queueCount == 0) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                p.text('play.playlistEmpty'),
                textAlign: TextAlign.center,
                style: TextStyle(color: p.secondary),
              ),
              const SizedBox(height: 14),
              Text(
                p.text('play.playlistEmptyTip'),
                textAlign: TextAlign.center,
                style: TextStyle(color: p.secondary),
              ),
            ],
          ),
        ),
      );
    }
    final rowHeight = _rowHeight(context);
    return LayoutBuilder(
      builder: (context, box) {
        final scrollable =
            widget.state.queueCount * rowHeight + (_selected == null ? 0 : 56) >
            box.maxHeight;
        return RawScrollbar(
          controller: _scroll,
          thumbVisibility: true,
          thickness: 8,
          radius: const Radius.circular(999),
          thumbColor: p
              .color('--td-text-color-placeholder')
              .withValues(alpha: .3),
          child: ReorderableList(
            key: ValueKey(_identity(widget.state)),
            controller: _scroll,
            padding: EdgeInsets.only(right: scrollable ? 8 : 0),
            itemCount: widget.state.queueCount,
            itemExtent: _selected == null ? rowHeight : null,
            itemExtentBuilder: _selected == null
                ? null
                : (index, _) => rowHeight + (_selected == index ? 56 : 0),
            itemBuilder: _row,
            onReorderStart: (_) => _dragSession = _session,
            onReorder: _reorder,
          ),
        );
      },
    );
  }

  String _duration(double seconds) {
    if (!seconds.isFinite || seconds < 0) return '--:--';
    final total = seconds.floor();
    return '${(total ~/ 60).toString().padLeft(2, '0')}:${(total % 60).toString().padLeft(2, '0')}';
  }

  Widget _header(bool narrow, double gutter) => Container(
    padding: EdgeInsets.symmetric(
      horizontal: narrow ? gutter : 16,
      vertical: narrow ? 12 : 16,
    ),
    decoration: BoxDecoration(
      border: Border(
        bottom: BorderSide(
          color: p.color('--td-text-color-primary').withValues(alpha: .1),
        ),
      ),
    ),
    child: Row(
      children: [
        Expanded(
          child: Text(
            '${p.text('play.playlistTitle')} (${widget.state.queueCount})',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: narrow ? 17 : 16,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
        SizedBox.square(
          dimension: narrow ? 44 : 36,
          child: MioButton(
            palette: p,
            label: p.text('common.close'),
            onPressed: widget.onClose,
            radius: 22,
            padding: EdgeInsets.zero,
            background: const Color(0x1f787880),
            child: Center(
              child: MioIcon('WindowClose', color: p.secondary, size: 18),
            ),
          ),
        ),
      ],
    ),
  );

  Widget _footerAction(
    String label,
    Color tint,
    VoidCallback? action,
    bool narrow, {
    String? icon,
  }) => Expanded(
    child: Container(
      constraints: BoxConstraints(minHeight: narrow ? 44 : 34),
      decoration: BoxDecoration(
        border: Border.all(color: tint.withValues(alpha: .2)),
        borderRadius: BorderRadius.circular(narrow ? 999 : 8),
      ),
      child: MioButton(
        palette: p,
        label: label,
        onPressed: action,
        radius: narrow ? 999 : 8,
        background: tint.withValues(alpha: .1),
        padding: EdgeInsets.symmetric(horizontal: 6, vertical: narrow ? 8 : 4),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            if (icon != null) ...[
              MioIcon(icon, color: tint, size: 16),
              const SizedBox(width: 6),
            ],
            Flexible(
              child: Text(
                label,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: tint,
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );

  Widget _footer(bool narrow, double gutter, double bottomInset) => Container(
    padding: EdgeInsets.fromLTRB(
      narrow ? gutter : 16,
      narrow ? 10 : 12,
      narrow ? gutter : 16,
      12 + bottomInset,
    ),
    decoration: BoxDecoration(
      border: Border(
        top: BorderSide(
          color: p.color('--td-text-color-primary').withValues(alpha: .1),
        ),
      ),
    ),
    child: Row(
      children: [
        _footerAction(
          p.text('play.locateCurrentPlay'),
          p.brand,
          _enabled ? () => unawaited(_locate()) : null,
          narrow,
        ),
        const SizedBox(width: 8),
        _footerAction(
          p.text('play.clearPlaylist'),
          p.color('--td-error-color'),
          _enabled ? () => setState(() => _confirm = true) : null,
          narrow,
          icon: 'TD-Delete',
        ),
      ],
    ),
  );

  List<Widget> _feedback() => [
    if (_notice != null)
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
        child: Semantics(
          liveRegion: true,
          child: Text(
            _notice!,
            style: TextStyle(color: p.secondary, fontSize: 12),
          ),
        ),
      ),
    if (_error != null)
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
        child: Row(
          children: [
            Expanded(
              child: Semantics(
                liveRegion: true,
                child: Text(
                  _error!,
                  style: TextStyle(
                    color: p.color('--td-error-color'),
                    fontSize: 12,
                  ),
                ),
              ),
            ),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _busy || _loading ? null : () => unawaited(_load()),
              child: MioIcon('Refresh', color: p.secondary),
            ),
          ],
        ),
      ),
  ];

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final narrow = !MioDeviceScope.wide(context, box.maxWidth);
      final bottomInset = narrow
          ? math.max(
              MediaQuery.paddingOf(context).bottom,
              MioDeviceScope.isTv(context) ? 16.0 : 0.0,
            )
          : 0.0;
      final gutter = (box.maxWidth * .04).clamp(16.0, 24.0);
      final height = narrow
          ? math.min(
              math.min(box.maxHeight * .76, 620.0 + bottomInset),
              math.max(0.0, box.maxHeight - 12),
            )
          : math.max(
              0.0,
              box.maxHeight - (box.maxHeight * .1).clamp(70.0, 80.0) - 80,
            );
      return FocusScope(
        node: _scope,
        autofocus: true,
        child: Shortcuts(
          shortcuts: {
            LogicalKeySet(LogicalKeyboardKey.escape): const DismissIntent(),
          },
          child: Actions(
            actions: {
              DismissIntent: CallbackAction<DismissIntent>(
                onInvoke: (_) {
                  if (!handleBack()) widget.onClose();
                  return null;
                },
              ),
            },
            child: Stack(
              children: [
                if (narrow)
                  Positioned.fill(
                    child: ClipRect(
                      child: BackdropFilter(
                        filter: ImageFilter.blur(sigmaX: 12, sigmaY: 12),
                        child: const SizedBox.expand(),
                      ),
                    ),
                  ),
                ModalBarrier(
                  color: narrow
                      ? const Color(0x47000000)
                      : const Color(0x00000000),
                  dismissible: true,
                  onDismiss: widget.onClose,
                  semanticsLabel: p.text('common.close'),
                ),
                Positioned(
                  right: 0,
                  top: narrow ? null : 72,
                  bottom: narrow ? 0 : null,
                  width: narrow ? box.maxWidth : math.min(box.maxWidth, 380.0),
                  height: height,
                  child: Semantics(
                    scopesRoute: true,
                    explicitChildNodes: true,
                    namesRoute: true,
                    label: p.text('play.playlistTitle'),
                    child: ClipRect(
                      child: Stack(
                        clipBehavior: Clip.none,
                        children: [
                          // The lens only supports uniform corners, so it is
                          // oversized past the screen edge and clipped there.
                          Positioned(
                            left: 0,
                            top: 0,
                            right: narrow ? 0 : -24,
                            bottom: narrow ? -24 : 0,
                            child: MioGlassSurface(
                              palette: p,
                              radius: narrow ? 22 : 16,
                              tint: narrow
                                  ? p.color('--mobile-glass-bg-strong')
                                  : p.surface.withValues(alpha: .5),
                              child: Padding(
                                padding: EdgeInsets.only(
                                  right: narrow ? 0 : 24,
                                  bottom: narrow ? 24 : 0,
                                ),
                                child: DecoratedBox(
                                  decoration: BoxDecoration(
                                    border: narrow
                                        ? Border(
                                            top: BorderSide(
                                              color: p.color(
                                                '--mobile-glass-border',
                                              ),
                                              width: .5,
                                            ),
                                            left: BorderSide(
                                              color: p.color(
                                                '--mobile-glass-border',
                                              ),
                                              width: .5,
                                            ),
                                            right: BorderSide(
                                              color: p.color(
                                                '--mobile-glass-border',
                                              ),
                                              width: .5,
                                            ),
                                          )
                                        : null,
                                  ),
                                  child: DefaultTextStyle.merge(
                                    style: TextStyle(
                                      color: p.color('--td-text-color-primary'),
                                      height: 1.6,
                                    ),
                                    child: Column(
                                      children: [
                                        if (narrow)
                                          Container(
                                            width: 38,
                                            height: 4,
                                            margin: const EdgeInsets.only(
                                              top: 8,
                                            ),
                                            decoration: BoxDecoration(
                                              color: const Color(0x5c787880),
                                              borderRadius:
                                                  BorderRadius.circular(999),
                                            ),
                                          ),
                                        _header(narrow, gutter),
                                        if (MioDeviceScope.isTv(context))
                                          Align(
                                            alignment: Alignment.centerLeft,
                                            child: _icon(
                                              playModeIcon(
                                                widget.state.playMode,
                                              ),
                                              p.text(
                                                playModeKey(
                                                  widget.state.playMode,
                                                ),
                                              ),
                                              _enabled
                                                  ? () => unawaited(
                                                      _run(
                                                        (s) => s.cycleMode(),
                                                      ),
                                                    )
                                                  : null,
                                            ),
                                          ),
                                        ..._feedback(),
                                        Expanded(
                                          child: Padding(
                                            padding: EdgeInsets.symmetric(
                                              horizontal: narrow ? 10 : 8,
                                              vertical: narrow ? 6 : 10,
                                            ),
                                            child: _loading
                                                ? Center(
                                                    child: Text(
                                                      p.english
                                                          ? 'Loading…'
                                                          : '加载中…',
                                                    ),
                                                  )
                                                : _body(),
                                          ),
                                        ),
                                        if (widget.state.queueCount > 0 &&
                                            !_confirm)
                                          _footer(narrow, gutter, bottomInset)
                                        else if (bottomInset > 0)
                                          SizedBox(height: bottomInset),
                                      ],
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}

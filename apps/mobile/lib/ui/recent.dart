import 'device.dart';

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

/// The desktop recent page is the reversed tail of the queue. Projection and
/// original index/identity validation live in Rust, sharing QueueSession.
class RecentView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final api.PlayerSnapshot? state;
  final bool active;
  final double viewportWidth;
  const RecentView({
    super.key,
    required this.core,
    required this.palette,
    required this.state,
    required this.active,
    required this.viewportWidth,
  });
  @override
  State<RecentView> createState() => _RecentViewState();
}

class _RecentViewState extends State<RecentView> with WidgetsBindingObserver {
  final _scroll = ScrollController();
  api.QueueSession? _session;
  List<api.RecentTrack> _rows = [];
  int _total = 0, _epoch = 0;
  bool _foreground = true, _loading = false, _busy = false;
  String? _error;
  MioPalette get p => widget.palette;
  bool get _live =>
      mounted && widget.active && _foreground && widget.state != null;
  bool _valid(int epoch) => _live && epoch == _epoch;
  Object _identity(api.PlayerSnapshot? state) =>
      (state?.queueRevision, state?.sourceRevision);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground =
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached;
    if (_live) unawaited(_load());
  }

  void _release() {
    _epoch++;
    _session?.close();
    _session?.dispose();
    _session = null;
    _loading = false;
    _busy = false;
  }

  @override
  void didUpdateWidget(RecentView old) {
    super.didUpdateWidget(old);
    if (!_live) {
      _release();
    } else if (!old.active || _identity(old.state) != _identity(widget.state)) {
      unawaited(_load());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      setState(() {
        _foreground = false;
        _release();
      });
    } else if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live) unawaited(_load());
    }
  }

  Future<void> _load() async {
    if (!_live) return;
    _release();
    final epoch = _epoch;
    final state = widget.state!;
    setState(() {
      _loading = true;
      _error = null;
    });
    api.QueueSession? session;
    var adopted = false;
    try {
      session = await api.beginQueue(
        revision: state.queueRevision,
        sourceRevision: state.sourceRevision,
      );
      if (!_valid(epoch)) return;
      final page = await session.recent();
      if (!_valid(epoch)) return;
      _session = session;
      adopted = true;
      setState(() {
        _rows = page.items;
        _total = page.total;
      });
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _error = p.english
              ? 'Unable to read recent tracks. Reload to continue.'
              : '最近播放读取失败，请重新加载。',
        );
      }
    } finally {
      if (!adopted) {
        session?.close();
        session?.dispose();
      }
      if (_valid(epoch)) setState(() => _loading = false);
    }
  }

  Future<void> _play(api.RecentTrack row, api.QueueSession? expected) async {
    final session = _session;
    if (!_live ||
        _busy ||
        _loading ||
        _error != null ||
        session == null ||
        !identical(session, expected)) {
      return;
    }
    final epoch = _epoch;
    setState(() => _busy = true);
    var accepted = false;
    await widget.core.command(() async {
      await session.play(index: row.index, id: row.track.id);
      accepted = true;
    });
    if (!_valid(epoch)) return;
    setState(() {
      _busy = false;
      if (!accepted) {
        _error = p.english
            ? 'Queue changed or playback failed. Reload and try again.'
            : '队列已变更或播放失败，请重新加载后重试。';
      }
    });
  }

  @override
  void dispose() {
    _release();
    WidgetsBinding.instance.removeObserver(this);
    _scroll.dispose();
    super.dispose();
  }

  String _time(double seconds) {
    if (!seconds.isFinite || seconds <= 0) return '--:--';
    final value = seconds.floor();
    return '${value ~/ 60}:${(value % 60).toString().padLeft(2, '0')}';
  }

  @override
  Widget build(BuildContext context) {
    final width = widget.viewportWidth;
    final narrow = !MioDeviceScope.wide(context, width);
    final title = Text(
      p.text('music.recent.title'),
      style: TextStyle(
        fontSize: narrow ? (width * .09).clamp(32, 41.6) : 20,
        fontWeight: FontWeight.w600,
      ),
    );
    final count = Text(
      '($_total ${p.text('common.unitSongs')})',
      style: TextStyle(fontSize: 14, color: p.secondary),
    );
    return Padding(
      padding: EdgeInsets.fromLTRB(
        narrow ? (width * .04).clamp(16, 24) : 20,
        narrow ? (width * .035).clamp(14, 22) : 20,
        narrow ? (width * .04).clamp(16, 24) : 20,
        narrow ? 0 : 20,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: narrow
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [title, const SizedBox(height: 8), count],
                      )
                    : Row(
                        crossAxisAlignment: CrossAxisAlignment.baseline,
                        textBaseline: TextBaseline.alphabetic,
                        children: [
                          Flexible(child: title),
                          const SizedBox(width: 8),
                          count,
                        ],
                      ),
              ),
              SizedBox(
                width: 44,
                height: 44,
                child: MioButton(
                  palette: p,
                  label: p.text('common.refresh'),
                  onPressed: _live && !_busy && !_loading
                      ? () => unawaited(_load())
                      : null,
                  child: MioIcon('Refresh', color: p.secondary),
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
          if (_error != null) ...[
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            MioButton(
              palette: p,
              label: p.text('common.retry'),
              onPressed: _live && !_loading ? () => unawaited(_load()) : null,
              child: Text(p.text('common.retry')),
            ),
          ],
          Expanded(
            child: _loading
                ? Center(
                    child: Text(
                      p.english ? 'Loading…' : '加载中…',
                      style: TextStyle(color: p.secondary),
                    ),
                  )
                : _error != null
                ? const SizedBox.shrink()
                : _rows.isEmpty
                ? Center(
                    child: Text(
                      p.text('music.recent.empty'),
                      style: TextStyle(color: p.secondary),
                    ),
                  )
                : ListView.builder(
                    controller: _scroll,
                    itemCount: _rows.length,
                    itemBuilder: (context, i) {
                      final row = _rows[i];
                      final song = row.track;
                      final session = _session;
                      return ConstrainedBox(
                        constraints: BoxConstraints(
                          minHeight: narrow ? 56 : 44,
                        ),
                        child: MioButton(
                          palette: p,
                          label: '${song.title} - ${song.artist}',
                          radius: 6,
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                          onPressed: _live && !_busy && session != null
                              ? () => unawaited(_play(row, session))
                              : null,
                          child: Row(
                            children: [
                              SizedBox(
                                width: 32,
                                child: Text(
                                  '${i + 1}',
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: p.secondary,
                                  ),
                                ),
                              ),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      song.title,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: const TextStyle(fontSize: 14),
                                    ),
                                    Text(
                                      song.artist,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                        fontSize: 12,
                                        color: p.secondary,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              const SizedBox(width: 12),
                              Text(
                                _time(song.duration),
                                style: TextStyle(
                                  fontSize: 12,
                                  color: p.secondary,
                                ),
                              ),
                            ],
                          ),
                        ),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }
}

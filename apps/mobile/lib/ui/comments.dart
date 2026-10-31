import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart' show SelectableText;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'cover.dart';
import 'resources.dart';

class CommentsDialog extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  const CommentsDialog({super.key, required this.core, required this.palette});
  @override
  State<CommentsDialog> createState() => _CommentsDialogState();
}

class _CommentsTab {
  final rows = <api.Comment>[];
  bool loading = false, loaded = false, more = true;
  int total = 0;
  String? error;
}

class _CommentsDialogState extends State<CommentsDialog>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  final _imageScope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  final _scrolls = [ScrollController(), ScrollController()];
  List<_CommentsTab> _tabs = [_CommentsTab(), _CommentsTab()];
  final _expanded = <(int, String)>{};
  api.CommentsSession? _session;
  Object? _identity, _appearance;
  int _epoch = 0, _active = 0;
  bool _foreground = true,
      _opening = false,
      _unsupported = false,
      _closing = false;
  String? _error, _image;
  MioPalette get p => MioPalette(
    widget.palette.assets,
    dark: widget.core.player?.dark ?? widget.palette.dark,
    english: widget.core.player?.english ?? widget.palette.english,
  );
  bool _valid(int epoch) =>
      mounted && !_closing && _foreground && epoch == _epoch;
  Object _key(api.PlayerSnapshot? s) => (s?.current?.id, s?.sourceRevision);

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    _identity = _key(widget.core.player);
    _appearance = (p.dark, p.english);
    widget.core.addListener(_changed);
    WidgetsBinding.instance.addObserver(this);
    for (final scroll in _scrolls) {
      scroll.addListener(_maybeMore);
    }
    if (_foreground) unawaited(_start());
  }

  void _release() {
    _epoch++;
    _session?.close();
    _session?.dispose();
    _session = null;
  }

  void _changed() {
    if (!mounted || _closing) return;
    final identity = _key(widget.core.player), appearance = (p.dark, p.english);
    if (identity != _identity) {
      _identity = identity;
      if (_foreground) {
        unawaited(_start());
      } else {
        _release();
      }
    } else if (_appearance != appearance) {
      setState(() {});
    }
    _appearance = appearance;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      unawaited(_start());
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      _release();
      setState(() => _image = null);
    }
  }

  Future<void> _start() async {
    if (!_foreground || _closing) return;
    _release();
    for (final scroll in _scrolls) {
      if (scroll.hasClients) scroll.jumpTo(0);
    }
    final epoch = _epoch, snapshot = widget.core.player;
    setState(() {
      _tabs = [_CommentsTab(), _CommentsTab()];
      _active = 0;
      _expanded.clear();
      _image = null;
      _opening = true;
      _unsupported = false;
      _error = null;
    });
    api.PlayerDetails? details;
    api.CommentsSession? candidate;
    try {
      if (snapshot?.current == null) {
        if (_valid(epoch)) {
          setState(() => _unsupported = true);
        }
        return;
      }
      details = await api.beginPlayerDetails(
        songId: snapshot!.current!.id,
        generation: snapshot.playbackGeneration,
        queueRevision: snapshot.queueRevision,
        sourceRevision: snapshot.sourceRevision,
      );
      if (!_valid(epoch)) return;
      if (!await details.commentsAvailable()) {
        if (_valid(epoch)) {
          setState(() => _unsupported = true);
        }
        return;
      }
      if (!_valid(epoch)) return;
      candidate = await details.openComments();
      if (!_valid(epoch)) return;
      _session = candidate;
      candidate = null;
    } catch (_) {
      if (_valid(epoch)) {
        setState(
          () => _error = p.english
              ? 'Unable to open comments. Retry.'
              : '评论打开失败，请重试。',
        );
      }
    } finally {
      details?.cancel();
      details?.dispose();
      candidate?.close();
      candidate?.dispose();
      if (_valid(epoch)) {
        setState(() => _opening = false);
      }
    }
    if (_valid(epoch) && _session != null) await _load(0);
  }

  Future<void> _load(int tab) async {
    final session = _session, epoch = _epoch, state = _tabs[tab];
    if (!_valid(epoch) ||
        session == null ||
        _tabs.any((t) => t.loading) ||
        !state.more ||
        _image != null) {
      return;
    }
    setState(() {
      state.loading = true;
      state.error = null;
    });
    try {
      final page = await session.next(hot: tab == 0);
      if (!_valid(epoch) || !identical(session, _session)) return;
      setState(() {
        state.rows.addAll(page.rows);
        state.total = page.total;
        state.more = page.hasMore;
        state.loaded = true;
      });
    } catch (error) {
      if (_valid(epoch) && identical(session, _session)) {
        final message = error.toString();
        if ([
          'source_changed',
          'runtime_closed',
          'cancelled',
          'player_changed',
        ].any(message.contains)) {
          _session?.close();
          _session?.dispose();
          _session = null;
          setState(() {
            state.loading = false;
            _error = p.english
                ? 'Music source changed. Reopen comments.'
                : '音乐来源已变化，请重新打开评论。';
          });
          return;
        }
        setState(
          () => state.error = p.english
              ? 'Unable to load comments. Retry this page.'
              : '评论加载失败，请重试本页。',
        );
      }
    } finally {
      if (_valid(epoch) && identical(session, _session)) {
        setState(() => state.loading = false);
      }
    }
    if (_valid(epoch)) _scheduleMore();
  }

  void _scheduleMore() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _maybeMore();
    });
  }

  void _maybeMore() {
    if (!_foreground || _closing || _image != null) return;
    final state = _tabs[_active], scroll = _scrolls[_active];
    if (_session != null &&
        !_tabs.any((t) => t.loading) &&
        state.error == null &&
        state.more &&
        (!state.loaded ||
            scroll.hasClients && scroll.position.extentAfter < 200)) {
      unawaited(_load(_active));
    }
  }

  void _select(int tab) {
    setState(() => _active = tab);
    _scheduleMore();
  }

  void _dismiss() {
    if (_image != null) {
      setState(() => _image = null);
      _scheduleMore();
      return;
    }
    if (_closing) return;
    _closing = true;
    _release();
    Navigator.of(context).pop();
  }

  @override
  void dispose() {
    _release();
    widget.core.removeListener(_changed);
    WidgetsBinding.instance.removeObserver(this);
    for (final scroll in _scrolls) {
      scroll.dispose();
    }
    _scope.dispose();
    _imageScope.dispose();
    super.dispose();
  }

  String _count(int n) =>
      n > 10000 ? '${(n / 10000).toStringAsFixed(1)}w' : '$n';
  Widget _button(String label, VoidCallback? action) => MioButton(
    palette: p,
    label: label,
    onPressed: action,
    child: Text(label),
  );
  Widget _media(String url, {required bool preview}) => Image.network(
    url,
    width: preview ? null : 100,
    height: preview ? null : 100,
    fit: preview ? BoxFit.contain : BoxFit.cover,
    cacheWidth: preview
        ? 2048
        : (100 * MediaQuery.devicePixelRatioOf(context)).ceil().clamp(1, 2048),
    errorBuilder: (_, _, _) =>
        Center(child: Text(p.english ? 'Image unavailable' : '图片加载失败')),
  );
  Widget _body(api.CommentBody row, bool reply) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      if (row.avatar == null)
        Container(
          width: reply ? 24 : 40,
          height: reply ? 24 : 40,
          decoration: const BoxDecoration(
            color: Color(0x14ffffff),
            shape: BoxShape.circle,
          ),
        )
      else
        MioCover(row.avatar, size: reply ? 24 : 40, radius: 100),
      SizedBox(width: reply ? 8 : 16),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(
              spacing: 10,
              runSpacing: 4,
              children: [
                Text(
                  row.name,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text(
                  row.time,
                  style: const TextStyle(
                    fontSize: 12,
                    color: Color(0x80ffffff),
                  ),
                ),
                Text(
                  '♡ ${_count(row.likedCount)}',
                  style: const TextStyle(
                    fontSize: 12,
                    color: Color(0x80ffffff),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 6),
            SelectableText(
              row.text,
              style: const TextStyle(color: Color(0xe6ffffff)),
            ),
            if (row.images.isNotEmpty) ...[
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (var i = 0; i < row.images.length; i++)
                    MioButton(
                      palette: p,
                      label: p.english
                          ? 'Open comment image ${i + 1}'
                          : '查看评论图片 ${i + 1}',
                      padding: EdgeInsets.zero,
                      onPressed: _foreground
                          ? () => setState(() => _image = row.images[i])
                          : null,
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(8),
                        child: _media(row.images[i], preview: false),
                      ),
                    ),
                ],
              ),
            ],
          ],
        ),
      ),
    ],
  );
  Widget _comment(api.Comment row, int tab) {
    final key = (tab, row.body.id),
        expanded = _expanded.contains((tab, row.body.id));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _body(row.body, false),
        if (row.replies.isNotEmpty)
          Container(
            margin: const EdgeInsets.only(top: 12, left: 16),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: const Color(0x26000000),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (expanded)
                  for (final reply in row.replies)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: _body(reply, true),
                    ),
                _button(
                  expanded
                      ? (p.english ? 'Collapse replies' : '收起回复')
                      : (p.english
                            ? 'Show ${row.replies.length} replies'
                            : '展开 ${row.replies.length} 条回复'),
                  _foreground
                      ? () => setState(() {
                          if (!_expanded.add(key)) {
                            _expanded.remove(key);
                          }
                        })
                      : null,
                ),
              ],
            ),
          ),
      ],
    );
  }

  Widget _list(int tab) {
    final state = _tabs[tab];
    return ListView.builder(
      key: ValueKey((tab, _epoch)),
      controller: _scrolls[tab],
      padding: const EdgeInsets.all(20),
      itemCount: state.rows.length + 1,
      itemBuilder: (context, i) {
        if (i < state.rows.length) {
          return Padding(
            padding: const EdgeInsets.only(bottom: 24),
            child: _comment(state.rows[i], tab),
          );
        }
        if (_opening || state.loading) return Text(p.text('common.loading'));
        if (_unsupported) {
          return Text(
            p.english ? 'Comments are unavailable for this source' : '该来源不支持评论',
          );
        }
        if (_error != null) {
          return Column(
            children: [
              Text(_error!),
              _button(
                p.english ? 'Retry' : '重试',
                _foreground ? () => unawaited(_start()) : null,
              ),
            ],
          );
        }
        if (state.error != null) {
          return Column(
            children: [
              Text(state.error!),
              _button(
                p.english ? 'Retry' : '重试',
                _foreground ? () => unawaited(_load(tab)) : null,
              ),
            ],
          );
        }
        if (state.loaded && state.rows.isEmpty) {
          return Text(p.text('play.comments.empty'));
        }
        if (state.loaded && !state.more) {
          return Text(p.text('play.comments.noMore'));
        }
        return _button(
          p.english ? 'Load more' : '加载更多',
          _foreground && _session != null ? () => unawaited(_load(tab)) : null,
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: false,
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop) _dismiss();
    },
    child: CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
      child: MioDialogScope(
        node: _scope,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.english ? 'Comments' : '评论',
          child: DefaultTextStyle(
            style: const TextStyle(
              color: Color(0xffffffff),
              fontSize: 14,
              decoration: TextDecoration.none,
            ),
            child: SafeArea(
              child: LayoutBuilder(
                builder: (context, box) => Stack(
                  children: [
                    Positioned.fill(
                      child: GestureDetector(
                        onTap: _dismiss,
                        child: const ColoredBox(color: Color(0x66000000)),
                      ),
                    ),
                    Center(
                      child: ExcludeFocus(
                        excluding: _image != null,
                        child: ExcludeSemantics(
                          excluding: _image != null,
                          child: SizedBox(
                            width: box.maxWidth * .8,
                            height: box.maxHeight * .8,
                            child: DecoratedBox(
                              decoration: BoxDecoration(
                                color: const Color(0xf525262d),
                                borderRadius: BorderRadius.circular(24),
                              ),
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(24),
                                child: Column(
                                  children: [
                                    ConstrainedBox(
                                      constraints: BoxConstraints(
                                        maxHeight: box.maxHeight * .4,
                                      ),
                                      child: SingleChildScrollView(
                                        child: Container(
                                          constraints: const BoxConstraints(
                                            minHeight: 60,
                                          ),
                                          padding: EdgeInsets.symmetric(
                                            horizontal: math.min(
                                              24,
                                              box.maxWidth * .04,
                                            ),
                                          ),
                                          decoration: const BoxDecoration(
                                            border: Border(
                                              bottom: BorderSide(
                                                color: Color(0x14ffffff),
                                              ),
                                            ),
                                          ),
                                          child: Row(
                                            children: [
                                              Expanded(
                                                child: Wrap(
                                                  spacing:
                                                      !MioDeviceScope.wide(
                                                        context,
                                                        box.maxWidth,
                                                      )
                                                      ? 8
                                                      : 20,
                                                  children: [
                                                    for (
                                                      var tab = 0;
                                                      tab < 2;
                                                      tab++
                                                    )
                                                      Semantics(
                                                        selected:
                                                            _active == tab,
                                                        child: Container(
                                                          decoration: BoxDecoration(
                                                            border: Border(
                                                              bottom: BorderSide(
                                                                color:
                                                                    _active ==
                                                                        tab
                                                                    ? p.brand
                                                                    : const Color(
                                                                        0x00000000,
                                                                      ),
                                                                width: 3,
                                                              ),
                                                            ),
                                                          ),
                                                          child: MioButton(
                                                            palette: p,
                                                            label:
                                                                '${p.text(tab == 0 ? 'play.comments.hot' : 'play.comments.latest')} ${_count(_tabs[tab].total)}',
                                                            onPressed:
                                                                _foreground
                                                                ? () => _select(
                                                                    tab,
                                                                  )
                                                                : null,
                                                            child: Text(
                                                              '${p.text(tab == 0 ? 'play.comments.hot' : 'play.comments.latest')} ${_count(_tabs[tab].total)}',
                                                              style: TextStyle(
                                                                color:
                                                                    _active ==
                                                                        tab
                                                                    ? const Color(
                                                                        0xffffffff,
                                                                      )
                                                                    : const Color(
                                                                        0x99ffffff,
                                                                      ),
                                                              ),
                                                            ),
                                                          ),
                                                        ),
                                                      ),
                                                  ],
                                                ),
                                              ),
                                              MioButton(
                                                palette: p,
                                                label: p.text('common.close'),
                                                onPressed: _dismiss,
                                                child: const Text(
                                                  '×',
                                                  style: TextStyle(
                                                    fontSize: 24,
                                                  ),
                                                ),
                                              ),
                                            ],
                                          ),
                                        ),
                                      ),
                                    ),
                                    Expanded(
                                      child: IndexedStack(
                                        index: _active,
                                        children: [_list(0), _list(1)],
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
                    if (_image != null)
                      Positioned.fill(
                        child: BlockSemantics(
                          child: MioDialogScope(
                            node: _imageScope,
                            child: ColoredBox(
                              color: const Color(0xe6000000),
                              child: Stack(
                                children: [
                                  Positioned.fill(
                                    child: GestureDetector(
                                      onTap: _dismiss,
                                      child: Padding(
                                        padding: EdgeInsets.all(
                                          math.min(
                                                box.maxWidth,
                                                box.maxHeight,
                                              ) *
                                              .075,
                                        ),
                                        child: _media(_image!, preview: true),
                                      ),
                                    ),
                                  ),
                                  Positioned(
                                    top: 8,
                                    right: 8,
                                    child: _button(
                                      p.text('common.close'),
                                      _dismiss,
                                    ),
                                  ),
                                ],
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
        ),
      ),
    ),
  );
}

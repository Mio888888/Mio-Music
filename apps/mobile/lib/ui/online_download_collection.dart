import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'online_downloads.dart';
import 'resources.dart';

Future<OnlineDownloadReport?> showCompleteOnlineDownloads({
  required BuildContext context,
  required api.OnlinePlaylistSession session,
  required CoreController core,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  api.OnlineDownloadCollection? intent;
  try {
    intent = await session.beginCompleteDownload();
    final state = WidgetsBinding.instance.lifecycleState;
    if (!context.mounted ||
        !isCurrent() ||
        (state != null && state != AppLifecycleState.resumed)) {
      return null;
    }
    final route = RawDialogRoute<OnlineDownloadReport>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => _CompleteDownloads(
        intent: intent!,
        core: core,
        palette: palette,
        isCurrent: isCurrent,
      ),
    );
    final result = await Navigator.of(context)
        .push<OnlineDownloadReport>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    intent?.dispose();
  }
}

class _CompleteDownloads extends StatefulWidget {
  final api.OnlineDownloadCollection intent;
  final CoreController core;
  final MioPalette palette;
  final bool Function() isCurrent;
  const _CompleteDownloads({
    required this.intent,
    required this.core,
    required this.palette,
    required this.isCurrent,
  });
  @override
  State<_CompleteDownloads> createState() => _CompleteDownloadsState();
}

class _CompleteDownloadsState extends State<_CompleteDownloads>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _loading = true,
      _busy = false,
      _failed = false,
      _closing = false,
      _foreground = true;
  int? _count;
  String? _error;
  OnlineDownloadReport? _report;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    if (!_foreground || !widget.isCurrent()) {
      _invalidate();
      _loading = false;
    } else {
      unawaited(_load());
    }
  }

  void _invalidate() {
    widget.intent.close();
    _failed = true;
    if (_report == null) {
      _error = p.english
          ? 'Download selection closed. Close and reopen.'
          : '下载选择已关闭，请关闭后重新打开。';
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted) return;
    setState(() {
      _foreground = state == AppLifecycleState.resumed;
      if (state == AppLifecycleState.paused ||
          state == AppLifecycleState.detached ||
          (state == AppLifecycleState.resumed && !widget.isCurrent())) {
        _invalidate();
      }
    });
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    widget.intent.close();
    Navigator.of(context).pop(_report);
  }

  Future<void> _load() async {
    try {
      final count = await widget.intent.load();
      if (!mounted || _closing || _failed) return;
      if (!_foreground || !widget.isCurrent()) {
        setState(_invalidate);
        return;
      }
      setState(() => _count = count);
    } catch (_) {
      if (mounted && !_closing && !_failed) {
        setState(() {
          _invalidate();
          _error = p.english
              ? 'Complete read failed, source changed, or pagination exceeded limits. No downloads were added. Close and reopen.'
              : '完整读取失败、来源变化或分页超限，没有添加下载。请关闭后重新打开。';
        });
      }
    } finally {
      if (mounted && !_closing) setState(() => _loading = false);
    }
  }

  Future<void> _run() async {
    if (_busy ||
        _loading ||
        _failed ||
        !_foreground ||
        _count == null ||
        _count == 0 ||
        _report != null) {
      return;
    }
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final report = await showCompleteDownloadChoices(
        context: context,
        collection: widget.intent,
        count: _count!,
        core: widget.core,
        palette: p,
        isCurrent: () =>
            mounted &&
            !_closing &&
            !_failed &&
            _foreground &&
            widget.isCurrent(),
      );
      if (!mounted || _closing) return;
      setState(() {
        _report = report;
        _error = null;
        _failed = true;
      });
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _invalidate();
          _error = p.english
              ? 'Download selection stopped. Accepted tasks continue; check Downloads for their status.'
              : '下载选择已停止。已接受的任务会继续，请在下载页查看状态。';
        });
      }
    } finally {
      widget.intent.close();
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    widget.intent.close();
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    super.dispose();
  }

  Widget _button(String label, VoidCallback? action) => MioButton(
    palette: p,
    label: label,
    onPressed: action,
    child: Text(label),
  );
  @override
  Widget build(BuildContext context) => PopScope(
    canPop: false,
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop) _dismiss();
    },
    child: CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
      child: FocusScope(
        node: _scope,
        autofocus: true,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.english ? 'Download complete collection' : '下载完整内容',
          child: DefaultTextStyle(
            style: TextStyle(
              color: p.color('--td-text-color-primary'),
              fontSize: 14,
              decoration: TextDecoration.none,
            ),
            child: LayoutBuilder(
              builder: (context, box) {
                final media = MediaQuery.of(context),
                    narrow = !MioDeviceScope.wide(context, box.maxWidth);
                return Stack(
                  children: [
                    Positioned.fill(
                      child: ModalBarrier(
                        color: p.color('--td-mask-active'),
                        dismissible: !_busy,
                        onDismiss: _dismiss,
                        semanticsLabel: p.text('common.close'),
                      ),
                    ),
                    Positioned.fill(
                      bottom: media.viewInsets.bottom,
                      child: SafeArea(
                        minimum: const EdgeInsets.all(12),
                        child: Align(
                          alignment: narrow
                              ? Alignment.bottomCenter
                              : Alignment.center,
                          child: Container(
                            width: math.min(
                              math.max(0, box.maxWidth - 24),
                              560,
                            ),
                            constraints: BoxConstraints(
                              maxHeight: math.max(
                                0,
                                box.maxHeight -
                                    media.viewInsets.bottom -
                                    media.padding.vertical -
                                    24,
                              ),
                            ),
                            padding: EdgeInsets.all(narrow ? 16 : 24),
                            decoration: BoxDecoration(
                              color: p.surface,
                              borderRadius: BorderRadius.circular(
                                narrow ? 22 : 14,
                              ),
                            ),
                            child: SingleChildScrollView(
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    p.english
                                        ? 'Download complete collection'
                                        : '下载完整内容',
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    p.english
                                        ? 'Read every page first (up to 400 pages, 12,000 songs and 32 MiB metadata). Confirm, then choose each song’s quality. Accepted tasks continue if selection stops.'
                                        : '先读取全部分页（最多400页、12000首、32 MiB元数据）。确认后逐首选择音质；停止选择不取消已接受的任务。',
                                  ),
                                  const SizedBox(height: 16),
                                  if (_loading)
                                    Semantics(
                                      liveRegion: true,
                                      child: Text(
                                        p.english
                                            ? 'Reading all pages…'
                                            : '正在读取全部分页…',
                                      ),
                                    ),
                                  if (_count != null)
                                    Text(
                                      p.english
                                          ? '$_count songs read completely.'
                                          : '已完整读取 $_count 首歌曲。',
                                    ),
                                  if (_error != null)
                                    Semantics(
                                      liveRegion: true,
                                      child: Text(_error!),
                                    ),
                                  if (_report != null)
                                    Semantics(
                                      liveRegion: true,
                                      child: Text(_report!.message(p)),
                                    ),
                                  const SizedBox(height: 16),
                                  Wrap(
                                    spacing: 12,
                                    runSpacing: 8,
                                    children: [
                                      _button(
                                        p.text('common.close'),
                                        _busy ? null : _dismiss,
                                      ),
                                      if (_report == null)
                                        _button(
                                          p.english
                                              ? 'Choose download qualities'
                                              : '确认并选择下载音质',
                                          !_loading &&
                                                  !_busy &&
                                                  !_failed &&
                                                  _foreground &&
                                                  (_count ?? 0) > 0
                                              ? () => unawaited(_run())
                                              : null,
                                        ),
                                    ],
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    ),
  );
}

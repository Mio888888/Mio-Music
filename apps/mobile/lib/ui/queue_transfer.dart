import 'device.dart';

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

Future<String?> showQueueTransfer({
  required BuildContext context,
  required MioPalette palette,
  required api.PlayerSnapshot snapshot,
  required bool exporting,
  required bool Function() isCurrent,
}) async {
  api.QueueDocumentSession? intent;
  try {
    intent = await api.beginQueueDocument(
      queueRevision: snapshot.queueRevision,
      sourceRevision: snapshot.sourceRevision,
    );
    if (!context.mounted || !isCurrent()) return null;
    final route = RawDialogRoute<String>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => QueueTransferDialog(
        intent: intent!,
        palette: palette,
        exporting: exporting,
        isCurrent: isCurrent,
      ),
    );
    final result = await Navigator.of(context).push<String>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    intent?.dispose();
  }
}

class QueueTransferDialog extends StatefulWidget {
  final api.QueueDocumentSession intent;
  final MioPalette palette;
  final bool exporting;
  final bool Function() isCurrent;
  const QueueTransferDialog({
    super.key,
    required this.intent,
    required this.palette,
    required this.exporting,
    required this.isCurrent,
  });
  @override
  State<QueueTransferDialog> createState() => _QueueTransferDialogState();
}

class _QueueTransferDialogState extends State<QueueTransferDialog>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false,
      _failed = false,
      _closing = false,
      _native = false,
      _foreground = true;
  int? _count;
  String? _error, _message;
  QueueDocumentSelection? _selection;
  Completer<void>? _resume;
  MioPalette get p => widget.palette;
  bool get _current => mounted && !_closing && widget.isCurrent();
  @override
  void initState() {
    super.initState();
    _foreground = ![
      AppLifecycleState.paused,
      AppLifecycleState.detached,
    ].contains(WidgetsBinding.instance.lifecycleState);
    WidgetsBinding.instance.addObserver(this);
  }

  void _wake() {
    final waiter = _resume;
    _resume = null;
    if (waiter != null && !waiter.isCompleted) waiter.complete();
  }

  Future<bool> _visible() async {
    if (!_current) return false;
    if (!_foreground) {
      _resume ??= Completer<void>();
      await _resume!.future;
    }
    return _current && _foreground && !_failed;
  }

  void _invalidate() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    if (mounted && !_closing) {
      setState(() {
        _failed = true;
        _error = p.english
            ? 'Transfer closed. Close and reopen to continue.'
            : '传输会话已关闭，请关闭并重新打开。';
      });
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      _wake();
      if (!widget.isCurrent()) _invalidate();
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _foreground = false;
      if (!_native || state == AppLifecycleState.detached) _invalidate();
    }
  }

  void _dismiss() {
    if (!mounted || _closing || _busy) return;
    _closing = true;
    widget.intent.close();
    _wake();
    Navigator.of(context).pop(_message);
  }

  Future<void> _run(Future<void> Function() action) async {
    if (!_current || !_foreground || _busy || _failed) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (_) {
      widget.intent.close();
      if (await _visible()) {
        setState(() {
          _failed = true;
          _error = widget.exporting
              ? (p.english
                    ? 'Export failed or queue changed. The destination may be incomplete. Close and reopen to retry.'
                    : '导出失败或队列已变化，目标文件可能不完整。请关闭并重新打开后重试。')
              : (p.english
                    ? 'Import failed or queue changed. Use encrypted .cpl/.cmpl or encrypted clipboard text. Close and reopen to retry.'
                    : '导入失败或队列已变化。请使用加密.cpl/.cmpl或加密剪贴板文本，关闭并重新打开后重试。');
        });
      }
    } finally {
      final selection = _selection;
      _selection = null;
      unawaited(selection?.cancel());
      if (mounted && !_closing) {
        setState(() {
          _busy = false;
          _native = false;
        });
      }
    }
  }

  Future<void> _export(bool file) => _run(() async {
    if (file) {
      final doc = await widget.intent.exportFile();
      if (!await _visible()) return;
      final selection = QueueDocumentSelection();
      _selection = selection;
      _native = true;
      final saved = await selection.save(doc.name, doc.bytes);
      if (!await _visible() || !saved) return;
      setState(
        () => _message = p.english ? 'Queue file exported.' : '队列文件已导出。',
      );
    } else {
      final text = await widget.intent.exportText();
      if (!await _visible()) return;
      await Clipboard.setData(ClipboardData(text: text));
      if (!await _visible()) return;
      setState(() => _message = p.text('settings.playlist.clipboardSuccess'));
    }
  });
  Future<void> _read(bool file) => _run(() async {
    if (_count != null) return;
    int count;
    if (file) {
      final selection = QueueDocumentSelection();
      _selection = selection;
      _native = true;
      final doc = await selection.read();
      if (!await _visible() || doc == null) return;
      _native = false;
      count = await widget.intent.prepareFile(
        bytes: doc.bytes,
        extension_: doc.name.split('.').last.toLowerCase(),
      );
    } else {
      final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text ?? '';
      if (!await _visible()) return;
      if (text.isEmpty || utf8.encode(text).length > 64 * 1024 * 1024) {
        throw const PluginFileException('file_too_large');
      }
      count = await widget.intent.prepareText(text: text);
    }
    if (!await _visible()) return;
    setState(() => _count = count);
  });
  Future<void> _merge() => _run(() async {
    if (_count == null) return;
    final result = await widget.intent.merge();
    // Accepted mutation returns its actual outcome even if the app paused meanwhile.
    if (!mounted || _closing) return;
    _closing = true;
    Navigator.of(context).pop(
      p.english
          ? '${result.added} added; ${result.skipped} duplicate or unavailable rows skipped.'
          : '已添加 ${result.added} 首，跳过 ${result.skipped} 行重复或不可用歌曲。',
    );
  });
  @override
  void dispose() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    super.dispose();
  }

  Widget _button(String text, VoidCallback? action) =>
      MioButton(palette: p, label: text, onPressed: action, child: Text(text));
  Widget _choice(String key, VoidCallback? action) => Container(
    decoration: BoxDecoration(
      border: Border.all(color: p.border),
      borderRadius: BorderRadius.circular(8),
    ),
    child: MioButton(
      palette: p,
      label: p.text('settings.playlist.$key'),
      onPressed: action,
      insetFocusBorder: false,
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            p.text('settings.playlist.$key'),
            style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          Text(
            p.text('settings.playlist.${key}Desc'),
            style: TextStyle(fontSize: 14, height: 1.5, color: p.secondary),
          ),
        ],
      ),
    ),
  );
  @override
  Widget build(BuildContext context) {
    final enabled = !_busy && !_failed && _foreground;
    final title = p.text(
      widget.exporting
          ? 'settings.playlist.exportPlaylist'
          : 'settings.playlist.importPlaylist',
    );
    return PopScope(
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
            label: title,
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
                          color: narrow
                              ? const Color(0x99000000)
                              : p.color('--td-mask-active'),
                          dismissible: !_busy,
                          onDismiss: _dismiss,
                          semanticsLabel: p.text('common.cancel'),
                        ),
                      ),
                      Positioned.fill(
                        bottom: media.viewInsets.bottom,
                        child: SafeArea(
                          minimum: EdgeInsets.fromLTRB(
                            narrow ? 16 : 12,
                            narrow ? box.maxHeight * .15 : 12,
                            narrow ? 16 : 12,
                            12,
                          ),
                          child: Align(
                            alignment: narrow
                                ? Alignment.topCenter
                                : Alignment.center,
                            child: Container(
                              width: math.min(
                                math.max(0, box.maxWidth - (narrow ? 32 : 24)),
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
                              padding: EdgeInsets.all(narrow ? 32 : 24),
                              decoration: BoxDecoration(
                                color: narrow
                                    ? p.color('--td-bg-color-container')
                                    : p.surface,
                                borderRadius: BorderRadius.circular(
                                  narrow ? 22 : 14,
                                ),
                              ),
                              child: SingleChildScrollView(
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    if (narrow) ...[
                                      Text(
                                        title,
                                        style: const TextStyle(
                                          fontSize: 17,
                                          fontWeight: FontWeight.w600,
                                        ),
                                      ),
                                      const SizedBox(height: 24),
                                      Text(
                                        p.text(
                                          'settings.playlist.${widget.exporting ? 'selectExportMethod' : 'selectImportMethod'}',
                                        ),
                                      ),
                                      const SizedBox(height: 16),
                                      if (_count == null) ...[
                                        _choice(
                                          widget.exporting
                                              ? 'exportToFile'
                                              : 'importFromFile',
                                          enabled
                                              ? () => unawaited(
                                                  widget.exporting
                                                      ? _export(true)
                                                      : _read(true),
                                                )
                                              : null,
                                        ),
                                        const SizedBox(height: 12),
                                        _choice(
                                          widget.exporting
                                              ? 'copyToClipboard'
                                              : 'importFromClipboard',
                                          enabled
                                              ? () => unawaited(
                                                  widget.exporting
                                                      ? _export(false)
                                                      : _read(false),
                                                )
                                              : null,
                                        ),
                                      ] else ...[
                                        Text(
                                          p.english
                                              ? '$_count rows selected.'
                                              : '已选择 $_count 行。',
                                        ),
                                        _button(
                                          p.english ? 'Confirm merge' : '确认合并',
                                          enabled
                                              ? () => unawaited(_merge())
                                              : null,
                                        ),
                                      ],
                                      if (_busy)
                                        Text(p.english ? 'Working…' : '正在处理…'),
                                      if (_error != null || _message != null)
                                        Text(
                                          _error ?? _message!,
                                          style: TextStyle(color: p.secondary),
                                        ),
                                      const SizedBox(height: 24),
                                      Align(
                                        alignment: Alignment.centerRight,
                                        child: _button(
                                          p.text('common.cancel'),
                                          _busy ? null : _dismiss,
                                        ),
                                      ),
                                    ] else ...[
                                      Text(
                                        title,
                                        style: const TextStyle(
                                          fontSize: 20,
                                          fontWeight: FontWeight.w600,
                                        ),
                                      ),
                                      const SizedBox(height: 12),
                                      Text(
                                        widget.exporting
                                            ? (p.english
                                                  ? 'Export the current queue as .cmpl or encrypted clipboard text.'
                                                  : '将当前队列导出为.cmpl文件或加密剪贴板文本。')
                                            : (p.english
                                                  ? 'Import encrypted .cpl/.cmpl or clipboard text (≤64 MiB / 20,000 rows). Merge appends new supported songs, keeping current playback. Saved queue capacity: 2,048 songs.'
                                                  : '导入加密.cpl/.cmpl或剪贴板文本（最多64 MiB / 20,000行）。合并时追加可用新歌曲，保留当前播放；保存队列上限为2,048首。'),
                                      ),
                                      if (_count != null)
                                        Padding(
                                          padding: const EdgeInsets.symmetric(
                                            vertical: 12,
                                          ),
                                          child: Text(
                                            p.english
                                                ? '$_count rows selected. Confirm merge; duplicates and unavailable songs will be skipped.'
                                                : '已选择 $_count 行。确认后合并，跳过重复和不可用歌曲。',
                                          ),
                                        ),
                                      if (_busy)
                                        Semantics(
                                          liveRegion: true,
                                          child: Padding(
                                            padding: const EdgeInsets.symmetric(
                                              vertical: 12,
                                            ),
                                            child: Text(
                                              p.english ? 'Working…' : '正在处理…',
                                            ),
                                          ),
                                        ),
                                      if (_error != null || _message != null)
                                        Semantics(
                                          liveRegion: true,
                                          child: Padding(
                                            padding: const EdgeInsets.symmetric(
                                              vertical: 12,
                                            ),
                                            child: Text(
                                              _error ?? _message!,
                                              style: TextStyle(
                                                color: _error == null
                                                    ? p.secondary
                                                    : p.color(
                                                        '--td-error-color',
                                                      ),
                                              ),
                                            ),
                                          ),
                                        ),
                                      const SizedBox(height: 16),
                                      Wrap(
                                        spacing: 8,
                                        runSpacing: 8,
                                        children: [
                                          if (widget.exporting) ...[
                                            _button(
                                              p.english
                                                  ? 'Export file'
                                                  : '导出文件',
                                              enabled
                                                  ? () =>
                                                        unawaited(_export(true))
                                                  : null,
                                            ),
                                            _button(
                                              p.english
                                                  ? 'Copy to clipboard'
                                                  : '复制到剪贴板',
                                              enabled
                                                  ? () => unawaited(
                                                      _export(false),
                                                    )
                                                  : null,
                                            ),
                                          ] else if (_count == null) ...[
                                            _button(
                                              p.english
                                                  ? 'Choose file'
                                                  : '选择文件',
                                              enabled
                                                  ? () => unawaited(_read(true))
                                                  : null,
                                            ),
                                            _button(
                                              p.english
                                                  ? 'Read clipboard'
                                                  : '读取剪贴板',
                                              enabled
                                                  ? () =>
                                                        unawaited(_read(false))
                                                  : null,
                                            ),
                                          ] else
                                            _button(
                                              p.english
                                                  ? 'Confirm merge'
                                                  : '确认合并',
                                              enabled
                                                  ? () => unawaited(_merge())
                                                  : null,
                                            ),
                                          _button(
                                            p.text('common.cancel'),
                                            _busy ? null : _dismiss,
                                          ),
                                        ],
                                      ),
                                    ],
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
}

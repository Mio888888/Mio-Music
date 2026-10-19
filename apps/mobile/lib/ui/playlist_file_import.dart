import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

Future<api.PlaylistInfo?> showPlaylistFileImport({
  required BuildContext context,
  required MioPalette palette,
  required bool Function() isCurrent,
  api.PlayerSnapshot? queue,
  api.PlaylistFileImport? prepared,
  String? networkName,
}) async {
  api.PlaylistFileImport? intent = prepared;
  try {
    intent ??= queue == null
        ? await api.beginPlaylistFileImport()
        : await api.beginPlaylistQueueImport(
            queueRevision: queue.queueRevision,
            sourceRevision: queue.sourceRevision,
          );
    final count = queue == null && prepared == null
        ? null
        : await intent.preparedCount();
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    if (!context.mounted ||
        !isCurrent() ||
        (lifecycle != null && lifecycle != AppLifecycleState.resumed)) {
      return null;
    }
    final route = RawDialogRoute<api.PlaylistInfo>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => PlaylistFileImportDialog(
        intent: intent!,
        palette: palette,
        isCurrent: isCurrent,
        queueCount: count,
        networkName: networkName,
      ),
    );
    final result = await Navigator.of(context).push<api.PlaylistInfo>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    intent?.dispose();
  }
}

class PlaylistFileImportDialog extends StatefulWidget {
  final api.PlaylistFileImport intent;
  final MioPalette palette;
  final bool Function() isCurrent;
  final int? queueCount;
  final String? networkName;
  const PlaylistFileImportDialog({
    super.key,
    required this.intent,
    required this.palette,
    required this.isCurrent,
    this.queueCount,
    this.networkName,
  });
  @override
  State<PlaylistFileImportDialog> createState() =>
      _PlaylistFileImportDialogState();
}

class _PlaylistFileImportDialogState extends State<PlaylistFileImportDialog>
    with WidgetsBindingObserver {
  final _name = TextEditingController(), _description = TextEditingController();
  final _nameFocus = FocusNode(), _descriptionFocus = FocusNode();
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false, _failed = false, _closing = false;
  int? _count;
  bool _picking = false, _foreground = true;
  PlaylistDocumentSelection? _selection;
  Completer<void>? _resume;
  bool get _current => mounted && !_closing && widget.isCurrent();
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
    return _current && _foreground;
  }

  String? _error;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _foreground =
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.paused &&
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.detached;
    if (!_foreground) {
      _failed = true;
      widget.intent.close();
      _error = p.english
          ? 'Import closed in background. Close and reopen.'
          : '导入已在后台关闭，请关闭并重新打开。';
    }
    _count = widget.queueCount;
    _name.text = widget.networkName != null
        ? p
              .text('music.songlist.importedFrom')
              .replaceAll('{platform}', widget.networkName!)
        : widget.queueCount == null
        ? (p.english ? 'Imported playlist' : '导入的歌单')
        : (p.english ? 'Playing queue' : '播放列表');
    if (_count != null) {
      _description.text = widget.networkName != null
          ? p
                .text('music.songlist.fromNetworkImport')
                .replaceAll('{platform}', widget.networkName!)
                .replaceAll('{count}', '$_count')
          : p
                .text('music.songlist.fromPlaylistImport')
                .replaceAll('{count}', '$_count');
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nameFocus.requestFocus();
    });
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    Navigator.of(context).pop();
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
      if (!_picking || state == AppLifecycleState.detached) _invalidate();
    }
  }

  void _invalidate() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    if (mounted) {
      setState(() {
        _failed = true;
        _error = p.english
            ? 'Import closed. Close and reopen to continue.'
            : '导入会话已关闭，请关闭并重新打开。';
      });
    }
  }

  Future<void> _choose() async {
    if (!_current || !_foreground || _busy || _failed || _count != null) return;
    final selection = PlaylistDocumentSelection();
    _selection = selection;
    setState(() {
      _busy = true;
      _picking = true;
      _error = null;
    });
    try {
      final document = await selection.read();
      if (!await _visible() || _failed || document == null) return;
      _picking = false;
      final count = await widget.intent.prepare(bytes: document.bytes);
      if (!await _visible() || _failed) return;
      final stem = document.name.replaceFirst(
        RegExp(r'\.(json|cpl|cmpl)$', caseSensitive: false),
        '',
      );
      setState(() {
        _count = count;
        _name.text = String.fromCharCodes(stem.runes.take(50));
      });
      _nameFocus.requestFocus();
    } catch (error) {
      if (await _visible() && !_failed) {
        if (error is! PluginFileException) {
          widget.intent.close();
          _failed = true;
        }
        setState(
          () => _error = p.english
              ? 'Unable to import. Choose a UTF-8 JSON song array ≤32 MiB / 20,000 songs. Encrypted queue files use the separate queue workflow.'
              : '无法导入。请选择 UTF-8 JSON 歌曲数组（最多32 MiB / 20,000首）。加密队列文件使用独立队列流程。',
        );
      }
    } finally {
      unawaited(selection.cancel());
      if (identical(_selection, selection)) _selection = null;
      if (mounted && !_closing) {
        setState(() {
          _busy = false;
          _picking = false;
        });
      }
    }
  }

  Future<void> _save() async {
    if (_busy || _failed || _closing || _count == null || !_foreground) return;
    if (!widget.isCurrent()) {
      _invalidate();
      return;
    }
    final name = _name.text.trim(), description = _description.text.trim();
    if (name.isEmpty ||
        name.length > 50 ||
        description.length > 200 ||
        name.contains('\u0000') ||
        description.contains('\u0000')) {
      setState(
        () => _error = p.english
            ? 'Enter a name up to 50 and a description up to 200 characters.'
            : '请填写名称（最多50）与描述（最多200），不能包含空字符。',
      );
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final saved = await widget.intent.save(
        name: name,
        description: description,
      );
      if (!mounted || _closing) return;
      _closing = true;
      Navigator.of(context).pop(saved);
    } catch (_) {
      widget.intent.close();
      if (mounted && !_closing) {
        setState(() {
          _failed = true;
          _error = p.english
              ? 'Import failed or runtime changed. Close and reopen before retrying.'
              : '导入失败或运行时已变化，请关闭并重新打开后重试。';
        });
      }
    } finally {
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    WidgetsBinding.instance.removeObserver(this);
    _name.dispose();
    _description.dispose();
    _nameFocus.dispose();
    _descriptionFocus.dispose();
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
          label: widget.networkName != null
              ? p.text('music.songlist.fromNetwork')
              : widget.queueCount == null
              ? (p.english ? 'Import playlist file' : '导入歌单文件')
              : (p.english ? 'Save playing queue' : '保存当前队列'),
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
                        semanticsLabel: p.text('common.cancel'),
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
                                    widget.networkName != null
                                        ? p.text('music.songlist.fromNetwork')
                                        : p.english
                                        ? (widget.queueCount == null
                                              ? 'Import playlist file'
                                              : 'Save playing queue')
                                        : (widget.queueCount == null
                                              ? '导入歌单文件'
                                              : '保存当前队列'),
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    widget.networkName != null
                                        ? (p.english
                                              ? '$_count songs read. Save creates a new playlist.'
                                              : '已读取 $_count 首歌曲，确认后新建歌单。')
                                        : widget.queueCount != null
                                        ? (p.english
                                              ? '$_count captured queue rows will be saved. A changed queue requires reopening.'
                                              : '将保存已捕获的 $_count 行队列；队列发生变化时需重新打开。')
                                        : p.english
                                        ? (_count == null
                                              ? 'Choose a plain JSON song array (.json/.cpl/.cmpl), up to 32 MiB and 20,000 songs.'
                                              : '$_count rows selected. Save creates a new playlist; duplicate IDs are kept once.')
                                        : (_count == null
                                              ? '选择普通 JSON 歌曲数组（.json/.cpl/.cmpl），最多32 MiB、20,000首。'
                                              : '已选择 $_count 行，确认后新建歌单；重复ID只保留一次。'),
                                  ),
                                  if (_count == null)
                                    _button(
                                      p.english ? 'Choose file' : '选择文件',
                                      _busy || _failed
                                          ? null
                                          : () => unawaited(_choose()),
                                    ),
                                  const SizedBox(height: 16),
                                  Text(
                                    '${p.text('music.songlist.playlistName')} · 50',
                                  ),
                                  const SizedBox(height: 8),
                                  MioInput(
                                    palette: p,
                                    controller: _name,
                                    focusNode: _nameFocus,
                                    placeholder: p.text(
                                      'music.songlist.playlistNamePlaceholder',
                                    ),
                                    readOnly: _busy || _failed,
                                    height: 44,
                                    textInputAction: TextInputAction.next,
                                    onSubmitted: (_) =>
                                        _descriptionFocus.requestFocus(),
                                  ),
                                  const SizedBox(height: 16),
                                  Text(
                                    '${p.text('music.songlist.playlistDesc')} · 200',
                                  ),
                                  const SizedBox(height: 8),
                                  MioInput(
                                    palette: p,
                                    controller: _description,
                                    focusNode: _descriptionFocus,
                                    placeholder: p.text(
                                      'music.songlist.playlistDescPlaceholder',
                                    ),
                                    readOnly: _busy || _failed,
                                    height: 112,
                                    maxLines: 4,
                                    keyboardType: TextInputType.multiline,
                                    textInputAction: TextInputAction.newline,
                                  ),
                                  if (_error != null)
                                    Padding(
                                      padding: const EdgeInsets.symmetric(
                                        vertical: 12,
                                      ),
                                      child: Semantics(
                                        liveRegion: true,
                                        child: Text(
                                          _error!,
                                          style: TextStyle(
                                            color: p.color('--td-error-color'),
                                          ),
                                        ),
                                      ),
                                    ),
                                  if (_busy)
                                    Semantics(
                                      liveRegion: true,
                                      child: Text(p.text('common.loading')),
                                    ),
                                  const SizedBox(height: 16),
                                  Wrap(
                                    spacing: 12,
                                    runSpacing: 8,
                                    children: [
                                      _button(
                                        p.text('common.cancel'),
                                        _busy ? null : _dismiss,
                                      ),
                                      _button(
                                        p.english ? 'Save' : '保存',
                                        _busy || _failed || _count == null
                                            ? null
                                            : () => unawaited(_save()),
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

import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

Future<api.PlaylistInfo?> showPlaylistCoverEditor({
  required BuildContext context,
  required api.LibrarySession session,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  api.PlaylistCoverEditor? intent;
  try {
    intent = await session.prepareCoverEditor();
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    if (!context.mounted ||
        !isCurrent() ||
        (lifecycle != null && lifecycle != AppLifecycleState.resumed)) {
      return null;
    }
    final route = RawDialogRoute<api.PlaylistInfo>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) =>
          _CoverEditor(intent: intent!, palette: palette, isCurrent: isCurrent),
    );
    final result = await Navigator.of(context).push<api.PlaylistInfo>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    intent?.dispose();
  }
}

class _CoverEditor extends StatefulWidget {
  final api.PlaylistCoverEditor intent;
  final MioPalette palette;
  final bool Function() isCurrent;
  const _CoverEditor({
    required this.intent,
    required this.palette,
    required this.isCurrent,
  });
  @override
  State<_CoverEditor> createState() => _CoverEditorState();
}

class _CoverEditorState extends State<_CoverEditor>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false,
      _picking = false,
      _foreground = true,
      _failed = false,
      _closing = false;
  CoverDocumentSelection? _selection;
  Completer<void>? _resume;
  Uint8List? _preview;
  String? _error;
  MioPalette get p => widget.palette;
  bool get _current => mounted && !_closing && widget.isCurrent();
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
    }
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
    return _current && _foreground;
  }

  void _invalidate() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    if (mounted) {
      setState(() {
        _failed = true;
        _error = p.english
            ? 'Cover editor closed. Close and reopen.'
            : '封面编辑已关闭，请关闭并重新打开。';
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
      if (!_picking || state == AppLifecycleState.detached) _invalidate();
    }
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    Navigator.of(context).pop();
  }

  Future<void> _choose() async {
    if (!_current || !_foreground || _busy || _failed) return;
    final selection = CoverDocumentSelection();
    _selection = selection;
    setState(() {
      _busy = true;
      _picking = true;
      _error = null;
    });
    try {
      final file = await selection.read();
      if (!await _visible() || _failed || file == null) return;
      _picking = false;
      final preview = await widget.intent.prepare(
        bytes: file.bytes,
        extension_: file.name.split('.').last.toLowerCase(),
      );
      if (!await _visible() || _failed) return;
      setState(() => _preview = preview);
    } catch (error) {
      if (await _visible() && !_failed) {
        final invalidImage = error.toString().contains('invalid_image');
        if (error is! PluginFileException && !invalidImage) {
          widget.intent.close();
          _failed = true;
        }
        setState(
          () => _error = p.english
              ? 'Choose PNG/JPG/GIF/WebP up to 5 MiB. Image limits or playlist changes can reject the edit.'
              : '请选择最大5 MiB的PNG/JPG/GIF/WebP。图片超限或歌单变化时会拒绝编辑。',
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
    if (!_current || !_foreground || _busy || _failed || _preview == null) {
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final saved = await widget.intent.save();
      if (!mounted || _closing) return;
      _closing = true;
      Navigator.of(context).pop(saved);
    } catch (_) {
      _invalidate();
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
      child: MioDialogScope(
        node: _scope,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.text('music.list.editCover'),
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
                                    p.text('music.list.editCover'),
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    p.english
                                        ? 'Original image is kept; preview shows a still frame. Save replaces only the playlist cover.'
                                        : '保留原图，预览显示静态帧。确认后只更换歌单封面。',
                                  ),
                                  const SizedBox(height: 16),
                                  if (_preview != null)
                                    Center(
                                      child: Image.memory(
                                        _preview!,
                                        width: math.max(
                                          0,
                                          math.min(box.maxWidth - 64, 256),
                                        ),
                                        height: math.min(
                                          box.maxWidth - 64,
                                          256,
                                        ),
                                        fit: BoxFit.cover,
                                        errorBuilder: (_, _, _) => Text(
                                          p.english
                                              ? 'Preview unavailable'
                                              : '无法预览',
                                        ),
                                      ),
                                    ),
                                  const SizedBox(height: 16),
                                  _button(
                                    p.english ? 'Choose image' : '选择图片',
                                    _busy || _failed
                                        ? null
                                        : () => unawaited(_choose()),
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
                                        p.text('common.save'),
                                        _busy || _failed || _preview == null
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

import 'device.dart';

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import '../platform_files.dart';
import 'controls.dart';
import 'resources.dart';

Future<api.LocalTagSaveResult?> showLocalTagEditor({
  required BuildContext context,
  required api.LocalTrackSession session,
  required String id,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  api.LocalTagEditor? intent;
  final operations = _TagOperations();
  try {
    intent = await session.prepareTagEditor(id: id);
    final draft = await intent.draft();
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    if (!context.mounted ||
        !isCurrent() ||
        (lifecycle != null && lifecycle != AppLifecycleState.resumed)) {
      return null;
    }
    final route = RawDialogRoute<api.LocalTagSaveResult>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => _TagEditor(
        intent: intent!,
        operations: operations,
        draft: draft,
        palette: palette,
        isCurrent: isCurrent,
      ),
    );
    final result = await Navigator.of(context)
        .push<api.LocalTagSaveResult>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    await operations.pending;
    intent?.dispose();
  }
}

// Retain the opaque handle if navigation removes the route during a Rust call.
class _TagOperations {
  Future<void>? pending;
  Future<T> run<T>(Future<T> Function() action) async {
    final done = Completer<void>();
    pending = done.future;
    try {
      return await action();
    } finally {
      done.complete();
    }
  }
}

class _TagEditor extends StatefulWidget {
  final api.LocalTagEditor intent;
  final _TagOperations operations;
  final api.LocalTagDraft draft;
  final MioPalette palette;
  final bool Function() isCurrent;
  const _TagEditor({
    required this.intent,
    required this.operations,
    required this.draft,
    required this.palette,
    required this.isCurrent,
  });
  @override
  State<_TagEditor> createState() => _TagEditorState();
}

class _TagEditorState extends State<_TagEditor> with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  late final List<TextEditingController> _fields;
  final _focus = List.generate(4, (_) => FocusNode());
  bool _busy = false, _failed = false, _closing = false, _foreground = true;
  bool _picking = false;
  CoverDocumentSelection? _selection;
  Completer<void>? _resume;
  Uint8List? _preview;
  api.LocalTagCoverMode _coverMode = api.LocalTagCoverMode.keep;
  String? _message;
  api.LocalTagSaveResult? _saved;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    final d = widget.draft;
    _fields = [
      d.title,
      d.artist,
      d.album,
      d.year == 0 ? '' : '${d.year}',
    ].map((s) => TextEditingController(text: s)).toList();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    if (!_foreground || !widget.isCurrent()) _invalidate();
  }

  void _invalidate() {
    widget.intent.close();
    unawaited(_selection?.cancel());
    _wake();
    _failed = true;
    if (_saved == null) {
      _message = p.english
          ? 'Editor closed. Close and reopen to edit.'
          : '编辑已关闭，请关闭后重新打开。';
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    if (_foreground) _wake();
    if ((state == AppLifecycleState.paused && !_picking) ||
        state == AppLifecycleState.detached ||
        (_foreground && !widget.isCurrent())) {
      if (mounted) setState(_invalidate);
    }
  }

  void _wake() {
    final waiter = _resume;
    _resume = null;
    if (waiter != null && !waiter.isCompleted) waiter.complete();
  }

  Future<bool> _visible() async {
    if (!mounted || _closing || _failed || !widget.isCurrent()) return false;
    if (!_foreground) {
      _resume ??= Completer<void>();
      await _resume!.future;
    }
    return mounted &&
        !_closing &&
        !_failed &&
        _foreground &&
        widget.isCurrent();
  }

  Future<void> _chooseCover() async {
    if (_busy ||
        _failed ||
        _saved != null ||
        !_foreground ||
        !widget.isCurrent()) {
      return;
    }
    final selection = CoverDocumentSelection();
    _selection = selection;
    setState(() {
      _busy = true;
      _picking = true;
      _message = null;
    });
    _scope.unfocus();
    try {
      final document = await selection.read();
      if (document == null || !await _visible()) return;
      _picking = false;
      final preview = await widget.operations.run(
        () => widget.intent.prepareCover(
          bytes: document.bytes,
          extension_: document.name.split('.').last.toLowerCase(),
        ),
      );
      if (!await _visible()) return;
      setState(() {
        _preview = preview;
        _coverMode = api.LocalTagCoverMode.replace;
      });
    } catch (error) {
      if (await _visible()) {
        setState(() {
          if (error is! PluginFileException &&
              !error.toString().contains('invalid_image')) {
            _invalidate();
          }
          _message = p.english
              ? 'Choose PNG/JPG/GIF/WebP up to 5 MiB. Invalid images or library changes reject the edit.'
              : '请选择最大5 MiB的PNG/JPG/GIF/WebP；图片超限或曲库变化时会拒绝编辑。';
        });
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

  void _dismiss() {
    if (!mounted || _closing || _busy) return;
    _closing = true;
    widget.intent.close();
    Navigator.of(context).pop(_saved);
  }

  Future<void> _save() async {
    if (!mounted ||
        _closing ||
        _busy ||
        _failed ||
        !_foreground ||
        _saved != null) {
      return;
    }
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    final yearText = _fields[3].text.trim();
    final year = yearText.isEmpty ? 0 : int.tryParse(yearText);
    if (year == null ||
        year < 0 ||
        year > 2099 ||
        _fields
            .take(3)
            .any(
              (c) =>
                  c.text.contains('\u0000') ||
                  utf8.encode(c.text).length > 4096,
            )) {
      setState(
        () => _message = p.english
            ? 'Each text must be at most 4096 UTF-8 bytes without NUL; year must be blank or 0–2099.'
            : '每项文本最多4096 UTF-8字节且不能含空字符；年份须留空或为0–2099。',
      );
      return;
    }
    setState(() {
      _busy = true;
      _message = null;
    });
    _scope.unfocus();
    try {
      final saved = await widget.operations.run(
        () => widget.intent.save(
          title: _fields[0].text,
          artist: _fields[1].text,
          album: _fields[2].text,
          year: year,
          coverMode: _coverMode,
        ),
      );
      if (!mounted || _closing) return;
      _saved = saved;
      widget.intent.close();
      if (_foreground &&
          widget.isCurrent() &&
          !_failed &&
          !saved.persistenceWarning &&
          !saved.playbackWarning) {
        _closing = true;
        Navigator.of(context).pop(saved);
      } else {
        setState(
          () => _message = [
            p.english ? 'Audio tags saved.' : '音频标签已保存。',
            if (saved.persistenceWarning)
              p.english
                  ? 'Library/session persistence failed. Reopen the library to check the saved file.'
                  : '曲库或会话持久化失败，请重新打开曲库核对已保存的文件。',
            if (saved.playbackWarning)
              p.english ? 'Playback could not resume.' : '播放未能恢复。',
          ].join('\n'),
        );
      }
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _invalidate();
          _message = p.english
              ? 'Tags could not be saved. The audio was not replaced. Close and reopen.'
              : '标签保存失败，音频未被替换。请关闭后重新打开。';
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
    for (final c in _fields) {
      c.dispose();
    }
    for (final f in _focus) {
      f.dispose();
    }
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
          label: p.text('music.tagEditor.title'),
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
                final keys = ['songName', 'artist', 'album', 'year'];
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
                                    p.text('music.tagEditor.title'),
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 16),
                                  for (var i = 0; i < 4; i++) ...[
                                    Text(p.text('music.tagEditor.${keys[i]}')),
                                    const SizedBox(height: 6),
                                    MioInput(
                                      palette: p,
                                      controller: _fields[i],
                                      focusNode: _focus[i],
                                      placeholder: p.text(
                                        'music.tagEditor.${keys[i]}',
                                      ),
                                      readOnly:
                                          _busy || _failed || _saved != null,
                                      keyboardType: i == 3
                                          ? TextInputType.number
                                          : TextInputType.text,
                                      textInputAction: i == 3
                                          ? TextInputAction.done
                                          : TextInputAction.next,
                                      onSubmitted: (_) {
                                        if (i < 3) _focus[i + 1].requestFocus();
                                      },
                                    ),
                                    const SizedBox(height: 12),
                                  ],
                                  Text(p.english ? 'Embedded artwork' : '内嵌封面'),
                                  const SizedBox(height: 8),
                                  if (_coverMode ==
                                          api.LocalTagCoverMode.replace &&
                                      _preview != null)
                                    Image.memory(
                                      _preview!,
                                      width: 120,
                                      height: 120,
                                      fit: BoxFit.contain,
                                    ),
                                  Text(switch (_coverMode) {
                                    api.LocalTagCoverMode.keep =>
                                      p.english
                                          ? 'Keep existing artwork'
                                          : '保留原有封面',
                                    api.LocalTagCoverMode.remove =>
                                      p.english
                                          ? 'Remove all embedded artwork on save'
                                          : '保存时移除所有内嵌图片',
                                    api.LocalTagCoverMode.replace =>
                                      p.english
                                          ? 'Replace all embedded artwork with this static PNG (up to 1024 px)'
                                          : '用此静态PNG替换所有内嵌图片（最长边1024像素）',
                                  }),
                                  const SizedBox(height: 8),
                                  Wrap(
                                    spacing: 8,
                                    runSpacing: 8,
                                    children: [
                                      _button(
                                        p.english ? 'Choose image' : '选择图片',
                                        _busy || _failed || _saved != null
                                            ? null
                                            : () => unawaited(_chooseCover()),
                                      ),
                                      _button(
                                        p.english ? 'Keep original' : '保留原图',
                                        _busy || _failed || _saved != null
                                            ? null
                                            : () => setState(
                                                () => _coverMode =
                                                    api.LocalTagCoverMode.keep,
                                              ),
                                      ),
                                      _button(
                                        p.english ? 'Remove artwork' : '移除封面',
                                        _busy || _failed || _saved != null
                                            ? null
                                            : () => setState(
                                                () => _coverMode = api
                                                    .LocalTagCoverMode
                                                    .remove,
                                              ),
                                      ),
                                    ],
                                  ),
                                  const SizedBox(height: 12),
                                  Text(p.text('music.tagEditor.filePath')),
                                  const SizedBox(height: 6),
                                  Text(widget.draft.path),
                                  if (_message != null)
                                    Padding(
                                      padding: const EdgeInsets.symmetric(
                                        vertical: 12,
                                      ),
                                      child: Semantics(
                                        liveRegion: true,
                                        child: Text(_message!),
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
                                        p.text('common.close'),
                                        _busy ? null : _dismiss,
                                      ),
                                      if (_saved == null)
                                        _button(
                                          p.text('music.tagEditor.saveTag'),
                                          _busy || _failed
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

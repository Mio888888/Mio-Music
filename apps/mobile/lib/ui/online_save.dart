import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

Future<api.PlaylistInfo?> showOnlineSave({
  required BuildContext context,
  required api.OnlinePlaylistSession session,
  required MioPalette palette,
  required String name,
  String? cover,
  required bool Function() isCurrent,
}) async {
  api.OnlineSaveSession? intent;
  try {
    intent = await session.prepareSave();
    if (!context.mounted || !isCurrent()) return null;
    final route = RawDialogRoute<api.PlaylistInfo>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => OnlineSaveDialog(
        intent: intent!,
        palette: palette,
        defaultName: name,
        cover: cover,
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

/// The caller owns disposal after this modal returns. Its immutable intent holds
/// precisely the loaded rows shown in the confirmation, including original data.
class OnlineSaveDialog extends StatefulWidget {
  final api.OnlineSaveSession intent;
  final MioPalette palette;
  final String defaultName;
  final String? cover;
  const OnlineSaveDialog({
    super.key,
    required this.intent,
    required this.palette,
    required this.defaultName,
    this.cover,
  });
  @override
  State<OnlineSaveDialog> createState() => _OnlineSaveDialogState();
}

class _OnlineSaveDialogState extends State<OnlineSaveDialog>
    with WidgetsBindingObserver {
  final _name = TextEditingController(), _description = TextEditingController();
  final _nameFocus = FocusNode(), _descriptionFocus = FocusNode();
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false, _failed = false, _closing = false;
  late final int _count;
  String? _error;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _count = widget.intent.count();
    // Leave a long provider title editable; do not split surrogate pairs.
    _name.text = String.fromCharCodes(widget.defaultName.runes.take(50));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nameFocus.requestFocus();
    });
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    widget.intent.close();
    Navigator.of(context).pop();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      widget.intent.close();
      setState(() {
        _failed = true;
        _error = p.english
            ? 'Save dialog closed in background. Reopen it to continue.'
            : '保存会话已在后台关闭，请关闭并重新打开。';
      });
    }
  }

  Future<void> _save() async {
    if (_busy || _failed || _closing) return;
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
        cover: widget.cover,
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
              ? 'Save failed or source changed. Close and reopen before retrying.'
              : '保存失败或来源已变化，请关闭并重新打开后重试。';
        });
      }
    } finally {
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
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
          label: p.english
              ? 'Save loaded songs as a new playlist'
              : '将已加载歌曲保存为新歌单',
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
                                    p.english
                                        ? 'Save as new playlist'
                                        : '保存为新歌单',
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    p.english
                                        ? '$_count loaded songs will be saved. Unloaded pages are excluded.'
                                        : '将保存已加载的 $_count 首歌曲，不包含未加载分页。',
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

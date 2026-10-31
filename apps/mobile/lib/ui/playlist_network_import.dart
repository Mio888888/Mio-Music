import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'playlist_file_import.dart';
import 'resources.dart';

typedef _Prepared = ({api.PlaylistFileImport intent, String name});

Future<api.PlaylistInfo?> showPlaylistNetworkImport({
  required BuildContext context,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  final sources = await api.playlistNetworkSources();
  if (!context.mounted || !isCurrent()) return null;
  final route = RawDialogRoute<_Prepared>(
    barrierDismissible: false,
    barrierColor: const Color(0x00000000),
    pageBuilder: (_, _, _) => _NetworkImportDialog(
      palette: palette,
      sources: sources,
      isCurrent: isCurrent,
    ),
  );
  final prepared = await Navigator.of(context).push<_Prepared>(route);
  await route.completed;
  if (prepared == null) return null;
  // Ownership passes only after the selector route has fully disposed.
  if (!context.mounted ||
      !isCurrent() ||
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.paused ||
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.detached) {
    prepared.intent.close();
    prepared.intent.dispose();
    return null;
  }
  return showPlaylistFileImport(
    context: context,
    palette: palette,
    isCurrent: isCurrent,
    prepared: prepared.intent,
    networkName: prepared.name,
  );
}

class _NetworkImportDialog extends StatefulWidget {
  final MioPalette palette;
  final List<api.DiscoverySource> sources;
  final bool Function() isCurrent;
  const _NetworkImportDialog({
    required this.palette,
    required this.sources,
    required this.isCurrent,
  });
  @override
  State<_NetworkImportDialog> createState() => _NetworkImportDialogState();
}

class _NetworkImportDialogState extends State<_NetworkImportDialog>
    with WidgetsBindingObserver {
  final _input = TextEditingController();
  final _inputFocus = FocusNode();
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  api.PlaylistFileImport? _intent;
  int _source = 0;
  bool _busy = false, _failed = false, _closing = false, _handedOff = false;
  String? _error;
  MioPalette get p => widget.palette;
  bool get _current => mounted && !_closing && !_failed && widget.isCurrent();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (WidgetsBinding.instance.lifecycleState == AppLifecycleState.paused ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.detached) {
      _failed = true;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _inputFocus.requestFocus();
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _intent?.close();
      if (mounted) {
        setState(() {
          _failed = true;
          _error = p.english
              ? 'Import closed in background. Close and reopen.'
              : '导入已在后台关闭，请关闭并重新打开。';
        });
      }
    }
  }

  void _dismiss() {
    if (!mounted || _closing) return;
    _closing = true;
    _intent?.close();
    Navigator.of(context).pop();
  }

  Future<void> _read() async {
    if (!_current || _busy || widget.sources.isEmpty) return;
    final input = _input.text.trim();
    if (input.isEmpty ||
        input.length > 4096 ||
        input.contains(RegExp(r'[\x00-\x1f\x7f]'))) {
      setState(
        () => _error = p.english
            ? 'Enter a playlist link or ID up to 4096 characters.'
            : '请输入歌单链接或ID，最多4096字符。',
      );
      return;
    }
    final source = widget.sources[_source];
    setState(() {
      _busy = true;
      _error = null;
    });
    api.PlaylistFileImport? intent;
    try {
      intent = await api.beginPlaylistNetworkImport(
        source: source.id,
        input: input,
      );
      _intent = intent;
      if (!mounted || !_current) return;
      await intent.prepareNetwork();
      if (!mounted || !_current) return;
      _handedOff = true;
      _closing = true;
      Navigator.of(context).pop<_Prepared>((intent: intent, name: source.name));
    } catch (error) {
      if (_current) {
        final code = error.toString();
        setState(
          () => _error = code.contains('invalid_input')
              ? (p.english
                    ? 'Unrecognized playlist link or ID for this source.'
                    : '无法识别此来源的歌单链接或ID。')
              : code.contains('empty_playlist')
              ? (p.english
                    ? 'The playlist is empty. Nothing was saved.'
                    : '歌单为空，未保存歌曲。')
              : (p.english
                    ? 'Unable to read the complete playlist, or source changed. Nothing was saved. Retry or reopen.'
                    : '无法读取完整歌单或来源已变化，未保存歌曲。请重试或重新打开。'),
        );
      }
    } finally {
      if (!_handedOff) {
        intent?.close();
        intent?.dispose();
      }
      // Observe the handed-off intent until this route fully disposes.
      // A pause during its exit animation must invalidate the next form.
      if (!_handedOff && identical(_intent, intent)) _intent = null;
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    if (!_handedOff) _intent?.close();
    // An in-flight read disposes its opaque handle in its own finally block.
    WidgetsBinding.instance.removeObserver(this);
    _input.dispose();
    _inputFocus.dispose();
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
          label: p.text('music.songlist.fromNetwork'),
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
                        dismissible: true,
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
                              borderRadius: BorderRadius.circular(12),
                            ),
                            child: SingleChildScrollView(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Text(
                                    p.text('music.songlist.fromNetwork'),
                                    style: const TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    p.english
                                        ? 'Read all pages from an installed source, then confirm the new playlist. Up to 12,000 songs / 32 MiB.'
                                        : '从已安装来源读取完整歌单，再确认新建歌单。最多12,000首 / 32 MiB。',
                                  ),
                                  const SizedBox(height: 16),
                                  if (widget.sources.isEmpty)
                                    Text(
                                      p.english
                                          ? 'Install and enable a list plugin with playlist support first.'
                                          : '请先安装并启用支持歌单详情的列表插件。',
                                    ),
                                  Wrap(
                                    spacing: 8,
                                    runSpacing: 8,
                                    children: [
                                      for (
                                        var i = 0;
                                        i < widget.sources.length;
                                        i++
                                      )
                                        Semantics(
                                          selected: _source == i,
                                          child: _button(
                                            '${_source == i ? '✓ ' : ''}${widget.sources[i].name}',
                                            _busy || _failed
                                                ? null
                                                : () => setState(
                                                    () => _source = i,
                                                  ),
                                          ),
                                        ),
                                    ],
                                  ),
                                  const SizedBox(height: 16),
                                  MioInput(
                                    palette: p,
                                    controller: _input,
                                    focusNode: _inputFocus,
                                    placeholder: p.text(
                                      'music.songlist.networkImportPlaceholder',
                                    ),
                                    readOnly: _busy || _failed,
                                    height: 80,
                                    maxLines: 2,
                                    textInputAction: TextInputAction.done,
                                    onSubmitted: (_) => unawaited(_read()),
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
                                        _dismiss,
                                      ),
                                      _button(
                                        p.english ? 'Read playlist' : '读取歌单',
                                        _busy ||
                                                _failed ||
                                                widget.sources.isEmpty
                                            ? null
                                            : () => unawaited(_read()),
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

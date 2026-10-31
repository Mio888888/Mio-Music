import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'saved_playlist_cover.dart';
import 'resources.dart';

class OnlineAppendResult {
  final int added;
  final String destination;
  const OnlineAppendResult(this.added, this.destination);
}

Future<OnlineAppendResult?> showOnlineAppend({
  required BuildContext context,
  required Future<api.OnlineSaveSession> Function() prepare,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  api.OnlineSaveSession? intent;
  try {
    intent = await prepare();
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    if (!context.mounted ||
        !isCurrent() ||
        (lifecycle != null && lifecycle != AppLifecycleState.resumed)) {
      return null;
    }
    final route = RawDialogRoute<OnlineAppendResult>(
      barrierDismissible: false,
      barrierColor: const Color(0x00000000),
      pageBuilder: (_, _, _) => _OnlineAppendDialog(
        intent: intent!,
        palette: palette,
        isCurrent: isCurrent,
      ),
    );
    final result = await Navigator.of(context).push<OnlineAppendResult>(route);
    await route.completed;
    return result;
  } finally {
    intent?.close();
    intent?.dispose();
  }
}

class _OnlineAppendDialog extends StatefulWidget {
  final api.OnlineSaveSession intent;
  final MioPalette palette;
  final bool Function() isCurrent;
  const _OnlineAppendDialog({
    required this.intent,
    required this.palette,
    required this.isCurrent,
  });
  @override
  State<_OnlineAppendDialog> createState() => _OnlineAppendDialogState();
}

class _OnlineAppendDialogState extends State<_OnlineAppendDialog>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  List<api.PlaylistInfo> _rows = [];
  int _offset = 0, _total = 0;
  bool _more = false, _loading = true, _busy = false, _closing = false;
  String? _error, _success;
  OnlineAppendResult? _saved;
  api.PlaylistInfo? _destination;
  bool _foreground = true;
  MioPalette get p => widget.palette;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    if (!_foreground || !widget.isCurrent()) {
      widget.intent.close();
      _loading = false;
      _error = p.english
          ? 'Session closed. Reopen to continue.'
          : '会话已关闭，请重新打开。';
    } else {
      unawaited(_load(0));
    }
  }

  void _dismiss() {
    if (!_busy && !_closing && mounted) {
      _closing = true;
      widget.intent.close();
      Navigator.of(context).pop(_saved);
    }
  }

  Future<void> _load(int offset) async {
    try {
      final page = await widget.intent.destinations(offset: offset, limit: 100);
      if (!mounted || _closing) return;
      setState(() {
        _rows = page.items;
        _offset = offset;
        _total = page.total;
        _more = page.hasMore;
        _loading = false;
      });
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _loading = false;
          _rows = [];
          _error = p.english
              ? 'Unable to read the original library. Close and refresh.'
              : '无法读取原曲库，请关闭并刷新。';
        });
      }
    }
  }

  void _page(int offset) {
    if (_loading || _busy || _closing || _error != null) return;
    setState(() => _loading = true);
    unawaited(_load(offset));
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted) return;
    setState(() {
      _foreground = state == AppLifecycleState.resumed;
      if (state == AppLifecycleState.paused ||
          state == AppLifecycleState.detached ||
          (state == AppLifecycleState.resumed && !widget.isCurrent())) {
        widget.intent.close();
        if (_saved == null) {
          _error = p.english
              ? 'Session closed. Reopen to continue.'
              : '会话已关闭，请重新打开。';
        }
      }
    });
  }

  Future<void> _copy() async {
    final destination = _destination;
    if (_loading ||
        _busy ||
        _closing ||
        _error != null ||
        destination == null ||
        !_foreground ||
        _saved != null) {
      return;
    }
    if (!widget.isCurrent()) {
      widget.intent.close();
      setState(
        () => _error = p.english
            ? 'Page changed. Reopen to continue.'
            : '页面已变化，请重新打开。',
      );
      return;
    }
    setState(() => _busy = true);
    try {
      final added = await widget.intent.append(destinationId: destination.id);
      if (!mounted || _closing) return;
      final result = OnlineAppendResult(added, destination.title);
      widget.intent.close();
      if (_foreground && widget.isCurrent() && _error == null) {
        _closing = true;
        Navigator.of(context).pop(result);
      } else {
        setState(() {
          _saved = result;
          _error = null;
          _success = p.english
              ? 'Added $added songs to ${destination.title}.'
              : '已向“${destination.title}”添加 $added 首歌曲。';
        });
      }
    } catch (_) {
      widget.intent.close();
      if (mounted && !_closing) {
        setState(
          () => _error = p.english
              ? 'Unable to add: the library/source changed, an ID conflicts, or storage is unavailable. Close and reopen.'
              : '添加失败：曲库或来源变化、歌曲ID来源冲突或存储不可用，请关闭并重新打开。',
        );
      }
    } finally {
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

  Widget _button(String label, VoidCallback? action) => ConstrainedBox(
    constraints: const BoxConstraints(minHeight: 44),
    child: MioButton(
      palette: p,
      label: label,
      onPressed: action,
      child: Text(label),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final enabled =
        !_busy && !_loading && _error == null && _foreground && _saved == null;
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, result) {
        if (!didPop) _dismiss();
      },
      child: CallbackShortcuts(
        bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
        child: MioDialogScope(
          node: _scope,
          child: Semantics(
            scopesRoute: true,
            namesRoute: true,
            label: p.text('common.addToPlaylistTitle'),
            explicitChildNodes: true,
            child: DefaultTextStyle(
              style: TextStyle(
                color: p.color('--td-text-color-primary'),
                fontFamily: 'PingFang SC',
                fontFamilyFallback: const ['sans-serif'],
                fontSize: 14,
                decoration: TextDecoration.none,
              ),
              child: LayoutBuilder(
                builder: (context, box) {
                  final narrow = !MioDeviceScope.wide(context, box.maxWidth);
                  final media = MediaQuery.of(context);
                  final available = math.max(
                    0.0,
                    box.maxHeight -
                        media.viewInsets.bottom -
                        media.padding.top -
                        media.padding.bottom -
                        24,
                  );
                  return Stack(
                    children: [
                      Positioned.fill(
                        child: BackdropFilter(
                          filter: ImageFilter.blur(
                            sigmaX: narrow ? 12 : 5,
                            sigmaY: narrow ? 12 : 5,
                          ),
                          child: ModalBarrier(
                            color: p.color(
                              narrow ? '--mobile-scrim' : '--td-mask-active',
                            ),
                            dismissible: !_busy,
                            onDismiss: _dismiss,
                            semanticsLabel: p.text('common.cancel'),
                          ),
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
                                narrow ? 480 : 560,
                              ),
                              constraints: BoxConstraints(
                                maxHeight: math.min(available, 640),
                              ),
                              padding: EdgeInsets.all(narrow ? 16 : 24),
                              decoration: BoxDecoration(
                                color: p.color(
                                  narrow
                                      ? '--mobile-glass-bg-strong'
                                      : '--td-bg-color-container',
                                ),
                                border: Border.all(color: p.border),
                                borderRadius: BorderRadius.circular(
                                  narrow ? 22 : 14,
                                ),
                              ),
                              child: SingleChildScrollView(
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  crossAxisAlignment:
                                      CrossAxisAlignment.stretch,
                                  children: [
                                    Wrap(
                                      alignment: WrapAlignment.spaceBetween,
                                      crossAxisAlignment:
                                          WrapCrossAlignment.center,
                                      children: [
                                        Text(
                                          p.text('common.addToPlaylistTitle'),
                                          style: const TextStyle(
                                            fontSize: 18,
                                            fontWeight: FontWeight.w600,
                                          ),
                                        ),
                                        _button(
                                          p.text('common.cancel'),
                                          !_busy ? _dismiss : null,
                                        ),
                                      ],
                                    ),
                                    Text(
                                      p
                                          .text('music.songList.selectedCount')
                                          .replaceAll(
                                            '{count}',
                                            '${widget.intent.count()}',
                                          ),
                                      style: TextStyle(color: p.secondary),
                                    ),
                                    if (_success != null)
                                      Semantics(
                                        liveRegion: true,
                                        child: Text(_success!),
                                      ),
                                    if (_error != null)
                                      Semantics(
                                        liveRegion: true,
                                        child: Text(
                                          _error!,
                                          style: TextStyle(
                                            color: p.color('--td-error-color'),
                                          ),
                                        ),
                                      ),
                                    if (_loading || _busy)
                                      Semantics(
                                        liveRegion: true,
                                        child: Text(
                                          p.english ? 'Working…' : '正在处理…',
                                        ),
                                      ),
                                    const SizedBox(height: 8),
                                    SizedBox(
                                      height: math.min(320, available * .45),
                                      child: ListView.builder(
                                        primary: false,
                                        itemCount: _rows.length,
                                        itemBuilder: (context, index) {
                                          final row = _rows[index];
                                          final label =
                                              '${row.title} · ${p.text('common.songCountShort').replaceAll('{count}', '${row.count}')}';
                                          return Padding(
                                            padding: const EdgeInsets.only(
                                              bottom: 6,
                                            ),
                                            child: MioButton(
                                              palette: p,
                                              label: _destination?.id == row.id
                                                  ? '${p.english ? 'Selected' : '已选'} · $label'
                                                  : label,
                                              radius: narrow ? 14 : 13,
                                              background: p.surface,
                                              onPressed: enabled
                                                  ? () => setState(
                                                      () => _destination = row,
                                                    )
                                                  : null,
                                              padding:
                                                  const EdgeInsets.symmetric(
                                                    horizontal: 12,
                                                    vertical: 10,
                                                  ),
                                              child: Row(
                                                children: [
                                                  MioSavedPlaylistCover(
                                                    key: ValueKey(row.id),
                                                    id: row.id,
                                                    source: row.cover,
                                                    appendSession:
                                                        widget.intent,
                                                    size: 44,
                                                    radius: 10,
                                                  ),
                                                  const SizedBox(width: 12),
                                                  Expanded(
                                                    child: Column(
                                                      crossAxisAlignment:
                                                          CrossAxisAlignment
                                                              .start,
                                                      children: [
                                                        Text(
                                                          row.title,
                                                          maxLines: 2,
                                                          overflow: TextOverflow
                                                              .ellipsis,
                                                          style:
                                                              const TextStyle(
                                                                fontSize: 13.5,
                                                                fontWeight:
                                                                    FontWeight
                                                                        .w500,
                                                              ),
                                                        ),
                                                        Text(
                                                          p
                                                              .text(
                                                                'common.songCountShort',
                                                              )
                                                              .replaceAll(
                                                                '{count}',
                                                                '${row.count}',
                                                              ),
                                                          style: TextStyle(
                                                            fontSize: 11.5,
                                                            color: p.secondary,
                                                          ),
                                                        ),
                                                      ],
                                                    ),
                                                  ),
                                                  Text(
                                                    _destination?.id == row.id
                                                        ? '✓'
                                                        : '›',
                                                    style: TextStyle(
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
                                    if (_rows.isEmpty &&
                                        !_loading &&
                                        _error == null)
                                      Text(p.text('common.noPlaylistYet')),
                                    Wrap(
                                      spacing: 8,
                                      crossAxisAlignment:
                                          WrapCrossAlignment.center,
                                      children: [
                                        _button(
                                          p.english ? 'Previous' : '上一页',
                                          enabled && _offset > 0
                                              ? () => _page(
                                                  math.max(0, _offset - 100),
                                                )
                                              : null,
                                        ),
                                        Text(
                                          '${_rows.isEmpty ? 0 : _offset + 1}–${_offset + _rows.length} / $_total',
                                          style: TextStyle(
                                            fontSize: 12,
                                            color: p.secondary,
                                          ),
                                        ),
                                        _button(
                                          p.english ? 'Next' : '下一页',
                                          enabled && _more
                                              ? () => _page(
                                                  _offset + _rows.length,
                                                )
                                              : null,
                                        ),
                                      ],
                                    ),
                                    const SizedBox(height: 8),
                                    if (_destination != null)
                                      Text(
                                        p.english
                                            ? 'Add ${widget.intent.count()} selected songs to ${_destination!.title}?'
                                            : '将选中的 ${widget.intent.count()} 首歌曲加入“${_destination!.title}”？',
                                      ),
                                    _button(
                                      p.english ? 'Confirm add' : '确认添加',
                                      enabled && _destination != null
                                          ? () => unawaited(_copy())
                                          : null,
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
}

/// Uses the same bounded selection across loaded pages; no provider fetch occurs here.
class OnlineSelectionToolbar extends StatelessWidget {
  final MioPalette palette;
  final List<api.Track> tracks;
  final Set<String> selected;
  final bool selecting, enabled;
  final void Function(bool, Set<String>) onChanged;
  final VoidCallback onAppend;
  const OnlineSelectionToolbar({
    super.key,
    required this.palette,
    required this.tracks,
    required this.selected,
    required this.selecting,
    required this.enabled,
    required this.onChanged,
    required this.onAppend,
  });
  @override
  Widget build(BuildContext context) {
    final p = palette;
    Widget button(String label, VoidCallback? action) => MioButton(
      palette: p,
      label: label,
      onPressed: action,
      child: Text(label),
    );
    return Wrap(
      spacing: 12,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        button(
          selecting
              ? (p.english ? 'Finish selecting' : '结束选择')
              : p.text('common.batchSelect'),
          enabled ? () => onChanged(!selecting, {}) : null,
        ),
        if (selecting) ...[
          Text(
            p.english
                ? '${selected.length} selected · limit 200'
                : '已选 ${selected.length} 首 · 最多200首',
          ),
          button(
            p.english ? 'Select loaded (up to 200)' : '选择已加载（最多200首）',
            enabled
                ? () =>
                      onChanged(true, tracks.take(200).map((t) => t.id).toSet())
                : null,
          ),
          button(
            p.english ? 'Clear selection' : '清空选择',
            enabled ? () => onChanged(true, {}) : null,
          ),
          button(
            p.text('common.addToPlaylistTitle'),
            enabled && selected.isNotEmpty ? onAppend : null,
          ),
        ],
      ],
    );
  }
}

class OnlineSelectionToggle extends StatelessWidget {
  final MioPalette palette;
  final api.Track track;
  final bool selected;
  final VoidCallback? onPressed;
  const OnlineSelectionToggle({
    super.key,
    required this.palette,
    required this.track,
    required this.selected,
    required this.onPressed,
  });
  @override
  Widget build(BuildContext context) => SizedBox(
    width: 44,
    height: 44,
    child: Semantics(
      checked: selected,
      child: MioButton(
        palette: palette,
        label: '${palette.text('common.batchSelect')} · ${track.title}',
        onPressed: onPressed,
        child: Container(
          width: 18,
          height: 18,
          decoration: BoxDecoration(
            border: Border.all(
              color: selected ? palette.brand : palette.border,
            ),
            borderRadius: BorderRadius.circular(4),
            color: selected ? palette.brand : null,
          ),
          child: selected
              ? const Center(
                  child: Text(
                    '✓',
                    style: TextStyle(color: Color(0xffffffff), fontSize: 12),
                  ),
                )
              : null,
        ),
      ),
    ),
  );
}

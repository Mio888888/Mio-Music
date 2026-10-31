import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'cover.dart';
import 'resources.dart';

class PlaylistCopyResult {
  final bool refresh;
  final int? added;
  final String? destination;
  const PlaylistCopyResult({
    this.refresh = false,
    this.added,
    this.destination,
  });
}

/// Retains the caller's original source cursor for destination pages and the
/// final atomic copy. It never opens a replacement cursor after a failure.
class PlaylistCopyDialog extends StatefulWidget {
  final api.LibrarySession session;
  final List<String> songIds;
  final CoreController core;
  final MioPalette palette;
  const PlaylistCopyDialog({
    super.key,
    required this.session,
    required this.songIds,
    required this.core,
    required this.palette,
  });
  @override
  State<PlaylistCopyDialog> createState() => _PlaylistCopyDialogState();
}

class _PlaylistCopyDialogState extends State<PlaylistCopyDialog> {
  final _name = TextEditingController();
  final _nameFocus = FocusNode();
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  List<api.PlaylistInfo> _rows = [];
  int _offset = 0, _total = 0;
  bool _more = false, _loading = true, _busy = false, _closing = false;
  String? _error, _nameError;
  MioPalette get p => widget.palette;

  @override
  void initState() {
    super.initState();
    unawaited(_load(0));
  }

  void _dismiss() {
    if (!_busy && !_closing && mounted) {
      _closing = true;
      Navigator.of(context)
          .pop(_error == null ? null : const PlaylistCopyResult(refresh: true));
    }
  }

  Future<void> _load(int offset) async {
    try {
      final page = await widget.session.destinations(
        offset: offset,
        limit: 100,
      );
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

  Future<void> _copy({String? id, String? title}) async {
    if (_loading || _busy || _closing || _error != null) return;
    final name = _name.text.trim();
    if (id == null && name.isEmpty) return;
    if (id == null && (name.length > 50 || name.contains('\u0000'))) {
      setState(
        () => _nameError = p.english
            ? 'The name is too long (limit 50) or contains a null character.'
            : '名称过长（上限 50）或包含空字符，请修改。',
      );
      return;
    }
    setState(() {
      _busy = true;
      _nameError = null;
    });
    int? added;
    await widget.core.command(() async {
      added = await widget.session.copySongs(
        songIds: widget.songIds,
        destinationId: id,
        newName: id == null ? name : null,
      );
    });
    if (!mounted || _closing) return;
    if (added != null) {
      _closing = true;
      Navigator.of(context)
          .pop(PlaylistCopyResult(added: added, destination: title ?? name));
    } else {
      setState(() {
        _busy = false;
        _error = p.english
            ? 'Copy failed: the library changed, a source conflicts, or storage is unavailable. Nothing was partially copied. Close and refresh.'
            : '添加失败：曲库变更、歌曲来源冲突或存储不可用。不会部分添加，请关闭并刷新。';
      });
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _nameFocus.dispose();
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
    final enabled = !_busy && !_loading && _error == null;
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
                                            '${widget.songIds.length}',
                                          ),
                                      style: TextStyle(color: p.secondary),
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
                                    ListView.builder(
                                      primary: false,
                                      shrinkWrap: true,
                                      physics:
                                          const NeverScrollableScrollPhysics(),
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
                                            label: label,
                                            radius: narrow ? 14 : 13,
                                            background: p.surface,
                                            onPressed: enabled
                                                ? () => unawaited(
                                                    _copy(
                                                      id: row.id,
                                                      title: row.title,
                                                    ),
                                                  )
                                                : null,
                                            padding: const EdgeInsets.symmetric(
                                              horizontal: 12,
                                              vertical: 10,
                                            ),
                                            child: Row(
                                              children: [
                                                MioCover(
                                                  row.cover,
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
                                                        style: const TextStyle(
                                                          fontSize: 13.5,
                                                          fontWeight:
                                                              FontWeight.w500,
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
                                                MioIcon(
                                                  'Right',
                                                  color: p.secondary,
                                                  size: 14,
                                                ),
                                              ],
                                            ),
                                          ),
                                        );
                                      },
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
                                    MioInput(
                                      palette: p,
                                      controller: _name,
                                      focusNode: _nameFocus,
                                      placeholder: p.text(
                                        'common.playlistName',
                                      ),
                                      readOnly: !enabled,
                                      textInputAction: TextInputAction.done,
                                      onChanged: (_) =>
                                          setState(() => _nameError = null),
                                      onSubmitted: enabled
                                          ? (_) => unawaited(_copy())
                                          : null,
                                      height: 44,
                                    ),
                                    if (_nameError != null)
                                      Semantics(
                                        liveRegion: true,
                                        child: Text(
                                          _nameError!,
                                          style: TextStyle(
                                            color: p.color('--td-error-color'),
                                          ),
                                        ),
                                      ),
                                    _button(
                                      p.text('common.createAndAdd'),
                                      enabled && _name.text.trim().isNotEmpty
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

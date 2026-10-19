import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

enum PlaylistDeleteResult { deleted, refresh }

/// Custom route content based on the desktop playlist-delete dialog. The
/// caller retains the exact displayed cursor; this dialog never opens a new one.
class PlaylistDeleteDialog extends StatefulWidget {
  final api.LibrarySession session;
  final api.PlaylistInfo playlist;
  final CoreController core;
  final MioPalette palette;
  const PlaylistDeleteDialog({
    super.key,
    required this.session,
    required this.playlist,
    required this.core,
    required this.palette,
  });
  @override
  State<PlaylistDeleteDialog> createState() => _PlaylistDeleteDialogState();
}

class _PlaylistDeleteDialogState extends State<PlaylistDeleteDialog>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false, _closing = false;
  String? _error;
  MioPalette get p => widget.palette;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  void _dismiss() {
    if (!_busy && !_closing && mounted) {
      _closing = true;
      Navigator.of(context)
          .pop(_error == null ? null : PlaylistDeleteResult.refresh);
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _dismiss();
    }
  }

  Future<void> _delete() async {
    if (_busy || _closing || _error != null) return;
    setState(() => _busy = true);
    var deleted = false;
    await widget.core.command(() async {
      await widget.session.deletePlaylist();
      deleted = true;
    });
    if (!mounted) return;
    if (deleted) {
      _closing = true;
      Navigator.of(context).pop(PlaylistDeleteResult.deleted);
    } else {
      setState(() {
        _busy = false;
        _error = p.english
            ? 'Delete failed or the playlist changed. Close, refresh and try again.'
            : '删除失败或歌单已变更，请关闭并刷新后重试。';
      });
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    super.dispose();
  }

  Widget _button(
    String label,
    VoidCallback? action,
    bool narrow, {
    bool danger = false,
  }) => ConstrainedBox(
    constraints: const BoxConstraints(minHeight: 44, minWidth: 64),
    child: MioButton(
      palette: p,
      label: label,
      onPressed: action,
      radius: narrow ? 12 : 3,
      background: danger ? p.color('--td-error-color-6') : p.surface,
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: narrow ? 12 : 8),
      child: Text(
        label,
        style: TextStyle(
          color: danger
              ? p.color('--td-font-white-1')
              : p.color('--td-text-color-primary'),
        ),
      ),
    ),
  );

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: false,
    onPopInvokedWithResult: (didPop, result) {
      if (!didPop) _dismiss();
    },
    child: Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
      },
      child: Actions(
        actions: {
          DismissIntent: CallbackAction<DismissIntent>(
            onInvoke: (_) {
              _dismiss();
              return null;
            },
          ),
        },
        child: FocusScope(
          node: _scope,
          autofocus: true,
          child: Semantics(
            scopesRoute: true,
            namesRoute: true,
            label: p.text('music.songlist.confirmDelete'),
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
                  final gutter = (box.maxWidth * .04).clamp(16.0, 24.0);
                  final width = math.max(
                    0.0,
                    math.min(box.maxWidth - 2 * gutter, narrow ? 420.0 : 480.0),
                  );
                  final padding = narrow ? 16.0 : 32.0;
                  final body = p
                      .text('music.songlist.deleteConfirm')
                      .replaceAll('{name}', widget.playlist.title);
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
                      SafeArea(
                        child: Align(
                          alignment: narrow
                              ? Alignment.bottomCenter
                              : Alignment.topCenter,
                          child: Padding(
                            padding: EdgeInsets.only(
                              top: narrow ? 0 : box.maxHeight * .2,
                              bottom: narrow ? 12 : 0,
                            ),
                            child: Container(
                              width: width,
                              constraints: BoxConstraints(
                                maxHeight: math.max(0, box.maxHeight * .76),
                              ),
                              padding: EdgeInsets.fromLTRB(
                                padding,
                                narrow ? 12 : padding,
                                padding,
                                padding,
                              ),
                              decoration: BoxDecoration(
                                color: p.color(
                                  narrow
                                      ? '--mobile-glass-bg-strong'
                                      : '--td-bg-color-container',
                                ),
                                border: Border.all(
                                  color: p.color(
                                    narrow
                                        ? '--mobile-glass-border'
                                        : '--td-border-level-1-color',
                                  ),
                                ),
                                borderRadius: BorderRadius.circular(
                                  narrow ? 22 : 6,
                                ),
                              ),
                              child: SingleChildScrollView(
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    if (narrow)
                                      Center(
                                        child: Container(
                                          width: 38,
                                          height: 4,
                                          margin: const EdgeInsets.only(
                                            bottom: 12,
                                          ),
                                          decoration: BoxDecoration(
                                            color: const Color(0x5c787880),
                                            borderRadius: BorderRadius.circular(
                                              999,
                                            ),
                                          ),
                                        ),
                                      ),
                                    Row(
                                      children: [
                                        MioIcon(
                                          'TD-ErrorCircle-Filled',
                                          color: p.color('--td-error-color'),
                                          size: 24,
                                        ),
                                        const SizedBox(width: 8),
                                        Expanded(
                                          child: Text(
                                            p.text(
                                              'music.songlist.confirmDelete',
                                            ),
                                            style: const TextStyle(
                                              fontSize: 16,
                                              height: 1.5,
                                              fontWeight: FontWeight.w600,
                                            ),
                                          ),
                                        ),
                                        SizedBox(
                                          width: 44,
                                          height: 44,
                                          child: MioButton(
                                            palette: p,
                                            label: p.text(
                                              'common.titleBar.close',
                                            ),
                                            onPressed: _busy ? null : _dismiss,
                                            radius: narrow ? 999 : 3,
                                            child: MioIcon(
                                              'WindowClose',
                                              color: p.secondary,
                                              size: 20,
                                            ),
                                          ),
                                        ),
                                      ],
                                    ),
                                    Padding(
                                      padding: EdgeInsets.symmetric(
                                        vertical: narrow ? 14 : 16,
                                      ),
                                      child: Column(
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          Text(
                                            body,
                                            style: TextStyle(
                                              color: p.secondary,
                                              height: 22 / 14,
                                            ),
                                          ),
                                          if (_error != null)
                                            Padding(
                                              padding: const EdgeInsets.only(
                                                top: 12,
                                              ),
                                              child: Text(
                                                _error!,
                                                style: TextStyle(
                                                  color: p.color(
                                                    '--td-error-color',
                                                  ),
                                                ),
                                              ),
                                            ),
                                        ],
                                      ),
                                    ),
                                    Padding(
                                      padding: EdgeInsets.only(
                                        top: narrow ? 12 : 16,
                                      ),
                                      child: Row(
                                        mainAxisAlignment:
                                            MainAxisAlignment.end,
                                        children: narrow
                                            ? [
                                                Expanded(
                                                  child: _button(
                                                    p.text('common.delete'),
                                                    _busy || _error != null
                                                        ? null
                                                        : () => unawaited(
                                                            _delete(),
                                                          ),
                                                    narrow,
                                                    danger: true,
                                                  ),
                                                ),
                                                const SizedBox(width: 16),
                                                Expanded(
                                                  child: _button(
                                                    p.text('common.cancel'),
                                                    _busy ? null : _dismiss,
                                                    narrow,
                                                  ),
                                                ),
                                              ]
                                            : [
                                                _button(
                                                  p.text('common.cancel'),
                                                  _busy ? null : _dismiss,
                                                  narrow,
                                                ),
                                                const SizedBox(width: 8),
                                                _button(
                                                  p.text('common.delete'),
                                                  _busy || _error != null
                                                      ? null
                                                      : () => unawaited(
                                                          _delete(),
                                                        ),
                                                  narrow,
                                                  danger: true,
                                                ),
                                              ],
                                      ),
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
    ),
  );
}

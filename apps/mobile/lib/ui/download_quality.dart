import 'device.dart';

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'glass_surface.dart';
import 'resources.dart';

/// Keep the native handle alive through the route's exit animation.
Future<api.DownloadEnqueueResult?> showDownloadChoice({
  required BuildContext context,
  required api.DownloadChoice choice,
  required api.DownloadChoiceInfo info,
  required CoreController core,
  required MioPalette palette,
  String? batchLabel,
  VoidCallback? onStop,
}) async {
  final route = RawDialogRoute<api.DownloadEnqueueResult>(
    barrierDismissible: false,
    barrierColor: const Color(0x00000000),
    pageBuilder: (_, _, _) => DownloadQualityDialog(
      choice: choice,
      info: info,
      core: core,
      palette: palette,
      batchLabel: batchLabel,
      onStop: onStop,
    ),
  );
  final result = await Navigator.of(context)
      .push<api.DownloadEnqueueResult>(route);
  await route.completed;
  return result;
}

/// The caller disposes the handle after this route returns. The route closes it
/// on background/disposal, cancelling work that has not reached admission.
class DownloadQualityDialog extends StatefulWidget {
  final api.DownloadChoice choice;
  final api.DownloadChoiceInfo info;
  final CoreController core;
  final MioPalette palette;
  final String? batchLabel;
  final VoidCallback? onStop;
  const DownloadQualityDialog({
    super.key,
    required this.choice,
    required this.info,
    required this.core,
    required this.palette,
    this.batchLabel,
    this.onStop,
  });
  @override
  State<DownloadQualityDialog> createState() => _DownloadQualityDialogState();
}

class _DownloadQualityDialogState extends State<DownloadQualityDialog>
    with WidgetsBindingObserver {
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  bool _busy = false, _closing = false, _failed = false;
  String? _error;
  MioPalette get p => widget.palette;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    widget.choice.close();
    Navigator.of(context).pop();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      widget.choice.close();
      _dismiss();
    }
  }

  Future<void> _enqueue(String quality) async {
    if (_busy || _closing || _failed) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    api.DownloadEnqueueResult? result;
    await widget.core.command(() async {
      result = await widget.choice.enqueue(quality: quality);
    });
    if (!mounted) return;
    if (result != null) {
      _closing = true;
      Navigator.of(context).pop(result);
    } else {
      setState(() {
        _busy = false;
        _failed = true;
        _error = p.english
            ? 'Unable to add the download. If the source changed, close and reopen this choice.'
            : '未能添加下载；若来源已变更，请关闭并重新打开音质选择。';
      });
    }
  }

  @override
  void dispose() {
    widget.choice.close();
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    super.dispose();
  }

  Widget _row(api.DownloadQuality row) {
    final active = row.quality == widget.info.remembered;
    final primary = p.color('--td-text-color-primary');
    final label = row.labelKey == null ? row.quality : p.text(row.labelKey!);
    return Semantics(
      selected: active,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: active
              ? p.brand.withValues(alpha: .14)
              : primary.withValues(alpha: .06),
          border: Border.all(
            color: active
                ? p.brand.withValues(alpha: .3)
                : primary.withValues(alpha: .08),
          ),
          borderRadius: BorderRadius.circular(12),
        ),
        child: MioButton(
          palette: p,
          label: label,
          onPressed: _busy || _closing || _failed
              ? null
              : () => unawaited(_enqueue(row.quality)),
          radius: 12,
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      label,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w500,
                        color: active ? p.brand : primary,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      row.quality,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(color: p.secondary, fontSize: 12),
                    ),
                  ],
                ),
              ),
              if (row.size != null) ...[
                const SizedBox(width: 16),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 96),
                  child: Text(
                    row.size!,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: p.secondary,
                      fontSize: 12,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  Widget _header(bool narrow) => Row(
    children: [
      Container(
        width: narrow ? 38 : 42,
        height: narrow ? 38 : 42,
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(narrow ? 12 : 13),
          border: Border.all(
            color: p.color('--td-text-color-primary').withValues(alpha: .08),
          ),
          gradient: LinearGradient(
            colors: [p.brand.withValues(alpha: .18), const Color(0x1f8c50ff)],
          ),
        ),
        child: Center(child: MioIcon('Download', color: p.brand, size: 20)),
      ),
      const SizedBox(width: 12),
      Expanded(
        child: Text(
          p.text('download.selectQuality'),
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(
            fontSize: 17,
            height: 1.2,
            fontWeight: FontWeight.w600,
          ),
        ),
      ),
      SizedBox(
        width: 44,
        height: 44,
        child: MioButton(
          palette: p,
          label: p.text('common.titleBar.close'),
          onPressed: _busy ? null : _dismiss,
          radius: narrow ? 999 : 9,
          child: MioIcon('WindowClose', color: p.secondary, size: 13),
        ),
      ),
    ],
  );
  Widget _panel(BoxConstraints box, bool narrow, double width) {
    final padding = narrow ? 16.0 : 28.0;
    return ConstrainedBox(
      constraints: BoxConstraints(
        maxWidth: width,
        maxHeight: math.max(0, box.maxHeight * .85),
      ),
      child: DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(22),
          boxShadow: const [
            BoxShadow(
              color: Color(0x40000000),
              offset: Offset(0, 12),
              blurRadius: 32,
            ),
          ],
        ),
        // Liquid glass sheet over the dimmed page.
        child: MioGlassSurface(
          palette: p,
          radius: 22,
          tint: p.surface.withValues(alpha: p.dark ? .5 : .42),
          child: Container(
            width: width,
            padding: EdgeInsets.fromLTRB(
              padding,
              narrow ? 12 : padding,
              padding,
              padding,
            ),
            decoration: BoxDecoration(
              border: Border.all(color: const Color(0x40ffffff)),
              borderRadius: BorderRadius.circular(22),
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
                        margin: const EdgeInsets.only(bottom: 12),
                        decoration: BoxDecoration(
                          color: const Color(0x5c787880),
                          borderRadius: BorderRadius.circular(999),
                        ),
                      ),
                    ),
                  _header(narrow),
                  const SizedBox(height: 8),
                  Text(
                    widget.info.title,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: p.secondary, fontSize: 12),
                  ),
                  if (widget.batchLabel != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      widget.batchLabel!,
                      style: TextStyle(color: p.secondary),
                    ),
                  ],
                  const SizedBox(height: 10),
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxHeight: math.min(320, box.maxHeight * .5),
                    ),
                    child: ListView.separated(
                      shrinkWrap: true,
                      itemCount: widget.info.choices.length,
                      separatorBuilder: (_, _) => const SizedBox(height: 8),
                      itemBuilder: (context, index) =>
                          _row(widget.info.choices[index]),
                    ),
                  ),
                  if (widget.onStop != null) ...[
                    const SizedBox(height: 12),
                    Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        MioButton(
                          palette: p,
                          label: p.english ? 'Skip this song' : '跳过当前歌曲',
                          onPressed: _busy ? null : _dismiss,
                          child: Text(p.english ? 'Skip this song' : '跳过当前歌曲'),
                        ),
                        MioButton(
                          palette: p,
                          label: p.english ? 'Cancel remaining' : '取消剩余歌曲',
                          onPressed: _busy
                              ? null
                              : () {
                                  widget.onStop!();
                                  _dismiss();
                                },
                          child: Text(
                            p.english ? 'Cancel remaining' : '取消剩余歌曲',
                          ),
                        ),
                      ],
                    ),
                  ],
                  if (_busy)
                    Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Semantics(
                        liveRegion: true,
                        child: Text(
                          p.english ? 'Adding download…' : '正在添加下载…',
                          style: TextStyle(color: p.secondary),
                        ),
                      ),
                    ),
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Text(
                        _error!,
                        style: TextStyle(color: p.color('--td-error-color')),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

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
        child: MioDialogScope(
          node: _scope,
          child: Semantics(
            scopesRoute: true,
            namesRoute: true,
            explicitChildNodes: true,
            label: p.text('download.selectQuality'),
            child: DefaultTextStyle(
              style: TextStyle(
                fontSize: 14,
                color: p.color('--td-text-color-primary'),
                fontFamily: 'PingFang SC',
                fontFamilyFallback: const ['sans-serif'],
                decoration: TextDecoration.none,
              ),
              child: LayoutBuilder(
                builder: (context, box) {
                  final narrow = !MioDeviceScope.wide(context, box.maxWidth);
                  final gutter = (box.maxWidth * .04).clamp(16.0, 24.0);
                  final width = math.max(
                    0.0,
                    math.min(
                      box.maxWidth - (narrow ? 2 * gutter : 32),
                      narrow ? 440.0 : 420.0,
                    ),
                  );
                  final top = narrow
                      ? 0.0
                      : math.max(0.0, (box.maxHeight - 250) / 2);
                  return Stack(
                    children: [
                      Positioned.fill(
                        child: BackdropFilter(
                          filter: ImageFilter.blur(
                            sigmaX: narrow ? 12 : 8,
                            sigmaY: narrow ? 12 : 8,
                          ),
                          child: ModalBarrier(
                            color: narrow
                                ? p.color('--mobile-scrim')
                                : const Color(0x73000000),
                            dismissible: !_busy,
                            onDismiss: _dismiss,
                            semanticsLabel: p.text('common.cancel'),
                          ),
                        ),
                      ),
                      SafeArea(
                        child: Padding(
                          padding: EdgeInsets.only(
                            top: top,
                            bottom: narrow ? 12 : 0,
                          ),
                          child: Align(
                            alignment: narrow
                                ? Alignment.bottomCenter
                                : Alignment.topCenter,
                            child: _panel(box, narrow, width),
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

import 'dart:ui' show ImageFilter;

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';
import 'visual_policy.dart';

/// Desktop download cards; commands still belong to the catalog's live snapshot.
class DownloadTaskCard extends StatefulWidget {
  final api.DownloadItem task;
  final MioPalette palette;
  final bool narrow, active, enabled;
  final ValueChanged<api.DownloadAction> onAction;
  final VoidCallback onOpen;
  const DownloadTaskCard({
    super.key,
    required this.task,
    required this.palette,
    required this.narrow,
    required this.active,
    required this.enabled,
    required this.onAction,
    required this.onOpen,
  });

  @override
  State<DownloadTaskCard> createState() => _DownloadTaskCardState();
}

class _DownloadTaskCardState extends State<DownloadTaskCard>
    with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  late final _pulse = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 2),
  );
  bool _hovered = false;
  bool _animate = false;
  bool _foreground = true;
  bool get _downloading => widget.task.state == api.DownloadState.downloading;

  @override
  void initState() {
    super.initState();
    final lifecycle = WidgetsBinding.instance.lifecycleState;
    _foreground = lifecycle == null || lifecycle == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
  }

  void _syncAnimation() {
    _animate =
        _foreground &&
        widget.active &&
        TickerMode.valuesOf(context).enabled &&
        !MioVisualPolicy.of(context).reduced &&
        !MediaQuery.disableAnimationsOf(context);
    if (_animate && _downloading) {
      if (!_pulse.isAnimating) _pulse.repeat();
    } else {
      _pulse.stop();
      _pulse.value = 0;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncAnimation();
  }

  @override
  void didUpdateWidget(DownloadTaskCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncAnimation();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _syncAnimation();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _pulse.dispose();
    super.dispose();
  }

  static String _bytes(num bytes, {bool speed = false}) {
    if (!bytes.isFinite || bytes <= 0) return speed ? '0 B/s' : '0 B';
    var value = bytes.toDouble();
    const units = ['B', 'KB', 'MB', 'GB'];
    var i = 0;
    while (value >= 1024 && i < units.length - 1) {
      value /= 1024;
      i++;
    }
    return '${value.toStringAsFixed(1)} ${units[i]}${speed ? '/s' : ''}';
  }

  String? _remaining() {
    final seconds = widget.task.remaining;
    if (seconds == null || !seconds.isFinite || seconds <= 0) return null;
    final p = widget.palette;
    return seconds < 60
        ? p
              .text('common.unitSecond')
              .replaceAll('{seconds}', '${seconds.round()}')
        : p
              .text('common.unitMinuteSecond')
              .replaceAll('{m}', '${seconds ~/ 60}')
              .replaceAll('{s}', '${(seconds % 60).round()}');
  }

  String _status() {
    final t = widget.task;
    final p = widget.palette;
    final total = t.total ?? 0;
    final progress = total > 0
        ? '${(t.received * 100 ~/ total).clamp(0, 100)}%'
        : null;
    return switch (t.state) {
      api.DownloadState.queued => p.english ? 'Waiting' : '等待中',
      api.DownloadState.resolving =>
        p.text('download.gettingUrl').replaceFirst(RegExp(r'：$'), ''),
      api.DownloadState.paused =>
        total > 0
            ? '${p.text('common.paused')} · $progress'
            : p.text('common.paused'),
      api.DownloadState.downloading => [
        ?progress,
        _bytes(t.speed, speed: true),
        if (_remaining() case final String remaining) remaining,
      ].join(' · '),
      api.DownloadState.done =>
        t.missing
            ? (p.english ? 'File moved or deleted' : '文件已被移动或删除')
            : t.skipped
            ? (p.english ? 'Already exists, skipped' : '文件已存在，已跳过')
            : [
                p.text('download.alreadyDownloaded'),
                if (t.tagFailed) p.text('download.tagWriteFailed'),
              ].join(' · '),
      // The mobile bridge intentionally does not expose raw provider error text.
      api.DownloadState.failed => p.text('download.tabFailed'),
      api.DownloadState.cancelled => p.english ? 'Cancelled' : '已取消',
    };
  }

  @override
  Widget build(BuildContext context) {
    final t = widget.task;
    final p = widget.palette;
    final narrow = widget.narrow;
    final primary = p.color('--td-text-color-primary');
    final failed = t.state == api.DownloadState.failed;
    final paused = t.state == api.DownloadState.paused;
    final done = t.state == api.DownloadState.done;
    final active =
        t.state == api.DownloadState.queued ||
        t.state == api.DownloadState.resolving ||
        _downloading;
    final statusColor = p.color(switch (t.state) {
      api.DownloadState.done => '--td-success-color',
      api.DownloadState.failed => '--td-error-color',
      api.DownloadState.paused ||
      api.DownloadState.queued => '--td-warning-color',
      api.DownloadState.cancelled => '--td-text-color-placeholder',
      _ => '--td-brand-color',
    });
    final gradientStart = failed || done ? statusColor : p.brand;
    final gradientEnd = Color.lerp(
      gradientStart,
      p.color(
        failed
            ? '--td-error-color-hover'
            : done
            ? '--td-success-color-hover'
            : '--td-brand-color-8',
      ),
      .3,
    )!;
    final fraction = done
        ? 1.0
        : (t.total ?? 0) > 0
        ? (t.received / t.total!).clamp(0.0, 1.0)
        : 0.0;
    final status = Text(
      _status(),
      maxLines: narrow ? null : 1,
      overflow: narrow ? null : TextOverflow.ellipsis,
      style: TextStyle(
        fontSize: 12,
        height: narrow ? 1.4 : 1.6,
        color: failed ? statusColor : p.secondary,
      ),
    );
    Widget action(
      String key,
      String icon,
      String label,
      VoidCallback callback, {
      bool danger = false,
    }) => SizedBox(
      key: ValueKey(key),
      width: narrow ? 44 : 32,
      height: narrow ? 44 : 32,
      child: MioButton(
        palette: p,
        label: '$label · ${t.title}',
        padding: EdgeInsets.zero,
        radius: 999,
        background: primary.withValues(alpha: .06),
        insetFocusBorder: false,
        preserveFocusWhenDisabled: true,
        onPressed: widget.enabled ? callback : null,
        child: Center(
          child: MioIcon(
            icon,
            size: narrow ? 16 : 14,
            color: danger ? p.color('--td-error-color') : p.secondary,
          ),
        ),
      ),
    );
    final actions = Wrap(
      spacing: narrow ? 8 : 6,
      runSpacing: 8,
      alignment: WrapAlignment.end,
      children: [
        if (active)
          action(
            'primary',
            'Pause',
            p.text('common.pause'),
            () => widget.onAction(api.DownloadAction.pause),
          ),
        if (paused)
          action(
            'primary',
            'Play',
            p.text('common.continue_'),
            () => widget.onAction(api.DownloadAction.resume),
          ),
        if (failed || t.state == api.DownloadState.cancelled)
          action(
            'primary',
            'Refresh',
            p.text('common.retry'),
            () => widget.onAction(api.DownloadAction.retry),
          ),
        if (active || paused || failed)
          action(
            'cancel',
            'WindowClose',
            p.text('common.cancel'),
            () => widget.onAction(api.DownloadAction.cancel),
            danger: true,
          ),
        if (done && !t.missing)
          action(
            'open',
            'Folder',
            p.english ? 'Open file' : '打开文件',
            widget.onOpen,
          ),
        if (!active && !paused)
          action(
            'remove',
            'Delete',
            p.text('common.delete'),
            () => widget.onAction(api.DownloadAction.remove),
            danger: true,
          ),
      ],
    );
    return Padding(
      padding: EdgeInsets.only(bottom: narrow ? 10 : 0),
      child: MouseRegion(
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: TweenAnimationBuilder<double>(
          tween: Tween(end: _hovered ? 1 : 0),
          duration: _animate
              ? const Duration(milliseconds: 280)
              : Duration.zero,
          curve: const Cubic(.16, 1, .3, 1),
          builder: (context, hover, _) => Transform.translate(
            offset: Offset(0, narrow ? 0 : -2 * hover),
            child: DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(16),
                boxShadow: [
                  BoxShadow(
                    color: const Color(0xff000000)
                        .withValues(alpha: .08 + .04 * hover),
                    offset: Offset(0, 3 + 3 * hover),
                    blurRadius: 12 + 8 * hover,
                  ),
                ],
              ),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(16),
                child: BackdropFilter(
                  enabled:
                      _foreground &&
                      widget.active &&
                      !MioVisualPolicy.of(context).reduced &&
                      TickerMode.valuesOf(context).enabled,
                  filter: ImageFilter.compose(
                    outer: ImageFilter.blur(sigmaX: 6, sigmaY: 6),
                    inner: const ColorFilter.matrix([
                      1.6296,
                      -.572,
                      -.0576,
                      0,
                      0,
                      -.1704,
                      1.228,
                      -.0576,
                      0,
                      0,
                      -.1704,
                      -.572,
                      1.7424,
                      0,
                      0,
                      0,
                      0,
                      0,
                      1,
                      0,
                    ]),
                  ),
                  child: Container(
                    padding: EdgeInsets.symmetric(
                      horizontal: narrow ? 14 : 12,
                      vertical: narrow ? 14 : 10,
                    ),
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(16),
                      border: Border.all(
                        color: primary.withValues(alpha: .10 + .08 * hover),
                      ),
                      gradient: LinearGradient(
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                        colors: [
                          p.surface.withValues(alpha: .60),
                          p.surface.withValues(alpha: .48),
                        ],
                      ),
                    ),
                    child: AnimatedBuilder(
                      animation: _pulse,
                      builder: (context, _) {
                        final phase = _pulse.value < .5
                            ? _pulse.value * 2
                            : (1 - _pulse.value) * 2;
                        final pulse = const Cubic(
                          .42,
                          0,
                          .58,
                          1,
                        ).transform(phase);
                        Color brighten(Color c) => Color.from(
                          alpha: c.a,
                          red: (c.r * (1 + .2 * pulse)).clamp(0, 1),
                          green: (c.g * (1 + .2 * pulse)).clamp(0, 1),
                          blue: (c.b * (1 + .2 * pulse)).clamp(0, 1),
                        );
                        final info = Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          spacing: narrow ? 10 : 4,
                          children: [
                            Row(
                              children: [
                                Opacity(
                                  opacity: paused ? .6 : 1 - .5 * pulse,
                                  child: Container(
                                    width: 8,
                                    height: 8,
                                    decoration: BoxDecoration(
                                      color: statusColor,
                                      shape: BoxShape.circle,
                                      boxShadow: _downloading
                                          ? [
                                              BoxShadow(
                                                color: p.brand.withValues(
                                                  alpha: .5,
                                                ),
                                                blurRadius: 6,
                                              ),
                                            ]
                                          : null,
                                    ),
                                  ),
                                ),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Text(
                                    t.title,
                                    maxLines: narrow ? null : 1,
                                    overflow: narrow
                                        ? null
                                        : TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: narrow ? 15 : 14,
                                      height: narrow ? 1.4 : 1.6,
                                      fontWeight: FontWeight.w600,
                                      color: primary,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                            DefaultTextStyle(
                              style: TextStyle(
                                fontSize: 12,
                                height: 1.6,
                                color: p.secondary,
                              ),
                              child: Wrap(
                                spacing: 6,
                                crossAxisAlignment: WrapCrossAlignment.center,
                                children: [
                                  if (narrow)
                                    Text(t.artist)
                                  else
                                    ConstrainedBox(
                                      constraints: const BoxConstraints(
                                        maxWidth: 150,
                                      ),
                                      child: Text(
                                        t.artist,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                    ),
                                  const Text('·'),
                                  Container(
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: 5,
                                    ),
                                    decoration: BoxDecoration(
                                      borderRadius: BorderRadius.circular(4),
                                      border: Border.all(
                                        color: primary.withValues(alpha: .08),
                                      ),
                                      color: p
                                          .color(
                                            '--td-bg-color-secondarycontainer',
                                          )
                                          .withValues(alpha: .85),
                                    ),
                                    child: Text(
                                      t.quality.toUpperCase(),
                                      style: const TextStyle(
                                        fontSize: 11,
                                        fontWeight: FontWeight.w500,
                                      ),
                                    ),
                                  ),
                                  const Text('·'),
                                  Text(
                                    '${_bytes(t.received)} / ${_bytes(t.total ?? 0)}',
                                    style: TextStyle(
                                      color: p.color(
                                        '--td-text-color-placeholder',
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            if (narrow && _downloading)
                              Wrap(
                                spacing: 10,
                                children: [
                                  Text(
                                    _bytes(t.speed, speed: true),
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: p.brand,
                                    ),
                                  ),
                                  if (_remaining() case final String remaining)
                                    Text(
                                      remaining,
                                      style: TextStyle(
                                        fontSize: 12,
                                        color: p.color(
                                          '--td-text-color-placeholder',
                                        ),
                                      ),
                                    ),
                                ],
                              ),
                            Semantics(
                              label: p.english ? 'Download progress' : '下载进度',
                              value: '${(fraction * 100).round()}%',
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(999),
                                child: SizedBox(
                                  height: narrow ? 8 : 6,
                                  child: ColoredBox(
                                    color: primary.withValues(alpha: .08),
                                    child: TweenAnimationBuilder<double>(
                                      tween: Tween(end: fraction),
                                      duration: _animate
                                          ? const Duration(milliseconds: 300)
                                          : Duration.zero,
                                      curve: const Cubic(.25, .1, .25, 1),
                                      builder: (context, value, _) => Align(
                                        alignment: Alignment.centerLeft,
                                        child: FractionallySizedBox(
                                          widthFactor: value.clamp(0, 1),
                                          heightFactor: 1,
                                          child: DecoratedBox(
                                            decoration: BoxDecoration(
                                              borderRadius:
                                                  BorderRadius.circular(999),
                                              gradient: LinearGradient(
                                                colors: [
                                                  brighten(gradientStart),
                                                  brighten(gradientEnd),
                                                ],
                                              ),
                                            ),
                                          ),
                                        ),
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ],
                        );
                        return narrow
                            ? Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                spacing: 10,
                                children: [
                                  info,
                                  if (failed ||
                                      (done &&
                                          (t.missing ||
                                              t.skipped ||
                                              t.tagFailed)))
                                    status,
                                  Container(
                                    padding: const EdgeInsets.only(top: 8),
                                    decoration: BoxDecoration(
                                      border: Border(
                                        top: BorderSide(
                                          color: primary.withValues(alpha: .06),
                                        ),
                                      ),
                                    ),
                                    child: actions,
                                  ),
                                ],
                              )
                            : Row(
                                children: [
                                  Expanded(child: info),
                                  const SizedBox(width: 12),
                                  ConstrainedBox(
                                    constraints: const BoxConstraints(
                                      maxWidth: 260,
                                    ),
                                    child: status,
                                  ),
                                  const SizedBox(width: 12),
                                  actions,
                                ],
                              );
                      },
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

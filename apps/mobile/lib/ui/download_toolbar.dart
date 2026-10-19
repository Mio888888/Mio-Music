import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

/// The desktop downloads page owns its toolbar, rather than using the generic
/// catalog heading. Callbacks keep all task ownership in the Rust snapshot.
class DownloadToolbar extends StatelessWidget {
  final MioPalette palette;
  final bool narrow, enabled, activeBucket, canStart, canPause, canClear;
  final bool settingsOpen;
  final bool searchEnabled;
  final double rem;
  final TextEditingController filter;
  final FocusNode filterFocus;
  final ValueChanged<String> onQuery;
  final ValueChanged<api.DownloadBulkAction> onBulk;
  final VoidCallback onSettings;

  /// Wide layouts drop the page title; the status tabs take its place.
  final Widget? leading;

  const DownloadToolbar({
    super.key,
    required this.palette,
    required this.narrow,
    required this.enabled,
    required this.activeBucket,
    required this.canStart,
    required this.canPause,
    required this.canClear,
    required this.settingsOpen,
    required this.searchEnabled,
    required this.rem,
    required this.filter,
    required this.filterFocus,
    required this.onQuery,
    required this.onBulk,
    required this.onSettings,
    this.leading,
  });

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final title = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 10,
          height: 10,
          decoration: BoxDecoration(
            color: p.brand,
            shape: BoxShape.circle,
            boxShadow: [
              BoxShadow(color: p.brand.withValues(alpha: .4), blurRadius: 8),
            ],
          ),
        ),
        const SizedBox(width: 12),
        Text(
          p.text('download.title'),
          style: TextStyle(
            fontSize: rem * (narrow ? 1.5 : 1.75),
            height: 1.3,
            fontWeight: FontWeight.w700,
          ),
        ),
      ],
    );
    final search = MioInput(
      palette: p,
      controller: filter,
      focusNode: filterFocus,
      placeholder: p.text('download.searchTask'),
      height: narrow ? 44 : 34,
      fontSize: narrow ? 16 : 13,
      leadingIcon: 'Search',
      background: p.color('--td-bg-color-component').withValues(alpha: .6),
      onChanged: onQuery,
      readOnly: !searchEnabled,
    );
    final actions = <Widget>[
      if (activeBucket) ...[
        _ToolbarButton(
          key: const ValueKey('start-all'),
          palette: p,
          narrow: narrow,
          iconOnly: leading != null,
          icon: 'Play',
          label: p.text('download.startAll'),
          onPressed: enabled && canStart
              ? () => onBulk(api.DownloadBulkAction.startAll)
              : null,
        ),
        _ToolbarButton(
          key: const ValueKey('pause-all'),
          palette: p,
          narrow: narrow,
          iconOnly: leading != null,
          icon: 'Pause',
          label: p.text('download.pauseAll'),
          onPressed: enabled && canPause
              ? () => onBulk(api.DownloadBulkAction.pauseAll)
              : null,
        ),
      ],
      if (canClear)
        _ToolbarButton(
          key: const ValueKey('clear'),
          palette: p,
          narrow: narrow,
          iconOnly: leading != null,
          icon: 'Delete',
          label: p.text(
            activeBucket ? 'download.clearQueue' : 'download.clearRecords',
          ),
          onPressed: enabled
              ? () => onBulk(api.DownloadBulkAction.clear)
              : null,
        ),
      _ToolbarButton(
        key: const ValueKey('settings'),
        palette: p,
        narrow: narrow,
        icon: 'nav-shezhi',
        label: p.text('common.titleBar.settings'),
        iconOnly: true,
        active: settingsOpen,
        onPressed: enabled ? onSettings : null,
      ),
    ];
    final buttons = Wrap(spacing: 8, runSpacing: 8, children: actions);
    if (narrow && leading != null) {
      // Narrow: status tabs and the settings toggle lead, then the search box
      // shares a row with the bulk actions.
      final settings = actions.last;
      return Padding(
        padding: const EdgeInsets.only(bottom: 12),
        child: Column(
          children: [
            Row(
              children: [
                Expanded(child: leading!),
                const SizedBox(width: 8),
                settings,
              ],
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(child: search),
                for (final action in actions.sublist(
                  0,
                  actions.length - 1,
                )) ...[const SizedBox(width: 8), action],
              ],
            ),
          ],
        ),
      );
    }
    return Padding(
      padding: EdgeInsets.only(bottom: narrow ? 12 : 16),
      child: narrow
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                title,
                const SizedBox(height: 12),
                search,
                const SizedBox(height: 8),
                buttons,
              ],
            )
          : Row(
              children: [
                if (leading != null)
                  Expanded(child: leading!)
                else ...[
                  title,
                  const Spacer(),
                ],
                const SizedBox(width: 12),
                SizedBox(width: 168, child: search),
                for (final action in actions) ...[
                  const SizedBox(width: 8),
                  action,
                ],
              ],
            ),
    );
  }
}

class _ToolbarButton extends StatelessWidget {
  final MioPalette palette;
  final bool narrow, iconOnly, active;
  final String icon, label;
  final VoidCallback? onPressed;
  const _ToolbarButton({
    super.key,
    required this.palette,
    required this.narrow,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.iconOnly = false,
    this.active = false,
  });
  @override
  Widget build(BuildContext context) {
    final p = palette;
    final primary = p.color('--td-text-color-primary');
    final color = active ? p.brand : p.secondary;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: active
            ? p.brand.withValues(alpha: .12)
            : primary.withValues(alpha: .04),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(
          color: active
              ? p.brand.withValues(alpha: .25)
              : primary.withValues(alpha: .10),
        ),
      ),
      child: MioButton(
        preserveFocusWhenDisabled: true,
        palette: p,
        label: label,
        radius: 999,
        disabledOpacity: .5,
        insetFocusBorder: false,
        padding: EdgeInsets.symmetric(
          horizontal: narrow
              ? 14
              : iconOnly
              ? 12
              : 10.4,
          vertical: narrow ? 0 : 6,
        ),
        onPressed: onPressed,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: narrow ? 44 : 17),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              MioIcon(icon, size: 14, color: color),
              if (!iconOnly) ...[
                const SizedBox(width: 5),
                Text(
                  label,
                  style: TextStyle(fontSize: 13, height: 16 / 13, color: color),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class DownloadTabs extends StatelessWidget {
  final MioPalette palette;
  final bool narrow, enabled;

  /// Drops the spacing below the tabs when they share a row with the toolbar.
  final bool inline;
  final api.DownloadBucket selected;
  final List<int> counts;
  final ValueChanged<api.DownloadBucket> onSelected;
  const DownloadTabs({
    super.key,
    required this.palette,
    required this.narrow,
    required this.enabled,
    required this.selected,
    required this.counts,
    required this.onSelected,
    this.inline = false,
  });
  @override
  Widget build(BuildContext context) {
    final p = palette;
    final primary = p.color('--td-text-color-primary');
    const keys = [
      'download.tabActive',
      'download.tabCompleted',
      'download.tabFailed',
    ];
    const dots = ['--td-brand-color', '--td-success-color', '--td-error-color'];
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Padding(
        padding: EdgeInsets.only(bottom: inline ? 0 : (narrow ? 20 : 16)),
        child: Row(
          children: [
            for (var i = 0; i < keys.length; i++)
              Padding(
                padding: EdgeInsets.only(right: i < 2 ? 8 : 0),
                child: Semantics(
                  selected: selected.index == i,
                  child: MioButton(
                    preserveFocusWhenDisabled: true,
                    palette: p,
                    label: '${p.text(keys[i])}, ${counts[i]}',
                    radius: 999,
                    insetFocusBorder: false,
                    background: selected.index == i
                        ? p.brand.withValues(alpha: .15)
                        : primary.withValues(alpha: .06),
                    padding: EdgeInsets.symmetric(
                      horizontal: narrow ? 18 : 15.35,
                      vertical: 8,
                    ),
                    onPressed: enabled
                        ? () => onSelected(api.DownloadBucket.values[i])
                        : null,
                    child: ConstrainedBox(
                      constraints: BoxConstraints(minHeight: narrow ? 28 : 16),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Container(
                            width: 8,
                            height: 8,
                            decoration: BoxDecoration(
                              color: p.color(dots[i]),
                              shape: BoxShape.circle,
                            ),
                          ),
                          const SizedBox(width: 8),
                          Text(
                            p.text(keys[i]),
                            style: TextStyle(
                              fontSize: 13,
                              height: 16 / 13,
                              color: selected.index == i
                                  ? p.brand
                                  : p.secondary,
                            ),
                          ),
                          if (counts[i] > 0) ...[
                            const SizedBox(width: 8),
                            Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 7,
                                vertical: 1,
                              ),
                              decoration: BoxDecoration(
                                color: selected.index == i
                                    ? p.brand.withValues(alpha: .20)
                                    : primary.withValues(alpha: .10),
                                borderRadius: BorderRadius.circular(999),
                              ),
                              child: Text(
                                '${counts[i]}',
                                style: TextStyle(
                                  fontSize: 11,
                                  height: 14 / 11,
                                  fontWeight: FontWeight.w600,
                                  color: selected.index == i
                                      ? p.brand
                                      : p.secondary,
                                ),
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class DownloadConcurrency extends StatelessWidget {
  final MioPalette palette;
  final bool narrow, enabled;
  final int value;
  final ValueChanged<int> onChanged;
  const DownloadConcurrency({
    super.key,
    required this.palette,
    required this.narrow,
    required this.enabled,
    required this.value,
    required this.onChanged,
  });
  @override
  Widget build(BuildContext context) {
    final p = palette;
    Widget step(String label, int delta) => SizedBox(
      width: narrow ? 44 : 32,
      height: narrow ? 44 : 32,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.color('--td-bg-color-component'),
          border: Border.all(color: p.border),
          shape: BoxShape.circle,
        ),
        child: MioButton(
          preserveFocusWhenDisabled: true,
          palette: p,
          label: delta < 0
              ? (p.english ? 'Fewer concurrent downloads' : '减少同时下载数')
              : (p.english ? 'More concurrent downloads' : '增加同时下载数'),
          radius: 999,
          padding: EdgeInsets.zero,
          onPressed: enabled && (delta < 0 ? value > 1 : value < 5)
              ? () => onChanged(value + delta)
              : null,
          child: Center(
            child: Text(label, style: const TextStyle(fontSize: 16)),
          ),
        ),
      ),
    );
    final controls = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        step('-', -1),
        const SizedBox(width: 12),
        SizedBox(
          width: 20,
          child: Semantics(
            liveRegion: true,
            child: Text(
              '$value',
              textAlign: TextAlign.center,
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
          ),
        ),
        const SizedBox(width: 12),
        step('+', 1),
      ],
    );
    final label = Text(
      p.text('download.concurrentDownloads'),
      style: TextStyle(fontSize: 13, color: p.secondary),
    );
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 12),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: p.color('--td-bg-color-component').withValues(alpha: .7),
        border: Border.all(color: p.border),
        borderRadius: BorderRadius.circular(12),
      ),
      child: narrow
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [label, const SizedBox(height: 10), controls],
            )
          : Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [label, controls],
            ),
    );
  }
}

class DownloadEmpty extends StatelessWidget {
  final MioPalette palette;
  final bool narrow, searching;
  const DownloadEmpty({
    super.key,
    required this.palette,
    required this.narrow,
    required this.searching,
  });
  @override
  Widget build(BuildContext context) {
    final p = palette;
    return Container(
      width: double.infinity,
      padding: EdgeInsets.symmetric(horizontal: 20, vertical: narrow ? 48 : 60),
      decoration: BoxDecoration(
        border: Border.all(
          color: p.color('--td-text-color-primary').withValues(alpha: .08),
        ),
        borderRadius: BorderRadius.circular(16),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            p.surface.withValues(alpha: .5),
            p.surface.withValues(alpha: .35),
          ],
        ),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          MioIcon(
            'nav-xiazai',
            size: 48,
            color: p
                .color('--td-text-color-placeholder')
                .withValues(
                  alpha: p.color('--td-text-color-placeholder').a * .5,
                ),
          ),
          const SizedBox(height: 16),
          Text(
            p.text(searching ? 'download.noMatch' : 'download.empty'),
            style: TextStyle(fontSize: 14, color: p.secondary),
          ),
        ],
      ),
    );
  }
}

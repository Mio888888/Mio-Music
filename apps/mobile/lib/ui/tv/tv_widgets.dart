import 'package:flutter/widgets.dart';

import '../controls.dart';
import '../cover.dart';
import '../resources.dart';

/// Rounded choice used for filters and actions on TV: filled with the brand
/// colour when selected, a soft fill otherwise.
class TvPill extends StatelessWidget {
  final MioPalette palette;
  final String label;
  final VoidCallback? onPressed;
  final bool selected, outlined, filled;
  final String? icon;
  final FocusNode? focusNode;
  final bool autofocus;
  const TvPill({
    super.key,
    required this.palette,
    required this.label,
    required this.onPressed,
    this.selected = false,
    this.outlined = false,
    this.filled = false,
    this.icon,
    this.focusNode,
    this.autofocus = false,
  });

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final strong = selected || filled;
    final color = strong ? p.color('--td-text-color-anti') : p.secondary;
    return Semantics(
      selected: selected,
      child: MioButton(
        palette: p,
        label: label,
        radius: 999,
        insetFocusBorder: false,
        focusNode: focusNode,
        autofocus: autofocus,
        padding: EdgeInsets.symmetric(
          horizontal: icon == null ? 16 : 18,
          vertical: 7,
        ),
        background: strong
            ? p.brand
            : outlined
            ? p.color('--td-bg-color-secondarycontainer')
            : p.surface,
        onPressed: onPressed,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (icon != null) ...[
              MioIcon(icon!, size: 16, color: color),
              const SizedBox(width: 8),
            ],
            Flexible(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 14,
                  height: 1.2,
                  fontWeight: strong ? FontWeight.w700 : FontWeight.w500,
                  color: color,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Two or more mutually exclusive views as one pill-shaped switch.
class TvSegmented extends StatelessWidget {
  final MioPalette palette;
  final List<String> labels;
  final int selected;
  final ValueChanged<int>? onSelect;
  const TvSegmented({
    super.key,
    required this.palette,
    required this.labels,
    required this.selected,
    required this.onSelect,
  });

  @override
  Widget build(BuildContext context) {
    final p = palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: p.color('--td-bg-color-secondarycontainer'),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Padding(
        padding: const EdgeInsets.all(3),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < labels.length; i++) ...[
              if (i > 0) const SizedBox(width: 2),
              Semantics(
                selected: i == selected,
                child: MioButton(
                  palette: p,
                  label: labels[i],
                  radius: 999,
                  insetFocusBorder: false,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 20,
                    vertical: 8,
                  ),
                  background: i == selected ? p.brand : const Color(0x00000000),
                  onPressed: onSelect == null ? null : () => onSelect!(i),
                  child: Text(
                    labels[i],
                    maxLines: 1,
                    style: TextStyle(
                      fontSize: 15,
                      height: 1.2,
                      fontWeight: FontWeight.w700,
                      color: i == selected
                          ? p.color('--td-text-color-anti')
                          : p.secondary,
                    ),
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// Large square cover with a title and one detail line, sized for grids on TV.
class TvCoverCard extends StatelessWidget {
  final MioPalette palette;
  final String label, title, detail;
  final String? cover;
  final double width;
  final VoidCallback? onPressed;
  final FocusNode? focusNode;

  /// Room the card needs below its cover for the two text lines.
  static const textHeight = 52.0;
  const TvCoverCard({
    super.key,
    required this.palette,
    required this.label,
    required this.title,
    required this.detail,
    required this.cover,
    required this.width,
    required this.onPressed,
    this.focusNode,
  });

  @override
  Widget build(BuildContext context) {
    final p = palette;
    return MioButton(
      palette: p,
      focusNode: focusNode,
      label: label,
      radius: 16,
      insetFocusBorder: false,
      padding: const EdgeInsets.all(6),
      onPressed: onPressed,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          MioCover(cover, size: width - 12, radius: 12),
          const SizedBox(height: 6),
          Text(
            title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 15,
              height: 1.3,
              fontWeight: FontWeight.w700,
              color: p.color('--td-text-color-primary'),
            ),
          ),
          if (detail.isNotEmpty)
            Text(
              detail,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12, height: 1.35, color: p.secondary),
            ),
        ],
      ),
    );
  }
}

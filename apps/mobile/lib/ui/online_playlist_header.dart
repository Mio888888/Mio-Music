import 'device.dart';

import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'cover.dart';
import 'resources.dart';

/// A collection header uses its artwork and remains above the scrolling rows.
class OnlinePlaylistHeader extends StatelessWidget {
  final MioPalette palette;
  final String title, description, count;
  final String? cover;
  final bool compact, wide, busy;
  final TextEditingController filter;
  final FocusNode filterFocus;
  final VoidCallback? onBack, onPlay, onShuffle;
  final ValueChanged<String> onQuery;
  final ValueChanged<Offset>? onMore;
  final bool search;
  const OnlinePlaylistHeader({
    super.key,
    required this.palette,
    required this.title,
    required this.description,
    required this.count,
    required this.cover,
    required this.compact,
    required this.wide,
    required this.busy,
    required this.filter,
    required this.filterFocus,
    required this.onBack,
    required this.onPlay,
    required this.onShuffle,
    required this.onQuery,
    required this.onMore,
    this.search = true,
  });

  @override
  Widget build(BuildContext context) {
    if (wide) return _buildWide(context);
    final p = palette;
    final primary = p.color('--td-text-color-primary');
    final small = compact && !wide;
    Widget action(
      String icon,
      String label,
      VoidCallback? run, {
      bool filled = false,
    }) {
      final radius = BorderRadius.circular(999);
      return DecoratedBox(
        decoration: BoxDecoration(
          color: filled ? p.brand : p.surface.withValues(alpha: .25),
          borderRadius: radius,
          border: filled ? null : Border.all(color: p.border),
        ),
        child: MioButton(
          palette: p,
          label: label,
          onPressed: run,
          radius: 999,
          insetFocusBorder: false,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: SizedBox(
            height: 44,
            width: small ? 20 : null,
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                MioIcon(
                  icon,
                  size: 18,
                  color: filled ? p.color('--td-text-color-anti') : primary,
                ),
                if (!small) ...[
                  const SizedBox(width: 8),
                  Flexible(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                        color: filled
                            ? p.color('--td-text-color-anti')
                            : primary,
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

    Widget more() => Builder(
      builder: (buttonContext) => SizedBox(
        width: 44,
        height: 44,
        child: DecoratedBox(
          decoration: BoxDecoration(
            border: Border.all(color: p.border),
            borderRadius: BorderRadius.circular(999),
          ),
          child: MioButton(
            palette: p,
            label: p.english ? 'More actions' : '更多操作',
            radius: 999,
            padding: EdgeInsets.zero,
            onPressed: onMore == null
                ? null
                : () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      onMore!(box.localToGlobal(Offset(0, box.size.height)));
                    }
                  },
            child: Center(
              child: MioIcon('TD-Ellipsis', color: primary, size: 20),
            ),
          ),
        ),
      ),
    );
    final artwork = MioCover(
      cover,
      size: small ? 52 : (wide ? 160 : 112),
      radius: small ? 8 : 12,
      cacheSize: wide ? null : 112,
    );
    final information = Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            title,
            maxLines: small ? 1 : 2,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: small
                  ? 16
                  : (wide
                        ? 30
                        : (MediaQuery.sizeOf(context).width * .055).clamp(
                            19.2,
                            24.8,
                          )),
              height: 1.12,
              fontWeight: FontWeight.w800,
              color: primary,
            ),
          ),
          const SizedBox(height: 6),
          if (!small && description.isNotEmpty) ...[
            Text(
              description,
              maxLines: wide ? 2 : 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: wide ? 16 : 12.8,
                height: 1.35,
                color: p.secondary,
              ),
            ),
            const SizedBox(height: 4),
          ],
          Text(
            count,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 12,
              color: p.color('--td-text-color-placeholder'),
            ),
          ),
        ],
      ),
    );
    final play = action(
      // The circled PlayFilled glyph turned into a dark disc on the brand fill.
      'Play',
      p.text('common.playAll'),
      onPlay,
      filled: true,
    );
    final shuffle = action('Shuffle', p.text('common.shufflePlay'), onShuffle);
    final content = small
        ? Row(
            children: [
              artwork,
              const SizedBox(width: 12),
              information,
              const SizedBox(width: 8),
              play,
              const SizedBox(width: 8),
              shuffle,
              const SizedBox(width: 8),
              more(),
            ],
          )
        : Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(children: [artwork, const SizedBox(width: 12), information]),
              const SizedBox(height: 16),
              Row(
                children: [
                  Expanded(child: play),
                  const SizedBox(width: 8),
                  Expanded(child: shuffle),
                ],
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  more(),
                  const SizedBox(width: 8),
                  Expanded(
                    child: MioInput(
                      palette: p,
                      controller: filter,
                      focusNode: filterFocus,
                      placeholder: p.english ? 'Search songs' : '请输入',
                      height: 44,
                      radius: 999,
                      leadingIcon: 'Search',
                      readOnly: busy,
                      onChanged: onQuery,
                      background: p.surface.withValues(alpha: .88),
                    ),
                  ),
                ],
              ),
            ],
          );
    return ClipRRect(
      borderRadius: small
          ? BorderRadius.circular(14)
          : const BorderRadius.vertical(bottom: Radius.circular(20)),
      child: Stack(
        children: [
          Positioned.fill(
            child: ImageFiltered(
              imageFilter: ImageFilter.blur(sigmaX: 28, sigmaY: 28),
              child: MioCover(
                cover,
                size: MediaQuery.sizeOf(context).width,
                radius: 0,
              ),
            ),
          ),
          Positioned.fill(
            child: ColoredBox(color: p.surface.withValues(alpha: .78)),
          ),
          Padding(
            padding: EdgeInsets.fromLTRB(
              small ? 62 : 12,
              small ? 8 : (wide ? 54 : 12),
              small ? 8 : 12,
              small ? 8 : 12,
            ),
            child: content,
          ),
          Positioned(
            left: 12,
            top: small ? 12 : 8,
            child: SizedBox(
              width: 44,
              height: 44,
              child: MioButton(
                palette: p,
                label: p.text('common.back'),
                radius: 999,
                background: p.surface.withValues(alpha: .6),
                padding: EdgeInsets.zero,
                onPressed: onBack,
                child: Center(
                  child: Text(
                    '‹',
                    style: TextStyle(fontSize: 34, height: 1, color: primary),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Desktop wide layout: a card with the cover on the left, title and counts
  /// in the middle and the action cluster on the right (stacked below the
  /// information on windows up to 1120 wide).
  Widget _buildWide(BuildContext context) {
    final p = palette;
    final primary = p.color('--td-text-color-primary');
    final width = MediaQuery.sizeOf(context).width;
    final rem = MioPalette.rem(width);
    final medium = width <= 1120;
    // Scrolled lists squeeze the header to a single 76px row so more rows fit.
    final height = compact ? 76.0 : 240.0;
    final pad = compact ? .5 * rem : 1.5 * rem;
    final side = compact
        ? height - 2 * pad
        : math.min(medium ? 160.0 : 192.0, height - 2 * pad);
    final radius = BorderRadius.circular(.75 * rem);

    Widget button(
      String icon,
      String label,
      VoidCallback? run, {
      bool filled = false,
    }) => SizedBox(
      height: 36,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: filled ? p.brand : null,
          borderRadius: BorderRadius.circular(8),
          border: filled
              ? null
              : Border.all(color: p.color('--td-component-border')),
        ),
        child: MioButton(
          palette: p,
          label: label,
          // On TV the opened page starts on its primary action.
          autofocus: filled && MioDeviceScope.isTv(context),
          radius: 8,
          insetFocusBorder: false,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          onPressed: run,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 96),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                MioIcon(
                  icon,
                  size: 16,
                  color: filled ? p.color('--td-text-color-anti') : primary,
                ),
                const SizedBox(width: 6),
                Text(
                  label,
                  maxLines: 1,
                  style: TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w500,
                    color: filled ? p.color('--td-text-color-anti') : primary,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );

    final more = Builder(
      builder: (buttonContext) => SizedBox(
        width: 36,
        height: 36,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: p.surface,
            border: Border.all(color: p.color('--td-border-level-1-color')),
            borderRadius: BorderRadius.circular(8),
          ),
          child: MioButton(
            palette: p,
            label: p.english ? 'More actions' : '更多操作',
            radius: 8,
            padding: EdgeInsets.zero,
            onPressed: onMore == null
                ? null
                : () {
                    final box = buttonContext.findRenderObject();
                    if (box is RenderBox) {
                      onMore!(box.localToGlobal(Offset(0, box.size.height)));
                    }
                  },
            child: Center(
              child: MioIcon('TD-Ellipsis', color: primary, size: 16),
            ),
          ),
        ),
      ),
    );

    final buttons = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        button('PlayFilled', p.text('common.playAll'), onPlay, filled: true),
        SizedBox(width: .75 * rem),
        button('Shuffle', p.text('common.shufflePlay'), onShuffle),
        SizedBox(width: .75 * rem),
        more,
      ],
    );
    final actions = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        buttons,
        if (search) SizedBox(height: .75 * rem),
        if (search)
          SizedBox(
            width: 260,
            child: MioInput(
              palette: p,
              controller: filter,
              focusNode: filterFocus,
              placeholder: p.english ? 'Search songs' : '请输入',
              height: 36,
              radius: 999,
              fontSize: .875 * rem,
              leadingIcon: 'Search',
              readOnly: busy,
              onChanged: onQuery,
              background: p
                  .color('--td-bg-color-container')
                  .withValues(alpha: .84),
            ),
          ),
      ],
    );

    final titleSize = compact ? 22.0 : 34.0;
    final information = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            fontSize: titleSize,
            height: 1,
            fontWeight: FontWeight.w800,
            color: primary,
          ),
        ),
        SizedBox(height: .5 * rem),
        if (compact)
          Text(
            count,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: .75 * rem,
              color: p.color('--td-text-color-placeholder'),
            ),
          )
        else ...[
          Text(
            description,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: rem, height: 1.5, color: p.secondary),
          ),
          SizedBox(height: .5 * rem),
          Padding(
            padding: const EdgeInsets.only(top: 5),
            child: Text(
              count,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: .875 * rem,
                color: p.color('--td-text-color-placeholder'),
              ),
            ),
          ),
        ],
      ],
    );
    final artwork = MioCover(cover, size: side, radius: .5 * rem);
    final content = compact
        ? Row(
            children: [
              artwork,
              SizedBox(width: rem),
              Expanded(child: information),
              SizedBox(width: rem),
              buttons,
            ],
          )
        : medium
        ? Row(
            children: [
              artwork,
              SizedBox(width: 1.5 * rem),
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    information,
                    SizedBox(height: rem),
                    Align(alignment: Alignment.centerRight, child: actions),
                  ],
                ),
              ),
            ],
          )
        : Row(
            children: [
              artwork,
              SizedBox(width: 1.5 * rem),
              Expanded(child: information),
              SizedBox(width: 1.5 * rem),
              ConstrainedBox(
                constraints: const BoxConstraints(minWidth: 240, maxWidth: 420),
                child: actions,
              ),
            ],
          );
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: radius,
        boxShadow: const [
          BoxShadow(
            offset: Offset(0, 2),
            color: Color(0x14000000),
            blurRadius: 8,
          ),
        ],
      ),
      child: ClipRRect(
        borderRadius: radius,
        child: AnimatedSize(
          alignment: Alignment.topCenter,
          duration: MediaQuery.disableAnimationsOf(context)
              ? Duration.zero
              : const Duration(milliseconds: 220),
          curve: Curves.easeOutCubic,
          child: SizedBox(
            height: compact || !medium ? height : null,
            child: ConstrainedBox(
              constraints: BoxConstraints(
                minHeight: medium && !compact ? height - 20 : 0,
              ),
              child: Stack(
                children: [
                  Positioned.fill(
                    child: ColoredBox(
                      color: p.color('--td-bg-color-container'),
                    ),
                  ),
                  Positioned.fill(
                    child: ImageFiltered(
                      imageFilter: ImageFilter.blur(sigmaX: 28, sigmaY: 28),
                      child: MioCover(cover, size: width, radius: 0),
                    ),
                  ),
                  Positioned.fill(
                    child: ColoredBox(
                      color: p
                          .color('--td-bg-color-container')
                          .withValues(alpha: .78),
                    ),
                  ),
                  Padding(
                    padding: EdgeInsets.all(pad),
                    child: Center(child: content),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'context_menu.dart';
import 'cover.dart';
import 'device.dart';
import 'resources.dart';

/// Search keeps playback, artist navigation and row actions as separate targets.
class SearchTrackRow extends StatefulWidget {
  final api.Track track;
  final api.SearchRowLabel? labels;
  final int number;
  final bool narrow;
  final MioPalette palette;
  final Widget artist;
  final Widget? trailing;
  final bool durationAtEnd;

  /// Title, artist and tags on three separate lines (wide singer lists).
  final bool threeLine;
  final VoidCallback? onPlay;
  final ValueChanged<Offset>? onMenu;

  const SearchTrackRow({
    super.key,
    required this.track,
    required this.labels,
    required this.number,
    required this.narrow,
    required this.palette,
    required this.artist,
    required this.trailing,
    required this.onPlay,
    this.onMenu,
    this.durationAtEnd = false,
    this.threeLine = false,
  });

  @override
  State<SearchTrackRow> createState() => _SearchTrackRowState();
}

class _SearchTrackRowState extends State<SearchTrackRow> {
  final _playFocus = FocusNode(debugLabel: 'Search song playback');
  final _artistGroup = FocusNode(debugLabel: 'Search song artist');

  @override
  void dispose() {
    _playFocus.dispose();
    _artistGroup.dispose();
    super.dispose();
  }

  KeyEventResult _navigate(FocusNode _, KeyEvent event) {
    if (!MioDeviceScope.isTv(context) || event is! KeyDownEvent) {
      return KeyEventResult.ignored;
    }
    // The artist lies inside the playback rectangle, so spatial traversal
    // cannot reach it. Keep vertical traversal between songs unchanged.
    if (_playFocus.hasPrimaryFocus &&
        event.logicalKey == LogicalKeyboardKey.arrowRight) {
      for (final node in _artistGroup.traversalDescendants) {
        if (node.canRequestFocus && !node.skipTraversal) {
          node.requestFocus();
          return KeyEventResult.handled;
        }
      }
    } else if (_artistGroup.hasFocus &&
        event.logicalKey == LogicalKeyboardKey.arrowLeft) {
      _playFocus.requestFocus();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final track = widget.track,
        labels = widget.labels,
        number = widget.number,
        narrow = widget.narrow,
        palette = widget.palette,
        artist = widget.artist,
        trailing = widget.trailing,
        onPlay = widget.onPlay,
        onMenu = widget.onMenu;
    final p = palette;
    final scale = MediaQuery.textScalerOf(context);
    final titleSize = narrow ? 15.0 : 14.0;
    final three = widget.threeLine;
    final stacked = narrow || three;
    final rowHeight = math.max(
      three ? 76.0 : (narrow ? 72.0 : 56.0),
      scale.scale(titleSize) +
          3 +
          math.max(scale.scale(12) + 4, scale.scale(10) + 4) +
          (stacked ? scale.scale(10) + 7 : 0) +
          16,
    );
    final duration = track.duration.isFinite && track.duration > 0
        ? '${track.duration ~/ 60}:${(track.duration.toInt() % 60).toString().padLeft(2, '0')}'
        : '--:--';
    final quality = labels?.quality.isNotEmpty == true
        ? p.text(labels!.quality)
        : '';
    final source = labels?.source ?? '';
    // Match flex-shrink: long album text gives up more space than a short artist.
    int textWeight(String value) {
      if (value.isEmpty) return 0;
      final painter = TextPainter(
        text: TextSpan(
          text: value,
          style: DefaultTextStyle.of(context).style.copyWith(fontSize: 12),
        ),
        textDirection: Directionality.of(context),
        textScaler: scale,
        maxLines: 1,
      )..layout();
      final width = math.max(1, painter.width.ceil());
      painter.dispose();
      return width;
    }

    final artistWeight = textWeight(track.artist);
    final albumWeight = textWeight(' - ${track.album}');
    Widget badge(String label, String bg, String fg, {bool fit = true}) {
      final tag = Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
        decoration: BoxDecoration(
          color: p.color(bg),
          borderRadius: BorderRadius.circular(4),
        ),
        child: Text(
          label,
          maxLines: 1,
          softWrap: false,
          overflow: fit ? TextOverflow.ellipsis : TextOverflow.visible,
          style: TextStyle(fontSize: 10, height: 1, color: p.color(fg)),
        ),
      );
      return fit ? Flexible(child: tag) : tag;
    }

    return Focus(
      canRequestFocus: false,
      skipTraversal: true,
      onKeyEvent: _navigate,
      child: MioContextRegion(
        label: p.english ? 'Song actions' : '歌曲操作',
        onOpen: onMenu,
        child: SizedBox(
          height: rowHeight,
          child: Row(
            children: [
              Expanded(
                child: MioButton(
                  palette: p,
                  focusNode: _playFocus,
                  radius: 6,
                  disabledOpacity: 1,
                  label: [
                    '$number. ${track.title}',
                    quality,
                    source,
                    track.artist,
                    track.album,
                    duration,
                  ].where((s) => s.isNotEmpty).join(' · '),
                  padding: EdgeInsets.symmetric(
                    horizontal: narrow ? 8 : 10,
                    vertical: 6,
                  ),
                  onPressed: onPlay,
                  child: Row(
                    children: [
                      SizedBox(
                        width: narrow ? 26 : 32,
                        child: Text(
                          '$number',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12,
                            height: 1,
                            color: p.secondary,
                          ),
                        ),
                      ),
                      MioCover(track.cover, size: 40, radius: 4),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              track.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: titleSize,
                                height: 1,
                                color: p.color('--theme-text-primary'),
                              ),
                            ),
                            const SizedBox(height: 3),
                            LayoutBuilder(
                              builder: (context, box) => Row(
                                children: [
                                  if (!stacked &&
                                      (quality.isNotEmpty ||
                                          source.isNotEmpty)) ...[
                                    ConstrainedBox(
                                      constraints: BoxConstraints(
                                        maxWidth: math.max(0, box.maxWidth - 4),
                                      ),
                                      child: Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          if (quality.isNotEmpty)
                                            badge(
                                              quality,
                                              '--song-list-quality-bg',
                                              '--song-list-quality-color',
                                            ),
                                          if (quality.isNotEmpty &&
                                              source.isNotEmpty)
                                            const SizedBox(width: 4),
                                          if (source.isNotEmpty)
                                            badge(
                                              source,
                                              '--song-list-source-bg',
                                              '--song-list-source-color',
                                            ),
                                        ],
                                      ),
                                    ),
                                    const SizedBox(width: 4),
                                  ],
                                  Flexible(
                                    flex: artistWeight,
                                    child: Focus(
                                      focusNode: _artistGroup,
                                      canRequestFocus: false,
                                      skipTraversal: true,
                                      child: artist,
                                    ),
                                  ),
                                  if (track.album.isNotEmpty && !three)
                                    Flexible(
                                      flex: albumWeight,
                                      child: Text(
                                        ' - ${track.album}',
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: TextStyle(
                                          fontSize: 12,
                                          height: 1,
                                          color: p.secondary,
                                        ),
                                      ),
                                    ),
                                ],
                              ),
                            ),
                            if (stacked) ...[
                              const SizedBox(height: 3),
                              // Tags stay whole; a tight column scales them down.
                              Align(
                                alignment: Alignment.centerLeft,
                                child: FittedBox(
                                  fit: BoxFit.scaleDown,
                                  alignment: Alignment.centerLeft,
                                  child: Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      if (quality.isNotEmpty)
                                        badge(
                                          quality,
                                          '--song-list-quality-bg',
                                          '--song-list-quality-color',
                                          fit: false,
                                        ),
                                      if (quality.isNotEmpty &&
                                          source.isNotEmpty)
                                        const SizedBox(width: 4),
                                      if (source.isNotEmpty)
                                        badge(
                                          source,
                                          '--song-list-source-bg',
                                          '--song-list-source-color',
                                          fit: false,
                                        ),
                                    ],
                                  ),
                                ),
                              ),
                            ],
                          ],
                        ),
                      ),
                      if (!widget.durationAtEnd) const SizedBox(width: 12),
                      if (!widget.durationAtEnd)
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 72),
                          child: Text(
                            duration,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12,
                              height: 1,
                              color: p.secondary,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              ?trailing,
              if (widget.durationAtEnd)
                SizedBox(
                  width: 44,
                  child: Text(
                    duration,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textAlign: TextAlign.center,
                    style: TextStyle(fontSize: 12, color: p.secondary),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

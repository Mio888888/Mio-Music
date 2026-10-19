import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

/// Desktop song_view sortable header. The Rust shared Sort owns the cycle;
/// this component only renders its active column, direction and hover hint.
class PlaylistSortHeader extends StatefulWidget {
  final MioPalette palette;
  final api.PlaylistSort sort;
  final api.SortColumn column;
  final bool narrow;
  final VoidCallback? onPressed;
  const PlaylistSortHeader({
    super.key,
    required this.palette,
    required this.sort,
    required this.column,
    required this.narrow,
    this.onPressed,
  });
  @override
  State<PlaylistSortHeader> createState() => _PlaylistSortHeaderState();
}

class _PlaylistSortHeaderState extends State<PlaylistSortHeader> {
  bool _hovered = false;
  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final sort = widget.sort;
    final activeColumn = switch (sort) {
      api.PlaylistSort.default_ => null,
      api.PlaylistSort.titleAsc ||
      api.PlaylistSort.titleDesc ||
      api.PlaylistSort.artistAsc ||
      api.PlaylistSort.artistDesc => api.SortColumn.title,
      api.PlaylistSort.albumAsc ||
      api.PlaylistSort.albumDesc => api.SortColumn.album,
      api.PlaylistSort.durationAsc ||
      api.PlaylistSort.durationDesc => api.SortColumn.duration,
    };
    final active = activeColumn == widget.column;
    final descending =
        active &&
        switch (sort) {
          api.PlaylistSort.titleDesc ||
          api.PlaylistSort.artistDesc ||
          api.PlaylistSort.albumDesc ||
          api.PlaylistSort.durationDesc => true,
          _ => false,
        };
    final title = p.text(switch (widget.column) {
      api.SortColumn.title => 'music.songList.title',
      api.SortColumn.album => 'music.songList.album',
      api.SortColumn.duration => 'music.songList.duration',
    });
    final hint = p.text(
      active
          ? switch (sort) {
              api.PlaylistSort.titleAsc => 'music.songList.titleAsc',
              api.PlaylistSort.titleDesc => 'music.songList.titleDesc',
              api.PlaylistSort.artistAsc => 'music.songList.artistAsc',
              api.PlaylistSort.artistDesc => 'music.songList.artistDesc',
              _ => 'music.songList.defaultSort',
            }
          : 'music.songList.defaultSort',
    );
    final showHint = active || _hovered;
    final hintText =
        widget.column == api.SortColumn.title ||
        (widget.column == api.SortColumn.album && !active);
    final color = p.color(
      active || _hovered
          ? '--song-list-title-hover'
          : '--song-list-header-text',
    );
    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: SizedBox(
        height: 44,
        child: MioButton(
          palette: p,
          label: '$title · $hint',
          onPressed: widget.onPressed,
          padding: const EdgeInsets.symmetric(horizontal: 2),
          child: Row(
            mainAxisAlignment: widget.column == api.SortColumn.duration
                ? MainAxisAlignment.center
                : MainAxisAlignment.start,
            children: [
              if (!(widget.narrow && widget.column == api.SortColumn.duration))
                Flexible(
                  child: Text(
                    title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12, height: 1.5, color: color),
                  ),
                ),
              if (showHint) ...[
                const SizedBox(width: 6),
                MioIcon(
                  !active
                      ? 'TD-Time'
                      : descending
                      ? 'TD-Arrow-Down'
                      : 'TD-Arrow-Up',
                  size: 14,
                  color: color,
                ),
                if (hintText) ...[
                  const SizedBox(width: 4),
                  Flexible(
                    child: Text(
                      hint,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, height: 1.5, color: color),
                    ),
                  ),
                ],
              ],
            ],
          ),
        ),
      ),
    );
  }
}

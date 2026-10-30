part of '../catalog.dart';

/// TV treatment of the saved-playlists page: large cover cards like Discover
/// and pill actions, sharing every handler with the other layouts.
extension _TvCatalog on _CatalogViewState {
  Widget _tvPlaylistGrid() => LayoutBuilder(
    builder: (context, box) {
      const gap = 14.0, columns = 5;
      final width = (box.maxWidth - (columns - 1) * gap) / columns;
      return GridView.builder(
        controller: _scroll,
        itemCount: _items.length + (_more ? 1 : 0),
        // Room for the focus lift and glow around the outer cards.
        padding: const EdgeInsets.fromLTRB(4, 6, 4, 12),
        gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: columns,
          crossAxisSpacing: gap,
          mainAxisSpacing: gap,
          mainAxisExtent: width + 52,
        ),
        itemBuilder: (context, i) => i == _items.length
            ? _loadMore()
            : _tvPlaylistCard(_items[i] as api.PlaylistInfo, width),
      );
    },
  );

  Widget _tvPlaylistCard(api.PlaylistInfo row, double width) {
    final count = '${row.count} ${p.english ? 'tracks' : '首歌曲'}';
    final detail = row.favorite
        ? '${p.text('music.songlist.myFavorite')} · $count'
        : count;
    return MioButton(
      palette: p,
      label: row.favorite
          ? '${row.title}, ${p.text('music.songlist.myFavorite')}'
          : row.title,
      radius: 16,
      insetFocusBorder: false,
      padding: const EdgeInsets.all(6),
      onPressed: () => widget.onOpenPlaylist?.call(row.id),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          MioSavedPlaylistCover(
            id: row.id,
            source: row.cover,
            session: _librarySession,
            size: width - 12,
            radius: 12,
          ),
          const SizedBox(height: 6),
          Text(
            row.title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 15,
              height: 1.3,
              fontWeight: FontWeight.w700,
              color: p.color('--td-text-color-primary'),
            ),
          ),
          Text(
            detail,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 12,
              height: 1.35,
              color: row.favorite ? p.color('--td-error-color') : p.secondary,
            ),
          ),
        ],
      ),
    );
  }

  /// "New playlist" and "Import" as pills sized for a remote.
  Widget _tvPlaylistActions() => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      MioButton(
        palette: p,
        label: p.text('music.songlist.newPlaylist'),
        radius: 999,
        background: p.surface,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 9),
        onPressed: _playlistActionsEnabled ? _newPlaylist : null,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            MioIcon(
              'nav-zengjia',
              size: 16,
              color: p.color('--td-text-color-primary'),
            ),
            const SizedBox(width: 8),
            Text(
              p.text('music.songlist.newPlaylist'),
              maxLines: 1,
              style: const TextStyle(
                fontSize: 15,
                height: 1.2,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      ),
      const SizedBox(width: 10),
      Builder(
        builder: (buttonContext) => MioButton(
          palette: p,
          label: p.text('common.import'),
          radius: 999,
          background: p.brand,
          padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 9),
          onPressed: _playlistActionsEnabled
              ? () {
                  final box = buttonContext.findRenderObject();
                  if (box is RenderBox) {
                    unawaited(
                      _playlistImportMenu(
                        box.localToGlobal(Offset(0, box.size.height)),
                      ),
                    );
                  }
                }
              : null,
          child: Text(
            p.text('common.import'),
            style: TextStyle(
              fontSize: 15,
              height: 1.2,
              fontWeight: FontWeight.w700,
              color: p.color('--td-text-color-anti'),
            ),
          ),
        ),
      ),
    ],
  );

  /// Local library toolbar: play and import as pills, refresh and more as round
  /// icon buttons, the count and the filter box on the right.
  Widget _tvLocalToolbar() {
    final playEnabled = _localActionsEnabled && _total > 0;
    final primary = p.color('--td-text-color-primary');
    Widget round(String icon, String label, VoidCallback? run) => SizedBox(
      width: 44,
      height: 44,
      child: MioButton(
        palette: p,
        label: label,
        radius: 999,
        padding: EdgeInsets.zero,
        background: p.color('--td-bg-color-secondarycontainer'),
        onPressed: run,
        child: Center(child: MioIcon(icon, size: 20, color: primary)),
      ),
    );
    return Padding(
      // Cards and rows sit 10 in from the page edge; keep the toolbar on theirs.
      padding: const EdgeInsets.symmetric(horizontal: 10),
      child: Row(
        children: [
          TvPill(
            palette: p,
            label: p.text('common.playAll'),
            icon: 'PlayFilled',
            filled: true,
            onPressed: playEnabled ? () => unawaited(_playAllLocal()) : null,
          ),
          const SizedBox(width: 10),
          TvPill(
            palette: p,
            label: p.text('music.local.importFiles'),
            icon: 'FolderPlus',
            onPressed: _live && !_importBusy && !_catalogBusy
                ? () => unawaited(_importAudio())
                : null,
          ),
          const SizedBox(width: 10),
          round(
            'Refresh',
            p.text('common.refresh'),
            _live && !_loading && !_importBusy && !_catalogBusy
                ? () => unawaited(_load(reset: true))
                : null,
          ),
          const SizedBox(width: 10),
          Builder(
            builder: (buttonContext) => round(
              'TD-Ellipsis',
              p.english ? 'More actions' : '更多操作',
              _live && !_catalogBusy && !_importBusy
                  ? () {
                      final box = buttonContext.findRenderObject();
                      if (box is RenderBox) {
                        unawaited(
                          _localLibraryMenu(
                            box.localToGlobal(Offset(0, box.size.height)),
                          ),
                        );
                      }
                    }
                  : null,
            ),
          ),
          const Spacer(),
          Text(
            p.english
                ? '$_total ${_total == 1 ? 'song' : 'songs'}'
                : p
                      .text('music.local.totalSongs')
                      .replaceAll('{count}', '$_total'),
            style: TextStyle(fontSize: 14, color: p.secondary),
          ),
          const SizedBox(width: 14),
          SizedBox(
            width: 260,
            child: MioInput(
              palette: p,
              controller: _filter,
              focusNode: _filterFocus,
              placeholder: p.text('music.local.searchPlaceholder'),
              onChanged: _queryChanged,
              readOnly: _downloadBusy,
              height: 44,
            ),
          ),
        ],
      ),
    );
  }
}

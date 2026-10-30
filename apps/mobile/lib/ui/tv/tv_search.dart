part of '../search.dart';

/// Search on TV: one header row (query, view switch, source filters, count)
/// above full-width results, so more than a few rows fit on screen. Sessions,
/// paging and every action are the ones the other layouts use.
extension _TvSearch on SearchViewState {
  Widget _tvBody() {
    final enabled = _live && !_loading && !_modalBusy && !_artistRoute;
    final session = _playlistTab ? _playlistSession : _session;
    final showCount =
        session != null &&
        _error == null &&
        !_loading &&
        ((_playlistTab ? _playlists.isNotEmpty : _items.isNotEmpty) ||
            !_sourceStates.any((s) => s.failed));
    return Padding(
      padding: const EdgeInsets.fromLTRB(10, 6, 10, 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 320),
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: '${p.text('music.search.searchKeyword')}"',
                      ),
                      TextSpan(
                        text: widget.query,
                        style: TextStyle(
                          color: p.color('--search-keyword-color'),
                        ),
                      ),
                      const TextSpan(text: '"'),
                    ],
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 20,
                    height: 1.3,
                    fontWeight: FontWeight.w700,
                    color: p.color('--search-title-color'),
                  ),
                ),
              ),
              const SizedBox(width: 16),
              TvSegmented(
                palette: p,
                labels: [
                  p.text('music.search.tabSongs'),
                  p.text('music.search.tabPlaylists'),
                ],
                selected: _playlistTab ? 1 : 0,
                onSelect: _live && !_modalBusy && !_artistRoute
                    ? (i) => _tab(i == 1)
                    : null,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    children: [
                      _tvSource(
                        null,
                        p.text('music.search.allSources'),
                        enabled,
                      ),
                      for (final source in _sources)
                        Padding(
                          padding: const EdgeInsets.only(left: 8),
                          child: _tvSource(source.id, source.name, enabled),
                        ),
                    ],
                  ),
                ),
              ),
              if (showCount) ...[
                const SizedBox(width: 12),
                Text(
                  _playlistTab
                      ? p
                            .text(
                              _source == null
                                  ? 'music.search.foundAggregatePlaylists'
                                  : 'music.search.foundPlaylists',
                            )
                            .replaceAll('{count}', '$_total')
                      : (p.english ? '$_total songs' : '$_total 首'),
                  maxLines: 1,
                  style: TextStyle(
                    fontSize: 13,
                    color: p.color('--search-info-color'),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 8),
          Expanded(
            child: _playlistTab
                ? _playlistResults(false)
                // Room above and below for the focus lift of the first and
                // last rows.
                : _results(
                    false,
                    const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
                  ),
          ),
        ],
      ),
    );
  }

  Widget _tvSource(String? source, String label, bool enabled) => TvPill(
    key: ValueKey(source ?? ''),
    palette: p,
    label: label,
    selected: _source == source,
    outlined: true,
    onPressed: enabled ? () => _select(source) : null,
  );

  /// Opens the song menu (favorite, batch select) without a long press or
  /// right click, which a remote does not have.
  Widget _tvRowMenu(api.Track song, bool enabled) => Builder(
    builder: (context) => SizedBox(
      width: 48,
      height: 48,
      child: MioButton(
        palette: p,
        focusNode: _tvMenuFocus.putIfAbsent(
          song.id,
          () => FocusNode(debugLabel: 'Song menu ${song.title}'),
        ),
        label: '${p.english ? 'More actions' : '更多操作'}: ${song.title}',
        radius: 999,
        padding: EdgeInsets.zero,
        onPressed: enabled
            ? () {
                final box = context.findRenderObject();
                if (box is RenderBox) {
                  unawaited(
                    _showSongMenu(
                      song,
                      box.localToGlobal(Offset(0, box.size.height)),
                    ),
                  );
                }
              }
            : null,
        child: Center(
          child: Text(
            '···',
            style: TextStyle(color: p.secondary, fontSize: 22, height: 1),
          ),
        ),
      ),
    ),
  );

  /// The grid is rebuilt after the details close, so wait for its card to mount.
  void _restoreTvCardFocus([int attempts = 3]) {
    final id = _tvReturnCard;
    if (id == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _selectedPlaylist != null) return;
      final node = _tvCardFocus[id];
      if (node != null && node.context != null && node.canRequestFocus) {
        node.requestFocus();
      } else if (attempts > 1) {
        _restoreTvCardFocus(attempts - 1);
      }
    });
  }

  Widget _tvPlaylistGrid() => LayoutBuilder(
    builder: (context, box) {
      const gap = 14.0, columns = 5;
      final width = (box.maxWidth - (columns - 1) * gap) / columns;
      final session = _playlistSession;
      return CustomScrollView(
        controller: _scroll,
        slivers: [
          SliverToBoxAdapter(child: _playlistNotices()),
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(4, 6, 4, 12),
            sliver: SliverGrid(
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: columns,
                mainAxisSpacing: gap,
                crossAxisSpacing: gap,
                mainAxisExtent: width + TvCoverCard.textHeight,
              ),
              delegate: SliverChildBuilderDelegate((context, index) {
                final item = _playlists[index], card = item.card;
                final key = '${item.source}/${card.id}';
                return TvCoverCard(
                  key: ValueKey((item.source, card.id)),
                  palette: p,
                  focusNode: _tvCardFocus.putIfAbsent(
                    key,
                    () => FocusNode(debugLabel: 'Playlist ${card.title}'),
                  ),
                  label: '${card.title} · ${card.author} · ${item.source}',
                  title: card.title,
                  detail: [
                    if (card.author.isNotEmpty) card.author,
                    item.source,
                  ].join(' · '),
                  cover: card.cover,
                  width: width,
                  onPressed: !_live || _loading || session == null
                      ? null
                      : () => _openPlaylistResult(item, key),
                );
              }, childCount: _playlists.length),
            ),
          ),
          SliverToBoxAdapter(child: _playlistFooter()),
        ],
      );
    },
  );
}

part of '../singer.dart';

/// Artist and album pages on TV: a compact hero row, pill tabs and large
/// album cards, so the song list gets most of the screen. Sessions, paging and
/// every action are shared with the other layouts.
extension _TvSinger on SingerPageState {
  /// Excluding focus for the confirmation drops it; hand it back on close.
  void _trackTvConfirmation() {
    final closed = _tvConfirmShown && !_confirm;
    _tvConfirmShown = _confirm;
    if (!closed) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && !_confirm && _tvPlayFocus.context != null) {
        _tvPlayFocus.requestFocus();
      }
    });
  }

  Widget _tvIconButton(
    String label,
    VoidCallback? action,
    Widget child, {
    bool autofocus = false,
  }) => SizedBox(
    width: 56,
    height: 56,
    child: MioButton(
      palette: p,
      label: label,
      radius: 28,
      autofocus: autofocus,
      padding: EdgeInsets.zero,
      background: p.color('--td-bg-color-secondarycontainer'),
      onPressed: action,
      child: Center(child: child),
    ),
  );

  Widget _tvHeader() {
    final card = _selected;
    final name = card?.title ?? _profile?.name ?? widget.fallbackName;
    final description = card?.description ?? _profile?.description ?? '';
    final counts = card != null
        ? (p.english
              ? '${_albumTracks.length} loaded · $_detailTotal songs'
              : '已加载 ${_albumTracks.length} 首 · 共 $_detailTotal 首')
        : [
            if (_profile?.songCount != null)
              (p.english
                  ? '${_profile!.songCount} songs'
                  : '${_profile!.songCount} 首歌曲'),
            if (_profile?.albumCount != null)
              (p.english
                  ? '${_profile!.albumCount} albums'
                  : '${_profile!.albumCount} 张专辑'),
          ].join(' · ');
    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(20),
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [
            p.brand.withValues(alpha: .14),
            p.brand.withValues(alpha: .03),
          ],
        ),
      ),
      padding: const EdgeInsets.all(12),
      child: Row(
        children: [
          _tvIconButton(
            p.text('common.back'),
            _back,
            MioIcon('NavigateBefore', size: 26, color: p.secondary),
          ),
          const SizedBox(width: 16),
          MioCover(
            card?.cover ?? _profile?.avatar,
            size: 84,
            radius: card == null ? 42 : 14,
          ),
          const SizedBox(width: 18),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 26,
                    height: 1.2,
                    fontWeight: FontWeight.w800,
                  ),
                ),
                if (counts.isNotEmpty)
                  Text(
                    counts,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 14,
                      height: 1.5,
                      color: p.secondary,
                    ),
                  ),
                if (description.isNotEmpty)
                  Text(
                    description,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      height: 1.5,
                      color: p.color('--td-text-color-placeholder'),
                    ),
                  ),
                if (card == null && _profileError != null)
                  _error(
                    _profileError!,
                    _live && !_profileLoading
                        ? () => unawaited(_profileLoad())
                        : null,
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  /// Songs/albums switch on the left, play and more actions on the right.
  Widget _tvControls() => Padding(
    padding: const EdgeInsets.fromLTRB(0, 10, 0, 6),
    // Fixed height: the row must not jump when the more-actions button hides.
    child: SizedBox(
      height: 56,
      child: Row(
        children: [
          if (!_detail)
            TvSegmented(
              palette: p,
              labels: [
                '${p.text('music.singer.tabSongs')}($_songTotal)',
                '${p.text('music.singer.tabAlbums')}(${_profile?.albumCount ?? _albumTotal})',
              ],
              selected: _tabAlbums ? 1 : 0,
              onSelect: _live && !_actionBusy ? (i) => _showTab(i == 1) : null,
            ),
          const Spacer(),
          if (_contentTracks.isNotEmpty) ...[
            TvPill(
              palette: p,
              label: p.text('music.singer.playAll'),
              icon: 'PlayFilled',
              filled: true,
              autofocus: true,
              focusNode: _tvPlayFocus,
              onPressed: _live && !_actionBusy && !_contentLoading
                  ? () => unawaited(_confirmPlay())
                  : null,
            ),
            if (!_tabAlbums || _detail)
              Padding(
                padding: const EdgeInsets.only(left: 10),
                child: Builder(
                  builder: (context) => _tvIconButton(
                    p.english ? 'More actions' : '更多操作',
                    _live && !_actionBusy
                        ? () => unawaited(_showActions(context))
                        : null,
                    Text(
                      '···',
                      style: TextStyle(color: p.secondary, fontSize: 24),
                    ),
                  ),
                ),
              ),
          ],
        ],
      ),
    ),
  );

  Widget _tvAlbumGrid() => LayoutBuilder(
    builder: (context, box) {
      const gap = 14.0, columns = 5, side = 4.0;
      final width = (box.maxWidth - side * 2 - (columns - 1) * gap) / columns;
      final open =
          _live &&
          !_actionBusy &&
          !_albumLoading &&
          _profile?.albumDetails == true;
      return CustomScrollView(
        controller: _albumScroll,
        slivers: [
          if (_albumError != null && _albums.isEmpty)
            SliverToBoxAdapter(
              child: _error(
                _albumError!,
                _live && !_albumLoading ? () => unawaited(_albumsLoad()) : null,
              ),
            ),
          if (_albums.isEmpty)
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text(
                  _albumLoading
                      ? p.text('common.loading')
                      : (_albumError == null
                            ? p.text('music.singer.noAlbums')
                            : ''),
                  style: const TextStyle(fontSize: 16),
                ),
              ),
            ),
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(side, 6, side, 12),
            sliver: SliverGrid.builder(
              itemCount: _albums.length,
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: columns,
                mainAxisSpacing: gap,
                crossAxisSpacing: gap,
                mainAxisExtent: width + TvCoverCard.textHeight,
              ),
              itemBuilder: (context, i) {
                final card = _albums[i];
                return TvCoverCard(
                  palette: p,
                  label: card.title,
                  title: card.title,
                  detail: (card.songCount ?? 0) > 0
                      ? (p.english
                            ? '${card.songCount} songs'
                            : '${card.songCount} 首歌曲')
                      : '',
                  cover: card.cover,
                  width: width,
                  onPressed: open ? () => unawaited(_openAlbum(card)) : null,
                );
              },
            ),
          ),
          if (_albumError != null && _albums.isNotEmpty)
            SliverToBoxAdapter(
              child: _error(
                _albumError!,
                _live && !_albumLoading ? () => unawaited(_albumsLoad()) : null,
              ),
            ),
          if (_albumMore && _albumError == null)
            SliverToBoxAdapter(
              child: Align(
                alignment: Alignment.center,
                child: TvPill(
                  palette: p,
                  label: _albumLoading
                      ? p.text('common.loading')
                      : (p.english ? 'Load more' : '加载更多'),
                  outlined: true,
                  onPressed: _live && !_albumLoading
                      ? () => unawaited(_albumsLoad())
                      : null,
                ),
              ),
            ),
        ],
      );
    },
  );
}

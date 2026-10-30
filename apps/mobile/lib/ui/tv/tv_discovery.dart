part of '../discovery.dart';

/// Console-style Discover browsing for TV: pill tabs and categories above a
/// grid of large cover cards. Paging, source and category state are shared
/// with the other layouts.
extension _TvDiscovery on DiscoveryViewState {
  Widget _browseTv() {
    final hot = _categories?.hot ?? <api.DiscoveryCategory>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // One header row; the card covers start 10 in, so it does too.
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: Row(
            key: _categoryRowKey,
            children: [
              _tvTabs(),
              const SizedBox(width: 14),
              Expanded(
                child: _charts
                    ? const SizedBox.shrink()
                    : SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        child: Row(
                          children: [
                            _tvPill(
                              p.text('music.playlistCategory.hot'),
                              _live ? () => _selectCategory('', '') : null,
                              selected: _category.isEmpty,
                            ),
                            for (final category in hot)
                              Padding(
                                padding: const EdgeInsets.only(left: 8),
                                child: _tvPill(
                                  category.name,
                                  _live
                                      ? () => _selectCategory(
                                          category.id,
                                          category.name,
                                        )
                                      : null,
                                  selected: category.id == _category,
                                ),
                              ),
                          ],
                        ),
                      ),
              ),
              if (!_charts && _categories?.groups.isNotEmpty == true) ...[
                const SizedBox(width: 8),
                _tvPill(
                  '${p.text('music.playlistCategory.moreCategory')} ▾',
                  _live ? _openCategories : null,
                  outlined: true,
                  focusNode: _tvMoreFocus,
                ),
              ],
              for (final source in _sources)
                Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: _tvPill(
                    source.name,
                    _live && !_selectingSource
                        ? () => unawaited(_selectSource(source.id))
                        : null,
                    // A lone source is a label, not a choice to highlight.
                    selected: source.id == _source && _sources.length > 1,
                    outlined: true,
                  ),
                ),
            ],
          ),
        ),
        if (_categoryError != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 8, 10, 0),
            child: Text(_categoryError!, style: TextStyle(color: p.secondary)),
          ),
        const SizedBox(height: 8),
        if (_error != null) _errorBar(),
        Expanded(
          child: _loading && _cards.isEmpty
              ? _skeletons()
              : _cards.isEmpty
              ? Center(
                  child: Text(
                    p.text(
                      _charts
                          ? 'music.leaderboard.noData'
                          : 'music.playlistCategory.noPlaylists',
                    ),
                    style: const TextStyle(fontSize: 16),
                  ),
                )
              : _tvGrid(),
        ),
        if (_loading && _cards.isNotEmpty)
          Text(p.english ? 'Loading…' : '加载中…'),
      ],
    );
  }

  /// The grid is rebuilt after the details close, so wait for its card to mount.
  void _restoreTvCardFocus([int attempts = 3]) {
    final id = _tvReturnCard;
    if (id == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _selected != null) return;
      final node = _tvCardFocus[id];
      if (node != null && node.context != null && node.canRequestFocus) {
        node.requestFocus();
      } else if (attempts > 1) {
        _restoreTvCardFocus(attempts - 1);
      }
    });
  }

  Widget _tvTabs() => DecoratedBox(
    decoration: BoxDecoration(
      color: p.color('--td-bg-color-secondarycontainer'),
      borderRadius: BorderRadius.circular(999),
    ),
    child: Padding(
      padding: const EdgeInsets.all(3),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [_tvTab(false), const SizedBox(width: 2), _tvTab(true)],
      ),
    ),
  );

  Widget _tvTab(bool charts) {
    final selected = _charts == charts;
    final label = p.text(
      charts ? 'music.find.tabLeaderboard' : 'music.find.tabPlaylist',
    );
    return Semantics(
      selected: selected,
      child: MioButton(
        palette: p,
        label: label,
        radius: 999,
        insetFocusBorder: false,
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
        background: selected ? p.brand : const Color(0x00000000),
        onPressed: _live ? () => _switchBrowseTab(charts) : null,
        child: Text(
          label,
          style: TextStyle(
            fontSize: 15,
            height: 1.2,
            fontWeight: FontWeight.w700,
            color: selected ? p.color('--td-text-color-anti') : p.secondary,
          ),
        ),
      ),
    );
  }

  Widget _tvPill(
    String label,
    VoidCallback? action, {
    bool selected = false,
    bool outlined = false,
    bool compact = false,
    FocusNode? focusNode,
  }) => Semantics(
    selected: selected,
    child: MioButton(
      palette: p,
      focusNode: focusNode,
      label: label,
      radius: 999,
      insetFocusBorder: false,
      padding: EdgeInsets.symmetric(horizontal: compact ? 14 : 16, vertical: 7),
      background: selected
          ? p.brand
          : outlined
          ? p.color('--td-bg-color-secondarycontainer')
          : p.surface,
      onPressed: action,
      child: Text(
        label,
        style: TextStyle(
          fontSize: compact ? 13 : 14,
          height: 1.2,
          fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
          color: selected ? p.color('--td-text-color-anti') : p.secondary,
        ),
      ),
    ),
  );

  Widget _tvGrid() => LayoutBuilder(
    builder: (context, box) {
      const gap = 14.0, columns = 5;
      final width = (box.maxWidth - (columns - 1) * gap) / columns;
      final count = _cards.length + (_more ? 1 : 0);
      return GridView.builder(
        controller: _scroll,
        itemCount: count,
        // Room for the focus lift and glow around the outer cards.
        padding: const EdgeInsets.fromLTRB(4, 6, 4, 12),
        gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: columns,
          mainAxisSpacing: gap,
          crossAxisSpacing: gap,
          mainAxisExtent: width + 52,
        ),
        itemBuilder: (context, index) {
          if (index == _cards.length) {
            return Align(
              alignment: Alignment.topCenter,
              child: _tvPill(
                p.english ? 'Load more' : '加载更多',
                _live && !_loading ? () => unawaited(_next()) : null,
                outlined: true,
              ),
            );
          }
          return _tvCard(_cards[index], width);
        },
      );
    },
  );

  Widget _tvCard(api.DiscoveryCard card, double width) {
    final songs = card.songCount ?? 0;
    final detail = [
      if (card.author.isNotEmpty) card.author,
      if (songs > 0) (p.english ? '$songs songs' : '$songs 首'),
    ].join(' · ');
    return MioButton(
      palette: p,
      focusNode: _tvCardFocus.putIfAbsent(
        card.id,
        () => FocusNode(debugLabel: 'Card ${card.title}'),
      ),
      label: card.title,
      radius: 16,
      insetFocusBorder: false,
      padding: const EdgeInsets.all(6),
      onPressed: _live && !_loading
          ? () {
              _browseMore = _more;
              unawaited(_openCard(card));
            }
          : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          MioCover(card.cover, size: width - 12, radius: 12),
          const SizedBox(height: 6),
          Text(
            card.title,
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

  /// Category picker as a centred console panel: groups on top, the chosen
  /// group's categories below, focus starting on the current category.
  Widget _tvCategorySheet() {
    final groups = _categories!.groups;
    final group = groups[_group.clamp(0, groups.length - 1)];
    final close = _closeCategories;
    var focusPlaced = false;
    return Positioned.fill(
      child: BlockSemantics(
        child: CallbackShortcuts(
          bindings: {const SingleActivator(LogicalKeyboardKey.escape): close},
          child: FocusScope(
            autofocus: true,
            child: Stack(
              children: [
                Positioned.fill(
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onTap: close,
                    child: const ColoredBox(color: Color(0xb3000000)),
                  ),
                ),
                Center(
                  child: Container(
                    width: 760,
                    constraints: const BoxConstraints(maxHeight: 420),
                    margin: const EdgeInsets.symmetric(
                      horizontal: 48,
                      vertical: 24,
                    ),
                    padding: const EdgeInsets.fromLTRB(24, 20, 24, 20),
                    decoration: BoxDecoration(
                      // Opaque, so the cards behind never show through.
                      color: Color.alphaBlend(
                        p.color('--td-bg-color-container'),
                        p.color('--td-bg-color-page'),
                      ),
                      border: Border.all(
                        color: p.color('--td-border-level-1-color'),
                      ),
                      borderRadius: BorderRadius.circular(24),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                p.english ? 'Categories' : '歌单分类',
                                style: const TextStyle(
                                  fontSize: 22,
                                  height: 1.3,
                                  fontWeight: FontWeight.w800,
                                ),
                              ),
                            ),
                            TvPill(
                              palette: p,
                              label: p.text('common.close'),
                              outlined: true,
                              onPressed: close,
                            ),
                          ],
                        ),
                        const SizedBox(height: 14),
                        SingleChildScrollView(
                          scrollDirection: Axis.horizontal,
                          // Room for the focus glow above and below the pills.
                          padding: const EdgeInsets.symmetric(vertical: 4),
                          child: Row(
                            children: [
                              for (var i = 0; i < groups.length; i++)
                                Padding(
                                  padding: const EdgeInsets.only(right: 8),
                                  child: TvPill(
                                    palette: p,
                                    label: groups[i].name,
                                    selected: i == _group,
                                    outlined: true,
                                    onPressed: () => _showCategoryGroup(i),
                                  ),
                                ),
                            ],
                          ),
                        ),
                        const SizedBox(height: 10),
                        Expanded(
                          child: SingleChildScrollView(
                            padding: const EdgeInsets.all(4),
                            child: Wrap(
                              spacing: 10,
                              runSpacing: 10,
                              children: [
                                for (final item in group.items)
                                  Builder(
                                    builder: (_) {
                                      // The current category, else the first.
                                      final place =
                                          !focusPlaced &&
                                          (item.id == _category ||
                                              !group.items.any(
                                                (i) => i.id == _category,
                                              ));
                                      if (place) focusPlaced = true;
                                      return TvPill(
                                        palette: p,
                                        label: item.name,
                                        selected: item.id == _category,
                                        autofocus: place,
                                        onPressed: () =>
                                            _selectCategory(item.id, item.name),
                                      );
                                    },
                                  ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

import 'settings_layout.dart';
import 'device.dart';

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart' show TextInputAction;

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

class StorageSettingsView extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool active, ready;
  final double viewportWidth;
  final bool embedded;
  final VoidCallback? onLocal, onBackup;
  const StorageSettingsView({
    super.key,
    required this.core,
    required this.palette,
    required this.active,
    required this.ready,
    required this.viewportWidth,
    this.embedded = false,
    this.onLocal,
    this.onBackup,
  });
  @override
  State<StorageSettingsView> createState() => _StorageSettingsViewState();
}

class _StorageSettingsViewState extends State<StorageSettingsView>
    with WidgetsBindingObserver {
  final _template = TextEditingController();
  final _templateFocus = FocusNode();
  api.StorageSettingsSession? _session;
  api.StorageSettings? _value;
  bool _loading = false, _busy = false, _foreground = true;
  String? _error;
  String _preview = '';
  int _epoch = 0;
  MioPalette get p => widget.palette;
  bool get _live => mounted && widget.active && widget.ready && _foreground;

  @override
  void initState() {
    super.initState();
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    WidgetsBinding.instance.addObserver(this);
    if (_live) unawaited(_open());
  }

  void _release() {
    final session = _session;
    _session = null;
    _value = null;
    session?.close();
    session?.dispose();
  }

  void _invalidate() {
    _epoch++;
    _release();
    _busy = false;
    _loading = false;
  }

  @override
  void didUpdateWidget(StorageSettingsView old) {
    super.didUpdateWidget(old);
    if (old.palette.english != widget.palette.english && _value != null) {
      _updatePreview();
    }
    if ((!widget.active && old.active) || (!widget.ready && old.ready)) {
      _invalidate();
    }
    if (_live &&
        ((!old.active && widget.active) || (!old.ready && widget.ready))) {
      unawaited(_open());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _foreground = true;
      if (_live) unawaited(_open());
    } else {
      _foreground = false;
      _invalidate();
    }
  }

  Future<void> _open() async {
    if (!_live) return;
    final epoch = ++_epoch;
    _release();
    setState(() {
      _loading = true;
      _busy = false;
      _error = null;
    });
    api.StorageSettingsSession? candidate;
    try {
      candidate = await api.beginStorageSettings();
      final value = await candidate.state();
      if (!_live || epoch != _epoch) return;
      _session = candidate;
      candidate = null;
      setState(() {
        _value = value;
        _template.text = value.filenameTemplate;
        _updatePreview();
        _loading = false;
      });
    } catch (_) {
      if (_live && epoch == _epoch) {
        setState(() {
          _loading = false;
          _error = p.english ? 'Unable to load storage settings.' : '无法读取存储设置。';
        });
      }
    } finally {
      candidate?.close();
      candidate?.dispose();
    }
  }

  Future<void> _save(api.StorageSettings next) async {
    final session = _session;
    final expected = _value;
    if (!_live || _busy || _loading || session == null || expected == null) {
      return;
    }
    final epoch = _epoch;
    setState(() => _busy = true);
    api.StorageSettings? saved;
    await widget.core.command(() async {
      saved = await session.save(expected: expected, next: next);
    });
    if (!_live || epoch != _epoch || !identical(session, _session)) return;
    setState(() {
      _busy = false;
      if (saved != null) {
        _value = saved;
        if (next.filenameTemplate != expected.filenameTemplate) {
          _template.text = saved!.filenameTemplate;
        }
        _updatePreview();
      } else {
        _release();
        _error = p.english
            ? 'Save failed or settings changed. Reload before trying again.'
            : '保存失败或设置已变更，请重新加载后重试。';
      }
    });
  }

  @override
  void dispose() {
    _invalidate();
    _template.dispose();
    _templateFocus.dispose();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  api.StorageSettings _next({
    bool? autoCache,
    int? cacheLimit,
    String? template,
    bool? basicInfo,
    bool? cover,
    bool? lyrics,
    bool? downloadLyrics,
  }) => api.StorageSettings(
    autoCache: autoCache ?? _value!.autoCache,
    cacheLimit: cacheLimit ?? _value!.cacheLimit,
    filenameTemplate: template ?? _value!.filenameTemplate,
    basicInfo: basicInfo ?? _value!.basicInfo,
    cover: cover ?? _value!.cover,
    lyrics: lyrics ?? _value!.lyrics,
    downloadLyrics: downloadLyrics ?? _value!.downloadLyrics,
  );

  void _updatePreview() {
    try {
      _preview = api.previewDownloadFilename(
        template: _template.text,
        english: p.english,
      );
    } catch (_) {
      _preview = p.english
          ? 'Use up to 256 characters without control characters.'
          : '请使用最多256个字符，且不能包含控制字符。';
    }
  }

  String t(String name) => p.text('settings.storage.$name');
  bool get _compact =>
      widget.embedded && !MioDeviceScope.wide(context, widget.viewportWidth);
  bool get _wideEmbedded =>
      widget.embedded && MioDeviceScope.wide(context, widget.viewportWidth);
  bool get _enabled => _live && !_busy && !_loading && _value != null;
  Widget _toggle(String key, String desc, bool checked, VoidCallback action) =>
      _compact
      ? Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t(key), style: const TextStyle(fontSize: 14)),
                    const SizedBox(height: 4),
                    Text(
                      t(desc),
                      style: TextStyle(
                        fontSize: 12,
                        height: 1.5,
                        color: p.secondary,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              MioSettingsSwitch(
                palette: p,
                label: t(key),
                value: checked,
                onPressed: _enabled ? action : null,
              ),
            ],
          ),
        )
      : widget.embedded
      ? MioSettingsField(
          palette: p,
          title: t(key),
          description: t(desc),
          child: MioSettingsSwitch(
            palette: p,
            label: t(key),
            value: checked,
            onPressed: _enabled ? action : null,
          ),
        )
      : Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t(key), style: const TextStyle(fontSize: 14)),
                    const SizedBox(height: 4),
                    Text(
                      t(desc),
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              Semantics(
                toggled: checked,
                child: SizedBox(
                  width: 60,
                  height: 44,
                  child: MioButton(
                    palette: p,
                    label: t(key),
                    background: checked ? p.brand.withValues(alpha: .14) : null,
                    onPressed: _enabled ? action : null,
                    child: Text(
                      checked
                          ? (p.english ? 'On' : '开')
                          : (p.english ? 'Off' : '关'),
                    ),
                  ),
                ),
              ),
            ],
          ),
        );

  Widget _section(String title, List<Widget> children) =>
      _compact || _wideEmbedded
      ? MioSettingsSection(
          palette: p,
          title: title,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: children,
          ),
        )
      : widget.embedded
      ? MioSettingsField(
          palette: p,
          title: title,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: children,
          ),
        )
      : Padding(
          padding: const EdgeInsets.only(bottom: 24),
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.all(16),
            decoration: BoxDecoration(
              color: p.surface,
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: p.secondary.withValues(alpha: .12)),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 12),
                ...children,
              ],
            ),
          ),
        );

  Widget _compactButton(
    String label,
    VoidCallback? action, {
    bool primary = false,
  }) => MioButton(
    palette: p,
    label: label,
    radius: 6,
    background: primary ? p.brand : p.color('--td-bg-color-specialcomponent'),
    onPressed: _enabled ? action : null,
    child: Text(
      label,
      style: TextStyle(
        fontSize: 13,
        color: primary ? p.color('--td-text-color-anti') : null,
      ),
    ),
  );

  Widget _tagCheck(
    String key,
    String description,
    bool checked,
    VoidCallback action,
  ) => Semantics(
    checked: checked,
    child: MioButton(
      palette: p,
      label: t(key),
      onPressed: _enabled ? action : null,
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 18,
            height: 18,
            margin: const EdgeInsets.only(top: 2),
            decoration: BoxDecoration(
              color: checked
                  ? p.brand
                  : p.color('--td-bg-color-specialcomponent'),
              borderRadius: BorderRadius.circular(4),
            ),
            child: checked
                ? Center(
                    child: Text(
                      '✓',
                      style: TextStyle(
                        fontSize: 12,
                        color: p.color('--td-text-color-anti'),
                      ),
                    ),
                  )
                : null,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  t(key),
                  style: TextStyle(
                    fontSize: 14,
                    fontWeight: _wideEmbedded
                        ? FontWeight.w400
                        : FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  t(description),
                  style: TextStyle(
                    fontSize: 12,
                    height: 1.5,
                    color: _wideEmbedded
                        ? p.color('--td-text-color-placeholder')
                        : p.secondary,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    ),
  );

  /// Desktop `.cache-limit` segmented control.
  Widget _limitSegments(api.StorageSettings v, List<(String, int)> limits) =>
      Container(
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(
          color: p.color('--td-bg-color-secondarycontainer'),
          border: Border.all(color: p.color('--td-component-border')),
          borderRadius: BorderRadius.circular(6),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final (label, bytes) in limits)
              Semantics(
                selected: v.cacheLimit == bytes,
                child: MioButton(
                  palette: p,
                  label: label,
                  radius: 0,
                  padding: EdgeInsets.zero,
                  background: const Color(0x00000000),
                  onPressed: _enabled
                      ? () => unawaited(_save(_next(cacheLimit: bytes)))
                      : null,
                  child: Container(
                    height: 32,
                    padding: const EdgeInsets.symmetric(horizontal: 14),
                    alignment: Alignment.center,
                    decoration: v.cacheLimit == bytes
                        ? BoxDecoration(
                            color: p.color('--td-bg-color-component-active'),
                            border: Border.all(color: p.brand),
                          )
                        : null,
                    child: Text(
                      label,
                      style: TextStyle(
                        fontSize: 14,
                        color: v.cacheLimit == bytes
                            ? p.color('--td-text-color-primary')
                            : p.secondary,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      );

  /// Template token with its meaning, as a tag and a label.
  Widget _token(String code, String label) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        decoration: BoxDecoration(
          color: p.color('--td-bg-color-secondarycontainer'),
          border: Border.all(color: p.color('--td-border-level-1-color')),
          borderRadius: BorderRadius.circular(3),
        ),
        child: Text('%$code', style: const TextStyle(fontSize: 12)),
      ),
      const SizedBox(width: 6),
      Text(label, style: const TextStyle(fontSize: 13)),
    ],
  );

  Widget _previewBox() => Container(
    width: double.infinity,
    margin: const EdgeInsets.only(top: 12),
    padding: const EdgeInsets.all(12),
    decoration: BoxDecoration(
      color: p.color('--settings-preview-bg'),
      border: Border.all(color: p.color('--settings-preview-border')),
      borderRadius: BorderRadius.circular(8),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(t('preview'), style: const TextStyle(fontSize: 13)),
        Text(_preview, style: const TextStyle(fontSize: 13)),
      ],
    ),
  );

  Widget _compactBody(
    api.StorageSettings v,
    List<(String, int)> limits,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      _section(t('cacheStrategy'), [
        MioSettingsField(
          palette: p,
          title: t('autoCacheMusic'),
          description: t('autoCacheMusicDesc'),
          child: MioSettingsSwitch(
            palette: p,
            label: t('autoCacheMusic'),
            value: v.autoCache,
            onPressed: _enabled
                ? () => unawaited(_save(_next(autoCache: !v.autoCache)))
                : null,
          ),
        ),
        MioSettingsField(
          palette: p,
          title: t('cacheSizeLimit'),
          description: t('cacheSizeLimitDesc'),
          child: _wideEmbedded
              ? _limitSegments(v, limits)
              : LayoutBuilder(
                  builder: (context, box) {
                    final width = (box.maxWidth / 4).clamp(64.0, 110.0);
                    return SingleChildScrollView(
                      scrollDirection: Axis.horizontal,
                      child: Container(
                        clipBehavior: Clip.antiAlias,
                        decoration: BoxDecoration(
                          border: Border.all(color: p.border),
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            for (final (label, bytes) in limits)
                              SizedBox(
                                width: width,
                                child: Semantics(
                                  selected: v.cacheLimit == bytes,
                                  child: MioButton(
                                    palette: p,
                                    label: label,
                                    radius: 0,
                                    background: v.cacheLimit == bytes
                                        ? p.brand.withValues(alpha: .7)
                                        : const Color(0x00000000),
                                    onPressed: _enabled
                                        ? () => unawaited(
                                            _save(_next(cacheLimit: bytes)),
                                          )
                                        : null,
                                    child: Text(
                                      label,
                                      textAlign: TextAlign.center,
                                      style: const TextStyle(fontSize: 13),
                                    ),
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    );
                  },
                ),
        ),
        if (!limits.any((limit) => limit.$2 == v.cacheLimit))
          Text(
            p.english
                ? 'Current limit: ${v.cacheLimit} bytes (0 = unlimited)'
                : '当前上限：${v.cacheLimit} 字节（0 表示不限）',
          ),
      ]),
      if (_wideEmbedded)
        _section(t('filenameFormat'), [
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Text(
              t('filenameFormatDesc'),
              style: TextStyle(fontSize: 13, color: p.secondary),
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Wrap(
              spacing: 12,
              runSpacing: 8,
              children: [
                for (final (code, key) in [
                  ('t', 'templateSongName'),
                  ('s', 'templateSinger'),
                  ('a', 'templateAlbum'),
                  ('u', 'templatePlatform'),
                  ('q', 'templateQuality'),
                  ('d', 'templateDate'),
                ])
                  _token(code, t(key)),
              ],
            ),
          ),
          MioInput(
            palette: p,
            controller: _template,
            focusNode: _templateFocus,
            placeholder: t('filenamePlaceholder'),
            height: 32,
            radius: 3,
            background: p.color('--td-bg-color-specialcomponent'),
            maxLength: 256,
            readOnly: !_enabled,
            textInputAction: TextInputAction.done,
            onChanged: (_) => setState(_updatePreview),
            onSubmitted: _enabled
                ? (_) => unawaited(_save(_next(template: _template.text)))
                : null,
          ),
          _previewBox(),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              MioSettingsButton(
                palette: p,
                label: p.text('common.save'),
                primary: true,
                onPressed: _enabled
                    ? () => unawaited(_save(_next(template: _template.text)))
                    : null,
              ),
              MioSettingsButton(
                palette: p,
                label: t('resetToDefault'),
                onPressed: _enabled
                    ? () {
                        setState(() {
                          _template.text = '%t - %s';
                          _updatePreview();
                        });
                        unawaited(_save(_next(template: '%t - %s')));
                      }
                    : null,
              ),
            ],
          ),
        ])
      else
        _section(t('filenameFormat'), [
          Text(
            t('filenameFormatDesc'),
            style: TextStyle(fontSize: 13, height: 1.5, color: p.secondary),
          ),
          const SizedBox(height: 14),
          MioInput(
            palette: p,
            controller: _template,
            focusNode: _templateFocus,
            placeholder: t('filenamePlaceholder'),
            height: 36,
            radius: 4,
            background: p.color('--td-bg-color-specialcomponent'),
            maxLength: 256,
            readOnly: !_enabled,
            textInputAction: TextInputAction.done,
            onChanged: (_) => setState(_updatePreview),
            onSubmitted: _enabled
                ? (_) => unawaited(_save(_next(template: _template.text)))
                : null,
          ),
          const SizedBox(height: 12),
          MioSettingsField(
            palette: p,
            title: t('preview'),
            child: Text(
              _preview,
              style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w600),
            ),
          ),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _compactButton(
                p.text('common.save'),
                () => unawaited(_save(_next(template: _template.text))),
                primary: true,
              ),
              _compactButton(t('resetToDefault'), () {
                setState(() {
                  _template.text = '%t - %s';
                  _updatePreview();
                });
                unawaited(_save(_next(template: '%t - %s')));
              }),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 12,
            runSpacing: 4,
            children: [
              for (final (code, key) in [
                ('t', 'templateSongName'),
                ('s', 'templateSinger'),
                ('a', 'templateAlbum'),
                ('u', 'templatePlatform'),
                ('q', 'templateQuality'),
                ('d', 'templateDate'),
              ])
                Text(
                  '%$code ${t(key)}',
                  style: TextStyle(fontSize: 11, color: p.secondary),
                ),
            ],
          ),
        ]),
      _section(t('tagWriteSettings'), [
        Text(
          t('tagWriteDesc'),
          style: TextStyle(fontSize: 13, color: p.secondary),
        ),
        const SizedBox(height: 14),
        _tagCheck(
          'tagBasicInfo',
          'tagBasicInfoDesc',
          v.basicInfo,
          () => unawaited(_save(_next(basicInfo: !v.basicInfo))),
        ),
        _tagCheck(
          'tagCover',
          'tagCoverDesc',
          v.cover,
          () => unawaited(_save(_next(cover: !v.cover))),
        ),
        _tagCheck(
          'tagLyricsInfo',
          'tagLyricsInfoDesc',
          v.lyrics,
          () => unawaited(_save(_next(lyrics: !v.lyrics))),
        ),
        _tagCheck(
          'tagDownloadLyricsFile',
          'tagDownloadLyricsFileDesc',
          v.downloadLyrics,
          () => unawaited(_save(_next(downloadLyrics: !v.downloadLyrics))),
        ),
        const SizedBox(height: 12),
        Text(
          '${t('tagStandardLrc')} / ${t('tagWordByWord')}',
          style: const TextStyle(fontSize: 14),
        ),
        const SizedBox(height: 6),
        Text(
          p.english
              ? 'Lyrics format follows the current download pipeline; choosing a format is not supported on mobile yet.'
              : '歌词格式由当前下载流程处理，移动端暂不支持手动切换。',
          style: TextStyle(fontSize: 12, height: 1.5, color: p.secondary),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 16),
          child: Container(height: 1, color: p.border),
        ),
        Text(
          '${t('tagCurrentConfig')}${[if (v.basicInfo) t('tagBasicInfo'), if (v.cover) t('tagCover'), if (v.lyrics) t('tagLyricsInfo'), if (v.downloadLyrics) t('tagDownloadLyricsFile')].join(p.english ? ', ' : '、')}',
          style: TextStyle(fontSize: 13, height: 1.5, color: p.brand),
        ),
      ]),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final v = _value;
    final wide = MioDeviceScope.wide(context, widget.viewportWidth);
    final gutter = wide ? 24.0 : (widget.viewportWidth * .04).clamp(16.0, 24.0);
    final limits = <(String, int)>[
      ('500 MB', 500 * 1024 * 1024),
      ('1 GB', 1024 * 1024 * 1024),
      ('2 GB', 2 * 1024 * 1024 * 1024),
      ('5 GB', 5 * 1024 * 1024 * 1024),
    ];
    if (_compact || _wideEmbedded) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_loading) Text(p.english ? 'Loading…' : '加载中…'),
          if (_error != null) ...[
            Text(_error!, style: TextStyle(color: p.color('--td-error-color'))),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          if (v != null) _compactBody(v, limits),
        ],
      );
    }
    return MioSettingsBody(
      embedded: widget.embedded,
      padding: EdgeInsets.all(gutter),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!widget.embedded) ...[
            Text(
              t('title'),
              style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 24),
          ],
          if (_loading || _busy)
            Semantics(
              liveRegion: true,
              child: Text(p.english ? 'Working…' : '正在处理…'),
            ),
          if (_error != null) ...[
            Semantics(
              liveRegion: true,
              child: Text(
                _error!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
            ),
            MioButton(
              palette: p,
              label: p.text('common.refresh'),
              onPressed: _live && !_loading ? () => unawaited(_open()) : null,
              child: Text(p.text('common.refresh')),
            ),
          ],
          if (v != null) ...[
            _section(t('cacheStrategy'), [
              _toggle(
                'autoCacheMusic',
                'autoCacheMusicDesc',
                v.autoCache,
                () => unawaited(_save(_next(autoCache: !v.autoCache))),
              ),
              const SizedBox(height: 12),
              Text(t('cacheSizeLimit')),
              Text(
                t('cacheSizeLimitDesc'),
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (final (label, bytes) in limits)
                    Semantics(
                      selected: v.cacheLimit == bytes,
                      child: MioButton(
                        palette: p,
                        label: label,
                        background: v.cacheLimit == bytes
                            ? p.brand.withValues(alpha: .14)
                            : null,
                        onPressed: _enabled
                            ? () => unawaited(_save(_next(cacheLimit: bytes)))
                            : null,
                        child: Text(label),
                      ),
                    ),
                ],
              ),
              if (!limits.any((limit) => limit.$2 == v.cacheLimit))
                Text(
                  p.english
                      ? 'Current limit: ${v.cacheLimit} bytes (0 = unlimited)'
                      : '当前上限：${v.cacheLimit} 字节（0表示不限）',
                ),
            ]),
            _section(t('filenameFormat'), [
              Text(
                t('filenameFormatDesc'),
                style: TextStyle(color: p.secondary, fontSize: 12),
              ),
              const SizedBox(height: 12),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 560),
                child: MioInput(
                  palette: p,
                  controller: _template,
                  focusNode: _templateFocus,
                  placeholder: t('filenamePlaceholder'),
                  height: 44,
                  maxLength: 256,
                  readOnly: !_enabled,
                  textInputAction: TextInputAction.done,
                  onChanged: (_) => setState(_updatePreview),
                  onSubmitted: _enabled
                      ? (_) => unawaited(_save(_next(template: _template.text)))
                      : null,
                ),
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (final entry in [
                    (
                      label: p.text('common.save'),
                      run: _enabled
                          ? () => unawaited(
                              _save(_next(template: _template.text)),
                            )
                          : null,
                    ),
                    (
                      label: t('resetToDefault'),
                      run: _enabled
                          ? () {
                              setState(() {
                                _template.text = '%t - %s';
                                _updatePreview();
                              });
                              unawaited(_save(_next(template: '%t - %s')));
                            }
                          : null,
                    ),
                  ])
                    MioDeviceScope.wide(context, widget.viewportWidth)
                        ? MioSettingsButton(
                            palette: p,
                            label: entry.label,
                            onPressed: entry.run,
                          )
                        : MioButton(
                            palette: p,
                            label: entry.label,
                            onPressed: entry.run,
                            child: Text(entry.label),
                          ),
                ],
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 12,
                runSpacing: 4,
                children: [
                  for (final (code, key) in [
                    ('t', 'templateSongName'),
                    ('s', 'templateSinger'),
                    ('a', 'templateAlbum'),
                    ('u', 'templatePlatform'),
                    ('q', 'templateQuality'),
                    ('d', 'templateDate'),
                  ])
                    Text(
                      '%$code ${t(key)}',
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                '${t('preview')}$_preview',
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
            ]),
            _section(t('tagWriteSettings'), [
              Text(
                t('tagWriteDesc'),
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
              _toggle(
                'tagBasicInfo',
                'tagBasicInfoDesc',
                v.basicInfo,
                () => unawaited(_save(_next(basicInfo: !v.basicInfo))),
              ),
              _toggle(
                'tagCover',
                'tagCoverDesc',
                v.cover,
                () => unawaited(_save(_next(cover: !v.cover))),
              ),
              _toggle(
                'tagLyricsInfo',
                'tagLyricsInfoDesc',
                v.lyrics,
                () => unawaited(_save(_next(lyrics: !v.lyrics))),
              ),
              _toggle(
                'tagDownloadLyricsFile',
                'tagDownloadLyricsFileDesc',
                v.downloadLyrics,
                () =>
                    unawaited(_save(_next(downloadLyrics: !v.downloadLyrics))),
              ),
            ]),
          ],
        ],
      ),
    );
  }
}

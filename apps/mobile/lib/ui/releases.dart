import 'settings_layout.dart';

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../release_controller.dart';
import 'controls.dart';
import 'resources.dart';

class ReleaseNotice extends StatelessWidget {
  final ReleaseController updates;
  final MioPalette palette;
  final VoidCallback onDetails;
  const ReleaseNotice({
    super.key,
    required this.updates,
    required this.palette,
    required this.onDetails,
  });
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: updates,
    builder: (context, _) {
      if (!updates.ready || !updates.hasNotice) return const SizedBox.shrink();
      final p = palette;
      return Container(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        decoration: BoxDecoration(
          color: p.surface,
          border: Border(bottom: BorderSide(color: p.border)),
        ),
        child: Wrap(
          spacing: 12,
          runSpacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Semantics(
              liveRegion: true,
              child: Text(
                '${p.text('settings.about.newVersionFound')} · ${updates.release!.version}',
              ),
            ),
            MioButton(
              palette: p,
              label: p.text('settings.about.releaseNotes'),
              onPressed: onDetails,
              child: Text(p.text('settings.about.releaseNotes')),
            ),
            MioButton(
              palette: p,
              label: p.text('settings.about.remindLater'),
              onPressed: updates.dismissNotice,
              child: Text(p.text('settings.about.remindLater')),
            ),
          ],
        ),
      );
    },
  );
}

class ReleaseSettingsView extends StatefulWidget {
  final ReleaseController updates;
  final MioPalette palette;
  final bool embedded;
  final bool compact;
  const ReleaseSettingsView({
    super.key,
    required this.updates,
    required this.palette,
    this.embedded = false,
    this.compact = false,
  });
  @override
  State<ReleaseSettingsView> createState() => _ReleaseSettingsViewState();
}

class _ReleaseSettingsViewState extends State<ReleaseSettingsView> {
  bool _expanded = false;
  MioPalette get p => widget.palette;
  String t(String en, String zh) => p.english ? en : zh;
  String _error(String code) => switch (code) {
    'release_not_public' => t(
      'No public release could be confirmed. The repository may be private or have no published releases.',
      '无法确认公开发布的版本。仓库可能尚未公开或尚无正式 Release。',
    ),
    'release_rate_limited' => t(
      'GitHub did not allow this check. Wait before retrying.',
      'GitHub 未允许此次检查，请稍后再试。',
    ),
    'release_cooldown' => t(
      'Wait at least one minute between checks.',
      '两次检查请至少间隔一分钟。',
    ),
    'release_network' => t(
      'Could not reach GitHub. Your installed version has not been confirmed as current.',
      '无法连接 GitHub，尚未确认当前版本是否最新。',
    ),
    'release_invalid' => t(
      'The release response could not be validated.',
      '无法验证发布信息。',
    ),
    'invalid_version' => t(
      'The installed app version could not be read or compared.',
      '无法读取或比较已安装应用的版本。',
    ),
    'release_open_failed' => t(
      'Could not open the release page in a browser.',
      '无法在浏览器中打开发布页。',
    ),
    'settings_changed' => t(
      'The update preference changed elsewhere. Check the current setting before retrying.',
      '更新设置已在其他位置变化，请核对当前值后重试。',
    ),
    'settings_not_saved' => t(
      'The update preference could not be saved.',
      '更新设置未能保存。',
    ),
    'busy' => t(
      'Another update operation is still running. Try again shortly.',
      '另一次更新操作尚未结束，请稍后重试。',
    ),
    _ => t(
      'The update check is unavailable. Retry when the music service is connected.',
      '暂时无法检查更新，请在音乐服务连接后重试。',
    ),
  };
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.updates,
    builder: (context, _) {
      final updates = widget.updates, release = widget.updates.release;
      final usable = updates.ready && !updates.busy;
      return MioSettingsBody(
        embedded: widget.embedded,
        key: const PageStorageKey('release-settings'),
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (!widget.embedded) ...[
              Text(
                p.text('settings.about.versionInfo'),
                style: const TextStyle(
                  fontSize: 24,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 16),
            ],
            if (widget.compact) ...[
              LayoutBuilder(
                builder: (context, box) {
                  final toggle = Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      MioSettingsSwitch(
                        palette: p,
                        label: p.text('settings.about.checkUpdateOnStart'),
                        value: updates.autoUpdate ?? false,
                        onPressed: usable && updates.autoUpdate != null
                            ? () => unawaited(
                                updates.setAutoUpdate(!updates.autoUpdate!),
                              )
                            : null,
                      ),
                      const SizedBox(width: 8),
                      Flexible(
                        child: Text(
                          p.text('settings.about.checkUpdateOnStart'),
                          style: const TextStyle(fontSize: 13),
                        ),
                      ),
                    ],
                  );
                  final check = MioButton(
                    palette: p,
                    radius: 6,
                    background: p.brand,
                    label: p.text(
                      updates.busy
                          ? 'settings.about.checking'
                          : 'settings.about.checkUpdate',
                    ),
                    onPressed: updates.busy
                        ? null
                        : () => unawaited(
                            updates.ready
                                ? updates.check()
                                : updates.connect(manual: true),
                          ),
                    child: Text(
                      p.text(
                        updates.busy
                            ? 'settings.about.checking'
                            : 'settings.about.checkUpdate',
                      ),
                      style: TextStyle(
                        fontSize: 13,
                        color: p.color('--td-text-color-anti'),
                      ),
                    ),
                  );
                  if (box.maxWidth < 290 ||
                      MediaQuery.textScalerOf(context).scale(13) > 16) {
                    return Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [toggle, const SizedBox(height: 8), check],
                    );
                  }
                  return Row(
                    children: [
                      Expanded(child: toggle),
                      const SizedBox(width: 8),
                      check,
                    ],
                  );
                },
              ),
              const SizedBox(height: 8),
              Text(
                p.text('settings.about.mobileUpdateHint'),
                style: TextStyle(fontSize: 12, height: 1.5, color: p.secondary),
              ),
            ] else ...[
              Text('Mio Music · ${updates.installedVersion ?? '—'}'),
              const SizedBox(height: 12),
              Text(p.text('settings.about.mobileUpdateHint')),
              const SizedBox(height: 24),
              Wrap(
                spacing: 12,
                runSpacing: 12,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Semantics(
                    toggled: updates.autoUpdate ?? false,
                    child: MioButton(
                      palette: p,
                      label: p.text('settings.about.checkUpdateOnStart'),
                      background: updates.autoUpdate == true
                          ? p.brand.withValues(alpha: .12)
                          : null,
                      onPressed: usable && updates.autoUpdate != null
                          ? () => unawaited(
                              updates.setAutoUpdate(!updates.autoUpdate!),
                            )
                          : null,
                      child: Text(
                        '${updates.autoUpdate == true ? '✓ ' : ''}${p.text('settings.about.checkUpdateOnStart')}',
                      ),
                    ),
                  ),
                  MioButton(
                    palette: p,
                    label: p.text(
                      updates.busy
                          ? 'settings.about.checking'
                          : 'settings.about.checkUpdate',
                    ),
                    onPressed: updates.busy
                        ? null
                        : () => unawaited(
                            updates.ready
                                ? updates.check()
                                : updates.connect(manual: true),
                          ),
                    child: Text(
                      p.text(
                        updates.busy
                            ? 'settings.about.checking'
                            : 'settings.about.checkUpdate',
                      ),
                    ),
                  ),
                ],
              ),
            ],
            if (updates.errorCode != null)
              Padding(
                padding: const EdgeInsets.only(top: 16),
                child: Semantics(
                  liveRegion: true,
                  child: Text(_error(updates.errorCode!)),
                ),
              ),
            if (release != null)
              Container(
                width: double.infinity,
                margin: const EdgeInsets.only(top: 16),
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: p.surface,
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: p.border),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Semantics(
                      liveRegion: true,
                      child: Text(
                        release.newer
                            ? '${p.text('settings.about.newVersionFound')} · ${release.version}'
                            : t(
                                'Installed version is at least as new as the latest stable release (${release.version}).',
                                '已安装版本不低于最新正式发布版本（${release.version}）。',
                              ),
                        style: TextStyle(
                          color: p.brand,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    if (release.newer) ...[
                      const SizedBox(height: 12),
                      Text(
                        p.text('settings.about.releaseNotes'),
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        release.notes.isEmpty
                            ? t('No release notes supplied.', '暂无更新说明。')
                            : release.notes,
                        maxLines: _expanded ? null : 8,
                        overflow: _expanded ? null : TextOverflow.ellipsis,
                      ),
                      if (release.notes.length > 500 ||
                          release.notes.split('\n').length > 8) ...[
                        const SizedBox(height: 8),
                        MioButton(
                          palette: p,
                          label: p.text(
                            _expanded
                                ? 'settings.about.hideFullNotes'
                                : 'settings.about.showFullNotes',
                          ),
                          onPressed: () =>
                              setState(() => _expanded = !_expanded),
                          child: Text(
                            p.text(
                              _expanded
                                  ? 'settings.about.hideFullNotes'
                                  : 'settings.about.showFullNotes',
                            ),
                          ),
                        ),
                      ],
                      if (release.notesTruncated)
                        Padding(
                          padding: const EdgeInsets.only(top: 8),
                          child: Text(
                            t(
                              'Long notes were shortened. Read the complete notes on the release page.',
                              '较长的说明已截短，完整内容请查看发布页。',
                            ),
                          ),
                        ),
                      const SizedBox(height: 12),
                      MioButton(
                        palette: p,
                        label: p.text('settings.about.downloadMobileUpdate'),
                        onPressed: usable
                            ? () => unawaited(updates.openRelease())
                            : null,
                        child: Text(
                          p.text('settings.about.downloadMobileUpdate'),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
          ],
        ),
      );
    },
  );
}

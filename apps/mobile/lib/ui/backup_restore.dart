import 'device.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'glass_surface.dart';
import 'resources.dart';

Future<void> showS3Restore({
  required BuildContext context,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  final route = RawDialogRoute<void>(
    barrierDismissible: false,
    barrierColor: const Color(0x00000000),
    pageBuilder: (_, _, _) => _Restore(palette: palette, isCurrent: isCurrent),
  );
  await Navigator.of(context).push<void>(route);
  await route.completed;
}

class _Restore extends StatefulWidget {
  final MioPalette palette;
  final bool Function() isCurrent;
  const _Restore({required this.palette, required this.isCurrent});
  @override
  State<_Restore> createState() => _RestoreState();
}

class _RestoreState extends State<_Restore> with WidgetsBindingObserver {
  final _fields = List.generate(8, (_) => TextEditingController());
  final _focus = List.generate(8, (_) => FocusNode());
  final _scope = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  api.S3RestorePreparation? _session;
  api.S3UploadPreparation? _uploadSession;
  api.S3SettingsSession? _settings;
  api.S3UploadOutcome? _uploaded;
  String? _lastUpload, _notice;
  bool _upload = false;
  int _tab = 0;
  api.S3RestorePreview? _preview;
  api.S3RestoreOutcome? _outcome;
  bool _busy = false, _failed = false, _closing = false, _foreground = true;
  bool _pathStyle = true, _overwrite = false, _acknowledged = false;
  String? _error;
  MioPalette get p => widget.palette;
  String t(String en, String zh) => p.english ? en : zh;

  @override
  void initState() {
    super.initState();
    _fields[1].text = 'auto';
    _fields[6].text = '10';
    WidgetsBinding.instance.addObserver(this);
    final state = WidgetsBinding.instance.lifecycleState;
    _foreground = state == null || state == AppLifecycleState.resumed;
    if (!_foreground || !widget.isCurrent()) {
      _invalidate();
    } else {
      unawaited(_readSettings());
    }
  }

  void _invalidate() {
    _session?.close();
    _uploadSession?.close();
    _settings?.close();
    _failed = true;
    if (_outcome == null && _uploaded == null) {
      _error = t(
        'Backup session closed. Close and reopen. Any completed writes will be reported here.',
        '备份会话已关闭，请关闭后重开。已完成的写入仍会在这里显示。',
      );
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted) return;
    setState(() {
      _foreground = state == AppLifecycleState.resumed;
      if (state == AppLifecycleState.paused ||
          state == AppLifecycleState.detached) {
        _invalidate();
      }
    });
  }

  void _dismiss() {
    if (!mounted || _busy || _closing) return;
    _closing = true;
    _session?.close();
    _uploadSession?.close();
    _settings?.close();
    Navigator.of(context).pop();
  }

  Future<void> _readSettings() async {
    _busy = true;
    try {
      final session = await api.beginS3Settings();
      if (!mounted || _closing) {
        session.close();
        session.dispose();
        return;
      }
      _settings = session;
      if (_failed || !_foreground || !widget.isCurrent()) {
        setState(_invalidate);
        return;
      }
      final value = await session.state();
      if (!mounted || _closing) return;
      if (_failed || !_foreground || !widget.isCurrent()) {
        setState(_invalidate);
        return;
      }
      setState(() {
        _fields[0].text = value.endpoint;
        _fields[1].text = value.region;
        _fields[2].text = value.accessKeyId;
        _fields[4].text = value.bucket;
        _fields[6].text = value.maxBackups.toString();
        _pathStyle = value.pathStyle;
        _lastUpload = value.lastBackupTime;
      });
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _failed = true;
          _error = t(
            'Could not load connection settings. Close and reopen.',
            '无法读取连接设置，请关闭后重开。',
          );
        });
      }
    } finally {
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  int? _retention() {
    final count = int.tryParse(_fields[6].text.trim());
    return count != null && count >= 1 && count <= 100 ? count : null;
  }

  Future<void> _saveSettings() async {
    if (_busy || _failed || !_foreground || _settings == null) return;
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    final count = _retention();
    if (count == null) {
      setState(() => _error = t('Keep 1 to 100 backups.', '保留份数应为 1 到 100。'));
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    try {
      final value = await _settings!.save(
        next: api.S3SavedConnection(
          endpoint: _fields[0].text.trim(),
          region: _fields[1].text.trim(),
          accessKeyId: _fields[2].text.trim(),
          bucket: _fields[4].text.trim(),
          pathStyle: _pathStyle,
          maxBackups: count,
          lastBackupTime: null,
        ),
      );
      if (!mounted || _closing) return;
      setState(() {
        _lastUpload = value.lastBackupTime;
        _error = null;
        _notice = t(
          'Connection saved. Secret key and password were not saved.',
          '连接已保存，密钥和备份密码未保存。',
        );
      });
    } catch (_) {
      if (mounted && !_closing) {
        setState(
          () => _error = t(
            'Connection not saved. It may have changed; close and reopen.',
            '连接未保存，设置可能已变化，请关闭后重开。',
          ),
        );
      }
    } finally {
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  api.S3RestoreConfig _config() => api.S3RestoreConfig(
    endpoint: _fields[0].text.trim(),
    region: _fields[1].text.trim(),
    accessKeyId: _fields[2].text.trim(),
    secretAccessKey: _fields[3].text,
    bucket: _fields[4].text.trim(),
    pathStyle: _pathStyle,
  );

  Future<void> _load() async {
    if (_busy || _failed || !_foreground || _preview != null) return;
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    if (_fields[0].text.trim().isEmpty ||
        _fields[1].text.trim().isEmpty ||
        _fields[2].text.trim().isEmpty ||
        _fields[3].text.isEmpty ||
        (!_upload && _fields[7].text.isEmpty) ||
        (_upload && _retention() == null)) {
      setState(
        () => _error = t(
          'Complete the connection fields, password for restore, and retention from 1 to 100 for upload.',
          '请填写连接信息；恢复需备份密码，上传需填写 1 到 100 的保留份数。',
        ),
      );
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    try {
      late api.S3RestorePreview preview;
      if (_upload) {
        final session = await api.beginS3UploadPreparation(
          config: _config(),
          maxBackups: _retention()!,
        );
        if (!mounted || _closing) {
          session.close();
          session.dispose();
          return;
        }
        _uploadSession = session;
        if (_failed || !_foreground || !widget.isCurrent()) {
          setState(_invalidate);
          return;
        }
        preview = await session.load();
      } else {
        final session = await api.beginS3RestorePreparation(config: _config());
        if (!mounted || _closing) {
          session.close();
          session.dispose();
          return;
        }
        _session = session;
        if (_failed || !_foreground || !widget.isCurrent()) {
          setState(_invalidate);
          return;
        }
        preview = await session.load(password: _fields[7].text);
      }
      if (!mounted || _closing) return;
      if (_failed || !_foreground || !widget.isCurrent()) {
        setState(_invalidate);
        return;
      }
      setState(() => _preview = preview);
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _failed = true;
          _error = t(
            'Could not prepare the backup. Check the connection, password and backup format, then close and reopen.',
            '无法准备备份。请检查连接、密码及备份格式，关闭后重新打开。',
          );
        });
      }
      _session?.close();
      _uploadSession?.close();
      _settings?.close();
    } finally {
      if (mounted && !_closing) {
        _fields[3].clear();
        _fields[5].clear();
        _fields[7].clear();
        setState(() => _busy = false);
      }
    }
  }

  Future<void> _commit() async {
    if (_busy ||
        _failed ||
        !_foreground ||
        !_acknowledged ||
        _preview == null ||
        _session == null ||
        _outcome != null) {
      return;
    }
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final outcome = await _session!.commit(overwrite: _overwrite);
      // Preserve the actual outcome even if the app went into the background
      // or this restore changed its own source revision while awaiting the call.
      if (!mounted || _closing) return;
      setState(() {
        _outcome = outcome;
        _error = null;
        _failed = true;
      });
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _failed = true;
          _error = t(
            'Restore did not complete. Close and reopen after checking the library and settings.',
            '恢复未完成。请检查歌单和设置，关闭后重新打开。',
          );
        });
      }
    } finally {
      _session?.close();
      _uploadSession?.close();
      _settings?.close();
      if (mounted && !_closing) setState(() => _busy = false);
    }
  }

  Future<void> _commitUpload() async {
    if (_busy ||
        _failed ||
        !_foreground ||
        !_acknowledged ||
        _uploadSession == null ||
        _preview == null ||
        _uploaded != null) {
      return;
    }
    if (!widget.isCurrent()) {
      setState(_invalidate);
      return;
    }
    if (_fields[5].text.isEmpty) {
      setState(
        () => _error = t(
          'Enter an encryption password. Keep it safe to restore this backup.',
          '请输入加密密码，并妥善保存以便恢复。',
        ),
      );
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final outcome = await _uploadSession!.upload(password: _fields[5].text);
      if (!mounted || _closing) return;
      setState(() {
        _uploaded = outcome;
        _error = null;
        _failed = true;
      });
    } catch (error) {
      if (mounted && !_closing) {
        // Only classify a fixed Rust error code; never display provider text.
        final unknown = error.toString().contains('upload_unknown:');
        setState(() {
          _failed = true;
          _error = unknown
              ? t(
                  'Server outcome unknown. Inspect the bucket before retrying. No automatic retry or cleanup was performed.',
                  '服务器结果未知，请检查桶内对象后再重试。未自动重试或清理。',
                )
              : t(
                  'Upload was not confirmed. Close and reopen after checking the connection and local data.',
                  '上传未确认，请检查连接和本地数据，关闭后重开。',
                );
        });
      }
    } finally {
      _uploadSession?.close();
      if (mounted && !_closing) {
        _fields[5].clear();
        setState(() => _busy = false);
      }
    }
  }

  @override
  void dispose() {
    _session?.close();
    _uploadSession?.close();
    _settings?.close();
    _session?.dispose();
    _uploadSession?.dispose();
    _settings?.dispose();
    WidgetsBinding.instance.removeObserver(this);
    for (final field in _fields) {
      field.clear();
      field.dispose();
    }
    for (final node in _focus) {
      node.dispose();
    }
    _scope.dispose();
    super.dispose();
  }

  Color get _primary => p.color('--td-text-color-primary');
  Color get _success => p.color('--td-success-color');
  bool get _configured =>
      [0, 1, 2, 3].every((i) => _fields[i].text.trim().isNotEmpty);
  String get _status {
    if (_uploaded != null) return t('Upload confirmed', '上传已确认');
    if (_outcome != null) return t('Restore completed', '恢复已完成');
    if (_failed) return t('Session closed', '会话已关闭');
    if (_preview != null && !_upload) return p.text('backup.connected');
    return _configured
        ? t('Configured · not verified', '已填写 · 待验证')
        : p.text('backup.disconnected');
  }

  bool get _editing =>
      _preview == null && _outcome == null && _uploaded == null;

  Widget _button(String label, VoidCallback? action, {bool? selected}) =>
      Semantics(
        selected: selected,
        child: MioButton(
          palette: p,
          label: label,
          radius: 11,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          onPressed: action,
          background: selected == true
              ? p.brand.withValues(alpha: .14)
              : _primary.withValues(alpha: .05),
          child: Center(child: Text(label, textAlign: TextAlign.center)),
        ),
      );

  Widget _input(int i, String label, {bool secret = false}) => Padding(
    padding: const EdgeInsets.only(bottom: 14),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: TextStyle(fontSize: 12, color: p.secondary)),
        const SizedBox(height: 6),
        MioInput(
          palette: p,
          controller: _fields[i],
          focusNode: _focus[i],
          placeholder: label,
          height: 40,
          radius: 11,
          fontSize: 13,
          background: _primary.withValues(alpha: .03),
          maxLength: i == 2 ? 256 : 4096,
          obscureText: secret,
          keyboardType: i == 6 ? TextInputType.number : TextInputType.text,
          readOnly: _busy || _failed || (_preview != null && i != 5),
          onChanged: (_) => setState(() {}),
          textInputAction: [3, 6, 7].contains(i) || _preview != null
              ? TextInputAction.done
              : TextInputAction.next,
          onSubmitted: (_) {
            final next = {0: 1, 1: 4, 4: 2, 2: 3, 5: 6}[i];
            if (_preview != null || next == null) {
              _focus[i].unfocus();
            } else {
              _focus[next].requestFocus();
            }
          },
        ),
      ],
    ),
  );

  Widget _pair(Widget left, Widget right) => LayoutBuilder(
    builder: (context, box) => box.maxWidth < 300
        ? Column(children: [left, right])
        : Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(child: left),
              const SizedBox(width: 12),
              Expanded(child: right),
            ],
          ),
  );

  Widget _hint(String text) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Padding(
        padding: const EdgeInsets.only(top: 2),
        child: MioIcon('Cloud', color: p.secondary, size: 14),
      ),
      const SizedBox(width: 6),
      Expanded(
        child: Text(
          text,
          style: TextStyle(fontSize: 11.5, height: 1.5, color: p.secondary),
        ),
      ),
    ],
  );

  Widget _card(List<Widget> children) => Container(
    width: double.infinity,
    padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(
      color: _primary.withValues(alpha: .03),
      border: Border.all(color: _primary.withValues(alpha: .08)),
      borderRadius: BorderRadius.circular(14),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: children,
    ),
  );

  Future<void> _openProvider(String provider) async {
    if (_busy || _failed) return;
    setState(() => _busy = true);
    try {
      await const MethodChannel('mio/host')
          .invokeMethod<void>('openBackupProvider', provider);
      if (mounted && !_closing) {
        setState(() => _busy = false);
        _dismiss();
      }
    } catch (_) {
      if (mounted && !_closing) {
        setState(() {
          _busy = false;
          _error = t('Could not open the provider website.', '无法打开服务商官网。');
        });
      }
    }
  }

  Widget _provider(String id, List<String> tags) => Container(
    decoration: BoxDecoration(
      border: Border.all(color: _primary.withValues(alpha: .08)),
      borderRadius: BorderRadius.circular(14),
    ),
    child: MioButton(
      palette: p,
      label: p.text('backup.$id.title'),
      onPressed: _busy || _failed ? null : () => unawaited(_openProvider(id)),
      radius: 14,
      padding: const EdgeInsets.all(14),
      background: _primary.withValues(alpha: .03),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                decoration: BoxDecoration(
                  color: _success.withValues(alpha: .12),
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Text(
                  p.text('backup.free'),
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: _success,
                  ),
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      p.text('backup.$id.title'),
                      style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    Text(
                      p.text('backup.$id.url'),
                      style: TextStyle(fontSize: 11, color: p.secondary),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final tag in tags)
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: _primary.withValues(alpha: .05),
                    borderRadius: BorderRadius.circular(6),
                  ),
                  child: Text(
                    p.text('backup.$id.$tag'),
                    style: TextStyle(fontSize: 11, color: p.secondary),
                  ),
                ),
            ],
          ),
          const SizedBox(height: 10),
          Text(
            p.text('backup.$id.desc'),
            style: TextStyle(fontSize: 12, height: 1.5, color: p.secondary),
          ),
        ],
      ),
    ),
  );

  List<Widget> _recommendation() => [
    Text(
      p.text('backup.recommendIntro'),
      style: TextStyle(fontSize: 13, height: 1.5, color: p.secondary),
    ),
    const SizedBox(height: 12),
    _provider('hi168', ['storage', 'traffic', 's3Compatible']),
    const SizedBox(height: 12),
    _provider('cstcloud', ['storage', 's3Compatible', 'research']),
    const SizedBox(height: 12),
    _hint(p.text('backup.recommendHint')),
  ];

  List<Widget> _configuration() => [
    _input(0, p.text('backup.endpoint')),
    _pair(
      _input(1, p.text('backup.region')),
      _input(4, p.text('backup.bucket')),
    ),
    _pair(
      _input(2, p.text('backup.accessKeyId'), secret: true),
      _input(3, p.text('backup.secretAccessKey'), secret: true),
    ),
    MioButton(
      palette: p,
      label: t('Path-style addressing', '路径风格（Path-style）'),
      onPressed: _busy || _failed
          ? null
          : () => setState(() => _pathStyle = !_pathStyle),
      padding: const EdgeInsets.symmetric(vertical: 8),
      radius: 11,
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  t('Path-style addressing', '路径风格（Path-style）'),
                  style: TextStyle(fontSize: 12, color: p.secondary),
                ),
                const SizedBox(height: 2),
                Text(
                  t(
                    'On: host/bucket/key. Off: bucket.host/key.',
                    '开：host/bucket/key；关：bucket.host/key（虚拟主机风格）',
                  ),
                  style: TextStyle(fontSize: 11.5, color: p.secondary),
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          Semantics(
            toggled: _pathStyle,
            child: Container(
              width: 36,
              height: 22,
              padding: const EdgeInsets.all(3),
              decoration: BoxDecoration(
                color: _pathStyle ? p.brand : p.border,
                borderRadius: BorderRadius.circular(11),
              ),
              child: Align(
                alignment: _pathStyle
                    ? Alignment.centerRight
                    : Alignment.centerLeft,
                child: Container(
                  width: 16,
                  height: 16,
                  decoration: const BoxDecoration(
                    color: Color(0xffffffff),
                    shape: BoxShape.circle,
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    ),
    const SizedBox(height: 14),
    _button(
      t('Save connection', '保存连接'),
      _busy || _failed ? null : () => unawaited(_saveSettings()),
      selected: true,
    ),
    const SizedBox(height: 12),
    _hint(
      t(
        'Connection is checked when reading or uploading a backup. Secret key and passwords are not saved.',
        '读取或上传备份时验证连接，密钥和密码不会保存。',
      ),
    ),
  ];

  void _prepareOperation({required bool upload, bool overwrite = false}) {
    // The preview replaces the form but reuses its password FocusNode. Release
    // the old EditableText connection before mounting the preview input.
    _scope.unfocus();
    setState(() {
      _upload = upload;
      _overwrite = overwrite;
      _acknowledged = false;
    });
    unawaited(_load());
  }

  Widget _operationHeading(
    String icon,
    Color tint,
    String title,
    String desc,
  ) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Container(
        width: 40,
        height: 40,
        decoration: BoxDecoration(
          color: tint.withValues(alpha: .14),
          borderRadius: BorderRadius.circular(12),
        ),
        child: Center(child: MioIcon(icon, color: tint, size: 20)),
      ),
      const SizedBox(width: 12),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              p.text(title),
              style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 2),
            Text(
              p.text(desc),
              style: TextStyle(fontSize: 12, color: p.secondary),
            ),
          ],
        ),
      ),
    ],
  );

  List<Widget> _operations() => [
    _pair(
      _input(5, p.text('backup.backupPassword'), secret: true),
      _input(6, p.text('backup.maxBackups')),
    ),
    _card([
      _operationHeading(
        'Cloud',
        p.brand,
        'backup.backupToCloud',
        'backup.backupToCloudDesc',
      ),
      if (_lastUpload != null)
        Padding(
          padding: const EdgeInsets.only(top: 12),
          child: Text(
            '${p.text('backup.lastBackup')}$_lastUpload',
            style: TextStyle(fontSize: 11.5, color: p.secondary),
          ),
        ),
      const SizedBox(height: 12),
      _button(
        p.text('backup.encryptBackup'),
        _busy || _failed || _fields[5].text.isEmpty
            ? null
            : () => _prepareOperation(upload: true),
        selected: true,
      ),
    ]),
    const SizedBox(height: 14),
    _card([
      _operationHeading(
        'TD-Download',
        _success,
        'backup.restoreFromCloud',
        'backup.restoreFromCloudDesc',
      ),
      const SizedBox(height: 12),
      _input(7, p.text('backup.restorePassword'), secret: true),
      _pair(
        _button(
          p.text('backup.mergeRestore'),
          _busy || _failed || _fields[7].text.isEmpty
              ? null
              : () => _prepareOperation(upload: false),
        ),
        _button(
          p.text('backup.overwriteRestore'),
          _busy || _failed || _fields[7].text.isEmpty
              ? null
              : () => _prepareOperation(upload: false, overwrite: true),
        ),
      ),
    ]),
    const SizedBox(height: 14),
    _hint(p.text('backup.opsHint')),
  ];

  Widget _header() => Row(
    children: [
      Container(
        width: 42,
        height: 42,
        decoration: BoxDecoration(
          gradient: LinearGradient(
            colors: [
              p.brand.withValues(alpha: .24),
              const Color(0xffa78bfa).withValues(alpha: .18),
            ],
          ),
          border: Border.all(color: _primary.withValues(alpha: .08)),
          borderRadius: BorderRadius.circular(13),
        ),
        child: Center(
          child: MioIcon('BackupCloud', color: p.secondary, size: 22),
        ),
      ),
      const SizedBox(width: 12),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              p.text('backup.backupAndRestore'),
              style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 4),
            Row(
              children: [
                Container(
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(
                    color:
                        _uploaded != null ||
                            (_preview != null && !_upload && !_failed)
                        ? _success
                        : p.secondary.withValues(alpha: .5),
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    _status,
                    style: TextStyle(fontSize: 11, color: p.secondary),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
      const SizedBox(width: 8),
      MioButton(
        palette: p,
        label: p.text('common.close'),
        onPressed: _busy ? null : _dismiss,
        radius: 9,
        background: _primary.withValues(alpha: .03),
        padding: const EdgeInsets.all(10),
        child: MioIcon('WindowClose', color: p.secondary, size: 13),
      ),
    ],
  );

  Widget _tabs() => Container(
    padding: const EdgeInsets.all(3),
    decoration: BoxDecoration(
      color: _primary.withValues(alpha: .04),
      border: Border.all(color: _primary.withValues(alpha: .08)),
      borderRadius: BorderRadius.circular(11),
    ),
    child: Row(
      children: [
        for (final (index, key) in [
          'tabRecommend',
          'tabConfig',
          'tabOps',
        ].indexed)
          Expanded(
            child: Semantics(
              selected: _tab == index,
              child: MioButton(
                palette: p,
                label: p.text('backup.$key'),
                onPressed: _busy || !_editing || (index == 2 && !_configured)
                    ? null
                    : () {
                        _scope.unfocus();
                        setState(() => _tab = index);
                      },
                radius: 8,
                padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 7),
                background: _tab == index
                    ? _primary.withValues(alpha: .1)
                    : null,
                disabledOpacity: _tab == index ? 1 : .4,
                child: Center(
                  child: Text(
                    p.text('backup.$key'),
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    ),
  );

  String _warning(String code) => switch (code) {
    'connection_not_saved' => t(
      'Upload succeeded, but connection details and last-upload time could not be saved.',
      '上传成功，但连接信息和上次上传时间未能保存。',
    ),
    'retention_cancelled' => t(
      'Upload succeeded. Cleanup was cancelled.',
      '上传成功，旧备份清理已取消。',
    ),
    'retention_not_completed' => t(
      'Upload succeeded. The full list could not be confirmed; cleanup was skipped.',
      '上传成功，无法确认完整列表，已跳过清理。',
    ),
    'retention_partial_or_unknown' => t(
      'Upload succeeded. Some cleanup results could not be confirmed.',
      '上传成功，部分旧备份清理结果未确认。',
    ),
    'plugins_skipped' => t(
      'Duplicate, unsupported or invalid plugins were skipped.',
      '已跳过重复、不支持或无效的插件。',
    ),
    'plugins_not_saved' => t(
      'Plugins could not be saved. Existing selection was retained.',
      '插件未能保存，保留原有选择。',
    ),
    'settings_not_saved' => t(
      'Settings could not be saved. Existing settings were retained.',
      '设置未能保存，保留原有设置。',
    ),
    _ => t(
      'Restore was cancelled after the library was saved. Check the step results below.',
      '歌单保存后恢复已取消，请查看各步骤结果。',
    ),
  };

  List<Widget> _body() {
    final preview = _preview, result = _outcome;
    final uploaded = _uploaded;
    if (uploaded != null) {
      return [
        Text(
          t('Upload confirmed', '上传已确认'),
          style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 12),
        Text(
          t('Snapshot: ${uploaded.timestamp}', '快照时间：${uploaded.timestamp}'),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            '${uploaded.deleted} old backups confirmed deleted',
            '已确认删除 ${uploaded.deleted} 份旧备份',
          ),
        ),
        for (final warning in uploaded.warnings)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(_warning(warning)),
          ),
      ];
    }
    if (result != null) {
      return [
        Text(
          t('Library saved', '歌单已保存'),
          style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            '${result.playlistsAdded} playlists added · ${result.songsAdded} songs added · ${result.songsSkipped} songs kept',
            '新增 ${result.playlistsAdded} 个歌单、${result.songsAdded} 首歌曲，保留 ${result.songsSkipped} 首已有歌曲',
          ),
        ),
        const SizedBox(height: 12),
        Text(
          result.pluginsApplied
              ? t(
                  'Plugins: ${result.pluginsAdded} added, ${result.pluginsSkipped} skipped',
                  '插件：新增 ${result.pluginsAdded} 个、跳过 ${result.pluginsSkipped} 个',
                )
              : t('Plugins: not saved', '插件：未保存'),
        ),
        const SizedBox(height: 8),
        Text(
          result.settingsApplied
              ? (_overwrite
                    ? t('Settings: applied', '设置：已应用')
                    : t('Current settings retained', '已保留当前设置'))
              : t('Settings: not saved', '设置：未保存'),
        ),
        for (final code in result.warnings)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(_warning(code)),
          ),
      ];
    }
    if (preview != null && _upload) {
      return [
        Text(t('Snapshot: ${preview.timestamp}', '快照时间：${preview.timestamp}')),
        const SizedBox(height: 12),
        Text(
          t(
            '${preview.playlists} playlists · ${preview.songs} songs · ${preview.plugins} plugins',
            '${preview.playlists} 个歌单 · ${preview.songs} 首歌曲 · ${preview.plugins} 个插件',
          ),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            '${preview.localReferences} local audio references. Audio files are not included.',
            '${preview.localReferences} 首本地音频引用，不包含音频文件。',
          ),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            'This encrypted snapshot includes library metadata, settings and plugin code, which may contain private information. Keep the encryption password safe; it is required to restore.',
            '加密快照包含歌单元数据、设置和插件代码，可能含有私人信息。请妥善保管加密密码，恢复时必须使用。',
          ),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            'After upload, older matching cloud backups beyond ${_fields[6].text} copies may be deleted. Connection details and the confirmed snapshot time will be saved without the secret key or password.',
            '上传后，超过 ${_fields[6].text} 份的较旧匹配云备份可能被删除。将保存连接信息和已确认的快照时间，不保存密钥或密码。',
          ),
        ),
        const SizedBox(height: 16),
        _input(5, t('Encryption password', '加密密码'), secret: true),
        _button(
          '${_acknowledged ? '✓ ' : ''}${t('I confirm upload and retention cleanup', '我确认上传及旧备份清理')}',
          _busy || _failed
              ? null
              : () => setState(() => _acknowledged = !_acknowledged),
          selected: _acknowledged,
        ),
        const SizedBox(height: 12),
        _button(
          t('Confirm upload', '确认上传'),
          _busy || _failed || !_acknowledged
              ? null
              : () => unawaited(_commitUpload()),
        ),
      ];
    }
    if (preview != null) {
      return [
        Text(t('Backup: ${preview.timestamp}', '备份时间：${preview.timestamp}')),
        const SizedBox(height: 12),
        Text(
          t(
            '${preview.playlists} playlists · ${preview.songs} songs · ${preview.plugins} plugins',
            '${preview.playlists} 个歌单 · ${preview.songs} 首歌曲 · ${preview.plugins} 个插件',
          ),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            '${preview.localReferences} local audio references. Audio files are not included.',
            '${preview.localReferences} 首本地音频引用。备份不包含音频文件。',
          ),
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            for (final overwrite in [false, true])
              _button(
                overwrite ? t('Overwrite', '覆盖') : t('Merge', '合并'),
                _busy || _failed
                    ? null
                    : () => setState(() {
                        _overwrite = overwrite;
                        _acknowledged = false;
                      }),
                selected: _overwrite == overwrite,
              ),
          ],
        ),
        const SizedBox(height: 12),
        Text(
          _overwrite
              ? t(
                  'Overwrite deletes all current playlists and replaces them with this backup. Backed-up settings replace their matching current settings.',
                  '覆盖会删除当前全部歌单，并替换为这份备份。备份设置会替换对应的当前设置。',
                )
              : t(
                  'Merge keeps current playlists, duplicate songs and current settings, adding missing playlists and songs.',
                  '合并保留当前歌单、已有歌曲及当前设置，只添加缺少的歌单和歌曲。',
                ),
        ),
        const SizedBox(height: 12),
        Text(
          t(
            'New valid plugins are installed inactive. Plugin validation runs their initialization code and may access the network. Library, plugin and settings writes have separate results.',
            '新增有效插件保持未激活。验证插件会运行其初始化代码，并可能访问网络。歌单、插件和设置分别保存，各步骤会报告实际结果。',
          ),
        ),
        const SizedBox(height: 16),
        _button(
          '${_acknowledged ? '✓ ' : ''}${t('I understand and confirm this restore', '我已了解并确认此次恢复')}',
          _busy || _failed
              ? null
              : () => setState(() => _acknowledged = !_acknowledged),
          selected: _acknowledged,
        ),
        const SizedBox(height: 12),
        _button(
          t('Confirm restore', '确认恢复'),
          _busy || _failed || !_acknowledged
              ? null
              : () => unawaited(_commit()),
        ),
      ];
    }
    return switch (_tab) {
      0 => _recommendation(),
      1 => _configuration(),
      _ => _operations(),
    };
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: false,
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop) _dismiss();
    },
    child: CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
      child: FocusScope(
        node: _scope,
        autofocus: true,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.text('backup.backupAndRestore'),
          child: DefaultTextStyle(
            style: TextStyle(
              color: p.color('--td-text-color-primary'),
              fontSize: 14,
              decoration: TextDecoration.none,
            ),
            child: LayoutBuilder(
              builder: (context, box) {
                final media = MediaQuery.of(context),
                    narrow = !MioDeviceScope.wide(context, box.maxWidth);
                return Stack(
                  children: [
                    Positioned.fill(
                      child: ModalBarrier(
                        color: p.color('--td-mask-active'),
                        dismissible: !_busy,
                        onDismiss: _dismiss,
                        semanticsLabel: p.text('common.close'),
                      ),
                    ),
                    Positioned.fill(
                      bottom: media.viewInsets.bottom,
                      child: SafeArea(
                        minimum: const EdgeInsets.all(12),
                        child: Align(
                          alignment: Alignment.center,
                          child: ConstrainedBox(
                            constraints: BoxConstraints(
                              maxWidth: math.min(
                                math.max(0, box.maxWidth - 32),
                                500,
                              ),
                              maxHeight: math.max(
                                0,
                                math.min(
                                  box.maxHeight * .9,
                                  box.maxHeight -
                                      media.viewInsets.bottom -
                                      media.padding.vertical -
                                      24,
                                ),
                              ),
                            ),
                            // The panel is a liquid-glass lens over the page; the
                            // shadow sits outside so the lens clip stays clean.
                            child: DecoratedBox(
                              decoration: BoxDecoration(
                                borderRadius: BorderRadius.circular(
                                  narrow ? 18 : 22,
                                ),
                                boxShadow: const [
                                  BoxShadow(
                                    offset: Offset(0, 12),
                                    color: Color(0x40000000),
                                    blurRadius: 32,
                                  ),
                                ],
                              ),
                              child: MioGlassSurface(
                                palette: p,
                                radius: narrow ? 18 : 22,
                                tint: p.surface.withValues(
                                  alpha: p.dark ? .5 : .42,
                                ),
                                child: Container(
                                  padding: EdgeInsets.all(narrow ? 20 : 28),
                                  decoration: BoxDecoration(
                                    border: Border.all(
                                      color: const Color(0x40ffffff),
                                    ),
                                    borderRadius: BorderRadius.circular(
                                      narrow ? 18 : 22,
                                    ),
                                  ),
                                  child: SingleChildScrollView(
                                    child: Column(
                                      mainAxisSize: MainAxisSize.min,
                                      crossAxisAlignment:
                                          CrossAxisAlignment.stretch,
                                      children: [
                                        _header(),
                                        const SizedBox(height: 20),
                                        _tabs(),
                                        const SizedBox(height: 20),
                                        ..._body(),
                                        if (_busy)
                                          Padding(
                                            padding: const EdgeInsets.only(
                                              top: 16,
                                            ),
                                            child: Text(
                                              t(
                                                'Working… Completed writes will be reported.',
                                                '处理中… 将显示已完成的写入。',
                                              ),
                                              semanticsLabel: t(
                                                'Backup operation in progress',
                                                '正在处理备份操作',
                                              ),
                                            ),
                                          ),
                                        if (_notice != null)
                                          Padding(
                                            padding: const EdgeInsets.only(
                                              top: 16,
                                            ),
                                            child: Semantics(
                                              liveRegion: true,
                                              child: Text(
                                                _notice!,
                                                style: TextStyle(
                                                  fontSize: 12,
                                                  color: p.secondary,
                                                ),
                                              ),
                                            ),
                                          ),
                                        if (_error != null)
                                          Padding(
                                            padding: const EdgeInsets.only(
                                              top: 16,
                                            ),
                                            child: Semantics(
                                              liveRegion: true,
                                              child: Text(
                                                _error!,
                                                style: TextStyle(
                                                  fontSize: 12,
                                                  color: p.color(
                                                    '--td-error-color',
                                                  ),
                                                ),
                                              ),
                                            ),
                                          ),
                                        if (_busy && !_failed)
                                          Padding(
                                            padding: const EdgeInsets.only(
                                              top: 12,
                                            ),
                                            child: _button(
                                              t(
                                                'Cancel remaining work',
                                                '取消后续操作',
                                              ),
                                              () => setState(_invalidate),
                                            ),
                                          ),
                                      ],
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    ),
  );
}

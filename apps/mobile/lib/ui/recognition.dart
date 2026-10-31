import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import '../core_controller.dart';
import '../platform_files.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'device.dart';
import 'cover.dart';
import 'resources.dart';

Future<String?> showFileRecognition({
  required BuildContext context,
  required CoreController core,
  required MioPalette palette,
}) async {
  final route = RawDialogRoute<String>(
    barrierDismissible: false,
    barrierColor: const Color(0x66000000),
    pageBuilder: (_, _, _) => _Recognition(core: core, palette: palette),
  );
  final query = await Navigator.of(context).push<String>(route);
  await route.completed;
  return query;
}

class RecognitionPage extends StatelessWidget {
  final CoreController core;
  final MioPalette palette;
  final ValueChanged<String> onSearch;
  const RecognitionPage({
    super.key,
    required this.core,
    required this.palette,
    required this.onSearch,
  });
  @override
  Widget build(BuildContext context) => _Recognition(
    core: core,
    palette: palette,
    page: true,
    onSearch: onSearch,
  );
}

class _Recognition extends StatefulWidget {
  final CoreController core;
  final MioPalette palette;
  final bool page;
  final ValueChanged<String>? onSearch;
  const _Recognition({
    required this.core,
    required this.palette,
    this.page = false,
    this.onSearch,
  });
  @override
  State<_Recognition> createState() => _RecognitionState();
}

class _RecognitionState extends State<_Recognition>
    with WidgetsBindingObserver {
  final _focus = FocusScopeNode(
    traversalEdgeBehavior: TraversalEdgeBehavior.closedLoop,
  );
  api.RecognitionSession? _session;
  static const _microphone = MethodChannel('mio/microphone');
  bool _micAvailable = false,
      _authorizing = false,
      _micAuthorized = false,
      _micRunning = false;
  int? _micToken;
  Timer? _micTimer;
  api.MicrophoneProgress? _micProgress;
  AudioFileSelection? _selection;
  String? _ticket, _message;
  List<api.RecognizedTrack>? _rows;
  api.RecognitionHistory? _history;
  String? _historyNotice;
  bool _loading = false, _clearing = false, _confirmClear = false;
  late final int? _revision;
  late final int? _selectionRevision;
  bool _choosing = false, _running = false, _playing = false, _invalid = false;
  bool _foreground = true;
  MioPalette get p => widget.palette;
  String t(String en, String zh) => p.english ? en : zh;
  bool get _current =>
      mounted &&
      !_invalid &&
      _revision != null &&
      widget.core.player?.sourceRevision == _revision &&
      (!widget.page ||
          widget.core.player?.sourceSelectionRevision == _selectionRevision);
  bool get _busy =>
      _choosing ||
      _running ||
      _playing ||
      _loading ||
      _clearing ||
      _authorizing;

  @override
  void initState() {
    super.initState();
    _revision = widget.core.player?.sourceRevision;
    _selectionRevision = widget.core.player?.sourceSelectionRevision;
    _foreground =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    widget.core.addListener(_coreChanged);
    WidgetsBinding.instance.addObserver(this);
    if (_foreground && _current) unawaited(_loadHistory());
    unawaited(_loadMicCapability());
  }

  void _coreChanged() {
    if (!_current && mounted && !_invalid) setState(_invalidate);
  }

  void _invalidate() {
    _invalid = true;
    _rows = null;
    _history = null;
    _confirmClear = false;
    _session?.close();
    _micTimer?.cancel();
    if (_authorizing) unawaited(_releaseMicrophone());
    unawaited(_selection?.cancel());
    if (!_busy) unawaited(_release());
    _message = t(
      'Session closed. Close and reopen to try again.',
      '会话已关闭，请关闭后重新打开。',
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted) return;
    setState(() {
      _foreground = state == AppLifecycleState.resumed;
      // System file choosers can pause Flutter. Recognition requires a separate
      // foreground button press after the chooser has returned.
      if (state == AppLifecycleState.detached ||
          (state == AppLifecycleState.paused && !_choosing && !_authorizing)) {
        _invalidate();
      }
    });
  }

  Future<void> _release() async {
    final session = _session;
    _session = null;
    _history = null;
    _confirmClear = false;
    session?.close();
    session?.dispose();
    await _discardSelection();
    await _releaseMicrophone();
  }

  Future<void> _discardSelection() async {
    final selection = _selection;
    _selection = null;
    _ticket = null;
    await selection?.discard();
  }

  String _historyError(String code) => switch (code) {
    'recognition_history_changed' => t(
      'History changed elsewhere. Close and reopen to read the latest records.',
      '历史已在其他位置变化，请关闭后重新打开读取新记录。',
    ),
    'recognition_history_invalid' => t(
      'The history file is invalid. It has been preserved; confirm Clear history to reset it.',
      '历史文件损坏，原文件已保留；可确认清空历史后重新保存。',
    ),
    'recognition_history_not_saved' => t(
      'History could not be saved. Recognition results are still available.',
      '历史未能保存，本次识曲结果仍可使用。',
    ),
    _ => t(
      'History could not be read or updated. The original file was preserved.',
      '历史暂时无法读取或更新，原文件已保留。',
    ),
  };

  Future<void> _readHistory(api.RecognitionSession session) async {
    try {
      final history = await session.history();
      if (!_current || !identical(session, _session)) return;
      _history = history;
      _historyNotice = history.warning == null
          ? null
          : _historyError(history.warning!);
    } catch (_) {
      if (_current) _historyNotice = _historyError('unavailable');
    }
  }

  Future<void> _loadHistory() async {
    _loading = true;
    try {
      final session = await api.beginRecognition();
      _session = session;
      if (_current) await _readHistory(session);
    } catch (_) {
      if (_current) _historyNotice = _historyError('unavailable');
    } finally {
      if (!_current) await _release();
      _loading = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _clearHistory() async {
    final session = _session, history = _history;
    if (!_current ||
        !_foreground ||
        _busy ||
        !_confirmClear ||
        session == null ||
        history == null ||
        !history.canClear) {
      return;
    }
    setState(() {
      _clearing = true;
      _confirmClear = false;
    });
    try {
      final actual = await session.clearHistory(revision: history.revision);
      if (_current) {
        _history = actual;
        _historyNotice = t('History cleared.', '历史已清空。');
      }
    } catch (_) {
      if (_current) {
        _historyNotice = t(
          'History was not confirmed cleared. Close and reopen before retrying.',
          '尚未确认历史已清空，请关闭后重新打开核对再试。',
        );
      }
    } finally {
      if (!_current) await _release();
      _clearing = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _loadMicCapability() async {
    try {
      if (api.capabilities().microphone == false) return;
      final available = await _microphone.invokeMethod<bool>('available');
      if (mounted) setState(() => _micAvailable = available == true);
    } on PlatformException {
      /* Capability remains unavailable. */
    } on MissingPluginException {
      /* Host capture integration is not installed. */
    }
  }

  Future<void> _releaseMicrophone() async {
    final token = _micToken;
    _micToken = null;
    _micAuthorized = false;
    if (token == null) return;
    try {
      await _microphone.invokeMethod<void>('release', token);
    } on PlatformException {
      /* Native detach revokes authorization independently. */
    } on MissingPluginException {
      /* Unsupported host. */
    }
  }

  Future<void> _authorizeMicrophone() async {
    if (!_current || !_foreground || _busy || !_micAvailable) return;
    setState(() {
      _authorizing = true;
      _rows = null;
      _message = null;
      _micProgress = null;
    });
    try {
      await _release();
      final session = await api.beginRecognition();
      _session = session;
      if (!_current) return;
      await _readHistory(session);
      if (!_current) return;
      final token = session.microphoneToken();
      _micToken = token;
      await _microphone.invokeMethod<void>('request', token);
      if (!_current) return;
      _micAuthorized = true;
      _message = t(
        'Microphone ready. Start below to record and send its fingerprint.',
        '麦克风已就绪，点击下方按钮开始录音并发送音频指纹。',
      );
    } catch (_) {
      if (_current) {
        _message = t(
          'Microphone permission was denied or is unavailable. You can enable it in system settings, or choose an audio file.',
          '未获得麦克风授权或设备暂不可用。可在系统设置中允许访问，或选择音频文件。',
        );
      }
    } finally {
      if (!_current || !_micAuthorized) await _releaseMicrophone();
      if (!_current) await _release();
      _authorizing = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _recognizeMicrophone() async {
    final session = _session;
    if (!_current ||
        !_foreground ||
        _busy ||
        !_micAuthorized ||
        _micToken == null ||
        session == null) {
      return;
    }
    setState(() {
      _running = true;
      _micRunning = true;
      _message = null;
      _rows = null;
    });
    _micTimer = Timer.periodic(const Duration(milliseconds: 200), (_) {
      if (!_current) return;
      final progress = session.microphoneProgress();
      if (mounted) setState(() => _micProgress = progress);
    });
    try {
      final rows = await session.recognizeMicrophone();
      if (!_current || !_foreground) return;
      _rows = rows;
      if (rows.isEmpty) _message = t('No matching song found.', '未识别到匹配歌曲。');
      await _readHistory(session);
    } catch (error) {
      if (_current) _message = _recognitionError(error);
    } finally {
      _micTimer?.cancel();
      _micTimer = null;
      final progress = session.microphoneProgress();
      if (_current && progress.restoreFailed) {
        _message = t(
          'The recording ended, but audio playback could not be restored. Use the player controls to resume.',
          '录音已结束，但音频播放未能恢复，请使用播放器控制继续播放。',
        );
      }
      // CaptureLease has already stopped input and restored native/owner state.
      await _releaseMicrophone();
      if (!_current) await _release();
      _running = false;
      _micRunning = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _choose() async {
    if (!_current || !_foreground || _busy) return;
    setState(() {
      _choosing = true;
      _message = null;
      _rows = null;
    });
    try {
      await _release();
      final session = await api.beginRecognition();
      _session = session;
      if (!_current) return;
      await _readHistory(session);
      if (!_current) return;
      final selection = AudioFileSelection();
      _selection = selection;
      final ticket = await selection.choose(single: true);
      if (!_current) return;
      _ticket = ticket;
    } catch (_) {
      if (_current) {
        _message = t(
          'Could not select audio. Choose one supported file up to 128 MiB.',
          '无法选择音频，请选择一个受支持且不超过 128 MiB 的文件。',
        );
      }
    } finally {
      if (!_current) await _release();
      _choosing = false;
      if (mounted) setState(() {});
    }
  }

  String _recognitionError(Object error) {
    final message = error.toString();
    const categories = [
      'fingerprint_missing_binding',
      'fingerprint_initialize',
      'fingerprint_calculation',
      'fingerprint_bindings',
      'fingerprint_function',
      'fingerprint_memory',
      'fingerprint_module',
      'fingerprint_result',
      'capture_or_unavailable',
      'no_match',
      'silence',
      'service',
      'audio',
      'cancelled',
    ];
    final category = categories.firstWhere(
      (value) => message.contains('recognition_$value'),
      orElse: () => 'unavailable',
    );
    // Only a fixed category reaches the log; never include selected audio or
    // fingerprints, file paths, request URLs, or remote response contents.
    debugPrint('Mio recognition outcome: $category');
    if (category == 'no_match') {
      return t('No matching song found.', '未识别到匹配歌曲。');
    }
    if (category == 'silence') {
      return t(
        'Not enough sound was detected. Choose audio with clear music and try again.',
        '未检测到足够的声音，请选择包含清晰音乐的音频后重试。',
      );
    }
    if (category == 'audio') {
      return t(
        'The audio could not be read or decoded. Choose a supported file up to 128 MiB.',
        '无法读取或解码音频，请选择受支持且不超过 128 MiB 的文件。',
      );
    }
    if (category == 'service') {
      return t(
        'The recognition service is unavailable. Check your connection and try again later.',
        '识曲服务暂不可用，请检查网络连接后稍后重试。',
      );
    }
    if (category.startsWith('fingerprint_')) {
      return t(
        'The audio fingerprint could not be generated. Restart the app or choose another file.',
        '无法生成音频指纹，请重启应用或选择其他音频文件。',
      );
    }
    return t(
      'Recognition did not finish. Select audio or authorize the microphone again to retry.',
      '识曲未完成，请重新选择音频或授权麦克风后再试。',
    );
  }

  Future<void> _recognize() async {
    final session = _session, ticket = _ticket;
    if (!_current ||
        !_foreground ||
        _busy ||
        session == null ||
        ticket == null) {
      return;
    }
    setState(() {
      _running = true;
      _message = null;
    });
    try {
      final rows = await session.recognizeFile(ticket: ticket);
      if (!_current || !_foreground) return;
      _rows = rows;
      if (rows.isEmpty) _message = t('No matching song found.', '未识别到匹配歌曲。');
      await _readHistory(session);
    } catch (error) {
      if (_current) _message = _recognitionError(error);
    } finally {
      // Native staging must outlive the Rust worker, even after cancel/dispose.
      // Retain the original result session for playback; only staging is spent.
      await _discardSelection();
      if (!_current) await _release();
      _running = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> _play(
    api.RecognizedTrack row, {
    required bool jump,
    bool history = false,
  }) async {
    final session = _session;
    if (!_current || !_foreground || _busy || session == null) return;
    setState(() {
      _playing = true;
      _message = null;
    });
    try {
      if (history) {
        final view = _history;
        if (view == null) return;
        await session.playHistory(index: row.index, revision: view.revision);
      } else {
        await session.play(index: row.index, jumpToMatch: jump);
      }
      if (_current && _foreground) {
        _message = t('Playback requested.', '已请求播放。');
      }
    } catch (_) {
      if (_current) {
        _message = t(
          'Could not start playback. Enable a music source that supports the recognized song, then reopen recognition, or search for this song.',
          '无法开始播放。请启用支持该歌曲来源的音乐源后重新打开识曲，或搜索这首歌。',
        );
      }
    } finally {
      if (!_current) await _release();
      _playing = false;
      if (mounted) setState(() {});
    }
  }

  void _dismiss([String? query]) {
    if (query != null && (!_current || !_foreground)) return;
    _invalidate();
    if (widget.page) {
      if (query != null) widget.onSearch?.call(query);
    } else {
      Navigator.of(context).pop(query);
    }
  }

  bool _pageStopRequested = false;

  Future<void> _startPageMicrophone() async {
    _pageStopRequested = false;
    if (!_micAuthorized) await _authorizeMicrophone();
    if (_current && _foreground && _micAuthorized && !_busy) {
      await _recognizeMicrophone();
      if (_pageStopRequested &&
          _current &&
          mounted &&
          _session?.microphoneProgress().restoreFailed != true) {
        setState(() => _message = t('Recognition stopped.', '识别已停止。'));
      }
    }
  }

  Widget _pageRow(api.RecognizedTrack row, {bool history = false}) {
    final enabled = _current && _foreground && !_busy;
    final date = row.timestamp >= 0 && row.timestamp <= 8640000000000000
        ? DateTime.fromMillisecondsSinceEpoch(row.timestamp).toLocal()
        : null;
    String two(int value) => value.toString().padLeft(2, '0');
    final content = Row(
      children: [
        MioCover(row.track.cover, size: 44, radius: 6),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                row.track.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              Text(
                '${row.track.artist} · ${row.track.album}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 12, color: p.secondary),
              ),
              if (history && date != null)
                Text(
                  '${two(date.month)}/${two(date.day)} ${two(date.hour)}:${two(date.minute)}',
                  style: TextStyle(fontSize: 11, color: p.secondary),
                ),
            ],
          ),
        ),
        if (!history) ...[
          MioButton(
            palette: p,
            label: t('Play', '播放'),
            onPressed: enabled
                ? () => unawaited(_play(row, jump: row.startTimeMs > 0))
                : null,
            child: Text(t('Play', '播放')),
          ),
          MioButton(
            palette: p,
            label: t('Search', '搜索'),
            onPressed: enabled
                ? () => _dismiss('${row.track.title} ${row.track.artist}')
                : null,
            child: Text(t('Search', '搜索')),
          ),
        ],
      ],
    );
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 8),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: p.border)),
      ),
      child: history
          ? MioButton(
              palette: p,
              label: '${t('Play', '播放')} ${row.track.title}',
              padding: EdgeInsets.zero,
              onPressed: enabled
                  ? () => unawaited(_play(row, jump: false, history: true))
                  : null,
              child: content,
            )
          : content,
    );
  }

  Widget _pageView() {
    final enabled = _current && _foreground && !_busy;
    final hasResults = _rows?.isNotEmpty == true;
    final hasHistory =
        _history?.rows.isNotEmpty == true || _historyNotice != null;
    return FocusScope(
      node: _focus,
      child: DefaultTextStyle.merge(
        style: TextStyle(
          color: p.color('--td-text-color-primary'),
          fontSize: 14,
        ),
        child: ListView(
          padding: const EdgeInsets.all(28),
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 24, bottom: 16),
              child: Column(
                children: [
                  SizedBox.square(
                    dimension: 280,
                    child: Stack(
                      alignment: Alignment.center,
                      children: [
                        if (_micRunning)
                          for (final inset in [0.0, 16.0])
                            Positioned.fill(
                              left: inset,
                              right: inset,
                              top: inset,
                              bottom: inset,
                              child: DecoratedBox(
                                decoration: BoxDecoration(
                                  shape: BoxShape.circle,
                                  border: Border.all(
                                    width: inset == 0 ? 3 : 2,
                                    color: p.brand.withValues(
                                      alpha: inset == 0 ? .15 : .25,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                        Container(
                          width: 120,
                          height: 120,
                          decoration: const BoxDecoration(
                            shape: BoxShape.circle,
                            boxShadow: [
                              BoxShadow(
                                color: Color(0x26000000),
                                blurRadius: 8,
                                offset: Offset(0, 4),
                              ),
                            ],
                          ),
                          child: MioButton(
                            palette: p,
                            radius: 60,
                            padding: EdgeInsets.zero,
                            background: p.brand,
                            label: p.text(
                              _micRunning
                                  ? 'music.recognize.stopRecognize'
                                  : 'music.recognize.startRecognize',
                            ),
                            onPressed: _micRunning
                                ? () => setState(() {
                                    _pageStopRequested = true;
                                    _session?.close();
                                    _message = t('Stopping…', '正在停止…');
                                  })
                                : enabled && _micAvailable
                                ? () => unawaited(_startPageMicrophone())
                                : null,
                            child: Center(
                              child: MioIcon(
                                _busy ? 'TD-StopCircle' : 'TD-Microphone',
                                size: 48,
                                color: const Color(0xffffffff),
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 24),
                  if (_busy) ...[
                    Text(
                      p.text('music.recognize.recognizing'),
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        fontSize: 20,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      p.text('music.recognize.recognizingTip'),
                      textAlign: TextAlign.center,
                      style: TextStyle(color: p.secondary),
                    ),
                    const SizedBox(height: 12),
                    SizedBox(
                      width: 220,
                      height: 6,
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(3),
                        child: ColoredBox(
                          color: p.color('--td-bg-color-secondarycontainer'),
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: FractionallySizedBox(
                              widthFactor: ((_micProgress?.seconds ?? 0) / 15)
                                  .clamp(0.0, 1.0),
                              child: ColoredBox(
                                color: p.brand,
                                child: const SizedBox.expand(),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      '${_micProgress?.seconds ?? 0}s / 15s',
                      style: const TextStyle(fontSize: 12),
                    ),
                  ] else if (!hasResults) ...[
                    Text(
                      p.text('music.recognize.tapToStart'),
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        fontSize: 24,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      t('Recognize music playing around you', '识别周围正在播放的声音'),
                      textAlign: TextAlign.center,
                      style: TextStyle(fontSize: 14, color: p.secondary),
                    ),
                    const SizedBox(height: 16),
                    MioButton(
                      palette: p,
                      label: p.text('music.recognize.orUpload'),
                      onPressed: enabled ? () => unawaited(_choose()) : null,
                      child: Text(p.text('music.recognize.orUpload')),
                    ),
                    if (_ticket != null)
                      MioButton(
                        palette: p,
                        label: t('Send fingerprint and recognize', '发送指纹并识别'),
                        onPressed: enabled
                            ? () => unawaited(_recognize())
                            : null,
                        child: Text(
                          t('Send fingerprint and recognize', '发送指纹并识别'),
                        ),
                      ),
                    const SizedBox(height: 8),
                    Text(
                      t(
                        'Up to 15 seconds. Only the audio fingerprint is sent.',
                        '最多录音15秒，仅发送音频指纹。',
                      ),
                      textAlign: TextAlign.center,
                      style: TextStyle(fontSize: 12, color: p.secondary),
                    ),
                  ],
                  if (_message != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Semantics(
                        liveRegion: true,
                        child: Text(
                          _message!,
                          textAlign: TextAlign.center,
                          style: const TextStyle(fontSize: 13),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            if (hasResults) ...[
              for (final row in _rows!) _pageRow(row),
              const SizedBox(height: 20),
              Align(
                alignment: Alignment.centerLeft,
                child: MioButton(
                  palette: p,
                  label: p.text('music.recognize.continueRecognize'),
                  onPressed: enabled
                      ? () => setState(() {
                          _rows = null;
                          _message = null;
                        })
                      : null,
                  child: Text(p.text('music.recognize.continueRecognize')),
                ),
              ),
            ],
            if (hasHistory) ...[
              const SizedBox(height: 20),
              Row(
                children: [
                  Expanded(child: Text(p.text('music.recognize.history'))),
                  if (_history?.canClear == true)
                    MioButton(
                      palette: p,
                      label: p.text('music.recognize.clearHistory'),
                      onPressed: enabled
                          ? () => setState(() => _confirmClear = !_confirmClear)
                          : null,
                      child: Text(p.text('music.recognize.clearHistory')),
                    ),
                ],
              ),
              if (_historyNotice != null) Text(_historyNotice!),
              if (_confirmClear) ...[
                Text(t('Clear the local recognition history?', '确认清空本机识曲历史？')),
                Wrap(
                  spacing: 12,
                  children: [
                    MioButton(
                      palette: p,
                      label: t('Confirm clear', '确认清空'),
                      onPressed: enabled
                          ? () => unawaited(_clearHistory())
                          : null,
                      child: Text(t('Confirm clear', '确认清空')),
                    ),
                    MioButton(
                      palette: p,
                      label: t('Cancel', '取消'),
                      onPressed: () => setState(() => _confirmClear = false),
                      child: Text(t('Cancel', '取消')),
                    ),
                  ],
                ),
              ],
              for (final row in _history?.rows ?? <api.RecognizedTrack>[])
                _pageRow(row, history: true),
            ],
          ],
        ),
      ),
    );
  }

  @override
  void dispose() {
    widget.core.removeListener(_coreChanged);
    WidgetsBinding.instance.removeObserver(this);
    _invalidate();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      widget.page ? _pageView() : _dialogView(context);

  Widget _dialogView(BuildContext context) => PopScope(
    canPop: true,
    onPopInvokedWithResult: (didPop, result) {
      if (didPop) _invalidate();
    },
    child: SafeArea(
      child: Center(
        child: MioDialogScope(
          node: _focus,
          child: Container(
            constraints: BoxConstraints(
              maxWidth: 640,
              maxHeight: MediaQuery.sizeOf(context).height * .85,
            ),
            margin: const EdgeInsets.all(16),
            padding: const EdgeInsets.all(24),
            decoration: BoxDecoration(
              color: p.surface,
              borderRadius: BorderRadius.circular(12),
            ),
            child: DefaultTextStyle.merge(
              // Keep text paired with this route's captured surface when playback
              // updates the surrounding cover theme.
              style: TextStyle(color: p.color('--td-text-color-primary')),
              child: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      t('Recognize music', '识曲'),
                      style: const TextStyle(
                        fontSize: 22,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 16),
                    Text(
                      t(
                        'Choose one audio file (up to 128 MiB). Recognition analyzes up to the first 15 seconds locally and sends an audio fingerprint to the service used by the recognition plugin. The audio file itself is not uploaded.',
                        '选择一个音频文件（不超过 128 MiB）。识曲将在本机分析开头最多 15 秒，向识曲插件所用的服务发送音频指纹，不上传音频文件本身。',
                      ),
                    ),
                    const SizedBox(height: 16),
                    Wrap(
                      spacing: 12,
                      runSpacing: 12,
                      children: [
                        MioButton(
                          palette: p,
                          label: t('Choose audio', '选择音频'),
                          onPressed: _current && _foreground && !_busy
                              ? () => unawaited(_choose())
                              : null,
                          child: Text(t('Choose audio', '选择音频')),
                        ),
                        MioButton(
                          palette: p,
                          label: t('Send fingerprint and recognize', '发送指纹并识别'),
                          onPressed:
                              _current &&
                                  _foreground &&
                                  !_busy &&
                                  _ticket != null
                              ? () => unawaited(_recognize())
                              : null,
                          child: Text(
                            t('Send fingerprint and recognize', '发送指纹并识别'),
                          ),
                        ),
                        MioButton(
                          palette: p,
                          label: t('Close', '关闭'),
                          onPressed: () => _dismiss(),
                          child: Text(t('Close', '关闭')),
                        ),
                      ],
                    ),
                    if (_micAvailable) ...[
                      const SizedBox(height: 16),
                      Text(
                        t(
                          'Microphone recognition records for up to 15 seconds and sends an audio fingerprint to the service used by the recognition plugin. Recording stops when the app leaves the foreground. Current playback pauses during recording.',
                          '麦克风识曲最多录音 15 秒，并向识曲插件所用的服务发送音频指纹。离开前台会停止录音；录音期间会暂停当前播放。',
                        ),
                      ),
                      const SizedBox(height: 8),
                      Wrap(
                        spacing: 12,
                        runSpacing: 8,
                        children: [
                          MioButton(
                            palette: p,
                            label: t('Authorize microphone', '授权麦克风'),
                            onPressed: _current && _foreground && !_busy
                                ? () => unawaited(_authorizeMicrophone())
                                : null,
                            child: Text(t('Authorize microphone', '授权麦克风')),
                          ),
                          MioButton(
                            palette: p,
                            label: t('Record and send fingerprint', '录音并发送指纹'),
                            onPressed:
                                _current &&
                                    _foreground &&
                                    !_busy &&
                                    _micAuthorized
                                ? () => unawaited(_recognizeMicrophone())
                                : null,
                            child: Text(
                              t('Record and send fingerprint', '录音并发送指纹'),
                            ),
                          ),
                        ],
                      ),
                      if (_micRunning)
                        Padding(
                          padding: const EdgeInsets.only(top: 8),
                          child: Text(
                            _micProgress?.capturing == true
                                ? t(
                                    'Recording: ${_micProgress!.seconds}/15 s · level ${_micProgress!.level}%',
                                    '正在录音：${_micProgress!.seconds}/15 秒 · 音量 ${_micProgress!.level}%',
                                  )
                                : t('Preparing or recognizing…', '正在准备或识别…'),
                          ),
                        ),
                    ],
                    if (_busy || _ticket != null || _message != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 16),
                        child: Semantics(
                          liveRegion: true,
                          child: Text(
                            _message ??
                                (_loading
                                    ? t('Loading history…', '正在读取历史…')
                                    : _clearing
                                    ? t('Clearing history…', '正在清空历史…')
                                    : _playing
                                    ? t('Starting playback…', '正在请求播放…')
                                    : _running
                                    ? t('Recognizing…', '正在识别…')
                                    : _choosing
                                    ? t('Selecting…', '正在选择…')
                                    : t(
                                        'Audio selected. Confirm above to send its fingerprint.',
                                        '已选择音频，点击上方按钮确认发送指纹。',
                                      )),
                          ),
                        ),
                      ),
                    if (_rows != null && _rows!.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: 24),
                        child: Text(
                          t('Recognition results', '识曲结果'),
                          style: const TextStyle(fontWeight: FontWeight.w600),
                        ),
                      ),
                    for (final row in _rows ?? <api.RecognizedTrack>[])
                      Padding(
                        padding: const EdgeInsets.only(top: 16),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              row.track.title,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            Text('${row.track.artist} · ${row.track.album}'),
                            Text(
                              t(
                                'Matched position: ${row.startTimeMs ~/ 1000} s',
                                '匹配位置：${row.startTimeMs ~/ 1000} 秒',
                              ),
                            ),
                            const SizedBox(height: 6),
                            Wrap(
                              spacing: 12,
                              runSpacing: 8,
                              children: [
                                MioButton(
                                  palette: p,
                                  label: t('Play', '播放'),
                                  onPressed: _current && _foreground && !_busy
                                      ? () => unawaited(_play(row, jump: false))
                                      : null,
                                  child: Text(t('Play', '播放')),
                                ),
                                if (row.startTimeMs > 0)
                                  MioButton(
                                    palette: p,
                                    label: t(
                                      'Play from matched position',
                                      '从匹配位置播放',
                                    ),
                                    onPressed: _current && _foreground && !_busy
                                        ? () =>
                                              unawaited(_play(row, jump: true))
                                        : null,
                                    child: Text(
                                      t(
                                        'Play from matched position',
                                        '从匹配位置播放',
                                      ),
                                    ),
                                  ),
                              ],
                            ),
                            const SizedBox(height: 6),
                            MioButton(
                              palette: p,
                              label: t('Search this song', '搜索这首歌'),
                              onPressed: _current && _foreground
                                  ? () => _dismiss(
                                      '${row.track.title} ${row.track.artist}',
                                    )
                                  : null,
                              child: Text(t('Search this song', '搜索这首歌')),
                            ),
                          ],
                        ),
                      ),
                    const SizedBox(height: 24),
                    Text(
                      t('Recognition history', '识曲历史'),
                      style: const TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      t(
                        'The most recent 20 matches are stored locally.',
                        '最近 20 条匹配记录保存在本机。',
                      ),
                    ),
                    if (_historyNotice != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Semantics(
                          liveRegion: true,
                          child: Text(_historyNotice!),
                        ),
                      ),
                    if (_history != null &&
                        _history!.rows.isEmpty &&
                        _history!.warning == null)
                      Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Text(t('No recognition history.', '暂无识曲历史。')),
                      ),
                    if (_history?.canClear == true &&
                        (_history!.rows.isNotEmpty ||
                            _history!.warning != null)) ...[
                      const SizedBox(height: 8),
                      MioButton(
                        palette: p,
                        label: t('Clear history', '清空历史'),
                        onPressed: _current && _foreground && !_busy
                            ? () =>
                                  setState(() => _confirmClear = !_confirmClear)
                            : null,
                        child: Text(t('Clear history', '清空历史')),
                      ),
                      if (_confirmClear) ...[
                        const SizedBox(height: 8),
                        Text(
                          t(
                            'Permanently clear the local recognition history? The audio files and current recognition results are kept.',
                            '确认清空本机识曲历史？音频文件和本次识曲结果会保留。',
                          ),
                        ),
                        const SizedBox(height: 8),
                        Wrap(
                          spacing: 12,
                          runSpacing: 8,
                          children: [
                            MioButton(
                              palette: p,
                              label: t('Confirm clear', '确认清空'),
                              onPressed: _current && _foreground && !_busy
                                  ? () => unawaited(_clearHistory())
                                  : null,
                              child: Text(t('Confirm clear', '确认清空')),
                            ),
                            MioButton(
                              palette: p,
                              label: t('Cancel', '取消'),
                              onPressed: () =>
                                  setState(() => _confirmClear = false),
                              child: Text(t('Cancel', '取消')),
                            ),
                          ],
                        ),
                      ],
                    ],
                    for (final row in _history?.rows ?? <api.RecognizedTrack>[])
                      Padding(
                        padding: const EdgeInsets.only(top: 16),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              row.track.title,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            Text('${row.track.artist} · ${row.track.album}'),
                            if (row.timestamp >= 0 &&
                                row.timestamp <= 8640000000000000)
                              Text(
                                DateTime.fromMillisecondsSinceEpoch(
                                  row.timestamp,
                                ).toLocal().toString(),
                              ),
                            const SizedBox(height: 6),
                            Wrap(
                              spacing: 12,
                              runSpacing: 8,
                              children: [
                                MioButton(
                                  palette: p,
                                  label: t('Play', '播放'),
                                  onPressed: _current && _foreground && !_busy
                                      ? () => unawaited(
                                          _play(
                                            row,
                                            jump: false,
                                            history: true,
                                          ),
                                        )
                                      : null,
                                  child: Text(t('Play', '播放')),
                                ),
                                MioButton(
                                  palette: p,
                                  label: t('Search this song', '搜索这首歌'),
                                  onPressed: _current && _foreground && !_busy
                                      ? () => _dismiss(
                                          '${row.track.title} ${row.track.artist}',
                                        )
                                      : null,
                                  child: Text(t('Search this song', '搜索这首歌')),
                                ),
                              ],
                            ),
                          ],
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
  );
}

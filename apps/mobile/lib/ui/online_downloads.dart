import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'download_quality.dart';
import 'resources.dart';

class OnlineDownloadReport {
  int added = 0, duplicate = 0, done = 0, skipped = 0;
  int remaining;
  bool failed = false;
  OnlineDownloadReport(this.remaining);

  String message(MioPalette p) => p.english
      ? 'Added $added, queued $duplicate, downloaded $done, skipped $skipped, remaining $remaining.${failed ? ' Download unavailable; reopen this page to continue.' : ''}'
      : '已添加 $added 首，已在队列 $duplicate 首，已下载 $done 首，跳过 $skipped 首，剩余 $remaining 首。${failed ? '下载不可用，请重新打开页面后继续。' : ''}';
}

/// Cancels only pending UI choices; accepted tasks belong to the core manager.
class _DownloadRun with WidgetsBindingObserver {
  bool active = true;
  api.DownloadChoice? choice;
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) {
      active = false;
      choice?.close();
    }
  }
}

/// Match desktop's per-song quality choices, last loaded song first. The parent
/// gates paging/navigation for the duration; Rust resolves each exact retained ID.
Future<OnlineDownloadReport> showOnlineDownloads({
  required BuildContext context,
  required api.OnlinePlaylistSession session,
  required List<api.Track> tracks,
  required CoreController core,
  required MioPalette palette,
  required bool Function() isCurrent,
}) async {
  final ids = tracks.reversed.map((track) => track.id).toList(growable: false);
  return _showDownloads(
    context: context,
    count: ids.length,
    prepare: (index) => session.prepareDownload(id: ids[index]),
    core: core,
    palette: palette,
    isCurrent: isCurrent,
    complete: false,
  );
}

Future<OnlineDownloadReport> showCompleteDownloadChoices({
  required BuildContext context,
  required api.OnlineDownloadCollection collection,
  required int count,
  required CoreController core,
  required MioPalette palette,
  required bool Function() isCurrent,
}) => _showDownloads(
  context: context,
  count: count,
  prepare: (index) => collection.prepareDownload(index: count - index - 1),
  core: core,
  palette: palette,
  isCurrent: isCurrent,
  complete: true,
);

Future<OnlineDownloadReport> _showDownloads({
  required BuildContext context,
  required int count,
  required Future<api.DownloadChoice> Function(int) prepare,
  required CoreController core,
  required MioPalette palette,
  required bool Function() isCurrent,
  required bool complete,
}) async {
  final report = OnlineDownloadReport(count);
  final run = _DownloadRun();
  final lifecycle = WidgetsBinding.instance.lifecycleState;
  run.active = lifecycle == null || lifecycle == AppLifecycleState.resumed;
  WidgetsBinding.instance.addObserver(run);
  try {
    for (var i = 0; i < count; i++) {
      if (!run.active || !context.mounted || !isCurrent()) break;
      api.DownloadChoice? choice;
      try {
        choice = await prepare(i);
        run.choice = choice;
        if (!run.active || !context.mounted || !isCurrent()) break;
        final info = await choice.info();
        if (!run.active || !context.mounted || !isCurrent()) break;
        final result = await showDownloadChoice(
          context: context,
          choice: choice,
          info: info,
          core: core,
          palette: palette,
          batchLabel: palette.english
              ? '${i + 1} / $count ${complete ? 'complete collection' : 'loaded'} songs. Choose a quality for this song.'
              : '${complete ? '完整内容' : '已加载'} $count 首中的第 ${i + 1} 首，请选择此歌曲的音质。',
          onStop: () => run.active = false,
        );
        // Accepted results are final even if the app went into the background.
        if (result != null) {
          switch (result) {
            case api.DownloadEnqueueResult.added:
              report.added++;
            case api.DownloadEnqueueResult.duplicate:
              report.duplicate++;
            case api.DownloadEnqueueResult.alreadyDone:
              report.done++;
          }
          report.remaining--;
        } else if (run.active && isCurrent()) {
          report.skipped++;
          report.remaining--;
        }
      } catch (_) {
        report.failed = true;
        break;
      } finally {
        run.choice = null;
        choice?.close();
        choice?.dispose();
      }
    }
  } finally {
    run.active = false;
    WidgetsBinding.instance.removeObserver(run);
  }
  return report;
}

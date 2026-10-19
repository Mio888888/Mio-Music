import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core_controller.dart';
import 'controls.dart';
import 'glass_surface.dart';
import 'resources.dart';

enum PlayChoice { now, queue }

/// Tapping a song plays it straight away when nothing is playing. While a track
/// is already loaded, the user picks between replacing the queue and adding it.
/// Returns null when the dialog is dismissed.
Future<PlayChoice?> choosePlayChoice({
  required BuildContext context,
  required MioPalette palette,
  required CoreController core,
  required String title,
}) async {
  if (core.player?.current == null) return PlayChoice.now;
  final route = RawDialogRoute<PlayChoice>(
    barrierDismissible: true,
    barrierLabel: palette.text('common.close'),
    barrierColor: const Color(0x66000000),
    transitionDuration: const Duration(milliseconds: 160),
    transitionBuilder: (context, animation, _, child) => FadeTransition(
      opacity: animation,
      child: ScaleTransition(
        scale: Tween<double>(begin: .96, end: 1).animate(animation),
        child: child,
      ),
    ),
    pageBuilder: (_, _, _) => _PlayChoiceDialog(palette: palette, title: title),
  );
  final result = await Navigator.of(context).push<PlayChoice>(route);
  await route.completed;
  return result;
}

/// A short, non-blocking confirmation shown near the bottom of the screen.
void showMioToast(BuildContext context, MioPalette palette, String text) {
  final overlay = Overlay.maybeOf(context, rootOverlay: true);
  if (overlay == null) return;
  late final OverlayEntry entry;
  entry = OverlayEntry(
    builder: (context) => Positioned(
      left: 24,
      right: 24,
      bottom: 150,
      child: IgnorePointer(
        child: Center(
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: const Color(0xd91a1a1a),
              borderRadius: BorderRadius.circular(999),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
              child: Text(
                text,
                style: const TextStyle(
                  color: Color(0xffffffff),
                  fontSize: 14,
                  decoration: TextDecoration.none,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  overlay.insert(entry);
  Timer(const Duration(milliseconds: 1800), entry.remove);
}

class _PlayChoiceDialog extends StatelessWidget {
  final MioPalette palette;
  final String title;
  const _PlayChoiceDialog({required this.palette, required this.title});

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final primary = p.color('--td-text-color-primary');
    Widget option(String label, String icon, PlayChoice value, bool filled) =>
        SizedBox(
          height: 48,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: filled ? p.brand : null,
              borderRadius: BorderRadius.circular(999),
              border: filled
                  ? null
                  : Border.all(color: primary.withValues(alpha: .18)),
            ),
            child: MioButton(
              palette: p,
              label: label,
              radius: 999,
              insetFocusBorder: false,
              autofocus: filled,
              padding: const EdgeInsets.symmetric(horizontal: 16),
              onPressed: () => Navigator.of(context).pop(value),
              child: Center(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    MioIcon(
                      icon,
                      size: 18,
                      color: filled ? p.color('--td-text-color-anti') : primary,
                    ),
                    const SizedBox(width: 8),
                    Text(
                      label,
                      style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w600,
                        color: filled
                            ? p.color('--td-text-color-anti')
                            : primary,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            Navigator.of(context).pop(),
      },
      child: FocusScope(
        autofocus: true,
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          explicitChildNodes: true,
          label: p.english ? 'Play song' : '播放歌曲',
          child: DefaultTextStyle(
            style: TextStyle(
              color: primary,
              fontSize: 14,
              decoration: TextDecoration.none,
            ),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 320),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(22),
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
                      radius: 22,
                      tint: p.surface.withValues(alpha: p.dark ? .55 : .5),
                      child: Padding(
                        padding: const EdgeInsets.all(20),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Text(
                              title,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              textAlign: TextAlign.center,
                              style: const TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              p.english
                                  ? 'Something is already playing'
                                  : '当前已有歌曲在播放',
                              textAlign: TextAlign.center,
                              style: TextStyle(
                                fontSize: 12,
                                color: p.secondary,
                              ),
                            ),
                            const SizedBox(height: 16),
                            option(
                              p.english ? 'Play now' : '立即播放',
                              'Play',
                              PlayChoice.now,
                              true,
                            ),
                            const SizedBox(height: 10),
                            option(
                              p.english ? 'Add to queue' : '加入播放列表',
                              'nav-zengjia',
                              PlayChoice.queue,
                              false,
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
      ),
    );
  }
}

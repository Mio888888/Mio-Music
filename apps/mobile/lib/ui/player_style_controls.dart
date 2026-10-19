import 'dart:async';

import 'package:flutter/widgets.dart';

import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'device.dart';
import 'resources.dart';
import 'visual_policy.dart';
import 'wide_auto_hide.dart';

/// The in-player presentation shares the visual settings page's Rust session.
class PlayerStyleControls extends StatelessWidget {
  final MioPalette palette;
  final api.VisualSettings? value;
  final bool busy;
  final String? error;
  final VoidCallback? onReload;
  final ValueChanged<bool> onCover;
  final VoidCallback onLeft, onColor, onBlur, onSpectrum, onAutoHide, onPause;
  const PlayerStyleControls({
    super.key,
    required this.palette,
    required this.value,
    required this.busy,
    required this.error,
    required this.onReload,
    required this.onCover,
    required this.onLeft,
    required this.onColor,
    required this.onBlur,
    required this.onSpectrum,
    required this.onAutoHide,
    required this.onPause,
  });

  String t(String key) => palette.text('play.$key');
  Widget _heading(String key, {bool compact = false}) => Padding(
    padding: EdgeInsets.only(bottom: compact ? 10 : 20),
    child: Text(
      t(key),
      style: TextStyle(
        fontSize: compact ? 16 : 18,
        height: compact ? 24 / 16 : 28 / 18,
        fontWeight: FontWeight.w600,
        color: Color(0xf2ffffff),
      ),
    ),
  );

  Widget _card(bool cover, {bool compact = false}) {
    final selected = value?.coverLayout == cover;
    final label = t(cover ? 'immersiveCover' : 'classicVinyl');
    return Expanded(
      child: Semantics(
        selected: selected,
        child: Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(16),
            border: Border.all(
              width: 2,
              color: selected
                  ? const Color(0xe6ffffff)
                  : const Color(0x00000000),
            ),
          ),
          child: MioButton(
            palette: palette,
            label: label,
            radius: 14,
            background: Color(selected ? 0x26ffffff : 0x0dffffff),
            padding: EdgeInsets.all(compact ? 8 : 14),
            onPressed: busy ? null : () => onCover(cover),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  padding: EdgeInsets.all(compact ? 6 : 10),
                  decoration: BoxDecoration(
                    color: const Color(0x33000000),
                    borderRadius: BorderRadius.circular(compact ? 8 : 10),
                  ),
                  child: Image.asset(
                    'assets/images/${cover ? 'cover-play' : 'cd'}.png',
                    fit: BoxFit.contain,
                  ),
                ),
                SizedBox(height: compact ? 6 : 12),
                Text(
                  label,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: compact ? 13 : 14,
                    height: compact ? 20 / 13 : 22 / 14,
                    fontWeight: FontWeight.w500,
                    color: Color(0xccffffff),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _row(
    BuildContext context,
    String key,
    bool checked,
    VoidCallback action,
  ) => PlayerStyleSwitch(
    palette: palette,
    label: t(key),
    checked: checked,
    onPressed: busy ? null : action,
  );

  @override
  Widget build(BuildContext context) {
    final compact = !MioDeviceScope.wide(
      context,
      MediaQuery.sizeOf(context).width,
    );
    final section = compact ? 14.0 : 24.0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _heading('playerStyle', compact: compact),
        if (error != null) ...[
          Text(error!, style: const TextStyle(color: Color(0xffffb4b4))),
          MioButton(
            palette: palette,
            label: palette.text('visuals.reload'),
            onPressed: onReload,
            child: Text(palette.text('visuals.reload')),
          ),
        ],
        if (value == null && error == null)
          Text(palette.text('visuals.working')),
        if (value != null) ...[
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _card(false, compact: compact),
              SizedBox(width: compact ? 10 : 16),
              _card(true, compact: compact),
            ],
          ),
          SizedBox(height: section),
          _heading('uiSettings', compact: compact),
          _row(context, 'showLeftPanel', value!.showLeftPanel, onLeft),
          _row(
            context,
            'immersiveLyricColor',
            value!.immersiveLyricColor,
            onColor,
          ),
          _row(context, 'lyricBlurEffect', value!.blurLyrics, onBlur),
          _row(context, 'audioVisualization', value!.spectrum, onSpectrum),
          // The bars hide only when this local preference is on (off by default).
          ValueListenableBuilder<bool>(
            valueListenable: MioWideAutoHide.value,
            builder: (context, enabled, _) => _row(
              context,
              'autoHideControlBar',
              enabled,
              () => unawaited(MioWideAutoHide.set(!enabled)),
            ),
          ),
          SizedBox(height: section),
          _heading('playbackSettings', compact: compact),
          _row(context, 'pauseTransition', value!.pauseTransition, onPause),
        ],
      ],
    );
  }
}

class PlayerStyleSwitch extends StatelessWidget {
  final MioPalette palette;
  final String label;
  final bool checked;
  final VoidCallback? onPressed;
  final FocusNode? focusNode;
  const PlayerStyleSwitch({
    super.key,
    required this.palette,
    required this.label,
    required this.checked,
    required this.onPressed,
    this.focusNode,
  });
  // Desktop rows are 30px tall (4 + 22 + 4); the whole row toggles so the
  // shorter switch keeps a comfortable touch target.
  @override
  Widget build(BuildContext context) {
    final wide = MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      excludeFromSemantics: true,
      onTap: onPressed,
      child: Padding(
        // The focus border adds 2px on each side of the 22px switch.
        padding: EdgeInsets.symmetric(horizontal: 8, vertical: wide ? 2 : 5),
        child: Row(
          children: [
            Expanded(
              child: Text(
                label,
                style: const TextStyle(
                  fontSize: 14,
                  height: 22 / 14,
                  fontWeight: FontWeight.w500,
                  color: Color(0xccf1f1f1),
                ),
              ),
            ),
            const SizedBox(width: 12),
            Semantics(
              toggled: checked,
              child: MioButton(
                palette: palette,
                label: label,
                radius: 11,
                padding: EdgeInsets.zero,
                onPressed: onPressed,
                focusNode: focusNode,
                child: SizedBox(
                  width: 44,
                  height: 22,
                  child: Center(
                    child: Container(
                      width: 20 / .618,
                      height: 20,
                      decoration: BoxDecoration(
                        color: Color(checked ? 0x26ffffff : 0xff4b4b4b),
                        borderRadius: BorderRadius.circular(10),
                      ),
                      child: AnimatedAlign(
                        duration:
                            MediaQuery.disableAnimationsOf(context) ||
                                MioVisualPolicy.of(context).reduced
                            ? Duration.zero
                            : const Duration(milliseconds: 200),
                        curve: const Cubic(.38, 0, .24, 1),
                        alignment: checked
                            ? Alignment.centerRight
                            : Alignment.centerLeft,
                        child: Padding(
                          padding: EdgeInsets.symmetric(
                            horizontal: checked ? 2.5 : 4,
                          ),
                          child: Container(
                            width: checked ? 15 : 12,
                            height: checked ? 15 : 12,
                            decoration: const BoxDecoration(
                              shape: BoxShape.circle,
                              color: Color(0xffffffff),
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
        ),
      ),
    );
  }
}

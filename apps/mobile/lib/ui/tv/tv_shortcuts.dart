part of '../shell.dart';

/// Remote-control shortcuts that work on every TV screen: three Up presses that
/// have nowhere left to go raise the full player, and the remote's Settings key
/// opens (and leaves) the settings page.
extension _TvShortcuts on _MioShellState {
  bool _tvRemoteKey(KeyEvent event) {
    if (event is! KeyDownEvent || !mounted || !MioDeviceScope.isTv(context)) {
      return false;
    }
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.settings) {
      if (_shortcutsBlocked) return false;
      _tvSettingsKey();
      return true;
    }
    if (key == LogicalKeyboardKey.arrowUp) _watchBlockedUp();
    return false;
  }

  /// Dialogs, the queue and the full player own the remote while they are open.
  bool get _shortcutsBlocked =>
      _queueOpen ||
      _backupOpen ||
      _recognitionOpen ||
      ModalRoute.of(context)?.isCurrent == false;

  void _tvSettingsKey() {
    if (_fullPlayer) {
      _openPlayerAppearance();
    } else if (_MioShellState._settingsPages.contains(_page)) {
      _headerBack();
    } else {
      _go(_Page.settings);
    }
  }

  /// Up counts when it moves no focus (nothing above) or lands on the top bar.
  /// Scrolling up through the rows of a list never counts.
  void _watchBlockedUp() {
    final origin = FocusManager.instance.primaryFocus;
    final editing =
        origin?.context?.findAncestorWidgetOfExactType<EditableText>() != null;
    if (_fullPlayer || _shortcutsBlocked || editing) {
      _blockedUps.clear();
      return;
    }
    // Focus moves after this handler runs, so look a moment later.
    Future<void>.delayed(const Duration(milliseconds: 40), () {
      if (!mounted) return;
      final now = DateTime.now();
      final focus = FocusManager.instance.primaryFocus;
      // Arriving at the top bar counts too, so three presses from the content
      // below still make it; moving between rows lower down does not.
      final atTop = focus != null && focus.hasFocus && focus.rect.top < 90;
      if (focus != origin && !atTop) {
        _blockedUps.clear();
        return;
      }
      _blockedUps.removeWhere(
        (time) => now.difference(time) > const Duration(milliseconds: 1500),
      );
      _blockedUps.add(now);
      if (_blockedUps.length >= 3) {
        _blockedUps.clear();
        _openPlayer();
      }
    });
  }
}

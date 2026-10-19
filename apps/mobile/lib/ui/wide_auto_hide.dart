import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

/// Flutter layouts (wide and narrow) keep the player bars on screen unless the
/// user opts in from the player style panel. The desktop default (hide) lives
/// in the shared core, so this one preference is kept locally and defaults off.
class MioWideAutoHide {
  static final value = ValueNotifier<bool>(false);
  static Future<void>? _loading;

  static Future<File> _file() async =>
      File('${(await getApplicationSupportDirectory()).path}/wide-auto-hide');

  static Future<void> ensureLoaded() => _loading ??= () async {
    try {
      final file = await _file();
      if (await file.exists()) {
        value.value = (await file.readAsString()).trim() == '1';
      }
    } catch (_) {
      // Keep the default (off) when the preference cannot be read.
    }
  }();

  static Future<void> set(bool enabled) async {
    value.value = enabled;
    try {
      await (await _file()).writeAsString(enabled ? '1' : '0');
    } catch (_) {
      // The in-memory value still applies for this session.
    }
  }
}

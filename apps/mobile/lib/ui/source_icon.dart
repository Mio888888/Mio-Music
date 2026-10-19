import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';

import 'controls.dart';

/// Same plugin icon forms as desktop, with bounded inline PNG dimensions.
class SourceIcon extends StatelessWidget {
  // Image.memory keys on the byte buffer's identity. Decoding on every build
  // gave the icon a new key on each player tick (a flicker), so keep one buffer
  // per icon string.
  static final _decoded = <String, Uint8List>{};
  static Uint8List _bytesFor(String icon, String prefix) {
    final cached = _decoded.remove(icon);
    final bytes = cached ?? base64Decode(icon.substring(prefix.length));
    _decoded[icon] = bytes;
    if (_decoded.length > 16) _decoded.remove(_decoded.keys.first);
    return bytes;
  }

  final String? value;
  final Color color;
  final double size;
  const SourceIcon({
    super.key,
    this.value,
    required this.color,
    this.size = 20,
  });

  @override
  Widget build(BuildContext context) {
    final icon = value;
    Widget fallback() => MioIcon('nav-liebiao', size: size, color: color);
    if (icon == null || icon.length > 350000) return fallback();
    Widget failed(BuildContext _, Object _, StackTrace? _) => fallback();
    const prefix = 'data:image/png;base64,';
    if (icon.startsWith(prefix)) {
      try {
        final bytes = _bytesFor(icon, prefix);
        const signature = [137, 80, 78, 71, 13, 10, 26, 10];
        if (bytes.length < 33 ||
            bytes.length > 256 * 1024 ||
            !List.generate(
              8,
              (i) => bytes[i] == signature[i],
            ).every((v) => v) ||
            ascii.decode(bytes.sublist(12, 16)) != 'IHDR') {
          return fallback();
        }
        final header = ByteData.sublistView(bytes);
        final width = header.getUint32(16), height = header.getUint32(20);
        if (width < 1 || width > 512 || height < 1 || height > 512) {
          return fallback();
        }
        return Image.memory(
          bytes,
          width: size,
          height: size,
          fit: BoxFit.contain,
          cacheWidth: 40,
          cacheHeight: 40,
          errorBuilder: failed,
        );
      } catch (_) {
        return fallback();
      }
    }
    final uri = icon.length <= 2048 ? Uri.tryParse(icon) : null;
    if (uri != null &&
        (uri.scheme == 'http' || uri.scheme == 'https') &&
        uri.host.isNotEmpty &&
        uri.userInfo.isEmpty &&
        !icon.contains(RegExp(r'[\x00-\x1f\x7f]'))) {
      return Image.network(
        icon,
        width: size,
        height: size,
        fit: BoxFit.contain,
        cacheWidth: 40,
        cacheHeight: 40,
        errorBuilder: failed,
      );
    }
    if (utf8.encode(icon).length <= 32 &&
        !icon.contains(RegExp(r'[\x00-\x1f\x7f]')) &&
        !icon.contains('://')) {
      return SizedBox(
        width: size,
        height: size,
        child: FittedBox(
          child: Text(
            icon,
            style: TextStyle(fontSize: size, color: color),
          ),
        ),
      );
    }
    return fallback();
  }
}

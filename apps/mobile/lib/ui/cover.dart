import 'package:flutter/widgets.dart';

class MioCover extends StatelessWidget {
  final String? source;
  final double size;
  final double radius;

  /// Decode size when it must stay the same while the displayed size changes
  /// (a shrinking header reuses the already decoded image instead of refetching).
  final double? cacheSize;
  const MioCover(
    this.source, {
    super.key,
    required this.size,
    this.radius = 8,
    this.cacheSize,
  });
  @override
  Widget build(BuildContext context) {
    final uri = source == null ? null : Uri.tryParse(source!);
    final cacheWidth =
        ((cacheSize ?? size) * MediaQuery.devicePixelRatioOf(context)).ceil();
    final fallback = Image.asset(
      'assets/images/default-cover.png',
      width: size,
      height: size,
      fit: BoxFit.cover,
      cacheWidth: cacheWidth,
    );
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: uri != null && ['http', 'https'].contains(uri.scheme)
          ? Image.network(
              source!,
              width: size,
              height: size,
              fit: BoxFit.cover,
              cacheWidth: cacheWidth,
              gaplessPlayback: true,
              errorBuilder: (context, error, stack) => fallback,
            )
          : fallback,
    );
  }
}

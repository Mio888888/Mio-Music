import 'package:flutter/widgets.dart';

import '../controls.dart';
import '../resources.dart';

/// One destination in the TV top navigation.
typedef TvNavTab = ({
  String icon,
  String label,
  bool active,
  VoidCallback onPressed,
});

/// Horizontal console-style navigation that replaces the desktop sidebar and
/// header on TV: destinations on the left, global actions on the right.
class TvTopBar extends StatelessWidget {
  final MioPalette palette;
  final List<TvNavTab> tabs;
  final List<Widget> actions;
  final bool autofocusFirst;
  const TvTopBar({
    super.key,
    required this.palette,
    required this.tabs,
    required this.actions,
    this.autofocusFirst = false,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 14, 24, 10),
      child: Row(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(9),
            child: Image.asset('assets/images/icon.png', width: 32, height: 32),
          ),
          const SizedBox(width: 14),
          for (var i = 0; i < tabs.length; i++)
            Padding(
              padding: const EdgeInsets.only(right: 4),
              child: _tab(tabs[i], i == 0 && autofocusFirst),
            ),
          const Spacer(),
          for (final action in actions)
            Padding(padding: const EdgeInsets.only(left: 6), child: action),
        ],
      ),
    );
  }

  Widget _tab(TvNavTab tab, bool autofocus) {
    final p = palette;
    final color = tab.active ? p.color('--td-text-color-anti') : p.secondary;
    return MioButton(
      palette: p,
      autofocus: autofocus,
      label: tab.label,
      radius: 999,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
      background: tab.active ? p.brand : null,
      onPressed: tab.onPressed,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          MioIcon('nav-${tab.icon}', size: 18, color: color),
          const SizedBox(width: 8),
          Text(
            tab.label,
            style: TextStyle(
              fontSize: 15,
              height: 1.2,
              fontWeight: tab.active ? FontWeight.w700 : FontWeight.w500,
              color: color,
            ),
          ),
        ],
      ),
    );
  }
}

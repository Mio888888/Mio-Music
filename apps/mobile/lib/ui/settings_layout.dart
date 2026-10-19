import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'device.dart';
import 'context_menu.dart';
import 'resources.dart';

/// Shared metrics of the desktop compact settings cards and their fields.
class MioSettingsSection extends StatelessWidget {
  final MioPalette palette;
  final String? title;
  final Widget child;
  final bool appearance, compactAppearance;
  const MioSettingsSection({
    super.key,
    required this.palette,
    this.title,
    required this.child,
    this.appearance = false,
    this.compactAppearance = false,
  });
  @override
  Widget build(BuildContext context) {
    final wide = MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width);
    return wide ? _wide(context) : _compact(context);
  }

  /// Desktop wide metrics: `.setting-group` (24 padding, 12 radius, 18px
  /// title) and the appearance `t-card` (6 radius, 24px title row).
  Widget _wide(BuildContext context) {
    if (appearance) {
      return Container(
        width: double.infinity,
        margin: const EdgeInsets.only(bottom: 24),
        decoration: BoxDecoration(
          color: palette.color('--td-bg-color-container'),
          border: Border.all(color: palette.color('--td-border-level-2-color')),
          borderRadius: BorderRadius.circular(6),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (title != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(24, 16, 24, 0),
                child: Text(
                  title!,
                  style: const TextStyle(
                    fontSize: 16,
                    height: 24 / 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 16, 24, 16),
              child: child,
            ),
          ],
        ),
      );
    }
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 24),
      padding: const EdgeInsets.all(24),
      decoration: BoxDecoration(
        color: palette.color('--settings-group-bg'),
        border: Border.all(color: palette.color('--settings-group-border')),
        borderRadius: BorderRadius.circular(12),
        boxShadow: [
          BoxShadow(
            color: palette.color('--settings-group-shadow'),
            offset: const Offset(0, 1),
            blurRadius: 3,
          ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (title != null) ...[
            Text(
              title!,
              style: const TextStyle(
                fontSize: 18,
                height: 1.35,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 8),
          ],
          child,
        ],
      ),
    );
  }

  Widget _compact(BuildContext context) => Container(
    width: double.infinity,
    margin: EdgeInsets.only(bottom: compactAppearance ? 20 : 10),
    padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(
      color: palette.color(
        appearance ? '--td-bg-color-container' : '--settings-group-bg',
      ),
      border: Border.all(
        color: palette.color(
          appearance ? '--td-border-level-2-color' : '--settings-group-border',
        ),
      ),
      borderRadius: BorderRadius.circular(16),
      boxShadow: appearance
          ? null
          : [
              BoxShadow(
                color: palette.color('--settings-group-shadow'),
                offset: const Offset(0, 1),
                blurRadius: 3,
              ),
            ],
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (title != null) ...[
          Text(
            title!,
            style: TextStyle(
              fontSize: 16,
              height: compactAppearance ? 1.5 : 1.35,
              fontWeight: FontWeight.w600,
            ),
          ),
          SizedBox(height: compactAppearance ? 16 : 8),
        ],
        child,
      ],
    ),
  );
}

class MioSettingsField extends StatelessWidget {
  final MioPalette palette;
  final String title;
  final String? description;
  final Widget child;
  final bool appearance, queueCard, backgroundEffect, inline;
  final double? topSpacing;
  const MioSettingsField({
    super.key,
    required this.palette,
    required this.title,
    this.description,
    required this.child,
    this.appearance = false,
    this.queueCard = false,
    this.backgroundEffect = false,
    this.inline = false,
    this.topSpacing,
  });
  @override
  Widget build(BuildContext context) {
    final wide = MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width);
    return wide && !queueCard ? _wide(context) : _compact(context);
  }

  /// Desktop `.setting-item` (16/14 padding, 8 radius, 15.2px title) and the
  /// lyric-font item (rem padding, page background, 0.95rem title).
  Widget _wide(BuildContext context) {
    final rem = MioPalette.rem(MediaQuery.sizeOf(context).width);
    return Container(
      width: double.infinity,
      margin: EdgeInsets.only(top: topSpacing ?? 12),
      padding: appearance
          ? EdgeInsets.fromLTRB(rem, 14.5, rem, 15)
          : const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
      decoration: BoxDecoration(
        color: palette.color(
          appearance
              ? '--td-bg-color-page'
              : backgroundEffect
              ? '--td-bg-color-container'
              : '--settings-feature-bg',
        ),
        border: Border.all(
          color: palette.color(
            appearance || backgroundEffect
                ? '--td-border-level-1-color'
                : '--settings-feature-border',
          ),
        ),
        borderRadius: BorderRadius.circular(appearance ? rem * .5 : 8),
      ),
      child:
          (inline ||
              child is MioSettingsSwitch ||
              child is MioSettingsStepper ||
              (child is SizedBox && (child as SizedBox).width == 200))
          ? Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style: TextStyle(
                          fontSize: appearance ? rem * .95 : 15.2,
                          height: appearance ? 22 / (rem * .95) : 1.2,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      if (description != null) ...[
                        SizedBox(height: appearance ? rem * .25 : 4),
                        Text(
                          description!,
                          style: TextStyle(
                            fontSize: appearance ? rem * .8 : 12.8,
                            height: appearance ? 22 / (rem * .8) : 1.2,
                            color: palette.secondary,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                SizedBox(width: appearance ? 0 : 16),
                child,
              ],
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  title,
                  style: TextStyle(
                    fontSize: appearance ? rem * .95 : 15.2,
                    height: appearance ? 22 / (rem * .95) : 1.2,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                if (description != null) ...[
                  SizedBox(height: appearance ? rem * .25 : 4),
                  Text(
                    description!,
                    style: TextStyle(
                      fontSize: appearance ? rem * .8 : 12.8,
                      height: appearance ? 22 / (rem * .8) : 1.2,
                      color: palette.secondary,
                    ),
                  ),
                ],
                const SizedBox(height: 10),
                Align(alignment: Alignment.centerLeft, child: child),
              ],
            ),
    );
  }

  Widget _compact(BuildContext context) => Container(
    width: double.infinity,
    margin: EdgeInsets.only(top: topSpacing ?? 12),
    padding: queueCard
        ? const EdgeInsets.all(16)
        : const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
    decoration: BoxDecoration(
      color: palette.color(
        appearance
            ? '--td-bg-color-page'
            : backgroundEffect
            ? '--td-bg-color-container'
            : '--settings-feature-bg',
      ),
      border: Border.all(
        color: palette.color(
          appearance || backgroundEffect
              ? '--td-border-level-1-color'
              : '--settings-feature-border',
        ),
      ),
      borderRadius: BorderRadius.circular(appearance ? 16 : 8),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          title,
          style: TextStyle(
            fontSize: queueCard
                ? 16
                : appearance
                ? 14
                : 15.2,
            height: appearance
                ? 18 / 14
                : backgroundEffect
                ? 1.5
                : 1.2,
            fontWeight: FontWeight.w600,
          ),
        ),
        if (description != null) ...[
          const SizedBox(height: 4),
          Text(
            description!,
            style: TextStyle(
              fontSize: appearance ? 12 : 12.8,
              height: appearance ? 17 / 12 : 1.45,
              color: palette.secondary,
            ),
          ),
        ],
        SizedBox(
          height: queueCard
              ? 12
              : appearance
              ? 8
              : 10,
        ),
        Align(alignment: Alignment.centerLeft, child: child),
      ],
    ),
  );
}

class MioSettingsSwitch extends StatelessWidget {
  final MioPalette palette;
  final String label;
  final bool value;
  final VoidCallback? onPressed;
  const MioSettingsSwitch({
    super.key,
    required this.palette,
    required this.label,
    required this.value,
    this.onPressed,
  });
  @override
  Widget build(BuildContext context) {
    final compact = !MioDeviceScope.wide(
      context,
      MediaQuery.sizeOf(context).width,
    );
    return Semantics(
      toggled: value,
      child: MioButton(
        palette: palette,
        label: label,
        radius: 22,
        padding: EdgeInsets.symmetric(vertical: compact ? 0 : 8),
        insetFocusBorder: !compact,
        onPressed: onPressed,
        child: Container(
          width: compact ? 36 : 40,
          height: compact ? 20 : 22,
          padding: const EdgeInsets.all(2),
          alignment: value ? Alignment.centerRight : Alignment.centerLeft,
          decoration: BoxDecoration(
            color: value ? palette.brand : palette.border,
            borderRadius: BorderRadius.circular(11),
          ),
          child: Container(
            width: compact ? 16 : 18,
            height: compact ? 16 : 18,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: palette.color('--td-text-color-anti'),
            ),
          ),
        ),
      ),
    );
  }
}

/// Embedded feature views share a single page scroll without losing their own
/// Rust session or lifecycle handling. Their existing wide pages remain intact.
class MioSettingsBody extends StatelessWidget {
  final bool embedded;
  final EdgeInsetsGeometry padding;
  final Widget child;
  const MioSettingsBody({
    super.key,
    required this.embedded,
    required this.padding,
    required this.child,
  });
  @override
  Widget build(BuildContext context) =>
      embedded ? child : SingleChildScrollView(padding: padding, child: child);
}

class MioSettingsSelect extends StatefulWidget {
  final MioPalette palette;
  final String label;
  final String? value;
  final bool compact, outlined;
  final List<({String value, String label})> options;
  final ValueChanged<String>? onChanged;
  const MioSettingsSelect({
    super.key,
    required this.palette,
    required this.label,
    required this.value,
    this.compact = false,
    this.outlined = false,
    required this.options,
    this.onChanged,
  });
  @override
  State<MioSettingsSelect> createState() => _MioSettingsSelectState();
}

class _MioSettingsSelectState extends State<MioSettingsSelect>
    with WidgetsBindingObserver {
  final _focus = FocusNode();
  RawDialogRoute<String>? _route;
  void _close() {
    final route = _route;
    _route = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route);
      });
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didUpdateWidget(MioSettingsSelect oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.onChanged == null) _close();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) _close();
  }

  @override
  void dispose() {
    _close();
    WidgetsBinding.instance.removeObserver(this);
    _focus.dispose();
    super.dispose();
  }

  Future<void> _open() async {
    if (_route != null || widget.onChanged == null) return;
    final box = context.findRenderObject();
    if (box is! RenderBox || !box.hasSize) return;
    final restore = _focus.hasFocus;
    final route = RawDialogRoute<String>(
      barrierDismissible: true,
      barrierLabel: widget.palette.text('common.close'),
      barrierColor: const Color(0x00000000),
      transitionDuration: Duration.zero,
      pageBuilder: (_, _, _) => MioContextMenu<String>(
        palette: widget.palette,
        anchor: box.localToGlobal(
          Offset(
            0,
            box.size.height +
                (MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width)
                    ? 0
                    : 8),
          ),
        ),
        label: widget.label,
        selectWidth:
            !MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width)
            ? box.size.width
            : null,
        selected: widget.value,
        entries: [
          for (final item in widget.options)
            MioMenuEntry(
              value: item.value,
              label:
                  !MioDeviceScope.wide(
                    context,
                    MediaQuery.sizeOf(context).width,
                  )
                  ? item.label
                  : '${item.label}${item.value == widget.value ? ' ✓' : ''}',
              icon: 'nav-liebiao',
            ),
        ],
      ),
    );
    _route = route;
    final value = await Navigator.of(context).push(route);
    final current = identical(_route, route);
    if (current) _route = null;
    if (!mounted || !current) return;
    if (value != null && value != widget.value) widget.onChanged?.call(value);
    if (restore) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted &&
            widget.onChanged != null &&
            _focus.canRequestFocus &&
            ModalRoute.of(context)?.isCurrent == true) {
          _focus.requestFocus();
        }
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final selected = widget.options
        .where((item) => item.value == widget.value)
        .firstOrNull;
    return Container(
      width: double.infinity,
      height: widget.compact ? 32 : 44,
      decoration: widget.outlined
          ? BoxDecoration(
              border: Border.all(color: widget.palette.border),
              borderRadius: BorderRadius.circular(3),
            )
          : null,
      child: MioButton(
        palette: widget.palette,
        focusNode: _focus,
        label: widget.label,
        radius: widget.outlined ? 3 : 6,
        background: widget.palette.color('--td-bg-color-specialcomponent'),
        onPressed: widget.onChanged == null || widget.options.isEmpty
            ? null
            : _open,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Row(
          children: [
            Expanded(
              child: Text(
                selected?.label ?? widget.label,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            MioIcon(
              widget.compact || widget.outlined ? 'TD-Down' : 'nav-xiangyou',
              color: widget.palette.secondary,
              size: 14,
            ),
          ],
        ),
      ),
    );
  }
}

class MioSettingsStepper extends StatelessWidget {
  final MioPalette palette;
  final String value, label;
  final VoidCallback? onDecrease, onIncrease;
  const MioSettingsStepper({
    super.key,
    required this.palette,
    required this.value,
    required this.label,
    this.onDecrease,
    this.onIncrease,
  });
  @override
  Widget build(BuildContext context) {
    final wide = MioDeviceScope.wide(context, MediaQuery.sizeOf(context).width);
    Widget cell(Widget child) => Container(
      height: wide ? 32 : 44,
      decoration: BoxDecoration(
        color: palette.color('--td-bg-color-specialcomponent'),
        border: Border.all(color: palette.border),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Center(child: child),
    );
    Widget button(String text, VoidCallback? callback) => SizedBox(
      width: 32,
      child: cell(
        MioButton(
          palette: palette,
          label: '$label $text',
          padding: EdgeInsets.zero,
          onPressed: callback,
          child: Center(
            child: Text(text, style: const TextStyle(fontSize: 16)),
          ),
        ),
      ),
    );
    if (wide) {
      // Desktop: 32px buttons around a 48px value box, 8px apart.
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          button('−', onDecrease),
          const SizedBox(width: 8),
          SizedBox(
            width: 48,
            child: cell(Text(value, style: const TextStyle(fontSize: 14))),
          ),
          const SizedBox(width: 8),
          button('+', onIncrease),
        ],
      );
    }
    return Row(
      children: [
        button('−', onDecrease),
        Expanded(
          child: cell(Text(value, style: const TextStyle(fontSize: 14))),
        ),
        button('+', onIncrease),
      ],
    );
  }
}

/// Desktop default button: 32px high, 16px padding, component background.
class MioSettingsButton extends StatelessWidget {
  final MioPalette palette;
  final String label;
  final VoidCallback? onPressed;
  final bool primary, danger;

  /// Transparent background with a border (brand coloured when [primary]).
  final bool outline;
  const MioSettingsButton({
    super.key,
    required this.palette,
    required this.label,
    this.onPressed,
    this.primary = false,
    this.danger = false,
    this.outline = false,
  });
  @override
  Widget build(BuildContext context) => Container(
    height: 32,
    decoration: outline
        ? BoxDecoration(
            borderRadius: BorderRadius.circular(6),
            border: Border.all(
              color: primary
                  ? palette.brand
                  : palette.color('--td-component-border'),
            ),
          )
        : null,
    child: MioButton(
      palette: palette,
      label: label,
      radius: 6,
      background: outline
          ? const Color(0x00000000)
          : danger
          ? palette.color('--td-error-color-6')
          : primary
          ? palette.brand
          : palette.color('--td-bg-color-component'),
      padding: const EdgeInsets.symmetric(horizontal: 16),
      onPressed: onPressed,
      child: Center(
        widthFactor: 1,
        child: Text(
          label,
          style: TextStyle(
            fontSize: 14,
            height: 22 / 14,
            color: outline
                ? (primary
                      ? palette.brand
                      : palette.color('--td-text-color-primary'))
                : danger
                ? palette.color('--td-font-white-1')
                : primary
                ? palette.color('--td-text-color-anti')
                : palette.color('--td-text-color-primary'),
          ),
        ),
      ),
    ),
  );
}

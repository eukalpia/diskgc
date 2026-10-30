/// Small building blocks shared by the screens.
library;

import 'package:cinder/cinder.dart';

/// A bordered box with a title, the basic unit of the dashboard layout.
class Panel extends StatelessWidget {
  const Panel({
    super.key,
    required this.title,
    required this.child,
    this.padding = const EdgeInsets.symmetric(horizontal: 1),
  });

  final String title;
  final Widget child;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        border: BoxBorder.all(
          style: BoxBorderStyle.rounded,
          color: theme.outline,
        ),
        title: BorderTitle(text: ' $title '),
      ),
      child: child,
    );
  }
}

/// `label ........ value` on one line.
class KeyValue extends StatelessWidget {
  const KeyValue(this.label, this.value, {super.key, this.valueStyle});

  final String label;
  final String value;
  final TextStyle? valueStyle;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    return Row(
      children: <Widget>[
        Expanded(
          child: Text(
            label,
            style: TextStyle(color: theme.onSurface),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
        const SizedBox(width: 1),
        Text(
          value,
          style:
              valueStyle ??
              TextStyle(fontWeight: FontWeight.bold, color: theme.onBackground),
          maxLines: 1,
        ),
      ],
    );
  }
}

/// Coloured badge for a cleanup level.
class LevelBadge extends StatelessWidget {
  const LevelBadge(this.level, {super.key});

  final String level;

  @override
  Widget build(BuildContext context) {
    final status = switch (level) {
      'critical' => StatusLevel.error,
      'low' => StatusLevel.warning,
      'normal' => StatusLevel.success,
      _ => StatusLevel.neutral,
    };
    return StatusBadge(level.isEmpty ? '—' : level, level: status);
  }
}

/// A single dim line of help at the bottom of a screen.
class Hint extends StatelessWidget {
  const Hint(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    return Text(
      text,
      style: TextStyle(color: theme.outline),
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
    );
  }
}

class Muted extends StatelessWidget {
  const Muted(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    return Text(
      text,
      style: TextStyle(color: theme.outline),
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
    );
  }
}

class Heading extends StatelessWidget {
  const Heading(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    return Text(
      text,
      style: TextStyle(color: theme.primary, fontWeight: FontWeight.bold),
      maxLines: 1,
    );
  }
}

import 'package:cinder/cinder.dart';

import '../widgets.dart';

/// Daemon log lines and events as they arrive.
class LogScreen extends StatelessWidget {
  const LogScreen({super.key, required this.lines});

  final List<String> lines;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          child: Panel(
            title: 'Activity · ${lines.length} lines',
            child: lines.isEmpty
                ? const Center(child: Muted('Waiting for the daemon…'))
                : ListView.builder(
                    reverse: true,
                    keyboardScrollable: true,
                    itemCount: lines.length,
                    itemBuilder: (context, i) => Text(
                      lines[lines.length - 1 - i],
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
          ),
        ),
        const SizedBox(height: 1),
        const Hint('newest at the bottom · ↑↓ scroll'),
      ],
    );
  }
}

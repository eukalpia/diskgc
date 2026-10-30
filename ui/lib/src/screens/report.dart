import 'package:cinder/cinder.dart';

import '../format.dart';
import '../model.dart';
import '../widgets.dart';

/// The biggest offenders per category.
class ReportScreen extends StatelessWidget {
  const ReportScreen({
    super.key,
    required this.report,
    required this.busy,
    required this.scanning,
  });

  final Report? report;
  final bool busy;
  final String? scanning;

  @override
  Widget build(BuildContext context) {
    final r = report;
    if (r == null) {
      return Center(
        child: Muted(
          busy ? 'Scanning ${scanning ?? ''}…' : 'Press r to scan the disk',
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(
                child: _table('Rust target/ directories', r.rustTargets),
              ),
              const SizedBox(width: 1),
              Expanded(child: _table('node_modules', r.nodeModules)),
            ],
          ),
        ),
        const SizedBox(height: 1),
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(child: _table('Agent scratch directories', r.scratch)),
              const SizedBox(width: 1),
              Expanded(
                child: Panel(
                  title: 'Docker',
                  child: r.dockerDf == null
                      ? const Center(child: Muted('Docker is not running'))
                      : ListView(
                          children: r.dockerDf!
                              .split('\n')
                              .where((l) => l.trim().isNotEmpty)
                              .map(
                                (l) => Text(
                                  l,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
                              )
                              .toList(),
                        ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 1),
        const Hint(
          'r rescan · sizes are bytes on disk, hard links counted once',
        ),
      ],
    );
  }

  Widget _table(String title, List<SizedPath> items) {
    return Panel(
      title:
          '$title · ${humanBytes(items.fold<int>(0, (s, i) => s + i.bytes))}',
      padding: EdgeInsets.zero,
      child: VirtualizedDataTable<SizedPath>(
        rows: items,
        rowKey: (s) => s.path,
        emptyMessage: 'none found',
        showHeader: false,
        columns: <DataColumn<SizedPath>>[
          DataColumn<SizedPath>(
            label: 'size',
            width: 9,
            numeric: true,
            value: (s) => s.bytes,
            format: (v) => humanBytes(v as int),
          ),
          DataColumn<SizedPath>(
            label: 'path',
            width: 58,
            value: (s) => s.path,
            format: (v) => fitPath(v.toString(), 58),
          ),
        ],
      ),
    );
  }
}

import 'package:cinder/cinder.dart';

import '../format.dart';
import '../model.dart';
import '../widgets.dart';

/// RAM by program and process; orphaned helpers of finished sessions.
class MemoryScreen extends StatelessWidget {
  const MemoryScreen({
    super.key,
    required this.report,
    required this.busy,
    required this.onKillOrphans,
    required this.killed,
  });

  final MemReport? report;
  final bool busy;
  final VoidCallback onKillOrphans;
  final int? killed;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    final m = report;
    if (m == null) {
      return Center(
        child: Muted(
          busy ? 'Reading processes…' : 'Press r to read memory usage',
        ),
      );
    }
    final usedPct = m.total == 0 ? 0.0 : m.used / m.total * 100;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          flex: 2,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(
                child: Panel(
                  title: 'RAM',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: <Widget>[
                      GaugeChart(
                        value: usedPct,
                        title: 'Used',
                        color: m.pressure ? theme.error : theme.success,
                        valueFormatter: (v) =>
                            '${v.toStringAsFixed(0)}%  ·  ${humanBytes(m.used)} of ${humanBytes(m.total)}',
                      ),
                      const SizedBox(height: 1),
                      KeyValue('Available', humanBytes(m.available)),
                      KeyValue(
                        'Swap',
                        '${humanBytes(m.swapUsed)} of ${humanBytes(m.swapTotal)}',
                        valueStyle: TextStyle(
                          color: m.pressure ? theme.error : theme.onBackground,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      if (m.pressure)
                        Text(
                          'Memory is under pressure: pages are being compressed or swapped.',
                          style: TextStyle(color: theme.warning),
                        ),
                      if (m.orphanContainers > 0)
                        Text(
                          '${m.orphanContainers} orphaned testcontainers are eating RAM in the container VM.',
                          style: TextStyle(color: theme.warning),
                        ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 1),
              Expanded(
                flex: 2,
                child: Panel(
                  title: 'By program',
                  child: BarChart(
                    bars: m.byProgram
                        .take(10)
                        .map(
                          (p) => ChartBar(
                            label: '${p.name} (${p.count})',
                            value: p.bytes / (1024 * 1024),
                          ),
                        )
                        .toList(),
                    orientation: Axis.horizontal,
                    labelWidth: 26,
                    height: 12,
                    valueFormatter: (v) => '${v.toStringAsFixed(0)} MB',
                  ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 1),
        Expanded(
          flex: 3,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(
                flex: 3,
                child: Panel(
                  title: 'Top processes',
                  padding: EdgeInsets.zero,
                  child: VirtualizedDataTable<ProcInfo>(
                    rows: m.top,
                    rowKey: (p) => p.pid,
                    columns: <DataColumn<ProcInfo>>[
                      DataColumn<ProcInfo>(
                        label: 'rss',
                        width: 9,
                        numeric: true,
                        value: (p) => p.rss,
                        format: (v) => humanBytes(v as int),
                      ),
                      DataColumn<ProcInfo>(
                        label: 'pid',
                        width: 8,
                        numeric: true,
                        value: (p) => p.pid,
                        format: (v) => '$v ',
                      ),
                      DataColumn<ProcInfo>(
                        label: 'up',
                        width: 8,
                        value: (p) => hours(p.runHours),
                        format: (v) => ' $v',
                      ),
                      DataColumn<ProcInfo>(
                        label: 'program',
                        width: 16,
                        value: (p) => p.program,
                      ),
                      DataColumn<ProcInfo>(
                        label: 'command',
                        width: 30,
                        value: (p) => p.command,
                        format: (v) => fitPath(v.toString(), 30),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 1),
              Expanded(
                flex: 2,
                child: Panel(
                  title: 'Orphaned helpers · ${m.orphans.length}',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: <Widget>[
                      const Muted('Parent process gone, running for 12+ hours'),
                      Expanded(
                        child: m.orphans.isEmpty
                            ? const Center(child: Muted('none'))
                            : ListView(
                                children: m.orphans
                                    .map(
                                      (p) => Text(
                                        '${humanBytes(p.rss).padLeft(9)}  ${p.pid.toString().padLeft(6)}  ${fitPath(p.command, 60)}',
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                    )
                                    .toList(),
                              ),
                      ),
                      Row(
                        children: <Widget>[
                          Button(
                            onPressed: busy || m.orphans.isEmpty
                                ? null
                                : onKillOrphans,
                            child: const Text('Terminate orphans'),
                          ),
                          const SizedBox(width: 2),
                          if (killed != null)
                            Text(
                              'terminated $killed',
                              style: TextStyle(color: theme.success),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 1),
        const Hint('r refresh · k terminate orphaned helpers'),
      ],
    );
  }
}

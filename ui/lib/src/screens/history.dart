import 'package:cinder/cinder.dart';

import '../format.dart';
import '../model.dart';
import '../widgets.dart';

/// Past runs, growth per day, repeat offenders.
class HistoryScreen extends StatelessWidget {
  const HistoryScreen({super.key, required this.runs, required this.stats});

  final List<RunRecord> runs;
  final Stats? stats;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    final s = stats;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          flex: 3,
          child: Panel(
            title: 'Runs',
            padding: EdgeInsets.zero,
            child: VirtualizedDataTable<RunRecord>(
              rows: runs.reversed.toList(),
              rowKey: (r) => r.ts,
              emptyMessage: 'No runs yet',
              columns: <DataColumn<RunRecord>>[
                DataColumn<RunRecord>(
                  label: 'when',
                  width: 19,
                  value: (r) => r.ts,
                ),
                DataColumn<RunRecord>(
                  label: 'level',
                  width: 9,
                  value: (r) => r.dryRun ? '${r.level} (dry)' : r.level,
                ),
                DataColumn<RunRecord>(
                  label: 'freed',
                  width: 10,
                  numeric: true,
                  value: (r) => r.freed,
                  format: (v) => humanBytes(v as int),
                ),
                DataColumn<RunRecord>(
                  label: 'actions',
                  width: 8,
                  numeric: true,
                  value: (r) => r.actions,
                ),
                DataColumn<RunRecord>(
                  label: 'free after',
                  width: 11,
                  numeric: true,
                  value: (r) => r.availableAfter,
                  format: (v) => humanBytes(v as int),
                ),
                DataColumn<RunRecord>(
                  label: 'largest',
                  width: 50,
                  value: (r) =>
                      r.top.isEmpty ? '' : fitPath(r.top.first.path, 50),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 1),
        Expanded(
          flex: 2,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(
                child: Panel(
                  title: 'Used per day',
                  padding: EdgeInsets.zero,
                  child: s == null || s.days.isEmpty
                      ? const Center(child: Muted('No history yet'))
                      : VirtualizedDataTable<DayStat>(
                          rows: s.days.reversed.toList(),
                          rowKey: (d) => d.day,
                          columns: <DataColumn<DayStat>>[
                            DataColumn<DayStat>(
                              label: 'day',
                              width: 11,
                              value: (d) => d.day,
                            ),
                            DataColumn<DayStat>(
                              label: 'am',
                              width: 9,
                              numeric: true,
                              value: (d) => d.usedFirst,
                              format: (v) => humanBytes(v as int),
                            ),
                            DataColumn<DayStat>(
                              label: 'pm',
                              width: 9,
                              numeric: true,
                              value: (d) => d.usedLast,
                              format: (v) => humanBytes(v as int),
                            ),
                            DataColumn<DayStat>(
                              label: 'delta',
                              width: 9,
                              numeric: true,
                              value: (d) => d.usedLast - d.usedFirst,
                              format: (v) =>
                                  '${(v as int) >= 0 ? '+' : '-'}${humanBytes(v.abs())}',
                            ),
                            DataColumn<DayStat>(
                              label: 'freed',
                              width: 9,
                              numeric: true,
                              value: (d) => d.freed,
                              format: (v) => humanBytes(v as int),
                            ),
                          ],
                        ),
                ),
              ),
              const SizedBox(width: 1),
              Expanded(
                child: Panel(
                  title: 'Repeat offenders',
                  child: s == null || s.repeatOffenders.isEmpty
                      ? const Center(
                          child: Muted('Nothing has been cleaned twice yet'),
                        )
                      : Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: <Widget>[
                            const Muted(
                              'Cleaned again and again — fix the cause, not the symptom',
                            ),
                            for (final r in s.repeatOffenders.take(8))
                              Text(
                                '${r.times.toString().padLeft(2)}×  ${humanBytes(r.bytes).padLeft(9)}  ${fitPath(r.path, 60)}',
                                style: TextStyle(color: theme.warning),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                          ],
                        ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 1),
        Hint(
          s == null
              ? 'r refresh'
              : 'r refresh · ${s.runs} runs, ${humanBytes(s.totalFreed)} freed in total',
        ),
      ],
    );
  }
}

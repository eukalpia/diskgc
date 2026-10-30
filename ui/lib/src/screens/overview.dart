import 'package:cinder/cinder.dart';

import '../format.dart';
import '../model.dart';
import '../widgets.dart';

/// Free space, level, forecast, category sizes, last run.
class OverviewScreen extends StatelessWidget {
  const OverviewScreen({
    super.key,
    required this.status,
    required this.report,
    required this.snapshots,
    required this.lastSummary,
    required this.connected,
  });

  final Status? status;
  final Report? report;
  final List<Snapshot> snapshots;
  final RunSummary? lastSummary;
  final bool connected;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    final st = status;
    if (st == null) {
      return Center(
        child: Text(
          connected ? 'Loading status…' : 'Connecting to the daemon…',
        ),
      );
    }
    final disk = st.disk;
    final usedPct = disk.usedFraction * 100;
    final gaugeColor = switch (disk.level) {
      'critical' => theme.error,
      'low' => theme.warning,
      _ => theme.success,
    };
    final cats = <MapEntry<String, int>>[...?report?.categories.entries];
    cats.sort((a, b) => b.value.compareTo(a.value));
    final barCats = cats.where((e) => e.value > 0).take(9).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          flex: 3,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(
                flex: 2,
                child: Panel(
                  title: 'Disk ${disk.volume}',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: <Widget>[
                      GaugeChart(
                        value: usedPct,
                        title: 'Used',
                        color: gaugeColor,
                        valueFormatter: (v) =>
                            '${v.toStringAsFixed(0)}%  ·  ${humanBytes(disk.used)} of ${humanBytes(disk.total)}',
                      ),
                      const SizedBox(height: 1),
                      KeyValue(
                        'Free',
                        humanBytes(disk.available),
                        valueStyle: TextStyle(
                          color: gaugeColor,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      Row(
                        children: <Widget>[
                          Expanded(child: const Text('Level')),
                          LevelBadge(disk.level),
                        ],
                      ),
                      KeyValue(
                        'Thresholds',
                        '${st.thresholdsGb[0].toStringAsFixed(0)} / ${st.thresholdsGb[1].toStringAsFixed(0)} / ${st.thresholdsGb[2].toStringAsFixed(0)} GB',
                      ),
                      KeyValue(
                        'Daemon',
                        st.daemonRunning ? 'running' : st.daemonDetail,
                        valueStyle: TextStyle(
                          color: st.daemonRunning ? theme.success : theme.error,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      KeyValue(
                        'Compression',
                        st.compressionSupported
                            ? 'filesystem (transparent)'
                            : 'not available here',
                      ),
                      const SizedBox(height: 1),
                      _forecast(st.forecast, theme),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 1),
              Expanded(
                flex: 3,
                child: Panel(
                  title: 'Where the space goes',
                  child: barCats.isEmpty
                      ? const Center(child: Muted('Press r to scan categories'))
                      : BarChart(
                          bars: barCats
                              .map(
                                (e) =>
                                    ChartBar(label: e.key, value: gib(e.value)),
                              )
                              .toList(),
                          orientation: Axis.horizontal,
                          labelWidth: 18,
                          height: barCats.length + 2,
                          valueFormatter: (v) => '${v.toStringAsFixed(1)} G',
                        ),
                ),
              ),
            ],
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
                  title: 'Free space this session',
                  child: snapshots.length < 2
                      ? const Center(
                          child: Muted('Snapshots arrive every daemon tick'),
                        )
                      : Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: <Widget>[
                            SparklineChart(
                              values: snapshots
                                  .map((s) => gib(s.available))
                                  .toList(),
                              color: theme.primary,
                            ),
                            const SizedBox(height: 1),
                            Muted(
                              '${snapshots.length} samples · ${humanBytes(snapshots.first.available)} → ${humanBytes(snapshots.last.available)}',
                            ),
                          ],
                        ),
                ),
              ),
              const SizedBox(width: 1),
              Expanded(
                child: Panel(
                  title: 'Last run',
                  child: _lastRun(st, lastSummary, theme),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _forecast(Forecast? f, TuiThemeData theme) {
    if (f == null) return const Muted('Forecast: not enough history yet');
    if (f.ratePerHour <= 0) {
      return Muted(
        'Forecast: stable (${humanBytes(f.ratePerHour.abs())}/h over ${f.windowHours.toStringAsFixed(0)} h)',
      );
    }
    final eta = f.hoursToCritical == null
        ? ''
        : ' · critical in ~${hours(f.hoursToCritical!)}';
    return Text(
      'Forecast: +${humanBytes(f.ratePerHour)}/h$eta',
      style: TextStyle(
        color: f.hoursToCritical != null && f.hoursToCritical! < 24
            ? theme.warning
            : theme.onBackground,
      ),
    );
  }

  Widget _lastRun(Status st, RunSummary? s, TuiThemeData theme) {
    if (s != null) {
      final failed = s.results.where((r) => !r.ok).length;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          KeyValue(
            s.dryRun ? 'Dry run' : 'Freed',
            humanBytes(s.totalFreed),
            valueStyle: TextStyle(
              color: theme.success,
              fontWeight: FontWeight.bold,
            ),
          ),
          KeyValue(
            'Actions',
            '${s.actionCount}${failed > 0 ? ' ($failed failed)' : ''}',
          ),
          KeyValue('Duration', '${s.durationSecs.toStringAsFixed(0)} s'),
          KeyValue('Free after', humanBytes(s.freeAfter)),
          for (final l in s.dockerLines.take(1)) Muted(l),
        ],
      );
    }
    final r = st.lastRun;
    if (r == null) return const Center(child: Muted('No runs recorded yet'));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        KeyValue('When', r.ts),
        Row(
          children: <Widget>[
            Expanded(child: const Text('Level')),
            LevelBadge(r.level),
          ],
        ),
        KeyValue(
          'Freed',
          humanBytes(r.freed),
          valueStyle: TextStyle(
            color: theme.success,
            fontWeight: FontWeight.bold,
          ),
        ),
        KeyValue('Actions', '${r.actions}'),
        for (final t in r.top.take(2))
          Muted('${humanBytes(t.bytes).padLeft(9)}  ${shortenPath(t.path)}'),
      ],
    );
  }
}

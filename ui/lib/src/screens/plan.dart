import 'package:cinder/cinder.dart';

import '../format.dart';
import '../model.dart';
import '../widgets.dart';

/// Review the plan action by action, untick what should stay, execute the rest.
class PlanScreen extends StatelessWidget {
  const PlanScreen({
    super.key,
    required this.plan,
    required this.selected,
    required this.busy,
    required this.scanning,
    required this.progressIndex,
    required this.progressTotal,
    required this.progressAction,
    required this.lastSummary,
    required this.onToggle,
    required this.onHighlight,
    required this.onPlan,
    required this.onExecute,
    required this.onToggleAll,
  });

  final Plan? plan;
  final Set<int> selected;
  final bool busy;
  final String? scanning;
  final int progressIndex;
  final int progressTotal;
  final PlanAction? progressAction;
  final RunSummary? lastSummary;
  final void Function(PlanAction action) onToggle;
  final void Function(PlanAction? action) onHighlight;
  final void Function(String level) onPlan;
  final void Function({required bool dryRun}) onExecute;
  final VoidCallback onToggleAll;

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    final p = plan;
    final compressCount = p?.actions.where((a) => a.isCompress).length ?? 0;
    final compressBytes =
        p?.actions
            .where((a) => a.isCompress)
            .fold<int>(0, (s, a) => s + a.bytes) ??
        0;
    final rows =
        p?.actions.where((a) => !a.isCompress).toList() ?? const <PlanAction>[];
    final selectedBytes = p?.estimatedBytes(selected) ?? 0;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Row(
          children: <Widget>[
            Button(
              onPressed: busy ? null : () => onPlan('normal'),
              child: const Text('Plan normal'),
            ),
            const SizedBox(width: 1),
            Button(
              onPressed: busy ? null : () => onPlan('low'),
              child: const Text('Plan low'),
            ),
            const SizedBox(width: 1),
            Button(
              onPressed: busy ? null : () => onPlan('critical'),
              child: const Text('Plan critical'),
            ),
            const SizedBox(width: 2),
            Button(
              onPressed: busy || p == null || selected.isEmpty
                  ? null
                  : () => onExecute(dryRun: false),
              child: Text('Execute ${selected.length}'),
            ),
            const SizedBox(width: 1),
            Button(
              onPressed: busy || p == null
                  ? null
                  : () => onExecute(dryRun: true),
              child: const Text('Dry run'),
            ),
            const SizedBox(width: 1),
            Button(
              onPressed: busy || p == null ? null : onToggleAll,
              child: const Text('Toggle all'),
            ),
            const Spacer(),
            if (p != null) LevelBadge(p.level),
          ],
        ),
        const SizedBox(height: 1),
        if (busy) _progress(theme),
        if (busy) const SizedBox(height: 1),
        Expanded(
          child: Panel(
            title: p == null
                ? 'Plan'
                : 'Plan · ${rows.length} actions · ${humanBytes(selectedBytes)} selected of ${humanBytes(p.estimatedBytes())}',
            padding: EdgeInsets.zero,
            child: p == null
                ? Center(
                    child: Muted(
                      busy
                          ? (scanning == null
                                ? 'Working…'
                                : 'Scanning $scanning…')
                          : 'Press p to build a plan',
                    ),
                  )
                : VirtualizedDataTable<PlanAction>(
                    rows: rows,
                    rowKey: (a) => a.id,
                    autofocus: true,
                    emptyMessage: 'Nothing to clean at this level',
                    onRowActivated: onToggle,
                    onSelectionChanged: onHighlight,
                    columns: <DataColumn<PlanAction>>[
                      DataColumn<PlanAction>(
                        label: ' ',
                        width: 3,
                        value: (a) => selected.contains(a.id) ? '[x]' : '[ ]',
                      ),
                      DataColumn<PlanAction>(
                        label: 'size',
                        width: 9,
                        numeric: true,
                        value: (a) => a.bytes,
                        format: (v) => '${humanBytes(v as int)} ',
                      ),
                      DataColumn<PlanAction>(
                        label: 'group',
                        width: 9,
                        value: (a) => a.group,
                        format: (v) => ' $v',
                      ),
                      DataColumn<PlanAction>(
                        label: 'action',
                        width: 17,
                        value: (a) => a.verb,
                        format: (v) => ' $v',
                      ),
                      DataColumn<PlanAction>(
                        label: 'path',
                        width: 48,
                        value: (a) => a.path,
                        format: (v) => fitPath(v.toString(), 48),
                      ),
                      DataColumn<PlanAction>(
                        label: 'why',
                        width: 44,
                        value: (a) => a.reason,
                      ),
                    ],
                  ),
          ),
        ),
        if (p != null &&
            (compressCount > 0 ||
                p.notes.isNotEmpty ||
                p.docker != null)) ...<Widget>[
          const SizedBox(height: 1),
          if (compressCount > 0)
            Muted(
              'Also: compress $compressCount cold files (${humanBytes(compressBytes)} on disk now) — transparent, always selected',
            ),
          if (p.docker != null && p.docker!.available)
            Muted(
              'Docker: ${p.docker!.orphanTestcontainers} orphaned testcontainers, ~${humanBytes(p.docker!.reclaimableBytes)} reclaimable — runs after the actions',
            ),
          for (final n in p.notes.take(3)) Muted('Kept: $n'),
        ],
        if (lastSummary != null) ...<Widget>[
          const SizedBox(height: 1),
          Text(
            lastSummary!.dryRun
                ? 'Dry run: ${humanBytes(lastSummary!.freedBytes)} would be freed by ${lastSummary!.actionCount} actions'
                : 'Freed ${humanBytes(lastSummary!.totalFreed)} in ${lastSummary!.actionCount} actions (${lastSummary!.durationSecs.toStringAsFixed(0)} s) · ${humanBytes(lastSummary!.freeAfter)} free now',
            style: TextStyle(color: theme.success, fontWeight: FontWeight.bold),
            maxLines: 1,
          ),
        ],
        const SizedBox(height: 1),
        const Hint(
          '↑↓ move · space/enter toggle · a toggle all · p plan · x execute selected · n dry run',
        ),
      ],
    );
  }

  Widget _progress(TuiThemeData theme) {
    if (progressTotal > 0) {
      final a = progressAction;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          ProgressBar(
            value: progressIndex / progressTotal,
            label: 'Executing $progressIndex / $progressTotal',
            showPercentage: true,
          ),
          if (a != null) Muted('${a.verb}  ${fitPath(a.path, 90)}'),
        ],
      );
    }
    return Row(
      children: <Widget>[
        Expanded(child: const ProgressBar(indeterminate: true)),
        const SizedBox(width: 1),
        Text(
          scanning == null ? 'Working…' : 'Scanning $scanning…',
          style: TextStyle(color: theme.primary),
        ),
      ],
    );
  }
}

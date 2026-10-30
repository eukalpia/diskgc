/// Typed views over the daemon's JSON (see docs/PROTOCOL.md). Unknown fields are ignored so
/// the UI keeps working with newer daemons.
library;

int _int(Object? v) => switch (v) {
  int i => i,
  double d => d.toInt(),
  String s => int.tryParse(s) ?? 0,
  _ => 0,
};

double _double(Object? v) => switch (v) {
  int i => i.toDouble(),
  double d => d,
  String s => double.tryParse(s) ?? 0,
  _ => 0,
};

String _str(Object? v) => v?.toString() ?? '';

Map<String, Object?> _map(Object? v) =>
    v is Map ? v.cast<String, Object?>() : const <String, Object?>{};

List<Map<String, Object?>> _list(Object? v) =>
    v is List ? v.map(_map).toList() : const <Map<String, Object?>>[];

class DiskStatus {
  const DiskStatus({
    required this.volume,
    required this.total,
    required this.used,
    required this.available,
    required this.level,
  });

  factory DiskStatus.fromJson(Map<String, Object?> j) => DiskStatus(
    volume: _str(j['volume']),
    total: _int(j['total']),
    used: _int(j['used']),
    available: _int(j['available']),
    level: _str(j['level']),
  );

  final String volume;
  final int total;
  final int used;
  final int available;
  final String level;

  double get usedFraction => total == 0 ? 0 : used / total;
}

class Forecast {
  const Forecast({
    required this.ratePerHour,
    required this.windowHours,
    required this.hoursToCritical,
    required this.availableIn24h,
  });

  factory Forecast.fromJson(Map<String, Object?> j) => Forecast(
    ratePerHour: _double(j['rate_bytes_per_hour']),
    windowHours: _double(j['window_hours']),
    hoursToCritical: j['hours_to_critical'] == null
        ? null
        : _double(j['hours_to_critical']),
    availableIn24h: _int(j['available_in_24h']),
  );

  final double ratePerHour;
  final double windowHours;
  final double? hoursToCritical;
  final int availableIn24h;
}

class Status {
  const Status({
    required this.version,
    required this.disk,
    required this.thresholdsGb,
    required this.daemonRunning,
    required this.daemonDetail,
    required this.forecast,
    required this.lastRun,
    required this.compressionSupported,
  });

  factory Status.fromJson(Map<String, Object?> j) {
    final auto = _map(j['autostart']);
    final t = j['thresholds_gb'];
    return Status(
      version: _str(j['version']),
      disk: DiskStatus.fromJson(_map(j['disk'])),
      thresholdsGb: t is List ? t.map(_double).toList() : const [150, 100, 60],
      daemonRunning: auto['running'] == true,
      daemonDetail: _str(auto['detail']),
      forecast: j['forecast'] == null
          ? null
          : Forecast.fromJson(_map(j['forecast'])),
      lastRun: j['last_run'] == null
          ? null
          : RunRecord.fromJson(_map(j['last_run'])),
      compressionSupported: j['compression_supported'] == true,
    );
  }

  final String version;
  final DiskStatus disk;
  final List<double> thresholdsGb;
  final bool daemonRunning;
  final String daemonDetail;
  final Forecast? forecast;
  final RunRecord? lastRun;
  final bool compressionSupported;
}

class PlanAction {
  const PlanAction({
    required this.id,
    required this.group,
    required this.kind,
    required this.verb,
    required this.path,
    required this.bytes,
    required this.reason,
  });

  factory PlanAction.fromJson(Map<String, Object?> j) {
    final kind = _str(j['kind']);
    final verb = switch (kind) {
      'remove' => 'remove',
      'sweep_older_than' => 'sweep >${_int(j['days'])}d',
      'sweep_big_binaries' => 'sweep test bins',
      'compress' => 'compress',
      'command' =>
        (j['argv'] is List)
            ? (j['argv'] as List).map(_str).join(' ')
            : 'command',
      _ => kind,
    };
    return PlanAction(
      id: _int(j['id']),
      group: _str(j['group']),
      kind: kind,
      verb: verb,
      path: _str(j['path']),
      bytes: _int(j['bytes']),
      reason: _str(j['reason']),
    );
  }

  final int id;
  final String group;
  final String kind;
  final String verb;
  final String path;
  final int bytes;
  final String reason;

  bool get isCompress => kind == 'compress';
}

class DockerPlan {
  const DockerPlan({
    required this.available,
    required this.orphanTestcontainers,
    required this.reclaimableBytes,
  });

  factory DockerPlan.fromJson(Map<String, Object?> j) => DockerPlan(
    available: j['available'] == true,
    orphanTestcontainers: _int(j['orphan_testcontainers']),
    reclaimableBytes: _int(j['reclaimable_bytes']),
  );

  final bool available;
  final int orphanTestcontainers;
  final int reclaimableBytes;
}

class Plan {
  const Plan({
    required this.level,
    required this.createdAt,
    required this.actions,
    required this.notes,
    required this.docker,
  });

  factory Plan.fromJson(Map<String, Object?> j) => Plan(
    level: _str(j['level']),
    createdAt: _str(j['created_at']),
    actions: _list(j['actions']).map(PlanAction.fromJson).toList(),
    notes: (j['notes'] as List?)?.map(_str).toList() ?? const [],
    docker: j['docker'] == null ? null : DockerPlan.fromJson(_map(j['docker'])),
  );

  final String level;
  final String createdAt;
  final List<PlanAction> actions;
  final List<String> notes;
  final DockerPlan? docker;

  int estimatedBytes([Set<int>? selected]) => actions
      .where(
        (a) => !a.isCompress && (selected == null || selected.contains(a.id)),
      )
      .fold(0, (s, a) => s + a.bytes);
}

class ActionResult {
  const ActionResult({
    required this.id,
    required this.freed,
    required this.ok,
    this.error,
  });

  factory ActionResult.fromJson(Map<String, Object?> j) => ActionResult(
    id: _int(j['id']),
    freed: _int(j['freed']),
    ok: j['ok'] == true,
    error: j['error']?.toString(),
  );

  final int id;
  final int freed;
  final bool ok;
  final String? error;
}

class RunSummary {
  const RunSummary({
    required this.level,
    required this.dryRun,
    required this.durationSecs,
    required this.actionCount,
    required this.freedBytes,
    required this.dockerFreedBytes,
    required this.results,
    required this.dockerLines,
    required this.freeAfter,
  });

  factory RunSummary.fromJson(Map<String, Object?> j) => RunSummary(
    level: _str(j['level']),
    dryRun: j['dry_run'] == true,
    durationSecs: _double(j['duration_secs']),
    actionCount: _int(j['action_count']),
    freedBytes: _int(j['freed_bytes']),
    dockerFreedBytes: _int(j['docker_freed_bytes']),
    results: _list(j['results']).map(ActionResult.fromJson).toList(),
    dockerLines: (j['docker_lines'] as List?)?.map(_str).toList() ?? const [],
    freeAfter: _int(j['free_after']),
  );

  final String level;
  final bool dryRun;
  final double durationSecs;
  final int actionCount;
  final int freedBytes;
  final int dockerFreedBytes;
  final List<ActionResult> results;
  final List<String> dockerLines;
  final int freeAfter;

  int get totalFreed => freedBytes + dockerFreedBytes;
}

class SizedPath {
  const SizedPath(this.path, this.bytes);

  factory SizedPath.fromJson(Map<String, Object?> j) =>
      SizedPath(_str(j['path']), _int(j['bytes']));

  final String path;
  final int bytes;
}

class Report {
  const Report({
    required this.total,
    required this.used,
    required this.available,
    required this.categories,
    required this.rustTargets,
    required this.nodeModules,
    required this.scratch,
    required this.dockerDf,
  });

  factory Report.fromJson(Map<String, Object?> j) => Report(
    total: _int(j['total']),
    used: _int(j['used']),
    available: _int(j['available']),
    categories: _map(j['categories']).map((k, v) => MapEntry(k, _int(v))),
    rustTargets: _list(j['rust_targets']).map(SizedPath.fromJson).toList(),
    nodeModules: _list(j['node_modules']).map(SizedPath.fromJson).toList(),
    scratch: _list(j['scratch']).map(SizedPath.fromJson).toList(),
    dockerDf: j['docker_df']?.toString(),
  );

  final int total;
  final int used;
  final int available;
  final Map<String, int> categories;
  final List<SizedPath> rustTargets;
  final List<SizedPath> nodeModules;
  final List<SizedPath> scratch;
  final String? dockerDf;
}

class TopAction {
  const TopAction({
    required this.path,
    required this.bytes,
    required this.reason,
    required this.group,
  });

  factory TopAction.fromJson(Map<String, Object?> j) => TopAction(
    path: _str(j['path']),
    bytes: _int(j['bytes']),
    reason: _str(j['reason']),
    group: _str(j['group']),
  );

  final String path;
  final int bytes;
  final String reason;
  final String group;
}

class RunRecord {
  const RunRecord({
    required this.ts,
    required this.level,
    required this.dryRun,
    required this.freed,
    required this.actions,
    required this.availableAfter,
    required this.top,
  });

  factory RunRecord.fromJson(Map<String, Object?> j) => RunRecord(
    ts: _str(j['ts']),
    level: _str(j['level']),
    dryRun: j['dry_run'] == true,
    freed: _int(j['freed']),
    actions: _int(j['actions']),
    availableAfter: _int(j['available_after']),
    top: _list(j['top']).map(TopAction.fromJson).toList(),
  );

  final String ts;
  final String level;
  final bool dryRun;
  final int freed;
  final int actions;
  final int availableAfter;
  final List<TopAction> top;
}

class DayStat {
  const DayStat({
    required this.day,
    required this.usedFirst,
    required this.usedLast,
    required this.freed,
    required this.runs,
  });

  final String day;
  final int usedFirst;
  final int usedLast;
  final int freed;
  final int runs;
}

class RepeatOffender {
  const RepeatOffender({
    required this.path,
    required this.times,
    required this.bytes,
  });

  factory RepeatOffender.fromJson(Map<String, Object?> j) => RepeatOffender(
    path: _str(j['path']),
    times: _int(j['times']),
    bytes: _int(j['bytes']),
  );

  final String path;
  final int times;
  final int bytes;
}

class Stats {
  const Stats({
    required this.days,
    required this.categoriesFirst,
    required this.categoriesLast,
    required this.totalFreed,
    required this.runs,
    required this.repeatOffenders,
  });

  factory Stats.fromJson(Map<String, Object?> j) {
    final days = <DayStat>[];
    for (final e in (j['days'] as List? ?? const [])) {
      if (e is List && e.length == 2) {
        final d = _map(e[1]);
        days.add(
          DayStat(
            day: _str(e[0]),
            usedFirst: _int(d['used_first']),
            usedLast: _int(d['used_last']),
            freed: _int(d['freed']),
            runs: _int(d['runs']),
          ),
        );
      }
    }
    Map<String, int>? cats(Object? v) {
      if (v is List && v.length == 2) {
        return _map(v[1]).map((k, x) => MapEntry(k, _int(x)));
      }
      return null;
    }

    return Stats(
      days: days,
      categoriesFirst: cats(j['categories_first']),
      categoriesLast: cats(j['categories_last']),
      totalFreed: _int(j['total_freed']),
      runs: _int(j['runs']),
      repeatOffenders: _list(
        j['repeat_offenders'],
      ).map(RepeatOffender.fromJson).toList(),
    );
  }

  final List<DayStat> days;
  final Map<String, int>? categoriesFirst;
  final Map<String, int>? categoriesLast;
  final int totalFreed;
  final int runs;
  final List<RepeatOffender> repeatOffenders;
}

class ProcInfo {
  const ProcInfo({
    required this.pid,
    required this.rss,
    required this.runHours,
    required this.program,
    required this.command,
  });

  factory ProcInfo.fromJson(Map<String, Object?> j) => ProcInfo(
    pid: _int(j['pid']),
    rss: _int(j['rss']),
    runHours: _double(j['run_hours']),
    program: _str(j['program']),
    command: _str(j['command']),
  );

  final int pid;
  final int rss;
  final double runHours;
  final String program;
  final String command;
}

class ProgramMem {
  const ProgramMem(this.name, this.bytes, this.count);

  final String name;
  final int bytes;
  final int count;
}

class MemReport {
  const MemReport({
    required this.total,
    required this.used,
    required this.available,
    required this.swapTotal,
    required this.swapUsed,
    required this.pressure,
    required this.top,
    required this.byProgram,
    required this.orphans,
    required this.orphanContainers,
  });

  factory MemReport.fromJson(Map<String, Object?> j) => MemReport(
    total: _int(j['total']),
    used: _int(j['used']),
    available: _int(j['available']),
    swapTotal: _int(j['swap_total']),
    swapUsed: _int(j['swap_used']),
    pressure: j['pressure'] == true,
    top: _list(j['top']).map(ProcInfo.fromJson).toList(),
    byProgram: (j['by_program'] as List? ?? const [])
        .whereType<List<Object?>>()
        .where((e) => e.length == 3)
        .map((e) => ProgramMem(_str(e[0]), _int(e[1]), _int(e[2])))
        .toList(),
    orphans: _list(j['orphans']).map(ProcInfo.fromJson).toList(),
    orphanContainers: _int(j['orphan_containers']),
  );

  final int total;
  final int used;
  final int available;
  final int swapTotal;
  final int swapUsed;
  final bool pressure;
  final List<ProcInfo> top;
  final List<ProgramMem> byProgram;
  final List<ProcInfo> orphans;
  final int orphanContainers;
}

/// A progress event from the daemon (`stage` discriminated).
class ProgressEvent {
  const ProgressEvent(this.stage, this.data);

  factory ProgressEvent.fromJson(Map<String, Object?> j) =>
      ProgressEvent(_str(j['stage']), j);

  final String stage;
  final Map<String, Object?> data;

  String? get rule => data['rule']?.toString();
  int get index => _int(data['index']);
  int get total => _int(data['total']);
  PlanAction? get action =>
      data['action'] == null ? null : PlanAction.fromJson(_map(data['action']));
  ActionResult? get result => data['result'] == null
      ? null
      : ActionResult.fromJson(_map(data['result']));
  String? get line => data['line']?.toString();
}

class Snapshot {
  const Snapshot(this.used, this.available, this.level, this.at);

  final int used;
  final int available;
  final String level;
  final DateTime at;
}

/// The dashboard: owns the daemon connection and all fetched state, renders one screen per tab.
library;

import 'dart:async';
import 'dart:io';

import 'package:cinder/cinder.dart';

import 'client.dart';
import 'format.dart';
import 'model.dart';
import 'screens/history.dart';
import 'screens/log.dart';
import 'screens/memory.dart';
import 'screens/overview.dart';
import 'screens/plan.dart';
import 'screens/report.dart';
import 'widgets.dart';

class DiskgcApp extends StatefulWidget {
  const DiskgcApp({super.key, this.connect = DaemonClient.connect});

  /// Injectable for tests.
  final Future<DaemonClient> Function() connect;

  @override
  State<DiskgcApp> createState() => _DiskgcAppState();
}

class _DiskgcAppState extends State<DiskgcApp> {
  static const _tabs = <String>[
    'Overview',
    'Plan',
    'Report',
    'History',
    'Memory',
    'Log',
  ];

  DaemonClient? _client;
  StreamSubscription<Map<String, Object?>>? _events;
  Timer? _statusTimer;
  int _tab = 0;
  bool _busy = false;
  String? _scanning;
  String? _error;

  Status? _status;
  Report? _report;
  Plan? _plan;
  final Set<int> _selected = <int>{};
  PlanAction? _highlighted;
  RunSummary? _lastSummary;
  int _progressIndex = 0;
  int _progressTotal = 0;
  PlanAction? _progressAction;
  List<RunRecord> _runs = const <RunRecord>[];
  Stats? _stats;
  MemReport? _mem;
  int? _killed;
  final List<Snapshot> _snapshots = <Snapshot>[];
  final List<String> _log = <String>[];

  @override
  void initState() {
    super.initState();
    // Global hotkeys run in the capture phase, before any focused widget sees the key, so they
    // work the same whether a table, a list or nothing at all has focus.
    TerminalBinding.instance.inputRouter.add(InputPhase.capture, _captureInput);
    _connect();
  }

  InputDisposition _captureInput(InputEvent event, InputPhase phase) {
    if (event is KeyboardInputEvent && _onKey(event.event)) {
      return InputDisposition.stopPropagation;
    }
    return InputDisposition.ignored;
  }

  @override
  void dispose() {
    TerminalBinding.instance.inputRouter.remove(
      InputPhase.capture,
      _captureInput,
    );
    _statusTimer?.cancel();
    _events?.cancel();
    _client?.close();
    super.dispose();
  }

  Future<void> _connect() async {
    try {
      final c = await widget.connect();
      _client = c;
      await c.subscribe();
      _events = c.events.listen(_onEvent);
      c.closed.then((_) {
        if (!mounted) return;
        setState(() {
          _client = null;
          _error = 'Connection to the daemon was lost. Press r to reconnect.';
        });
      });
      _addLog('connected to the daemon');
      await _refreshStatus();
      _statusTimer = Timer.periodic(
        const Duration(seconds: 30),
        (_) => _refreshStatus(),
      );
    } catch (e) {
      setState(
        () => _error =
            'Cannot reach the daemon: $e  (start it with `diskgc install`, then press r)',
      );
    }
  }

  void _addLog(String line) {
    final ts = DateTime.now();
    final stamp =
        '${ts.hour.toString().padLeft(2, '0')}:${ts.minute.toString().padLeft(2, '0')}:${ts.second.toString().padLeft(2, '0')}';
    _log.add('$stamp  $line');
    if (_log.length > 500) _log.removeRange(0, _log.length - 500);
  }

  void _onEvent(Map<String, Object?> msg) {
    final data = msg['data'];
    final map = data is Map
        ? data.cast<String, Object?>()
        : const <String, Object?>{};
    switch (msg['event']) {
      case 'snapshot':
        final s = Snapshot(
          (map['used'] as num?)?.toInt() ?? 0,
          (map['available'] as num?)?.toInt() ?? 0,
          map['level']?.toString() ?? '',
          DateTime.now(),
        );
        setState(() {
          _snapshots.add(s);
          if (_snapshots.length > 300) _snapshots.removeAt(0);
        });
      case 'log':
        setState(() => _addLog('${map['level']}  ${map['message']}'));
      case 'progress':
        _onProgress(ProgressEvent.fromJson(map));
      case 'run_finished':
        final s = RunSummary.fromJson(map);
        setState(() {
          _lastSummary = s;
          _addLog(
            'run finished: ${humanBytes(s.totalFreed)} freed in ${s.actionCount} actions',
          );
        });
        _refreshStatus();
      default:
        break;
    }
  }

  void _onProgress(ProgressEvent p) {
    setState(() {
      switch (p.stage) {
        case 'scanning':
          _scanning = p.rule;
        case 'planned':
          _scanning = null;
          _progressIndex = 0;
          _progressTotal = 0;
        case 'executing':
          _progressIndex = p.index;
          _progressTotal = p.total;
          _progressAction = p.action;
        case 'executed':
          final r = p.result;
          if (r != null && !r.ok) _addLog('failed: ${r.error}');
        case 'docker':
          if (p.line != null) _addLog(p.line!);
        case 'finished':
          _progressIndex = 0;
          _progressTotal = 0;
          _progressAction = null;
          _scanning = null;
      }
    });
  }

  Future<T?> _guard<T>(Future<T> Function(DaemonClient c) f) async {
    final c = _client;
    if (c == null) {
      await _connect();
      return null;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      return await f(c);
    } on DaemonError catch (e) {
      setState(() => _error = e.message);
      _addLog('error: ${e.message}');
      return null;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _refreshStatus() async {
    final c = _client;
    if (c == null) return;
    try {
      final s = Status.fromJson(await c.callMap('status'));
      if (mounted) setState(() => _status = s);
    } on DaemonError catch (e) {
      if (mounted) setState(() => _error = e.message);
    }
  }

  Future<void> _refreshReport({bool force = true}) => _guard((c) async {
    _addLog(force ? 'scanning the disk…' : 'loading the report…');
    final r = Report.fromJson(await c.callMap('report', {'refresh': force}));
    setState(() => _report = r);
    _addLog('report ready: ${r.categories.length} categories');
  });

  Future<void> _refreshHistory() => _guard((c) async {
    final runs = (await c.callList('history', {'limit': 50}))
        .whereType<Map<Object?, Object?>>()
        .map((m) => RunRecord.fromJson(m.cast<String, Object?>()))
        .toList();
    final stats = Stats.fromJson(await c.callMap('stats'));
    setState(() {
      _runs = runs;
      _stats = stats;
    });
  });

  Future<void> _refreshMemory() => _guard((c) async {
    final m = MemReport.fromJson(await c.callMap('memory'));
    setState(() {
      _mem = m;
      _killed = null;
    });
  });

  Future<void> _killOrphans() => _guard((c) async {
    final killed = await c.callList('kill_orphans');
    _addLog('terminated ${killed.length} orphaned helper processes');
    final m = MemReport.fromJson(await c.callMap('memory'));
    setState(() {
      _mem = m;
      _killed = killed.length;
    });
  });

  Future<void> _buildPlan(String? level) => _guard((c) async {
    _addLog('building a ${level ?? 'auto'} plan…');
    final p = Plan.fromJson(await c.callMap('plan', {'level': level}));
    setState(() {
      _plan = p;
      _lastSummary = null;
      _selected
        ..clear()
        ..addAll(p.actions.map((a) => a.id));
      _tab = 1;
    });
    _addLog(
      'plan ready: ${p.actions.length} actions, ~${humanBytes(p.estimatedBytes())}',
    );
  });

  Future<void> _execute({required bool dryRun}) => _guard((c) async {
    final p = _plan;
    if (p == null) return;
    final ids = p.actions
        .where((a) => a.isCompress || _selected.contains(a.id))
        .map((a) => a.id)
        .toList();
    _addLog('${dryRun ? 'dry run' : 'executing'} ${ids.length} actions…');
    final s = RunSummary.fromJson(
      await c.callMap('execute', {'action_ids': ids, 'dry_run': dryRun}),
    );
    setState(() => _lastSummary = s);
    if (!dryRun) {
      // Actions that ran are gone; keep the plan for the ones the user unticked.
      final done = s.results.where((r) => r.ok).map((r) => r.id).toSet();
      setState(() {
        _plan = Plan(
          level: p.level,
          createdAt: p.createdAt,
          actions: p.actions.where((a) => !done.contains(a.id)).toList(),
          notes: p.notes,
          docker: p.docker,
        );
        _selected.removeAll(done);
      });
      await _refreshStatus();
    }
  });

  void _toggle(PlanAction a) {
    setState(() {
      if (!_selected.remove(a.id)) _selected.add(a.id);
    });
  }

  void _toggleAll() {
    final p = _plan;
    if (p == null) return;
    setState(() {
      if (_selected.length >= p.actions.length) {
        _selected.clear();
      } else {
        _selected.addAll(p.actions.map((a) => a.id));
      }
    });
  }

  void _refreshCurrent() {
    if (_client == null) {
      _connect();
      return;
    }
    switch (_tab) {
      case 0:
        _refreshStatus();
        _refreshReport();
      case 1:
        _buildPlan(null);
      case 2:
        _refreshReport();
      case 3:
        _refreshHistory();
      case 4:
        _refreshMemory();
      default:
        _refreshStatus();
    }
  }

  void _selectTab(int i) {
    setState(() => _tab = i);
    // Lazily load what a tab needs the first time it is opened.
    if (i == 2 && _report == null && !_busy) _refreshReport(force: false);
    if (i == 3 && _stats == null && !_busy) _refreshHistory();
    if (i == 4 && _mem == null && !_busy) _refreshMemory();
  }

  /// Restore the terminal, drop the daemon connection and end the process. `shutdownApp` alone
  /// only tears the widget tree down; the open socket would keep the VM alive.
  void _quit() {
    _statusTimer?.cancel();
    _events?.cancel();
    _client?.close();
    shutdownApp();
    Future<void>.delayed(const Duration(milliseconds: 150), () => exit(0));
  }

  bool _onKey(KeyboardEvent e) {
    if (e.isControlPressed && e.logicalKey == LogicalKey.keyC) {
      _quit();
      return true;
    }
    if (e.isControlPressed || e.isAltPressed || e.isMetaPressed) return false;
    final k = e.logicalKey;
    if (k == LogicalKey.keyQ) {
      _quit();
      return true;
    }
    if (k == LogicalKey.keyR) {
      _refreshCurrent();
      return true;
    }
    if (k == LogicalKey.digit1) return _switch(0);
    if (k == LogicalKey.digit2) return _switch(1);
    if (k == LogicalKey.digit3) return _switch(2);
    if (k == LogicalKey.digit4) return _switch(3);
    if (k == LogicalKey.digit5) return _switch(4);
    if (k == LogicalKey.digit6) return _switch(5);
    if (k == LogicalKey.tab) return _switch((_tab + 1) % _tabs.length);
    if (k == LogicalKey.keyP) {
      if (!_busy) _buildPlan(null);
      return true;
    }
    if (_tab == 1) {
      if (k == LogicalKey.space && _highlighted != null) {
        _toggle(_highlighted!);
        return true;
      }
      if (k == LogicalKey.keyA) {
        _toggleAll();
        return true;
      }
      if (k == LogicalKey.keyX &&
          !_busy &&
          _plan != null &&
          _selected.isNotEmpty) {
        _execute(dryRun: false);
        return true;
      }
      if (k == LogicalKey.keyN && !_busy && _plan != null) {
        _execute(dryRun: true);
        return true;
      }
    }
    if (_tab == 4 && k == LogicalKey.keyK && !_busy) {
      _killOrphans();
      return true;
    }
    return false;
  }

  bool _switch(int i) {
    _selectTab(i);
    return true;
  }

  @override
  Widget build(BuildContext context) {
    final theme = TuiTheme.of(context);
    final disk = _status?.disk;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 1),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          Row(
            children: <Widget>[
              Text(
                'diskgc',
                style: TextStyle(
                  color: theme.primary,
                  fontWeight: FontWeight.bold,
                ),
              ),
              const SizedBox(width: 2),
              if (disk != null) ...<Widget>[
                Text(
                  '${humanBytes(disk.available)} free',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: theme.onBackground,
                  ),
                ),
                const SizedBox(width: 1),
                LevelBadge(disk.level),
              ],
              const Spacer(),
              if (_busy)
                Text(
                  _scanning == null ? 'working…' : 'scanning $_scanning…',
                  style: TextStyle(color: theme.warning),
                ),
              if (!_busy)
                Text(
                  _client == null ? 'disconnected' : 'connected',
                  style: TextStyle(
                    color: _client == null ? theme.error : theme.success,
                  ),
                ),
            ],
          ),
          if (_error != null)
            Text(
              _error!,
              style: TextStyle(color: theme.error),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          const SizedBox(height: 1),
          Expanded(
            child: Tabs(
              tabs: _tabs
                  .asMap()
                  .entries
                  .map((e) => TabItem(label: '${e.key + 1} ${e.value}'))
                  .toList(),
              selectedIndex: _tab,
              onSelected: _selectTab,
              children: <Widget>[
                OverviewScreen(
                  status: _status,
                  report: _report,
                  snapshots: _snapshots,
                  lastSummary: _lastSummary,
                  connected: _client != null,
                ),
                PlanScreen(
                  plan: _plan,
                  selected: _selected,
                  busy: _busy,
                  scanning: _scanning,
                  progressIndex: _progressIndex,
                  progressTotal: _progressTotal,
                  progressAction: _progressAction,
                  lastSummary: _lastSummary,
                  onToggle: _toggle,
                  onHighlight: (a) => _highlighted = a,
                  onPlan: (level) => _buildPlan(level),
                  onExecute: ({required bool dryRun}) =>
                      _execute(dryRun: dryRun),
                  onToggleAll: _toggleAll,
                ),
                ReportScreen(report: _report, busy: _busy, scanning: _scanning),
                HistoryScreen(runs: _runs, stats: _stats),
                MemoryScreen(
                  report: _mem,
                  busy: _busy,
                  onKillOrphans: _killOrphans,
                  killed: _killed,
                ),
                LogScreen(lines: _log),
              ],
            ),
          ),
          const SizedBox(height: 1),
          const Hint('1-6 / tab switch screens · r refresh · p plan · q quit'),
        ],
      ),
    );
  }
}

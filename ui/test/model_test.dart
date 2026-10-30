import 'dart:convert';

import 'package:diskgc_ui/src/format.dart';
import 'package:diskgc_ui/src/model.dart';
import 'package:test/test.dart';

void main() {
  group('format', () {
    test('human bytes match the Rust side', () {
      expect(humanBytes(0), '0 B');
      expect(humanBytes(2048), '2 KB');
      expect(humanBytes(5 * 1024 * 1024), '5 MB');
      expect(humanBytes(1.5 * 1024 * 1024 * 1024), '1.5 GB');
    });

    test('fitPath keeps the tail', () {
      expect(fitPath('/a/b/c/d/e', 5), '…/d/e');
      expect(fitPath('/a/b', 10), '/a/b');
    });
  });

  group('model', () {
    test('parses a plan with flattened action kinds', () {
      final json =
          jsonDecode('''
      {"level":"low","created_at":"2026-09-05 15:00:00",
       "actions":[
         {"id":1,"group":"rust","kind":"sweep_older_than","days":3,"path":"/p/target","bytes":1000,"reason":"old"},
         {"id":2,"group":"cache","kind":"command","argv":["go","clean","-cache"],"measure":"/c","path":"/c","bytes":5,"reason":"big"},
         {"id":3,"group":"compress","kind":"compress","path":"/l.jsonl","bytes":7,"reason":"cold"}
       ],
       "notes":["kept x"],
       "docker":{"available":true,"orphan_testcontainers":2,"stopped_hours":24,"build_cache_days":7,"image_prune_all":false,"reclaimable_bytes":9}}
      ''')
              as Map;
      final plan = Plan.fromJson(json.cast<String, Object?>());
      expect(plan.level, 'low');
      expect(plan.actions.length, 3);
      expect(plan.actions[0].verb, 'sweep >3d');
      expect(plan.actions[1].verb, 'go clean -cache');
      expect(plan.actions[2].isCompress, isTrue);
      expect(plan.estimatedBytes(), 1005, reason: 'compression is not counted');
      expect(plan.estimatedBytes({1}), 1000);
      expect(plan.docker?.orphanTestcontainers, 2);
    });

    test('parses stats with tuple-shaped days and categories', () {
      final json =
          jsonDecode('''
      {"days":[["2026-09-05",{"used_first":10,"used_last":12,"freed":3,"runs":1}]],
       "categories_first":["2026-09-05 10:00:00",{"rust_targets":5}],
       "categories_last":["2026-09-05 11:00:00",{"rust_targets":2}],
       "total_freed":3,"runs":1,
       "repeat_offenders":[{"path":"/x","group":"rust","times":2,"bytes":9}]}
      ''')
              as Map;
      final s = Stats.fromJson(json.cast<String, Object?>());
      expect(s.days.single.usedLast, 12);
      expect(s.categoriesLast?['rust_targets'], 2);
      expect(s.repeatOffenders.single.times, 2);
    });

    test('memory report tolerates missing fields', () {
      final m = MemReport.fromJson(<String, Object?>{
        'total': 1,
        'by_program': [
          ['node', 5, 2],
        ],
      });
      expect(m.byProgram.single.name, 'node');
      expect(m.orphans, isEmpty);
      expect(m.pressure, isFalse);
    });

    test('status parses forecast and last run', () {
      final json =
          jsonDecode('''
      {"version":"0.2.0","disk":{"volume":"/","total":100,"used":40,"available":60,"level":"normal"},
       "thresholds_gb":[150,100,60],"autostart":{"installed":true,"running":true,"pid":1,"detail":"ok"},
       "forecast":{"rate_bytes_per_hour":1000.5,"window_hours":6,"samples":9,"hours_to_critical":null,"available_in_24h":50},
       "last_run":{"kind":"run","ts":"t","level":"low","dry_run":false,"freed":9,"actions":2,"available_after":60,"duration_secs":1.0,"top":[]},
       "config_path":"/c","log_path":"/l","compression_supported":true}
      ''')
              as Map;
      final s = Status.fromJson(json.cast<String, Object?>());
      expect(s.disk.usedFraction, 0.4);
      expect(s.forecast?.hoursToCritical, isNull);
      expect(s.lastRun?.freed, 9);
      expect(s.daemonRunning, isTrue);
    });
  });
}

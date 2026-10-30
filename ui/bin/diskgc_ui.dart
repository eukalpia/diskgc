import 'dart:io';

import 'package:cinder/cinder.dart';
import 'package:diskgc_ui/src/app.dart';
import 'package:diskgc_ui/src/client.dart';

Future<void> main(List<String> args) async {
  if (args.contains('--help') || args.contains('-h')) {
    stdout.writeln('diskgc-ui: terminal dashboard for the diskgc daemon.');
    stdout.writeln(
      'Keys: 1-6 screens, r refresh, p plan, space toggle, x execute, n dry run, q quit.',
    );
    return;
  }
  try {
    await ensureDaemon();
  } on DaemonError catch (e) {
    stderr.writeln('diskgc-ui: $e');
    exitCode = 1;
    return;
  }
  await runApp(
    const CinderApp(
      title: 'diskgc',
      theme: TuiThemeData.nord,
      child: DiskgcApp(),
    ),
  );
  // runApp returns once the widget tree is torn down; the socket to the daemon and the status
  // timer would otherwise keep the VM alive.
  exit(0);
}

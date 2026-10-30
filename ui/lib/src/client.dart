/// NDJSON client for the diskgc daemon (docs/PROTOCOL.md).
///
/// Unix: a Unix domain socket. Windows: loopback TCP; the endpoint file holds `port\ntoken\n`
/// and the token is sent as the first line.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

class DaemonError implements Exception {
  DaemonError(this.message);
  final String message;
  @override
  String toString() => message;
}

/// Where the daemon listens; mirrors `paths.rs` in the Rust crate.
String endpointPath() {
  final env = Platform.environment;
  final runtime = env['DISKGC_RUNTIME_DIR'];
  final home = env['HOME'] ?? env['USERPROFILE'] ?? '';
  if (Platform.isWindows) {
    final base = runtime ?? '${env['LOCALAPPDATA'] ?? home}\\diskgc';
    return '$base\\diskgc.ipc';
  }
  if (runtime != null && runtime.isNotEmpty) return '$runtime/diskgc.sock';
  final xdgRuntime = env['XDG_RUNTIME_DIR'];
  if (xdgRuntime != null && xdgRuntime.isNotEmpty) {
    return '$xdgRuntime/diskgc/diskgc.sock';
  }
  final data = env['XDG_DATA_HOME'] ?? '$home/.local/share';
  return '$data/diskgc/diskgc.sock';
}

class DaemonClient {
  DaemonClient._(this._socket);

  final Socket _socket;
  final _pending = <int, Completer<Object?>>{};
  final _events = StreamController<Map<String, Object?>>.broadcast();
  final _closed = Completer<void>();
  int _nextId = 1;

  /// Events pushed by the daemon after [subscribe]: `{event: ..., data: ...}`.
  Stream<Map<String, Object?>> get events => _events.stream;

  Future<void> get closed => _closed.future;
  bool get isClosed => _closed.isCompleted;

  static Future<DaemonClient> connect({
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final Socket socket;
    if (Platform.isWindows) {
      final text = await File(endpointPath()).readAsString();
      final lines = text.split('\n');
      final port = int.parse(lines[0].trim());
      socket = await Socket.connect(
        InternetAddress.loopbackIPv4,
        port,
        timeout: timeout,
      );
      socket.write('${lines[1].trim()}\n');
    } else {
      final addr = InternetAddress(
        endpointPath(),
        type: InternetAddressType.unix,
      );
      socket = await Socket.connect(addr, 0, timeout: timeout);
    }
    final client = DaemonClient._(socket);
    client._listen();
    return client;
  }

  void _listen() {
    _socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(_onLine, onDone: _onDone, onError: (Object e) => _onDone());
  }

  void _onLine(String line) {
    if (line.trim().isEmpty) return;
    final Object? decoded;
    try {
      decoded = jsonDecode(line);
    } catch (_) {
      return;
    }
    if (decoded is! Map) return;
    final msg = decoded.cast<String, Object?>();
    if (msg.containsKey('event')) {
      _events.add(msg);
      return;
    }
    final id = msg['id'];
    if (id is! int) return;
    final completer = _pending.remove(id);
    if (completer == null) return;
    if (msg['result'] == 'ok') {
      completer.complete(msg['data']);
    } else {
      final data = msg['data'];
      final message = data is Map ? data['message']?.toString() : null;
      completer.completeError(DaemonError(message ?? 'unknown daemon error'));
    }
  }

  void _onDone() {
    for (final c in _pending.values) {
      if (!c.isCompleted) c.completeError(DaemonError('connection closed'));
    }
    _pending.clear();
    if (!_closed.isCompleted) _closed.complete();
    _events.close();
  }

  Future<Object?> call(String method, [Map<String, Object?>? params]) {
    if (isClosed) return Future.error(DaemonError('connection closed'));
    final id = _nextId++;
    final completer = Completer<Object?>();
    _pending[id] = completer;
    final req = <String, Object?>{'id': id, 'method': method};
    if (params != null) req['params'] = params;
    _socket.write('${jsonEncode(req)}\n');
    return completer.future;
  }

  Future<Map<String, Object?>> callMap(
    String method, [
    Map<String, Object?>? params,
  ]) async {
    final r = await call(method, params);
    return r is Map ? r.cast<String, Object?>() : <String, Object?>{};
  }

  Future<List<Object?>> callList(
    String method, [
    Map<String, Object?>? params,
  ]) async {
    final r = await call(method, params);
    return r is List ? r : const <Object?>[];
  }

  Future<void> subscribe() => call('subscribe');

  Future<void> close() async {
    await _socket.close();
    _onDone();
  }
}

/// Start `diskgc daemon` detached when no daemon answers.
Future<void> ensureDaemon() async {
  try {
    final c = await DaemonClient.connect(
      timeout: const Duration(milliseconds: 800),
    );
    await c.call('ping');
    await c.close();
    return;
  } catch (_) {}
  final exe = Platform.isWindows ? 'diskgc.exe' : 'diskgc';
  final candidates = <String>[
    '${File(Platform.resolvedExecutable).parent.path}${Platform.pathSeparator}$exe',
    exe,
  ];
  for (final bin in candidates) {
    try {
      await Process.start(bin, const [
        'daemon',
      ], mode: ProcessStartMode.detached);
      break;
    } catch (_) {
      continue;
    }
  }
  for (var i = 0; i < 25; i++) {
    await Future<void>.delayed(const Duration(milliseconds: 200));
    try {
      final c = await DaemonClient.connect(
        timeout: const Duration(milliseconds: 500),
      );
      await c.close();
      return;
    } catch (_) {}
  }
  throw DaemonError('could not start the diskgc daemon; is `diskgc` on PATH?');
}

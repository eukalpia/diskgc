/// Formatting helpers shared by all screens.
library;

import 'dart:io';

const int _kib = 1024;
const int _mib = 1024 * _kib;
const int _gib = 1024 * _mib;

/// `1.5 GB`, `320 MB`, `12 KB`, `0 B` — same units as the Rust side.
String humanBytes(num bytes) {
  final b = bytes.toDouble();
  if (b >= _gib) return '${(b / _gib).toStringAsFixed(1)} GB';
  if (b >= _mib) return '${(b / _mib).toStringAsFixed(0)} MB';
  if (b >= _kib) return '${(b / _kib).toStringAsFixed(0)} KB';
  return '${b.toStringAsFixed(0)} B';
}

double gib(num bytes) => bytes / _gib;

String? _home;

String homeDir() {
  return _home ??=
      Platform.environment['HOME'] ?? Platform.environment['USERPROFILE'] ?? '';
}

/// Replace the home directory prefix with `~`.
String shortenPath(String path) {
  final home = homeDir();
  if (home.isNotEmpty && path.startsWith(home)) {
    return '~${path.substring(home.length)}';
  }
  return path;
}

/// Keep the tail of a path when it does not fit.
String fitPath(String path, int width) {
  final p = shortenPath(path);
  if (p.length <= width) return p;
  if (width <= 1) return p.substring(p.length - width);
  return '…${p.substring(p.length - width + 1)}';
}

String pad(String s, int width, {bool right = false}) {
  if (s.length >= width) return s.substring(0, width);
  return right ? s.padLeft(width) : s.padRight(width);
}

String hours(double h) {
  if (h < 1) return '${(h * 60).round()} min';
  if (h < 48) return '${h.toStringAsFixed(h < 10 ? 1 : 0)} h';
  return '${(h / 24).toStringAsFixed(1)} d';
}

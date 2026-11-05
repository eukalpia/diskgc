# Contributing to diskgc

Thanks for helping. diskgc deletes files on other people's machines, so the bar
for rules is "provably safe", not "usually fine".

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
# if you touched the dashboard:
cd ui && dart analyze --fatal-infos && dart test
```

CI runs the same on macOS, Linux and Windows.

## Rules

- A rule only proposes what a tool can regenerate, and every action carries a
  human-readable reason.
- Activity is judged on the whole project, not on the directory being removed.
- New or changed rules come with tests in `crates/diskgc-core/tests/rules.rs`
  (or the rule's own module) covering what is removed **and** what must stay.
- Platform-specific code lives in `crates/diskgc-core/src/platform/`; keep the
  other platforms compiling (`cargo clippy --target x86_64-pc-windows-msvc`
  works from any host for a check).

## Reporting a bug

Include `diskgc --version`, your OS, and the relevant part of `diskgc plan` or
`diskgc history` (paths can be redacted). If diskgc removed something it should
not have, say so in the title — those are fixed first.

User-facing strings are in English.

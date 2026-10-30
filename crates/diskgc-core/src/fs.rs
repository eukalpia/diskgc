//! Filesystem scanning primitives: sizes that agree with `du`, idleness checks with early exit,
//! and lookups for well-known build directories.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use walkdir::WalkDir;

use crate::platform;

pub fn days(n: u64) -> Duration {
    Duration::from_secs(n * 86_400)
}

pub fn hours(n: u64) -> Duration {
    Duration::from_secs(n * 3_600)
}

pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Bytes on disk under `root` for files matching `pred` (directories always count). Hard links
/// are counted once.
pub fn walk_bytes(root: &Path, pred: impl Fn(&Path, &std::fs::Metadata) -> bool) -> u64 {
    match std::fs::symlink_metadata(root) {
        Ok(m) if !m.is_dir() => return platform::file_usage(&m),
        Err(_) => return 0,
        _ => {}
    }
    // Directories are read and their entries stat'ed in parallel (jwalk + rayon); the cheap
    // part — dedupe and the predicate — runs on the consuming thread.
    let walker = jwalk::WalkDirGeneric::<((), Option<std::fs::Metadata>)>::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .process_read_dir(|_depth, _path, _state, children| {
            children.retain(|c| {
                c.as_ref()
                    .map(|c| !platform::is_protected(&c.path()))
                    .unwrap_or(true)
            });
            for child in children.iter_mut().flatten() {
                child.client_state = child.metadata().ok();
            }
        });
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut total = 0u64;
    for e in walker.into_iter().filter_map(Result::ok) {
        let Some(m) = e.client_state.as_ref() else {
            continue;
        };
        if let Some(key) = platform::hardlink_key(m) {
            if !seen.insert(key) {
                continue;
            }
        }
        if e.file_type.is_dir() || (!e.file_type.is_symlink() && pred(&e.path(), m)) {
            total += platform::file_usage(m);
        }
    }
    total
}

pub fn disk_usage(p: &Path) -> u64 {
    walk_bytes(p, |_, _| true)
}

/// Any regular file below `p` (skipping directories named in `prune`) modified after `cutoff`?
/// Stops at the first hit. Unreadable entries count as "alive" so we err on the safe side.
pub fn has_file_newer_than(p: &Path, cutoff: SystemTime, prune: &[&str]) -> bool {
    if let Ok(m) = std::fs::symlink_metadata(p) {
        if !m.is_dir() {
            return m.modified().map(|t| t > cutoff).unwrap_or(true);
        }
    }
    let walker = WalkDir::new(p)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !(e.depth() > 0
                && e.file_type().is_dir()
                && (prune.iter().any(|n| e.file_name().to_string_lossy() == *n)
                    || platform::is_protected(e.path())))
        });
    for e in walker {
        let Ok(e) = e else { return true };
        if !e.file_type().is_file() {
            continue;
        }
        match e.metadata().ok().and_then(|m| m.modified().ok()) {
            Some(t) if t > cutoff => return true,
            None => return true,
            _ => {}
        }
    }
    false
}

/// Nothing below `p` changed for `d`.
pub fn idle_for(p: &Path, d: Duration, prune: &[&str]) -> bool {
    let cutoff = SystemTime::now()
        .checked_sub(d)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    !has_file_newer_than(p, cutoff, prune)
}

/// Directories never descended into while looking for build outputs.
const ALWAYS_SKIP: &[&str] = &["node_modules", ".git", "Library", "AppData"];

/// Directories called `name` under `roots`, at most `depth` deep. The contents of a match are not
/// searched further (a `node_modules` inside `node_modules` is part of the outer one).
pub fn find_dirs_named(roots: &[PathBuf], depth: usize, name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(depth)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !(e.depth() > 0
                    && e.file_type().is_dir()
                    && ((ALWAYS_SKIP.contains(&n.as_ref()) && n != name)
                        || platform::is_protected(e.path())))
            });
        while let Some(Ok(e)) = it.next() {
            if e.depth() > 0 && e.file_type().is_dir() && e.file_name().to_string_lossy() == name {
                out.push(e.path().to_path_buf());
                it.skip_current_dir();
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Cargo build directories: any directory holding `.rustc_info.json` or a cargo `CACHEDIR.TAG`.
/// Matching by content rather than by the name `target` also finds the ad-hoc
/// `CARGO_TARGET_DIR`s that tools and agents create in temp folders.
pub fn find_cargo_targets(roots: &[PathBuf], depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(depth)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !(e.depth() > 0
                    && e.file_type().is_dir()
                    && (ALWAYS_SKIP.contains(&n.as_ref()) || platform::is_protected(e.path())))
            });
        while let Some(Ok(e)) = it.next() {
            if !e.file_type().is_file() {
                continue;
            }
            let is_cargo = match e.file_name().to_str() {
                Some(".rustc_info.json") => true,
                Some("CACHEDIR.TAG") => std::fs::read_to_string(e.path())
                    .map(|t| t.to_lowercase().contains("cargo"))
                    .unwrap_or(false),
                _ => false,
            };
            if is_cargo {
                if let Some(parent) = e.path().parent() {
                    out.push(parent.to_path_buf());
                }
                it.skip_current_dir();
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Delete files matching `pred` under `root`, then remove directories left empty.
/// Returns bytes freed, measured (not estimated).
pub fn sweep(root: &Path, pred: impl Fn(&Path, &std::fs::Metadata) -> bool) -> u64 {
    let before = disk_usage(root);
    for e in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !e.file_type().is_file() {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if pred(e.path(), &m) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        let _ = std::fs::remove_dir(&d); // fails harmlessly when not empty
    }
    before.saturating_sub(disk_usage(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path, age: Duration) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
        let t = SystemTime::now() - age;
        filetime::set_file_mtime(p, filetime::FileTime::from_system_time(t)).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn protected_locations_are_skipped() {
        let h = crate::paths::home();
        assert!(platform::is_protected(
            &h.join("Library/Caches/com.apple.Music")
        ));
        assert!(platform::is_protected(
            &h.join("Library/Caches/com.apple.Safari/x")
        ));
        assert!(platform::is_protected(
            &h.join("Music/Music Library.musiclibrary")
        ));
        assert!(platform::is_protected(&h.join("Library/Mail/V10")));
        assert!(platform::is_protected(
            &h.join("Library/Group Containers/HUAQ24HBR6.dev.orbstack/data")
        ));
        assert!(!platform::is_protected(&h.join("Library/Caches/go-build")));
        assert!(!platform::is_protected(
            &h.join("Library/Application Support/Claude")
        ));
        assert!(!platform::is_protected(&h.join("Projects/app/target")));
    }

    #[test]
    fn idle_detection_respects_prune() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("src/main.rs"), days(40));
        touch(&d.path().join("node_modules/x/index.js"), days(0));
        assert!(idle_for(d.path(), days(30), &["node_modules"]));
        assert!(!idle_for(d.path(), days(30), &[]));
    }

    #[test]
    fn finds_cargo_targets_by_content() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("a/target/.rustc_info.json"), days(1));
        touch(&d.path().join("b/odd-name/CACHEDIR.TAG"), days(1));
        fs::write(d.path().join("b/odd-name/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by cargo.").unwrap();
        touch(&d.path().join("c/not-target/CACHEDIR.TAG"), days(1));
        fs::write(
            d.path().join("c/not-target/CACHEDIR.TAG"),
            "Signature: 8a477f597d28d172789f06886806bc55\n# created by pip",
        )
        .unwrap();
        let found = find_cargo_targets(&[d.path().to_path_buf()], 6);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("a/target")));
        assert!(found.iter().any(|p| p.ends_with("b/odd-name")));
    }

    #[test]
    fn sweep_removes_old_files_and_empty_dirs() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("deps/old.rlib"), days(10));
        touch(&d.path().join("deps/new.rlib"), days(1));
        let cutoff = SystemTime::now() - days(7);
        sweep(d.path(), |_, m| {
            m.modified().map(|t| t < cutoff).unwrap_or(false)
        });
        assert!(!d.path().join("deps/old.rlib").exists());
        assert!(d.path().join("deps/new.rlib").exists());
        touch(&d.path().join("empty/x"), days(10));
        sweep(d.path(), |_, m| {
            m.modified().map(|t| t < cutoff).unwrap_or(false)
        });
        assert!(!d.path().join("empty").exists());
    }

    #[test]
    fn find_dirs_named_does_not_descend_into_matches() {
        let d = tempfile::tempdir().unwrap();
        touch(
            &d.path().join("app/node_modules/a/node_modules/b/i.js"),
            days(1),
        );
        let found = find_dirs_named(&[d.path().to_path_buf()], 8, "node_modules");
        assert_eq!(found.len(), 1);
    }
}

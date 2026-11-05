//! Filesystem scanning primitives: sizes that agree with `du`, idleness checks with early exit,
//! and lookups for well-known build directories.

use std::collections::{BTreeMap, HashSet};
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

/// The thread pool directory walks run on. jwalk's default is rayon's global pool with a 1 s
/// "busy" timeout, after which the walk silently ends early: with the global pool occupied (by
/// other walks, or by anything else using rayon) sizes came out short or zero and recent files
/// went unseen — so a target being built could look idle. A pool of our own, with no busy
/// timeout, makes concurrent walks queue instead. Walks never start from inside this pool (the
/// visitor runs on the calling thread), so they cannot deadlock on it.
fn walk_pool() -> std::sync::Arc<jwalk::rayon::ThreadPool> {
    static POOL: std::sync::OnceLock<std::sync::Arc<jwalk::rayon::ThreadPool>> =
        std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);
        std::sync::Arc::new(
            jwalk::rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|i| format!("diskgc-walk-{i}"))
                .build()
                .expect("cannot start the directory walk thread pool"),
        )
    })
    .clone()
}

/// Every entry below `root` with its metadata: hard links once, symbolic links not followed,
/// macOS-protected locations skipped. Directories are read and their entries stat'ed in parallel
/// (jwalk on [`walk_pool`]); `f` runs on the calling thread. `root` itself is only reported when
/// it is not a directory.
pub fn visit(root: &Path, mut f: impl FnMut(&Path, &std::fs::Metadata)) {
    match std::fs::symlink_metadata(root) {
        Ok(m) if !m.is_dir() => return f(root, &m),
        Err(_) => return,
        _ => {}
    }
    let walker = jwalk::WalkDirGeneric::<((), Option<std::fs::Metadata>)>::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonExistingPool {
            pool: walk_pool(),
            busy_timeout: None,
        })
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
    for e in walker.into_iter().filter_map(Result::ok) {
        let Some(m) = e.client_state.as_ref() else {
            continue;
        };
        if let Some(key) = platform::hardlink_key(m) {
            if !seen.insert(key) {
                continue;
            }
        }
        f(&e.path(), m);
    }
}

/// Bytes on disk under `root` for files matching `pred` (directories always count). Hard links
/// are counted once.
pub fn walk_bytes(root: &Path, pred: impl Fn(&Path, &std::fs::Metadata) -> bool) -> u64 {
    if let Ok(m) = std::fs::symlink_metadata(root) {
        if !m.is_dir() {
            return platform::file_usage(&m);
        }
    }
    let mut total = 0u64;
    visit(root, |p, m| {
        if m.is_dir() || (!m.file_type().is_symlink() && pred(p, m)) {
            total += platform::file_usage(m);
        }
    });
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

/// What one walk over the project roots finds: Cargo build directories and directories with one
/// of the wanted names.
#[derive(Debug, Default)]
pub struct TreeIndex {
    pub cargo_targets: Vec<PathBuf>,
    pub named: BTreeMap<String, Vec<PathBuf>>,
}

impl TreeIndex {
    pub fn named(&self, name: &str) -> &[PathBuf] {
        self.named.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Walk `roots` once, at most `depth` deep, collecting Cargo build directories and directories
/// called one of `names`. Searching once per rule and per name used to mean a dozen passes over
/// every project tree.
///
/// A Cargo build directory is any directory holding `.rustc_info.json` or a cargo `CACHEDIR.TAG`:
/// matching by content rather than by the name `target` also finds the ad-hoc `CARGO_TARGET_DIR`s
/// that tools and agents create in temp folders. It is never descended into — its hundreds of
/// thousands of files are the rust rule's business. A named directory is not descended into when
/// `is_leaf(name, dir)` says its contents belong to it (a `node_modules` inside `node_modules` is
/// part of the outer one).
pub fn index_tree(
    roots: &[PathBuf],
    depth: usize,
    names: &[&str],
    is_leaf: impl Fn(&str, &Path) -> bool,
) -> TreeIndex {
    let mut idx = TreeIndex::default();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        // Sorted, so the marker files of a build directory come before its subdirectories and
        // skipping the rest of the directory skips all of it.
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(depth)
            .sort_by_file_name()
            .into_iter();
        while let Some(Ok(e)) = it.next() {
            if e.depth() == 0 {
                continue;
            }
            let n = e.file_name().to_string_lossy();
            if e.file_type().is_dir() {
                if platform::is_protected(e.path()) {
                    it.skip_current_dir();
                } else if let Some(name) = names.iter().find(|w| **w == n) {
                    idx.named
                        .entry(name.to_string())
                        .or_default()
                        .push(e.path().to_path_buf());
                    if is_leaf(name, e.path()) {
                        it.skip_current_dir();
                    }
                } else if ALWAYS_SKIP.contains(&n.as_ref()) {
                    it.skip_current_dir();
                }
                continue;
            }
            if !e.file_type().is_file() {
                continue;
            }
            let is_cargo = match n.as_ref() {
                ".rustc_info.json" => true,
                "CACHEDIR.TAG" => std::fs::read_to_string(e.path())
                    .map(|t| t.to_lowercase().contains("cargo"))
                    .unwrap_or(false),
                _ => false,
            };
            if is_cargo {
                if let Some(parent) = e.path().parent() {
                    idx.cargo_targets.push(parent.to_path_buf());
                }
                it.skip_current_dir();
            }
        }
    }
    idx.cargo_targets.sort();
    idx.cargo_targets.dedup();
    for v in idx.named.values_mut() {
        v.sort();
        v.dedup();
    }
    idx
}

/// Directories called `name` under `roots`, at most `depth` deep, not searched further once found.
pub fn find_dirs_named(roots: &[PathBuf], depth: usize, name: &str) -> Vec<PathBuf> {
    let mut idx = index_tree(roots, depth, &[name], |_, _| true);
    idx.named.remove(name).unwrap_or_default()
}

/// Cargo build directories under `roots` (see [`index_tree`]).
pub fn find_cargo_targets(roots: &[PathBuf], depth: usize) -> Vec<PathBuf> {
    index_tree(roots, depth, &[], |_, _| true).cargo_targets
}

/// Delete files matching `pred` under `root`, then remove directories left empty. Returns the bytes
/// freed: the blocks of every deleted file that was its last link, plus the emptied directories.
/// One pass; it used to measure the whole tree before and after, three walks more.
pub fn sweep(root: &Path, pred: impl Fn(&Path, &std::fs::Metadata) -> bool) -> u64 {
    let mut freed = 0u64;
    let mut dirs: Vec<(PathBuf, u64)> = Vec::new();
    for e in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if e.file_type().is_dir() {
            if e.depth() > 0 {
                let usage = e.metadata().map(|m| platform::file_usage(&m)).unwrap_or(0);
                dirs.push((e.path().to_path_buf(), usage));
            }
            continue;
        }
        if !e.file_type().is_file() {
            continue;
        }
        // Fresh metadata: a hard link whose twin went earlier in this sweep is now the last
        // link, and deleting it really frees the blocks.
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if pred(e.path(), &m)
            && std::fs::remove_file(e.path()).is_ok()
            && platform::hardlink_key(&m).is_none()
        {
            freed += platform::file_usage(&m);
        }
    }
    dirs.sort_by_key(|(d, _)| std::cmp::Reverse(d.components().count()));
    for (d, usage) in dirs {
        // Fails harmlessly when the directory is not empty.
        if std::fs::remove_dir(&d).is_ok() {
            freed += usage;
        }
    }
    freed
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
    fn walks_are_complete_while_the_global_rayon_pool_is_busy() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..50 {
            let sub = d.path().join(format!("d{i}"));
            fs::create_dir_all(&sub).unwrap();
            fs::write(sub.join("f"), vec![1u8; 10_000]).unwrap();
        }
        let expected = disk_usage(d.path());
        let files = || {
            let mut n = 0;
            visit(d.path(), |_, m| n += m.is_file() as usize);
            n
        };
        assert_eq!(files(), 50);
        // Occupy every thread of rayon's global pool for longer than jwalk's default 1 s busy
        // timeout; a walk on that pool would end early and report less.
        let threads = jwalk::rayon::current_num_threads();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        for _ in 0..threads {
            let started_tx = started_tx.clone();
            jwalk::rayon::spawn(move || {
                started_tx.send(()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(2_500));
            });
        }
        for _ in 0..threads {
            started_rx.recv().unwrap();
        }
        assert_eq!(files(), 50);
        assert_eq!(disk_usage(d.path()), expected);
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

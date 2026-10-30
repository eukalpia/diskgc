//! Append-only JSONL history: free-space snapshots, category sizes and run summaries.
//! Small on purpose (a few hundred bytes per record, a snapshot every tick) and compacted when it
//! grows past a few megabytes.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::forecast::{self, Forecast};
use crate::model::{Level, RunSummary};
use crate::paths;

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const KEEP_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRec {
    pub path: String,
    pub bytes: u64,
    pub reason: String,
    pub group: String,
    /// What the action really freed; `None` for dry runs and older records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freed: Option<u64>,
    /// For command actions: the command line, so a fruitless command can be put on cooldown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    Snapshot {
        ts: String,
        used: u64,
        available: u64,
        level: Level,
    },
    Categories {
        ts: String,
        sizes: BTreeMap<String, u64>,
    },
    Run {
        ts: String,
        level: Option<Level>,
        dry_run: bool,
        freed: u64,
        actions: usize,
        available_after: u64,
        duration_secs: f64,
        top: Vec<ActionRec>,
    },
}

impl Record {
    pub fn ts(&self) -> &str {
        match self {
            Record::Snapshot { ts, .. }
            | Record::Categories { ts, .. }
            | Record::Run { ts, .. } => ts,
        }
    }
}

pub struct History {
    path: PathBuf,
}

impl Default for History {
    fn default() -> Self {
        Self::new(paths::history_file())
    }
}

impl History {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, rec: &Record) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(rec)?)?;
        if f.metadata().map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
            self.compact()?;
        }
        Ok(())
    }

    pub fn load(&self) -> Vec<Record> {
        std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Drop snapshots older than `KEEP_DAYS`; runs and category samples are kept.
    pub fn compact(&self) -> Result<()> {
        let cutoff = (chrono::Local::now() - chrono::Duration::days(KEEP_DAYS))
            .format("%Y-%m-%d")
            .to_string();
        let kept: Vec<Record> = self
            .load()
            .into_iter()
            .filter(|r| !matches!(r, Record::Snapshot { .. }) || r.ts() >= cutoff.as_str())
            .collect();
        let tmp = self.path.with_extension("jsonl.tmp");
        let mut f = std::fs::File::create(&tmp)?;
        for r in &kept {
            writeln!(f, "{}", serde_json::to_string(r)?)?;
        }
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    pub fn record_run(&self, summary: &RunSummary, top: Vec<ActionRec>) -> Result<()> {
        self.append(&Record::Run {
            ts: now(),
            level: summary.level,
            dry_run: summary.dry_run,
            freed: summary.freed_bytes + summary.docker_freed_bytes,
            actions: summary.action_count,
            available_after: summary.free_after,
            duration_secs: summary.duration_secs,
            top,
        })
    }

    pub fn record_snapshot(&self, used: u64, available: u64, level: Level) -> Result<()> {
        self.append(&Record::Snapshot {
            ts: now(),
            used,
            available,
            level,
        })
    }

    pub fn record_categories(&self, sizes: BTreeMap<String, u64>) -> Result<()> {
        self.append(&Record::Categories { ts: now(), sizes })
    }

    /// Growth forecast from snapshots within the last `window_hours`.
    pub fn forecast(&self, window_hours: f64, available: u64, critical: u64) -> Option<Forecast> {
        let now = chrono::Local::now().naive_local();
        let pts: Vec<(f64, f64)> =
            self.load()
                .into_iter()
                .filter_map(|r| match r {
                    Record::Snapshot { ts, used, .. } => parse_ts(&ts)
                        .map(|t| ((t - now).num_seconds() as f64 / 3600.0, used as f64)),
                    _ => None,
                })
                .filter(|(h, _)| *h >= -window_hours)
                .collect();
        forecast::forecast(&pts, window_hours, available, critical)
    }

    pub fn stats(&self) -> Stats {
        let recs = self.load();
        let mut days: BTreeMap<String, DayStat> = BTreeMap::new();
        for r in &recs {
            let day = r.ts().get(..10).unwrap_or("").to_string();
            let e = days.entry(day).or_default();
            match r {
                Record::Snapshot { used, .. } => {
                    if e.used_first == 0 {
                        e.used_first = *used;
                    }
                    e.used_last = *used;
                }
                Record::Run {
                    freed,
                    dry_run: false,
                    ..
                } => {
                    e.freed += *freed;
                    e.runs += 1;
                }
                _ => {}
            }
        }
        let cats: Vec<&Record> = recs
            .iter()
            .filter(|r| matches!(r, Record::Categories { .. }))
            .collect();
        let (categories_first, categories_last) = match (cats.first(), cats.last()) {
            (
                Some(Record::Categories { sizes: a, ts: ta }),
                Some(Record::Categories { sizes: b, ts: tb }),
            ) => (Some((ta.clone(), a.clone())), Some((tb.clone(), b.clone()))),
            _ => (None, None),
        };
        let mut repeat: BTreeMap<String, RepeatOffender> = BTreeMap::new();
        let mut total_freed = 0u64;
        let mut runs = 0usize;
        for r in &recs {
            if let Record::Run {
                freed,
                dry_run: false,
                top,
                ..
            } = r
            {
                total_freed += *freed;
                runs += 1;
                for a in top {
                    let e = repeat
                        .entry(a.path.clone())
                        .or_insert_with(|| RepeatOffender {
                            path: a.path.clone(),
                            times: 0,
                            bytes: 0,
                            group: a.group.clone(),
                        });
                    e.times += 1;
                    e.bytes += a.bytes;
                }
            }
        }
        let mut repeat_offenders: Vec<RepeatOffender> =
            repeat.into_values().filter(|r| r.times >= 2).collect();
        repeat_offenders.sort_by_key(|r| std::cmp::Reverse(r.bytes));
        repeat_offenders.truncate(10);
        Stats {
            days: days.into_iter().collect(),
            categories_first,
            categories_last,
            total_freed,
            runs,
            repeat_offenders,
        }
    }

    /// Command lines that ran within the last `days` days and freed nothing. Running them again
    /// would only cost time (and, as with `uv cache clean`, may block on another process's lock).
    pub fn fruitless_commands(&self, days: i64) -> std::collections::HashSet<String> {
        let cutoff = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let mut out = std::collections::HashSet::new();
        for r in self.load() {
            if let Record::Run {
                ts,
                dry_run: false,
                top,
                ..
            } = r
            {
                if ts < cutoff {
                    continue;
                }
                for a in top {
                    if let (Some(cmd), Some(0)) = (a.command, a.freed) {
                        out.insert(cmd);
                    }
                }
            }
        }
        out
    }

    pub fn runs(&self, limit: usize) -> Vec<Record> {
        let mut v: Vec<Record> = self
            .load()
            .into_iter()
            .filter(|r| matches!(r, Record::Run { .. }))
            .collect();
        let n = v.len();
        if n > limit {
            v.drain(..n - limit);
        }
        v
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayStat {
    pub used_first: u64,
    pub used_last: u64,
    pub freed: u64,
    pub runs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatOffender {
    pub path: String,
    pub group: String,
    pub times: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub days: Vec<(String, DayStat)>,
    pub categories_first: Option<(String, BTreeMap<String, u64>)>,
    pub categories_last: Option<(String, BTreeMap<String, u64>)>,
    pub total_freed: u64,
    pub runs: usize,
    pub repeat_offenders: Vec<RepeatOffender>,
}

pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_ts(ts: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_stats() {
        let d = tempfile::tempdir().unwrap();
        let h = History::new(d.path().join("h.jsonl"));
        h.record_snapshot(100, 50, Level::Normal).unwrap();
        h.record_snapshot(120, 30, Level::Low).unwrap();
        let s = RunSummary {
            freed_bytes: 40,
            action_count: 3,
            ..Default::default()
        };
        let rec = || ActionRec {
            path: "/x".into(),
            bytes: 40,
            reason: "r".into(),
            group: "rust".into(),
            freed: Some(0),
            command: Some("go clean -cache".into()),
        };
        h.record_run(&s, vec![rec()]).unwrap();
        h.record_run(&s, vec![rec()]).unwrap();
        assert!(h.fruitless_commands(7).contains("go clean -cache"));
        let st = h.stats();
        assert_eq!(st.runs, 2);
        assert_eq!(st.total_freed, 80);
        assert_eq!(st.repeat_offenders.len(), 1);
        assert_eq!(st.days.len(), 1);
        assert_eq!(st.days[0].1.used_first, 100);
        assert_eq!(st.days[0].1.used_last, 120);
    }
}

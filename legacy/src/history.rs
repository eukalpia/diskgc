//! История: ~/.local/share/diskgc/history.jsonl — снимки занятости по тикам демона,
//! размеры категорий и результаты чисток. Из неё строится статистика «как копится».

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use crate::fsutil::home;

pub fn history_path() -> PathBuf {
    home().join(".local/share/diskgc/history.jsonl")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRec {
    pub path: String,
    pub gb: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub ts: String,
    /// "snapshot" | "clean" | "categories"
    pub kind: String,
    pub free_gb: f64,
    pub used_gb: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freed_gb: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_actions: Option<Vec<ActionRec>>,
}

pub fn append(rec: &Record) -> Result<()> {
    let p = history_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(rec)?)?;
    Ok(())
}

pub fn load() -> Vec<Record> {
    std::fs::read_to_string(history_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn parse_ts(ts: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()
}

/// Скорость роста занятого места (ГБ/час) по снимкам за последние `hours` часов.
/// Нужно ≥ 3 снимка и разброс по времени ≥ 30 мин, иначе None.
pub fn growth_rate_gb_per_hour(hours: f64) -> Option<f64> {
    let now = chrono::Local::now().naive_local();
    let pts: Vec<(f64, f64)> = load()
        .into_iter()
        .filter(|r| r.kind == "snapshot")
        .filter_map(|r| parse_ts(&r.ts).map(|t| ((t - now).num_seconds() as f64 / 3600.0, r.used_gb)))
        .filter(|(h, _)| *h >= -hours)
        .collect();
    if pts.len() < 3 {
        return None;
    }
    let span = pts.last()?.0 - pts.first()?.0;
    if span < 0.5 {
        return None;
    }
    // линейная регрессия used = a + b*t
    let n = pts.len() as f64;
    let (sx, sy): (f64, f64) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / n, sy / n);
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    Some(sxy / sxx)
}

pub fn forecast_line(free_gb: f64, critical_gb: f64) -> Option<String> {
    let rate = growth_rate_gb_per_hour(6.0)?;
    if rate <= 0.05 {
        return Some(format!("рост за 6 ч: {:+.2} ГБ/ч — стабильно", rate));
    }
    let eta_h = (free_gb - critical_gb) / rate;
    Some(format!("рост за 6 ч: {:+.2} ГБ/ч → до критического порога ~{:.0} ч", rate, eta_h.max(0.0)))
}

fn day(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// Статистика: по дням — занято в начале/конце, дельта; тренд категорий; сколько чисток и сколько освобождено.
pub fn print_stats() {
    let recs = load();
    if recs.is_empty() {
        println!("Истории пока нет — демон ещё не работал (diskgc install), или запусти diskgc clean.");
        return;
    }
    println!("== Занято на диске по дням (ГБ) ==");
    println!("{:<12}{:>10}{:>10}{:>10}{:>10}", "день", "утро", "вечер", "дельта", "убрано");
    let mut by_day: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
    for r in &recs {
        by_day.entry(day(&r.ts).to_string()).or_default().push(r);
    }
    for (d, rs) in &by_day {
        let first = rs.first().unwrap().used_gb;
        let last = rs.last().unwrap().used_gb;
        let freed: f64 = rs.iter().filter(|r| r.kind == "clean" && r.dry != Some(true)).filter_map(|r| r.freed_gb).sum();
        println!("{:<12}{:>10.1}{:>10.1}{:>+10.1}{:>10.1}", d, first, last, last - first, freed);
    }
    let cats: Vec<&Record> = recs.iter().filter(|r| r.kind == "categories" && r.categories.is_some()).collect();
    if cats.len() >= 1 {
        let first = cats.first().unwrap();
        let last = cats.last().unwrap();
        println!("\n== Категории (ГБ): {} → {} ==", day(&first.ts), day(&last.ts));
        let fc = first.categories.as_ref().unwrap();
        let lc = last.categories.as_ref().unwrap();
        let mut keys: Vec<&String> = lc.keys().collect();
        keys.sort_by(|a, b| lc[*b].partial_cmp(&lc[*a]).unwrap());
        for k in keys {
            let a = fc.get(k).copied().unwrap_or(0.0);
            let b = lc[k];
            println!("  {:<18}{:>8.1} → {:>8.1}  ({:+.1})", k, a, b, b - a);
        }
    }
    let cleans: Vec<&Record> = recs.iter().filter(|r| r.kind == "clean" && r.dry != Some(true)).collect();
    let total: f64 = cleans.iter().filter_map(|r| r.freed_gb).sum();
    println!("\nЧисток: {}, освобождено всего: {:.1} ГБ", cleans.len(), total);
    if let Some(r6) = growth_rate_gb_per_hour(6.0) {
        let r24 = growth_rate_gb_per_hour(24.0).unwrap_or(r6);
        println!("Скорость роста: {:+.2} ГБ/ч за 6 ч, {:+.2} ГБ/ч за сутки", r6, r24);
    }
    // повторные виновники: одни и те же пути чистятся снова и снова — лечить причину, не симптом
    let mut rep: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for r in &cleans {
        for a in r.top_actions.iter().flatten() {
            let e = rep.entry(a.path.clone()).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += a.gb;
        }
    }
    let mut v: Vec<(&String, &(usize, f64))> = rep.iter().filter(|(_, (n, _))| *n >= 2).collect();
    v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());
    if !v.is_empty() {
        println!("\n== Повторно растущие пути (чистились ≥ 2 раз) ==");
        for (p, (n, g)) in v.iter().take(8) {
            println!("  {:>2}×  {:>7.1} ГБ  {}", n, g, p);
        }
    }
}

pub fn print_history(n: usize) {
    let recs = load();
    let cleans: Vec<&Record> = recs.iter().filter(|r| r.kind == "clean").collect();
    if cleans.is_empty() {
        println!("Чисток ещё не было.");
        return;
    }
    for r in cleans.iter().rev().take(n).rev() {
        println!(
            "{}  [{}{}]  освобождено {:.1} ГБ, действий {}, свободно после {:.0} ГБ",
            r.ts,
            r.level.clone().unwrap_or_default(),
            if r.dry == Some(true) { ", dry-run" } else { "" },
            r.freed_gb.unwrap_or(0.0),
            r.action_count.unwrap_or(0),
            r.free_gb
        );
        for a in r.top_actions.iter().flatten().take(5) {
            println!("      {:>7.1} ГБ  {}  — {}", a.gb, a.path, a.reason);
        }
    }
}

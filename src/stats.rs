//! Trvalé statistiky (lifetime) – ukládají se lokálně do %LOCALAPPDATA%\TomasekConvert\stats.json
use crate::util::{app_dir, unix_now};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Engine {
    Cpu,
    Gpu,
    /// HW dekódování na GPU + enkódování na CPU
    Mixed,
}

impl Engine {
    pub fn label(&self) -> &'static str {
        match self {
            Engine::Cpu => "CPU",
            Engine::Gpu => "GPU",
            Engine::Mixed => tr!("GPU dekód + CPU enkód", "GPU decode + CPU encode"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct HistoryEntry {
    pub when: u64,
    pub name: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub engine: Engine,
    pub secs: f64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub output: String,
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Stats {
    pub first_use: u64,
    pub total_jobs: u64,
    pub failed_jobs: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub secs_total: f64,
    pub cpu_jobs: u64,
    pub gpu_jobs: u64,
    pub mixed_jobs: u64,
    pub cpu_secs: f64,
    pub gpu_secs: f64,
    pub mixed_secs: f64,
    pub by_target: BTreeMap<String, u64>,
    pub by_kind: BTreeMap<String, u64>,
    pub longest_job_secs: f64,
    pub history: Vec<HistoryEntry>,
}

fn path() -> std::path::PathBuf {
    app_dir().join("stats.json")
}

impl Stats {
    pub fn load() -> Stats {
        let mut s: Stats = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.first_use == 0 {
            s.first_use = unix_now();
        }
        s
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), t);
        }
    }
    pub fn record(&mut self, e: HistoryEntry) {
        self.total_jobs += 1;
        self.bytes_in += e.bytes_in;
        self.bytes_out += e.bytes_out;
        self.secs_total += e.secs;
        self.longest_job_secs = self.longest_job_secs.max(e.secs);
        match e.engine {
            Engine::Cpu => {
                self.cpu_jobs += 1;
                self.cpu_secs += e.secs;
            }
            Engine::Gpu => {
                self.gpu_jobs += 1;
                self.gpu_secs += e.secs;
            }
            Engine::Mixed => {
                self.mixed_jobs += 1;
                self.mixed_secs += e.secs;
            }
        }
        *self.by_target.entry(e.to.clone()).or_default() += 1;
        *self.by_kind.entry(e.kind.clone()).or_default() += 1;
        self.history.push(e);
        if self.history.len() > 300 {
            let n = self.history.len() - 300;
            self.history.drain(0..n);
        }
        self.save();
    }
    pub fn record_failure(&mut self) {
        self.failed_jobs += 1;
        self.save();
    }
    pub fn top_targets(&self, n: usize) -> Vec<(String, u64)> {
        let mut v: Vec<_> = self.by_target.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }
    pub fn top_kinds(&self) -> Vec<(String, u64)> {
        let mut v: Vec<_> = self.by_kind.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }
}

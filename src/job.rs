//! Společné typy pro úlohy: sdílený průběh, zrušení, spouštění procesů.
use crate::{hw::SysInfo, opts::Opts, stats::Engine, tools::Tools};
use std::{
    collections::VecDeque,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub type Res<T> = Result<T, String>;
pub const CANCELLED: &str = "Zrušeno";

/// Stav, který UI čte a úloha zapisuje.
pub struct Shared {
    started: Mutex<Option<std::time::Instant>>,
    frac: AtomicU32,
    cancel: AtomicBool,
    note: Mutex<String>,
}

impl Shared {
    pub fn new() -> Arc<Shared> {
        Arc::new(Shared { started: Mutex::new(None), frac: AtomicU32::new((-1.0f32).to_bits()), cancel: AtomicBool::new(false), note: Mutex::new(String::new()) })
    }
    /// Úloha skutečně začala běžet (před tím čekala ve frontě).
    pub fn mark_started(&self) {
        *self.started.lock().unwrap() = Some(std::time::Instant::now());
    }
    pub fn started(&self) -> Option<std::time::Instant> {
        *self.started.lock().unwrap()
    }
    pub fn set_frac(&self, f: f32) {
        self.frac.store(f.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    pub fn set_indeterminate(&self) {
        self.frac.store((-1.0f32).to_bits(), Ordering::Relaxed);
    }
    /// None = procento není známé (UI ukáže animovaný pruh).
    pub fn frac(&self) -> Option<f32> {
        let f = f32::from_bits(self.frac.load(Ordering::Relaxed));
        if f < 0.0 {
            None
        } else {
            Some(f)
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn set_note(&self, s: impl Into<String>) {
        *self.note.lock().unwrap() = s.into();
    }
    pub fn note(&self) -> String {
        self.note.lock().unwrap().clone()
    }
}

#[derive(Clone)]
pub struct JobCtx {
    pub shared: Arc<Shared>,
    pub si: Arc<SysInfo>,
    pub tools: Arc<Tools>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub outputs: Vec<PathBuf>,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub engine: Engine,
    pub detail: String,
    /// Délka zpracovaného média (pro "rychlost ×realtime").
    pub media_secs: f64,
}

impl Outcome {
    pub fn simple(outputs: Vec<PathBuf>, bytes_in: u64, bytes_out: u64, detail: impl Into<String>) -> Outcome {
        Outcome { outputs, bytes_in, bytes_out, engine: Engine::Cpu, detail: detail.into(), media_secs: 0.0 }
    }
}

#[derive(Clone)]
pub struct ConvertJob {
    pub input: PathBuf,
    pub in_ext: String,
    pub kind: crate::formats::Kind,
    pub fmt: crate::formats::Fmt,
    pub opts: Opts,
    pub out_dir: PathBuf,
}

fn spawn_reader<R: Read + Send + 'static>(mut r: R, is_err: bool, tx: mpsc::Sender<(bool, String)>) {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        if b == b'\n' || b == b'\r' {
                            if !line.is_empty() {
                                let _ = tx.send((is_err, String::from_utf8_lossy(&line).to_string()));
                                line.clear();
                            }
                        } else {
                            line.push(b);
                        }
                    }
                }
            }
        }
        if !line.is_empty() {
            let _ = tx.send((is_err, String::from_utf8_lossy(&line).to_string()));
        }
    });
}

/// Spustí proces, průběžně volá `on_line(je_stderr, řádek)`, reaguje na zrušení.
/// Při úspěchu vrátí posledních pár řádků stderr, při chybě je dá do chybové zprávy.
pub fn run_cmd(ctx: &JobCtx, mut cmd: Command, mut on_line: impl FnMut(bool, &str)) -> Res<Vec<String>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Nelze spustit {:?}: {e}", cmd.get_program()))?;
    let (tx, rx) = mpsc::channel();
    spawn_reader(child.stdout.take().unwrap(), false, tx.clone());
    spawn_reader(child.stderr.take().unwrap(), true, tx);
    let mut tail: VecDeque<String> = VecDeque::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((is_err, line)) => {
                if is_err && !line.contains("% complete") {
                    tail.push_back(line.clone());
                    if tail.len() > 12 {
                        tail.pop_front();
                    }
                }
                on_line(is_err, &line);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if ctx.shared.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CANCELLED.into());
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let tail: Vec<String> = tail.into_iter().collect();
    if status.success() {
        Ok(tail)
    } else if tail.is_empty() {
        Err(trf!("Proces skončil s kódem {}", "Process exited with code {}", status.code().unwrap_or(-1)))
    } else {
        Err(tail.join("\n"))
    }
}

/// Dočasný výstup vedle cílového souboru; po úspěchu se přejmenuje.
pub fn part_path(out: &std::path::Path) -> PathBuf {
    let stem = crate::util::stem_of(out);
    let ext = crate::util::full_ext(out);
    out.with_file_name(format!("{stem}.tcpart.{ext}"))
}

pub fn finish_part(part: &std::path::Path, out: &std::path::Path) -> Res<()> {
    std::fs::rename(part, out).map_err(|e| trf!("Nelze uložit výsledek: {e}", "Could not save the result: {e}"))
}

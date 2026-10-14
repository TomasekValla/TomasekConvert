//! Detekce CPU/GPU a schopností vložených nástrojů (kodeky, formáty, AMF...).
use crate::{gpu::{self, Backend, Fail}, tools::Tools, util::command};
use std::collections::{BTreeSet, HashSet};
use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    #[default]
    Other,
}

impl Vendor {
    pub fn from_name(name: &str) -> Vendor {
        let n = name.to_lowercase();
        if ["nvidia", "geforce", "quadro", "tesla", "rtx ", "gtx "].iter().any(|k| n.contains(k)) {
            Vendor::Nvidia
        } else if ["amd", "radeon", "advanced micro"].iter().any(|k| n.contains(k)) {
            Vendor::Amd
        } else if n.contains("intel") {
            Vendor::Intel
        } else if n.contains("apple") {
            Vendor::Apple
        } else {
            Vendor::Other
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Vendor,
    /// Verze ovladače, když se ji podařilo zjistit (u NVIDIA ve tvaru 551.23).
    pub driver: Option<String>,
}

impl GpuInfo {
    /// Název + verze ovladače (pokud je známá).
    pub fn label(&self) -> String {
        match &self.driver {
            Some(d) => format!("{} ({} {d})", self.name, tr!("ovladač", "driver")),
            None => self.name.clone(),
        }
    }
}

/// Výsledek zkušebního enkódu jednoho GPU enkodéru.
#[derive(Clone, Debug)]
pub struct EncProbe {
    pub backend: Backend,
    pub encoder: String,
    /// Kodek (id z `opts::VCODECS`): h264 / hevc / av1.
    pub codec: &'static str,
    pub result: Result<(), Fail>,
}

/// Co umí tenhle konkrétní stroj + vložené nástroje.
#[derive(Clone, Debug, Default)]
pub struct SysInfo {
    pub cpu: String,
    pub cores: usize,
    pub threads: usize,
    pub gpus: Vec<GpuInfo>,
    /// GPU enkodéry, které FAKTICKY fungují (ověřeno zkušebním enkódem): (backend, název enkodéru).
    pub hw_encoders: Vec<(Backend, String)>,
    /// Všechny zkušební enkódy včetně neúspěšných (kvůli vysvětlení v Nastavení).
    pub probes: Vec<EncProbe>,
    /// CPU a OS podporují AVX-512 (F + CD + BW + DQ + VL).
    pub avx512: bool,
    /// Podporované části AVX-512 (VNNI, VBMI, …) – jen pro zobrazení.
    pub avx512_features: Vec<&'static str>,
    /// Tenhle ffmpeg přijímá `-cpuflags +avx512`.
    pub ffmpeg_avx512_flag: bool,
    /// Rozhraní pro HW dekódování v ffmpeg (d3d11va, dxva2...).
    pub hwaccels: HashSet<String>,
    pub encoders: HashSet<String>,
    pub decoders: HashSet<String>,
    pub magick_read: BTreeSet<String>,
    pub magick_write: BTreeSet<String>,
    pub pandoc_in: Vec<String>,
    pub pandoc_out: Vec<String>,
    pub versions: Vec<(String, String)>,
}

impl SysInfo {
    /// Neúspěšné zkoušky jednoho backendu.
    pub fn failures_of(&self, b: Backend) -> Vec<(&'static str, &Fail)> {
        self.probes.iter().filter(|p| p.backend == b).filter_map(|p| p.result.as_ref().err().map(|f| (p.codec, f))).collect()
    }
    /// Je v počítači NVIDIA karta?
    pub fn has_nvidia(&self) -> bool {
        self.gpus.iter().any(|g| g.vendor == Vendor::Nvidia)
    }
    /// Verze ovladače první NVIDIA karty (když je známa).
    pub fn nvidia_driver(&self) -> Option<&str> {
        self.gpus.iter().find(|g| g.vendor == Vendor::Nvidia).and_then(|g| g.driver.as_deref())
    }
    pub fn gpu_label(&self) -> String {
        if self.gpus.is_empty() {
            tr!("nenalezeno", "not detected").into()
        } else {
            self.gpus.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(" + ")
        }
    }
    pub fn has_hw_enc(&self, b: Backend, enc: &str) -> bool {
        self.hw_encoders.iter().any(|(bb, e)| *bb == b && e == enc)
    }
    /// Backendy, které mají aspoň jeden funkční enkodér (v pořadí priority).
    pub fn backends(&self) -> Vec<Backend> {
        gpu::ALL.iter().copied().filter(|b| self.hw_encoders.iter().any(|(bb, _)| bb == b)).collect()
    }
    pub fn encoders_of(&self, b: Backend) -> Vec<String> {
        self.hw_encoders.iter().filter(|(bb, _)| *bb == b).map(|(_, e)| e.clone()).collect()
    }
    /// Popisek rozhraní pro HW dekódování na tomhle systému.
    pub fn hw_decoder(&self) -> Option<&'static str> {
        let has = |n: &str| self.hwaccels.contains(n);
        if cfg!(windows) && has("d3d11va") {
            Some("D3D11VA")
        } else if cfg!(target_os = "macos") && has("videotoolbox") {
            Some("VideoToolbox")
        } else if cfg!(target_os = "linux") && (has("cuda") && self.hw_encoders.iter().any(|(b, _)| *b == Backend::Nvenc)) {
            Some("CUDA")
        } else if cfg!(target_os = "linux") && has("vaapi") && gpu::vaapi_device().is_some() {
            Some("VA-API")
        } else {
            None
        }
    }
    /// Argumenty ffmpeg pro HW dekódování (před `-i`).
    pub fn hw_decode_args(&self) -> Option<Vec<String>> {
        let s = |x: &str| x.to_string();
        Some(match self.hw_decoder()? {
            "D3D11VA" => vec![s("-hwaccel"), s("d3d11va")],
            "VideoToolbox" => vec![s("-hwaccel"), s("videotoolbox")],
            "CUDA" => vec![s("-hwaccel"), s("cuda")],
            _ => vec![s("-hwaccel"), s("vaapi"), s("-hwaccel_device"), gpu::vaapi_device()?],
        })
    }
    pub fn has_encoder(&self, e: &str) -> bool {
        self.encoders.is_empty() || self.encoders.contains(e)
    }
    #[allow(dead_code)]
    pub fn d3d11(&self) -> bool {
        self.hwaccels.contains("d3d11va")
    }
}

fn run_text(exe: &std::path::Path, args: &[&str]) -> String {
    match command(exe).args(args).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s
        }
        Err(_) => String::new(),
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

pub fn detect(tools: &Tools) -> SysInfo {
    let mut si = SysInfo::default();

    // ── CPU ──
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all();
    si.threads = sys.cpus().len();
    si.cores = sysinfo::System::physical_core_count().unwrap_or(si.threads);
    si.cpu = sys
        .cpus()
        .first()
        .map(|c| c.brand().split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| tr!("Neznámé CPU", "Unknown CPU").into());

    // ── AVX-512 ──
    let (avx, feats) = detect_avx512();
    si.avx512 = avx;
    si.avx512_features = feats;

    // ── GPU ──
    si.gpus = detect_gpus();

    // ── ffmpeg schopnosti ──
    if let Some(ff) = &tools.ffmpeg {
        si.versions.push(("FFmpeg".into(), first_line(&run_text(ff, &["-hide_banner", "-version"]))));
        for line in run_text(ff, &["-hide_banner", "-encoders"]).lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.len() >= 2 && t[0].len() == 6 && t[0].starts_with(['V', 'A', 'S']) {
                si.encoders.insert(t[1].to_string());
            }
        }
        for line in run_text(ff, &["-hide_banner", "-decoders"]).lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.len() >= 2 && t[0].len() == 6 && t[0].starts_with(['V', 'A', 'S']) {
                si.decoders.insert(t[1].to_string());
            }
        }
        for line in run_text(ff, &["-hide_banner", "-hwaccels"]).lines().skip(1) {
            let l = line.trim();
            if !l.is_empty() {
                si.hwaccels.insert(l.to_string());
            }
        }
        if si.avx512 {
            si.ffmpeg_avx512_flag = command(ff).args(["-hide_banner", "-cpuflags", "+avx512", "-version"]).output().map(|o| o.status.success()).unwrap_or(false);
        }
        // GPU enkodéry: zkušební enkód 0,2 s videa. Jen to, co opravdu projde, nabízíme.
        // Backendy se zkouší souběžně, ale kodeky jednoho backendu po sobě: starší GeForce karty
        // povolují jen 2 souběžné NVENC relace, takže tři zkoušky naráz (h264 + hevc + av1) náhodně
        // selhávaly a karta pak vypadala jako bez NVENC.
        let ff = ff.clone();
        let groups: Vec<(Backend, Vec<(&'static str, String)>)> = gpu::ALL
            .iter()
            .map(|b| {
                let encs: Vec<(&'static str, String)> = b.codecs().iter().filter_map(|c| b.encoder(c).map(|e| (*c, e.to_string()))).filter(|(_, e)| si.encoders.contains(e)).collect();
                (*b, encs)
            })
            .filter(|(_, e)| !e.is_empty())
            .collect();
        let probes: Vec<Vec<EncProbe>> = std::thread::scope(|sc| {
            let hs: Vec<_> = groups
                .iter()
                .map(|(b, encs)| {
                    let ff = ff.clone();
                    sc.spawn(move || probe_backend(&ff, *b, encs))
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
        });
        si.probes = probes.into_iter().flatten().collect();
        si.hw_encoders = si.probes.iter().filter(|p| p.result.is_ok()).map(|p| (p.backend, p.encoder.clone())).collect();
    }

    // ── ImageMagick ──
    if let Some(m) = &tools.magick {
        si.versions.push(("ImageMagick".into(), first_line(&run_text(m, &["-version"]))));
        let (r, w) = parse_magick_formats(&run_text(m, &["-list", "format"]));
        si.magick_read = r;
        si.magick_write = w;
    } else {
        // bez ImageMagicku: vestavěný převodník (běžné formáty)
        si.magick_read = crate::imgnative::read_set();
        si.magick_write = crate::imgnative::write_set();
    }

    // ── pandoc / qpdf / typst ──
    if let Some(p) = &tools.pandoc {
        si.versions.push(("Pandoc".into(), first_line(&run_text(p, &["--version"]))));
        si.pandoc_in = run_text(p, &["--list-input-formats"]).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        si.pandoc_out = run_text(p, &["--list-output-formats"]).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    }
    if let Some(q) = &tools.qpdf {
        si.versions.push(("qpdf".into(), first_line(&run_text(q, &["--version"]))));
    }
    if let Some(t) = &tools.typst {
        si.versions.push(("Typst".into(), first_line(&run_text(t, &["--version"]))));
    }
    si
}

/// Spustí ffmpeg se zkušebním enkódem. Vrací (úspěch, vypsaný text). Po `secs` sekundách proces ukončí,
/// aby zaseknutý ovladač nezablokoval start aplikace.
fn run_probe(ff: &std::path::Path, args: &[String], secs: u64) -> (bool, String) {
    let mut cmd = command(ff);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else { return (false, String::new()) };
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).to_string()
    });
    let t0 = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if t0.elapsed() > Duration::from_secs(secs) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break false,
        }
    };
    (ok, reader.join().unwrap_or_default())
}

/// Zkusí všechny enkodéry jednoho backendu po sobě. Když chyba říká, že nefunguje celý backend
/// (starý ovladač, chybějící knihovna), zbylé enkodéry se už nezkouší a dostanou stejný důvod.
fn probe_backend(ff: &std::path::Path, b: Backend, encs: &[(&'static str, String)]) -> Vec<EncProbe> {
    let mut out: Vec<EncProbe> = Vec::new();
    let mut whole_backend: Option<Fail> = None;
    for (codec, enc) in encs {
        let codec: &'static str = *codec;
        if let Some(f) = &whole_backend {
            out.push(EncProbe { backend: b, encoder: enc.clone(), codec, result: Err(f.clone()) });
            continue;
        }
        let args = gpu::probe_args(b, enc);
        if args.is_empty() {
            out.push(EncProbe { backend: b, encoder: enc.clone(), codec, result: Err(Fail::NoDevice) });
            continue;
        }
        let mut res = run_probe(ff, &args, 20);
        // nejasná chyba může být náhodná (vytížená karta, souběh) -> jedna druhá šance
        if !res.0 && matches!(gpu::classify(&res.1), Fail::Other(_)) {
            std::thread::sleep(Duration::from_millis(300));
            res = run_probe(ff, &args, 20);
        }
        let result = if res.0 { Ok(()) } else { Err(gpu::classify(&res.1)) };
        if let Err(f @ (Fail::DriverTooOld(_) | Fail::NoLibrary)) = &result {
            whole_backend = Some(f.clone());
        }
        out.push(EncProbe { backend: b, encoder: enc.clone(), codec, result });
    }
    out
}

/// Zjistí podporu AVX-512 (CPU i operační systém). Na ARM vrací false.
fn detect_avx512() -> (bool, Vec<&'static str>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let base = std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512cd")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512dq")
            && std::is_x86_feature_detected!("avx512vl");
        if !base {
            return (false, Vec::new());
        }
        let mut f = vec!["F", "CD", "BW", "DQ", "VL"];
        if std::is_x86_feature_detected!("avx512vnni") {
            f.push("VNNI");
        }
        if std::is_x86_feature_detected!("avx512vbmi") {
            f.push("VBMI");
        }
        if std::is_x86_feature_detected!("avx512vbmi2") {
            f.push("VBMI2");
        }
        if std::is_x86_feature_detected!("avx512ifma") {
            f.push("IFMA");
        }
        if std::is_x86_feature_detected!("avx512bitalg") {
            f.push("BITALG");
        }
        (true, f)
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (false, Vec::new())
    }
}

fn detect_gpus() -> Vec<GpuInfo> {
    // (název, verze ovladače hlášená systémem)
    let lines: Vec<(String, Option<String>)> = if cfg!(windows) {
        command(std::path::Path::new("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", "Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name + '|' + $_.DriverVersion }"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| {
                        let mut it = l.splitn(2, '|');
                        let name = it.next().unwrap_or("").trim().to_string();
                        let drv = it.next().map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
                        (name, drv)
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        command(std::path::Path::new("system_profiler"))
            .arg("SPDisplaysDataType")
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix("Chipset Model:").map(|n| (n.trim().to_string(), None)))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        command(std::path::Path::new("sh"))
            .args(["-c", "lspci 2>/dev/null | grep -iE 'vga|3d|display'"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| (l.splitn(3, ':').last().unwrap_or(l).trim().to_string(), None)).collect())
            .unwrap_or_default()
    };
    let mut v: Vec<GpuInfo> = Vec::new();
    for (n, drv) in lines {
        let low = n.to_lowercase();
        if n.is_empty() || low.contains("basic render") || low.contains("remote") || low.contains("virtual") || v.iter().any(|g| g.name == n) {
            continue;
        }
        let vendor = Vendor::from_name(&n);
        let driver = if vendor == Vendor::Nvidia {
            // Windows: 31.0.15.5123 -> 551.23; Linux: z jaderného modulu / nvidia-smi
            drv.as_deref().and_then(gpu::nvidia_driver_from_windows).or_else(nvidia_driver_linux)
        } else {
            None
        };
        v.push(GpuInfo { name: n, vendor, driver });
    }
    v
}

/// Verze NVIDIA ovladače na Linuxu (z `/proc/driver/nvidia/version`, jinak z `nvidia-smi`).
fn nvidia_driver_linux() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    if let Ok(t) = std::fs::read_to_string("/proc/driver/nvidia/version") {
        if let Some(rest) = t.split("Kernel Module").nth(1) {
            if let Some(v) = rest.split_whitespace().next() {
                return Some(v.to_string());
            }
        }
    }
    let o = command(std::path::Path::new("nvidia-smi")).args(["--query-gpu=driver_version", "--format=csv,noheader"]).output().ok()?;
    String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

/// Rozparsuje `magick -list format`. Umí starší výstup (Format Module Mode Description)
/// i ImageMagick 7 (Format Mode Description) – sloupec Mode se hledá podle tvaru "rw+".
pub fn parse_magick_formats(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut read = BTreeSet::new();
    let mut write = BTreeSet::new();
    let mut in_table = false;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim_start().starts_with("---") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 2 {
            continue;
        }
        let name = t[0].trim_end_matches('*').to_lowercase();
        let is_mode = |x: &str| x.len() == 3 && x.chars().all(|c| matches!(c, 'r' | 'w' | '+' | '-'));
        let Some(mode) = t.iter().skip(1).take(2).find(|x| is_mode(x)) else { continue };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        if mode.starts_with('r') {
            read.insert(name.clone());
        }
        if mode.as_bytes()[1] == b'w' {
            write.insert(name);
        }
    }
    (read, write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_old_and_new_magick_list_format() {
        let old = "   Format  Module    Mode  Description\n-------------------------------------------------------------------------------\n      3FR  DNG       r--   Hasselblad CFV/H3D39II\n      PNG* PNG       rw-   Portable Network Graphics\n";
        let new = "   Format  Mode  Description\r\n-------------------------------------------------------------------------------\r\n      3FR  r--   Hasselblad CFV/H3D39II\r\n     AVIF* rw+   AV1 Image File Format\r\n      JXL  rw+   JPEG XL\r\n      PDF* rw+   Portable Document Format\r\n";
        let (r, w) = parse_magick_formats(old);
        assert!(r.contains("3fr") && !w.contains("3fr") && w.contains("png"));
        let (r, w) = parse_magick_formats(new);
        assert!(r.contains("3fr") && !w.contains("3fr"));
        assert!(w.contains("avif") && w.contains("jxl") && w.contains("pdf"));
        assert_eq!(r.len(), 4);
    }
}

//! Drobné pomocné funkce.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Vytvoří `Command`, který na Windows neotevírá okno konzole.
pub fn command(program: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir);
    let d = base.join("TomasekConvert");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        for cand in [name.to_string(), format!("{name}.exe")] {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn fmt_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn fmt_dur(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".into();
    }
    let s = secs.round() as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Dlouhá doba pro statistiky ("3 h 12 min").
pub fn fmt_long(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    let (d, h, m, s) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60, s % 60);
    if d > 0 {
        format!("{d} d {h} h {m} min")
    } else if h > 0 {
        format!("{h} h {m} min")
    } else if m > 0 {
        format!("{m} min {s} s")
    } else {
        format!("{s} s")
    }
}

/// Přípona malými písmeny; zná i dvojité (tar.gz apod.).
pub fn full_ext(p: &Path) -> String {
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    for d in ["tar.gz", "tar.bz2", "tar.xz", "tar.zst"] {
        if name.ends_with(&format!(".{d}")) {
            return d.to_string();
        }
    }
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn stem_of(p: &Path) -> String {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = full_ext(p);
    if !ext.is_empty() && name.to_lowercase().ends_with(&format!(".{ext}")) {
        name[..name.len() - ext.len() - 1].to_string()
    } else {
        name
    }
}

static RESERVED: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);

/// Když cesta existuje (nebo ji právě používá jiná úloha), přidá " (1)", " (2)"...
/// Jméno se rezervuje, takže souběžné převody nikdy nezapíšou do stejného souboru.
pub fn unique_path(p: PathBuf) -> PathBuf {
    let mut g = RESERVED.lock().unwrap();
    let set = g.get_or_insert_with(Default::default);
    let key = |c: &Path| c.to_string_lossy().to_lowercase();
    if !p.exists() && !set.contains(&key(&p)) {
        set.insert(key(&p));
        return p;
    }
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = stem_of(&p);
    let ext = full_ext(&p);
    for i in 1..10_000 {
        let name = if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") };
        let c = dir.join(name);
        if !c.exists() && !set.contains(&key(&c)) {
            set.insert(key(&c));
            return c;
        }
    }
    p
}

pub fn path_size(p: &Path) -> u64 {
    if p.is_file() {
        return std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(p)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Windows: cesta bez prefixu `\\?\\` (ten pochází z `canonicalize`) a s lomítky `\\`; Explorer jinak neumí vybrat soubor.
#[cfg(windows)]
fn win_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    let s = s.strip_prefix(r"\\?\UNC\").map(|r| format!(r"\\{r}")).unwrap_or_else(|| s.strip_prefix(r"\\?\").unwrap_or(&s).to_string());
    s.replace('/', "\\")
}

/// Ukáže soubor/složku ve správci souborů (soubor se rovnou označí).
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Rust by argument s mezerou obalil do uvozovek jako celek ("/select,C:\a b\c") a Explorer to
        // nepochopí (otevře Dokumenty). Proto se příkazová řádka skládá ručně.
        let p = win_path(path);
        let mut c = Command::new("explorer.exe");
        if path.is_dir() {
            c.raw_arg(format!("\"{p}\""));
        } else {
            c.raw_arg(format!("/select,\"{p}\""));
        }
        let _ = c.spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        if path.is_dir() {
            c.arg(path);
        } else {
            c.arg("-R").arg(path);
        }
        let _ = c.spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // správce souborů přes D-Bus (Nautilus, Dolphin, Nemo, Thunar…) – soubor se označí; jinak otevřeme složku
        if path.exists() {
            let uri = format!("file://{}", path.to_string_lossy().replace(' ', "%20"));
            let ok = Command::new("dbus-send")
                .args(["--session", "--print-reply", "--dest=org.freedesktop.FileManager1", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems"])
                .arg(format!("array:string:{uri}"))
                .arg("string:")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return;
            }
        }
        let dir = if path.is_file() { path.parent().unwrap_or(path) } else { path };
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Otevře soubor/složku výchozí aplikací.
pub fn open_path(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("explorer.exe").raw_arg(format!("\"{}\"", win_path(path))).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

/// Otevře adresu v prohlížeči.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

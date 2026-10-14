//! Aktualizace z GitHub Releases – bez účtu, bez registrací, jen veřejné HTTPS.
//!
//! * Kontrola běží JEN na požádání (nebo při startu, pokud to zapneš) a stáhne jen informaci o poslední verzi.
//! * Instalace: Windows instalátor se spustí potichu přes stejný `Setup.exe` (nahradí exe, nastavení v
//!   `%LOCALAPPDATA%\TomasekConvert` zůstane); přenosná exe / Linuxová binárka se vymění na místě.
//! * Každé stažení se ověří proti `.sha256` souboru z vydání; bez shody se nic nespustí.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// Repozitář s vydáními.
pub const REPO: &str = "TomasekValla/TomasekConvert";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallMode {
    /// Windows: spustí se nový `TomasekConvert-Setup-<arch>.exe` (/SILENT)
    WindowsInstaller,
    /// Přenosná exe (Windows) nebo Linuxová binárka: výměna souboru na místě
    ReplaceFile,
    /// Nelze udělat automaticky (balíček z apt, macOS .app…): otevře se stránka vydání
    Manual,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub asset: Option<Asset>,
    pub mode: InstallMode,
}

#[derive(Clone, Debug, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading(UpdateInfo, f32),
    Ready(UpdateInfo, PathBuf),
    Installing,
    Error(String),
}

fn parse_ver(s: &str) -> Vec<u64> {
    s.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Je `latest` novější než `current`? (porovnání čísel verze 1.2.3)
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_ver(latest), parse_ver(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn arch_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Jak se tahle kopie aplikace dá aktualizovat.
pub fn install_mode() -> InstallMode {
    let Ok(exe) = std::env::current_exe() else { return InstallMode::Manual };
    let Some(dir) = exe.parent() else { return InstallMode::Manual };
    if cfg!(windows) {
        // instalováno přes Inno Setup => vedle exe leží unins000.exe
        if dir.join("unins000.exe").exists() {
            return InstallMode::WindowsInstaller;
        }
        return if dir_writable(dir) { InstallMode::ReplaceFile } else { InstallMode::Manual };
    }
    if cfg!(target_os = "macos") {
        return InstallMode::Manual;
    }
    // Linux: /usr/bin (balíček .deb) spravuje apt; ~/.local/bin a jiné zapisovatelné složky vyměníme sami
    if exe.starts_with("/usr/") && !exe.starts_with("/usr/local/") {
        return InstallMode::Manual;
    }
    if dir_writable(dir) {
        InstallMode::ReplaceFile
    } else {
        InstallMode::Manual
    }
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".tc-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Názvy souborů ve vydání pro tuhle platformu a způsob instalace (nejnovější pojmenování první).
/// Přenosné verze mají v názvu `portable` (od 2.2.1); starší název zůstává jako záloha, ať se dá
/// aktualizovat i ze starších vydání.
pub fn asset_names(mode: &InstallMode, os: &str, arch: &str) -> Vec<String> {
    match (mode, os) {
        (InstallMode::WindowsInstaller, "windows") => vec![format!("TomasekConvert-Setup-{arch}.exe")],
        (InstallMode::ReplaceFile, "windows") => vec![format!("TomasekConvert-portable-windows-{arch}.exe"), format!("TomasekConvert-windows-{arch}.exe")],
        (InstallMode::ReplaceFile, "linux") => vec![format!("TomasekConvert-portable-linux-{arch}.tar.gz"), format!("TomasekConvert-linux-{arch}.tar.gz")],
        _ => Vec::new(),
    }
}

/// Vybere z přiložených souborů vydání ten správný (a k němu `.sha256`). `assets` = (název, url, velikost).
pub fn pick_asset(assets: &[(String, String, u64)], mode: &InstallMode, os: &str, arch: &str) -> Option<Asset> {
    asset_names(mode, os, arch).into_iter().find_map(|want| {
        let (name, url, size) = assets.iter().find(|(n, _, _)| *n == want)?.clone();
        let sha_url = assets.iter().find(|(n, _, _)| *n == format!("{want}.sha256")).map(|(_, u, _)| u.clone());
        Some(Asset { name, url, size, sha_url })
    })
}

fn agent(timeout: u64) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(Duration::from_secs(timeout))).build().into()
}

pub fn check(current: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent(15).get(&url).header("User-Agent", "TomasekConvert-update-check").header("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?;
    let text = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_release(&text, current, &install_mode(), os_name(), arch_name())
}

pub fn parse_release(json: &str, current: &str, mode: &InstallMode, os: &str, arch: &str) -> Result<Option<UpdateInfo>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = v.get("tag_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if tag.is_empty() {
        return Err(crate::tr!("Žádné vydání nenalezeno.", "No release found.").to_string());
    }
    if !is_newer(&tag, current) {
        return Ok(None);
    }
    let assets: Vec<(String, String, u64)> = v
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| Some((x.get("name")?.as_str()?.to_string(), x.get("browser_download_url")?.as_str()?.to_string(), x.get("size").and_then(|s| s.as_u64()).unwrap_or(0))))
                .collect()
        })
        .unwrap_or_default();
    let asset = pick_asset(&assets, mode, os, arch);
    let mode = if asset.is_some() { mode.clone() } else { InstallMode::Manual };
    Ok(Some(UpdateInfo {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: v.get("html_url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        notes: v.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        asset,
        mode,
    }))
}

fn update_dir() -> PathBuf {
    let d = crate::util::app_dir().join("update");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Stahovat se smí jen z vydání našeho repozitáře.
fn trusted_url(u: &str) -> bool {
    u.starts_with(&format!("https://github.com/{REPO}/releases/download/"))
}

fn parse_sha(text: &str) -> Option<String> {
    let t = text.split_whitespace().next()?.to_lowercase();
    (t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit())).then_some(t)
}

/// Stáhne soubor aktualizace, hlásí průběh (0..1) a ověří SHA-256. Vrací cestu ke staženému souboru.
pub fn download(info: &UpdateInfo, progress: &dyn Fn(f32)) -> Result<PathBuf, String> {
    let a = info.asset.as_ref().ok_or_else(|| crate::tr!("K téhle verzi není soubor pro tvůj systém.", "This release has no file for your system.").to_string())?;
    if !trusted_url(&a.url) {
        return Err(crate::tr!("Odkaz ke stažení nepochází z oficiálního repozitáře.", "The download link is not from the official repository.").to_string());
    }
    let sha_url = a.sha_url.as_ref().filter(|u| trusted_url(u)).ok_or_else(|| crate::tr!("Chybí kontrolní součet (.sha256) – aktualizaci nestahuji.", "The checksum (.sha256) is missing – not downloading the update.").to_string())?;
    let ag = agent(900);
    let expected = {
        let mut r = ag.get(sha_url).header("User-Agent", "TomasekConvert-updater").call().map_err(|e| e.to_string())?;
        parse_sha(&r.body_mut().read_to_string().map_err(|e| e.to_string())?).ok_or_else(|| crate::tr!("Soubor s kontrolním součtem je neplatný.", "The checksum file is invalid.").to_string())?
    };
    let dir = update_dir();
    let dest = dir.join(&a.name);
    let part = dir.join(format!("{}.part", a.name));
    let mut resp = ag.get(&a.url).header("User-Agent", "TomasekConvert-updater").call().map_err(|e| e.to_string())?;
    let total = resp.body().content_length().or(Some(a.size).filter(|s| *s > 0));
    let mut reader = resp.body_mut().as_reader();
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut got: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if let Some(t) = total {
            progress((got as f32 / t as f32).clamp(0.0, 1.0));
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        let _ = std::fs::remove_file(&part);
        return Err(crate::tr!("Kontrolní součet se neshoduje – stažený soubor je poškozený, nespouštím ho.", "Checksum mismatch – the downloaded file is corrupt, not running it.").to_string());
    }
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Nainstaluje stažený soubor a ukončí appku (nová verze se spustí sama). Při chybě se vrátí.
pub fn apply(info: &UpdateInfo, file: &Path, hidden: bool) -> Result<(), String> {
    match info.mode {
        InstallMode::WindowsInstaller => {
            #[cfg(windows)]
            {
                // /RELAUNCH=1 přidává náš instalátor: po tiché instalaci appku znovu spustí
                std::process::Command::new(file)
                    .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
                    .arg(if hidden { "/RELAUNCH=tray" } else { "/RELAUNCH=1" })
                    .spawn()
                    .map_err(|e| e.to_string())?;
                finish_and_exit();
            }
            #[cfg(not(windows))]
            {
                let _ = file;
                Err("installer".into())
            }
        }
        InstallMode::ReplaceFile => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let new_bin = if cfg!(windows) { file.to_path_buf() } else { extract_binary(file, &exe)? };
            replace_exe(&exe, &new_bin)?;
            let mut c = std::process::Command::new(&exe);
            c.arg("--after-update");
            if hidden {
                c.arg("--tray");
            }
            c.spawn().map_err(|e| e.to_string())?;
            finish_and_exit();
        }
        InstallMode::Manual => {
            crate::util::open_url(&info.url);
            Ok(())
        }
    }
}

fn finish_and_exit() -> ! {
    crate::instance::release();
    std::process::exit(0)
}

/// Z `.tar.gz` vytáhne binárku vedle staré (aby šlo atomicky přejmenovat).
fn extract_binary(tgz: &Path, exe: &Path) -> Result<PathBuf, String> {
    let f = std::fs::File::open(tgz).map_err(|e| e.to_string())?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    let target = exe.with_extension("new");
    for e in ar.entries().map_err(|e| e.to_string())? {
        let mut e = e.map_err(|e| e.to_string())?;
        let p = e.path().map_err(|e| e.to_string())?.to_path_buf();
        if p.file_name().map(|n| n == "TomasekConvert").unwrap_or(false) && e.header().entry_type().is_file() {
            e.unpack(&target).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
            }
            return Ok(target);
        }
    }
    Err(crate::tr!("V archivu aktualizace chybí program.", "The update archive does not contain the program.").to_string())
}

/// Vymění běžící program za nový. Windows dovolí běžící exe přejmenovat, ale ne přepsat; Linux dovolí obojí.
fn replace_exe(exe: &Path, new_bin: &Path) -> Result<(), String> {
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| e.to_string())?;
    let res = if new_bin.parent() == exe.parent() { std::fs::rename(new_bin, exe) } else { std::fs::copy(new_bin, exe).map(|_| ()) };
    if let Err(e) = res {
        let _ = std::fs::rename(&old, exe); // vrátit starý program
        return Err(e.to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Po startu uklidí zbytky předchozí aktualizace (stará exe, stažené soubory).
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
        let _ = std::fs::remove_file(exe.with_extension("new"));
    }
    let _ = std::fs::remove_dir_all(crate::util::app_dir().join("update"));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_compare() {
        assert!(is_newer("v2.1.0", "2.0.9"));
        assert!(is_newer("2.10.0", "2.9.0"));
        assert!(!is_newer("2.0.0", "2.0.0"));
        assert!(!is_newer("v1.9.9", "2.0.0"));
        assert!(is_newer("3", "2.9.9"));
        assert!(is_newer("v2.2.0", "2.1.0"));
    }

    fn sample() -> String {
        let base = format!("https://github.com/{REPO}/releases/download/v2.3.0");
        serde_json::json!({
            "tag_name": "v2.3.0",
            "html_url": format!("https://github.com/{REPO}/releases/tag/v2.3.0"),
            "body": "notes",
            "assets": [
                {"name": "TomasekConvert-Setup-x64.exe", "browser_download_url": format!("{base}/TomasekConvert-Setup-x64.exe"), "size": 100},
                {"name": "TomasekConvert-Setup-x64.exe.sha256", "browser_download_url": format!("{base}/TomasekConvert-Setup-x64.exe.sha256"), "size": 90},
                {"name": "TomasekConvert-portable-windows-x64.exe", "browser_download_url": format!("{base}/TomasekConvert-portable-windows-x64.exe"), "size": 100},
                {"name": "TomasekConvert-portable-linux-arm64.tar.gz", "browser_download_url": format!("{base}/TomasekConvert-portable-linux-arm64.tar.gz"), "size": 100},
                {"name": "TomasekConvert-portable-linux-arm64.tar.gz.sha256", "browser_download_url": format!("{base}/TomasekConvert-portable-linux-arm64.tar.gz.sha256"), "size": 90},
                {"name": "TomasekConvert-linux-x64.tar.gz", "browser_download_url": format!("{base}/TomasekConvert-linux-x64.tar.gz"), "size": 100}
            ]
        })
        .to_string()
    }

    #[test]
    fn picks_the_right_file_per_platform() {
        let j = sample();
        let u = parse_release(&j, "2.2.0", &InstallMode::WindowsInstaller, "windows", "x64").unwrap().unwrap();
        assert_eq!(u.version, "2.3.0");
        let a = u.asset.unwrap();
        assert_eq!(a.name, "TomasekConvert-Setup-x64.exe");
        assert!(a.sha_url.unwrap().ends_with(".sha256"));
        assert!(trusted_url(&a.url));
        // přenosná exe nemá .sha256 v seznamu -> stahování se pak odmítne (kontrolní součet je povinný)
        let u = parse_release(&j, "2.2.0", &InstallMode::ReplaceFile, "windows", "x64").unwrap().unwrap();
        assert!(u.asset.as_ref().unwrap().sha_url.is_none());
        let u = parse_release(&j, "2.2.0", &InstallMode::ReplaceFile, "linux", "arm64").unwrap().unwrap();
        assert_eq!(u.asset.unwrap().name, "TomasekConvert-portable-linux-arm64.tar.gz");
        // starý název (před 2.2.1) se pořád pozná
        let u = parse_release(&j, "2.2.0", &InstallMode::ReplaceFile, "linux", "x64").unwrap().unwrap();
        assert_eq!(u.asset.unwrap().name, "TomasekConvert-linux-x64.tar.gz");
        let u = parse_release(&j, "2.2.0", &InstallMode::ReplaceFile, "windows", "x64").unwrap().unwrap();
        assert_eq!(u.asset.unwrap().name, "TomasekConvert-portable-windows-x64.exe");
        // pro macOS / balíček .deb nic automatického -> ruční stažení
        let u = parse_release(&j, "2.2.0", &InstallMode::Manual, "macos", "arm64").unwrap().unwrap();
        assert!(u.asset.is_none() && u.mode == InstallMode::Manual);
        // aktuální verze -> žádná aktualizace
        assert!(parse_release(&j, "2.3.0", &InstallMode::Manual, "linux", "x64").unwrap().is_none());
    }

    #[test]
    fn only_official_urls_and_valid_checksums_are_accepted() {
        assert!(!trusted_url("https://evil.example/TomasekValla/TomasekConvert/releases/download/x"));
        assert!(!trusted_url(&format!("http://github.com/{REPO}/releases/download/x")));
        assert!(parse_sha("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  file.exe\n").is_some());
        assert!(parse_sha("nonsense").is_none());
    }
}

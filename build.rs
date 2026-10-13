//! build.rs
//!
//! Při `cargo build` stáhne (JEDNOU) potřebné nástroje (ffmpeg, ImageMagick, pandoc,
//! typst, pdfium), zkomprimuje je zstd a vloží přímo do výsledného .exe.
//! Za běhu appka NEKOMUNIKUJE s internetem a nic nebere ze systému.
//!
//! Hotové balíčky se cachují do `vendor-cache/` – další buildy už nic nestahují.
//! Chceš aktualizovat nástroje? Smaž složku `vendor-cache/`.
//! Za firewallem? Ručně stažené archivy dej do `vendor-cache/downloads/`.

use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

struct Tool {
    name: &'static str,
    feature: &'static str,
}

const TOOLS: &[Tool] = &[
    Tool { name: "ffmpeg", feature: "BUNDLE_FFMPEG" },
    Tool { name: "magick", feature: "BUNDLE_MAGICK" },
    Tool { name: "pandoc", feature: "BUNDLE_PANDOC" },
    Tool { name: "typst", feature: "BUNDLE_TYPST" },
    Tool { name: "pdfium", feature: "BUNDLE_PDFIUM" },
    Tool { name: "qpdf", feature: "BUNDLE_QPDF" },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_windows_resources();
    println!("cargo:rerun-if-env-changed=TC_ZSTD_LEVEL");
    println!("cargo:rerun-if-env-changed=TC_NO_BUNDLE");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    // cache je oddělená podle platformy, aby šlo buildit víc cílů ze stejné složky
    let cache = manifest.join("vendor-cache").join(format!("{target_os}-{target_arch}"));
    let no_bundle = env::var("TC_NO_BUNDLE").is_ok();

    for t in TOOLS {
        let dest = out.join(format!("{}.tar.zst", t.name));
        let enabled = env::var(format!("CARGO_FEATURE_{}", t.feature)).is_ok() && !no_bundle;
        if !enabled {
            fs::write(&dest, b"").unwrap();
            continue;
        }
        let plat = Plat { os: &target_os, arch: &target_arch };
        if !plat.supported(t.name) {
            println!("cargo:warning={}: pro {target_os}/{target_arch} se nebundluje – použije se nástroj nainstalovaný v systému (PATH)", t.name);
            fs::write(&dest, b"").unwrap();
            continue;
        }

        let cached = cache.join(format!("{}.tar.zst", t.name));
        if !cached.exists() {
            fs::create_dir_all(cache.join("downloads")).unwrap();
            let stage = out.join(format!("stage-{}", t.name));
            let _ = fs::remove_dir_all(&stage);
            fs::create_dir_all(&stage).unwrap();
            println!("cargo:warning=Připravuji {} (stahuje se jen při prvním buildu)...", t.name);
            match t.name {
                "magick" => prep_magick(&cache, &stage),
                "qpdf" if plat.win() => prep_qpdf(&cache, &stage),
                "qpdf" => prep_qpdf_unix(&cache, &stage, &target_os, &target_arch),
                name => {
                    for src in plat.sources(name) {
                        fetch_extract(&cache, &src, &stage);
                    }
                }
            }
            make_executable(&stage);
            pack_dir(&stage, &cached);
            let _ = fs::remove_dir_all(&stage);
        }
        fs::copy(&cached, &dest).unwrap_or_else(|e| panic!("kopie {cached:?} -> {dest:?}: {e}"));
    }
}

/// Cílová platforma a zdroje nástrojů pro ni.
struct Plat<'a> {
    os: &'a str,
    arch: &'a str,
}

/// Jeden stažený archiv + soubory, které z něj chceme (koncovka cesty v archivu, jméno ve stage).
struct Source {
    url: String,
    files: Vec<(String, String)>,
}

fn src(url: String, files: &[(&str, &str)]) -> Source {
    Source { url, files: files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
}

impl Plat<'_> {
    fn win(&self) -> bool {
        self.os == "windows"
    }
    fn arm(&self) -> bool {
        self.arch == "aarch64"
    }
    fn supported(&self, tool: &str) -> bool {
        match (tool, self.os, self.arch) {
            ("magick" | "qpdf", "windows", "x86_64" | "aarch64") => true,
            // qpdf má oficiální přenosné binárky i pro Linux a macOS (bin/ + lib/)
            ("qpdf", "linux", "x86_64" | "aarch64") | ("qpdf", "macos", "aarch64" | "x86_64") => true,
            // ImageMagick pro Linux/macOS nemá oficiální přenosný balíček -> použije se systémový, jinak vestavěný převodník
            ("magick", _, _) => false,
            ("qpdf", _, _) => false,
            (_, "windows", "x86_64" | "aarch64") => true,
            (_, "linux", "x86_64" | "aarch64") => true,
            (_, "macos", "aarch64" | "x86_64") => true,
            _ => false,
        }
    }
    fn sources(&self, tool: &str) -> Vec<Source> {
        let (win, arm) = (self.win(), self.arm());
        match tool {
            "ffmpeg" => {
                if self.os == "macos" {
                    // statické buildy pro macOS (BtbN je nemá)
                    let a = if arm { "arm64" } else { "amd64" };
                    let u = url_or("TC_FFMPEG_URL", || format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{a}/release/ffmpeg.zip"));
                    let up = format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{a}/release/ffprobe.zip");
                    return vec![src(u, &[("ffmpeg", "ffmpeg")]), src(up, &[("ffprobe", "ffprobe")])];
                }
                let (name, ext) = match (self.os, arm) {
                    ("windows", false) => ("win64", "zip"),
                    ("windows", true) => ("winarm64", "zip"),
                    (_, false) => ("linux64", "tar.xz"),
                    (_, true) => ("linuxarm64", "tar.xz"),
                };
                let u = url_or("TC_FFMPEG_URL", || format!("https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-{name}-gpl.{ext}"));
                if win {
                    vec![src(u, &[("bin/ffmpeg.exe", "ffmpeg.exe"), ("bin/ffprobe.exe", "ffprobe.exe")])]
                } else {
                    vec![src(u, &[("/bin/ffmpeg", "ffmpeg"), ("/bin/ffprobe", "ffprobe")])]
                }
            }
            "pandoc" => {
                let tag = latest_tag("jgm/pandoc");
                let (asset, ext) = match (self.os, self.arch) {
                    ("windows", _) => (format!("pandoc-{tag}-windows-x86_64.zip"), "pandoc.exe"), // na ARM běží v emulaci
                    ("linux", "x86_64") => (format!("pandoc-{tag}-linux-amd64.tar.gz"), "/bin/pandoc"),
                    ("linux", _) => (format!("pandoc-{tag}-linux-arm64.tar.gz"), "/bin/pandoc"),
                    (_, "aarch64") => (format!("pandoc-{tag}-arm64-macOS.zip"), "/bin/pandoc"),
                    _ => (format!("pandoc-{tag}-x86_64-macOS.zip"), "/bin/pandoc"),
                };
                let u = url_or("TC_PANDOC_URL", || format!("https://github.com/jgm/pandoc/releases/download/{tag}/{asset}"));
                vec![src(u, &[(ext, if win { "pandoc.exe" } else { "pandoc" })])]
            }
            "typst" => {
                let triple = match (self.os, self.arch) {
                    ("windows", "x86_64") => "x86_64-pc-windows-msvc.zip",
                    ("windows", _) => "aarch64-pc-windows-msvc.zip",
                    ("linux", "x86_64") => "x86_64-unknown-linux-musl.tar.xz",
                    ("linux", _) => "aarch64-unknown-linux-musl.tar.xz",
                    (_, "aarch64") => "aarch64-apple-darwin.tar.xz",
                    _ => "x86_64-apple-darwin.tar.xz",
                };
                let u = url_or("TC_TYPST_URL", || format!("https://github.com/typst/typst/releases/latest/download/typst-{triple}"));
                vec![src(u, &[(if win { "typst.exe" } else { "/typst" }, if win { "typst.exe" } else { "typst" })])]
            }
            "pdfium" => {
                let (asset, inner, out_name) = match (self.os, self.arch) {
                    ("windows", "x86_64") => ("pdfium-win-x64.tgz", "bin/pdfium.dll", "pdfium.dll"),
                    ("windows", _) => ("pdfium-win-arm64.tgz", "bin/pdfium.dll", "pdfium.dll"),
                    ("linux", "x86_64") => ("pdfium-linux-x64.tgz", "lib/libpdfium.so", "libpdfium.so"),
                    ("linux", _) => ("pdfium-linux-arm64.tgz", "lib/libpdfium.so", "libpdfium.so"),
                    (_, "aarch64") => ("pdfium-mac-arm64.tgz", "lib/libpdfium.dylib", "libpdfium.dylib"),
                    _ => ("pdfium-mac-x64.tgz", "lib/libpdfium.dylib", "libpdfium.dylib"),
                };
                let u = url_or("TC_PDFIUM_URL", || format!("https://github.com/bblanchon/pdfium-binaries/releases/latest/download/{asset}"));
                vec![src(u, &[(inner, out_name)])]
            }
            _ => Vec::new(),
        }
    }
}

// ───────────────────────────── stahování ─────────────────────────────

fn agent(follow_redirects: bool) -> ureq::Agent {
    // systémové certifikáty (funguje i za firemní/školní proxy)
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let mut b = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(3600)));
    if !follow_redirects {
        b = b.max_redirects(0).max_redirects_will_error(false).http_status_as_error(false);
    }
    b.build().into()
}

/// Zjistí poslední tag z GitHubu přes přesměrování (bez API limitů).
fn latest_tag(repo: &str) -> String {
    let url = format!("https://github.com/{repo}/releases/latest");
    let resp = agent(false)
        .get(&url)
        .call()
        .unwrap_or_else(|e| panic!("Nepodařilo se zjistit poslední verzi {repo}: {e}"));
    let loc = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| panic!("GitHub nevrátil přesměrování pro {repo}"))
        .to_string();
    loc.rsplit('/').next().unwrap().to_string()
}

/// Stáhne soubor do vendor-cache/downloads (nebo použije ručně vložený).
fn download(cache: &Path, url: &str) -> PathBuf {
    let file_name = url.rsplit('/').next().unwrap();
    let dest = cache.join("downloads").join(file_name);
    if dest.exists() {
        println!("cargo:warning=Používám lokální {}", dest.display());
        return dest;
    }
    println!("cargo:warning=Stahuji {url}");
    let resp = agent(true).get(url).call().unwrap_or_else(|e| {
        panic!(
            "\n\nNepodařilo se stáhnout {url}\n  {e}\n\
             Stáhni soubor ručně do {} a spusť build znovu.\n",
            dest.parent().unwrap().display()
        )
    });
    let tmp = dest.with_extension("part");
    let mut reader = resp.into_body().into_reader();
    let mut f = File::create(&tmp).unwrap();
    io::copy(&mut reader, &mut f).unwrap_or_else(|e| panic!("stahování {url} selhalo: {e}"));
    drop(f);
    fs::rename(&tmp, &dest).unwrap();
    dest
}

fn url_or(env_name: &str, default: impl FnOnce() -> String) -> String {
    println!("cargo:rerun-if-env-changed={env_name}");
    env::var(env_name).unwrap_or_else(|_| default())
}

// ───────────────────────────── rozbalování ─────────────────────────────


/// Stáhne archiv (zip / tar.gz / tgz / tar.xz) a vytáhne z něj požadované soubory do `stage`.
fn fetch_extract(cache: &Path, source: &Source, stage: &Path) {
    let archive = download(cache, &source.url);
    let lower = source.url.to_lowercase();
    let mut found = vec![false; source.files.len()];
    let mut take = |name: &str, reader: &mut dyn io::Read| {
        let n = name.replace('\\', "/").to_lowercase();
        for (k, (suffix, out_name)) in source.files.iter().enumerate() {
            if !found[k] && n.ends_with(&suffix.to_lowercase()) {
                let mut out = File::create(stage.join(out_name)).unwrap();
                io::copy(reader, &mut out).unwrap();
                found[k] = true;
                return;
            }
        }
    };
    if lower.ends_with(".zip") {
        let mut ar = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        for i in 0..ar.len() {
            let mut e = ar.by_index(i).unwrap();
            if e.is_file() {
                let name = e.name().to_string();
                take(&name, &mut e);
            }
        }
    } else {
        let f = File::open(&archive).unwrap();
        let reader: Box<dyn io::Read> = if lower.ends_with(".tar.xz") {
            Box::new(liblzma::read::XzDecoder::new(f))
        } else {
            Box::new(flate2::read::GzDecoder::new(f))
        };
        let mut ar = tar::Archive::new(reader);
        for e in ar.entries().unwrap() {
            let mut e = e.unwrap();
            if e.header().entry_type().is_file() {
                let name = e.path().unwrap().to_string_lossy().to_string();
                take(&name, &mut e);
            }
        }
    }
    for (k, (suffix, _)) in source.files.iter().enumerate() {
        assert!(found[k], "V archivu {} chybí {suffix}", source.url);
    }
    let _ = fs::remove_file(archive);
}

/// Na Unixu musí být vložené programy spustitelné.
fn make_executable(stage: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(rd) = fs::read_dir(stage) {
            for e in rd.flatten() {
                let _ = fs::set_permissions(e.path(), fs::Permissions::from_mode(0o755));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = stage;
}

fn prep_qpdf(cache: &Path, stage: &Path) {
    let url = url_or("TC_QPDF_URL", || {
        let tag = latest_tag("qpdf/qpdf"); // např. v12.2.0
        let ver = tag.trim_start_matches('v');
        format!("https://github.com/qpdf/qpdf/releases/download/{tag}/qpdf-{ver}-msvc64.zip")
    });
    let z = download(cache, &url);
    // qpdf.exe potřebuje své DLL z bin/
    let mut ar = zip::ZipArchive::new(File::open(&z).unwrap()).unwrap();
    let mut got = false;
    for i in 0..ar.len() {
        let mut e = ar.by_index(i).unwrap();
        let name = e.name().replace('\\', "/");
        if let Some(pos) = name.rfind("/bin/") {
            let file = &name[pos + 5..];
            if !file.is_empty() && (file.ends_with(".exe") || file.ends_with(".dll")) && !file.starts_with("fix-qdf") && !file.starts_with("zlib-flate") {
                let mut out = File::create(stage.join(file)).unwrap();
                io::copy(&mut e, &mut out).unwrap();
                if file == "qpdf.exe" {
                    got = true;
                }
            }
        }
    }
    assert!(got, "V qpdf archivu chybí bin/qpdf.exe");
    let _ = fs::remove_file(z);
}

/// Linux / macOS: oficiální přenosný balíček qpdf (`bin/qpdf` + `lib/*`, program si knihovny hledá přes RUNPATH ../lib).
fn prep_qpdf_unix(cache: &Path, stage: &Path, os: &str, arch: &str) {
    let url = url_or("TC_QPDF_URL", || {
        let tag = latest_tag("qpdf/qpdf");
        let ver = tag.trim_start_matches('v');
        let plat = match (os, arch) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", _) => "macos-x86_64",
            (_, "aarch64") => "linux-aarch64",
            _ => "linux-x86_64",
        };
        format!("https://github.com/qpdf/qpdf/releases/download/{tag}/qpdf-{ver}-bin-{plat}.zip")
    });
    let z = download(cache, &url);
    let mut ar = zip::ZipArchive::new(File::open(&z).unwrap()).unwrap();
    let mut got = false;
    for i in 0..ar.len() {
        let mut e = ar.by_index(i).unwrap();
        let name = e.name().replace('\\', "/");
        let rel = name.trim_start_matches("./").to_string();
        if !(rel == "bin/qpdf" || (rel.starts_with("lib/") && !e.is_dir())) {
            continue;
        }
        let dest = stage.join(&rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let mode = e.unix_mode().unwrap_or(0o644);
        if mode & 0o170000 == 0o120000 {
            // symbolický odkaz (např. libqpdf.so.30 -> libqpdf.so.30.4.2): v zipu je jeho cíl jako text
            let mut target = String::new();
            io::Read::read_to_string(&mut e, &mut target).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(target.trim(), &dest).unwrap();
            #[cfg(not(unix))]
            fs::write(&dest, target).unwrap();
            continue;
        }
        let mut out = File::create(&dest).unwrap();
        io::copy(&mut e, &mut out).unwrap();
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
        }
        if rel == "bin/qpdf" {
            got = true;
        }
    }
    assert!(got, "V qpdf archivu chybí bin/qpdf");
    let _ = fs::remove_file(z);
}

fn prep_magick(cache: &Path, stage: &Path) {
    let url = url_or("TC_MAGICK_URL", || {
        let tag = latest_tag("ImageMagick/ImageMagick");
        format!(
            "https://github.com/ImageMagick/ImageMagick/releases/download/{tag}/ImageMagick-{tag}-portable-Q16-HDRI-x64.7z"
        )
    });
    let z = download(cache, &url);
    let mut rd = sevenz_rust2::ArchiveReader::open(&z, sevenz_rust2::Password::empty())
        .unwrap_or_else(|e| panic!("7z: {e}"));
    let mut got_magick = false;
    rd.for_each_entries(|entry, reader| {
        let name = entry.name().to_lowercase();
        let keep = name == "magick.exe"
            || name.ends_with(".xml")
            || name.ends_with(".icc")
            || name == "license.txt";
        if keep && !entry.is_directory() {
            let mut out = File::create(stage.join(entry.name()))?;
            io::copy(reader, &mut out)?;
            if name == "magick.exe" {
                got_magick = true;
            }
        } else {
            io::copy(reader, &mut io::sink())?;
        }
        Ok(true)
    })
    .unwrap_or_else(|e| panic!("rozbalení ImageMagick: {e}"));
    assert!(got_magick, "V ImageMagick archivu chybí magick.exe");
    let _ = fs::remove_file(z);
}

// ───────────────────────────── balení ─────────────────────────────

fn pack_dir(stage: &Path, dest: &Path) {
    let level: i32 = env::var("TC_ZSTD_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(12);
    let tmp = dest.with_extension("part");
    let f = File::create(&tmp).unwrap();
    let mut enc = zstd::Encoder::new(f, level).unwrap();
    // ffmpeg.exe a ffprobe.exe sdílí většinu kódu -> velké okno + LDM je zdeduplikuje
    enc.long_distance_matching(true).ok();
    enc.window_log(27).ok();
    let mut b = tar::Builder::new(enc);
    b.follow_symlinks(false); // odkazy (knihovny qpdf) zůstanou odkazy, ne kopie
    b.append_dir_all(".", stage).unwrap();
    let enc = b.into_inner().unwrap();
    enc.finish().unwrap();
    fs::rename(&tmp, dest).unwrap();
}

/// Ikona a informace o produktu přímo v .exe (Průzkumník, Start menu, hledání Windows).
fn embed_windows_resources() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" || !cfg!(windows) {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("ProductName", "TomasekConvert");
    res.set("FileDescription", "TomasekConvert");
    res.set("InternalName", "TomasekConvert");
    res.set("OriginalFilename", "TomasekConvert.exe");
    if let Err(e) = res.compile() {
        // build nesmí kvůli ikoně spadnout (chybí Windows SDK / rc.exe)
        println!("cargo:warning=Ikonu se nepodařilo vložit do exe: {e}");
    }
}

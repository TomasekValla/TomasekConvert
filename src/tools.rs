//! Vložené nástroje (ffmpeg, ImageMagick, pandoc, typst, pdfium).
//!
//! `build.rs` je při buildu stáhne a zabalí do .exe. Při prvním spuštění se
//! rozbalí do %LOCALAPPDATA%\TomasekConvert\tools. Žádný síťový přístup za běhu.
use crate::util::{app_dir, find_in_path};
use std::{
    fs,
    path::{Path, PathBuf},
};

static FFMPEG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ffmpeg.tar.zst"));
static MAGICK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/magick.tar.zst"));
static PANDOC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pandoc.tar.zst"));
static TYPST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/typst.tar.zst"));
static PDFIUM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pdfium.tar.zst"));
static QPDF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/qpdf.tar.zst"));

#[derive(Clone, Debug, Default)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub qpdf: Option<PathBuf>,
    pub magick: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    pub typst: Option<PathBuf>,
    pub pdfium: Option<PathBuf>,
    /// Kolik nástrojů běží z vložených balíčků (ne z PATH).
    pub bundled: usize,
}

impl Tools {
    pub fn prepare(progress: impl Fn(&str)) -> Tools {
        let root = app_dir().join("tools");
        let _ = fs::create_dir_all(&root);
        let mut t = Tools::default();

        let mut get = |name: &str, blob: &'static [u8], exe: &str, path_name: Option<&str>| -> Option<PathBuf> {
            if !blob.is_empty() {
                progress(&trf!("Připravuji {name}…", "Preparing {name}…"));
                if let Some(dir) = extract(name, blob, &root) {
                    let p = dir.join(exe);
                    if p.exists() {
                        t.bundled += 1;
                        return Some(p);
                    }
                }
            }
            path_name.and_then(find_in_path)
        };

        let exe = |n: &str| format!("{n}{}", std::env::consts::EXE_SUFFIX);
        let pdfium_lib = if cfg!(windows) { "pdfium.dll" } else if cfg!(target_os = "macos") { "libpdfium.dylib" } else { "libpdfium.so" };
        let ffmpeg = get("ffmpeg", FFMPEG, &exe("ffmpeg"), Some("ffmpeg"));
        // ImageMagick 7 = "magick"; na Linuxu často jen starší "convert" (na Windows ne – to je jiný program)
        let magick = get("magick", MAGICK, &exe("magick"), Some("magick")).or_else(|| if cfg!(windows) { None } else { find_in_path("convert") });
        let pandoc = get("pandoc", PANDOC, &exe("pandoc"), Some("pandoc"));
        let typst = get("typst", TYPST, &exe("typst"), Some("typst"));
        let pdfium = get("pdfium", PDFIUM, pdfium_lib, None).or_else(|| std::env::var_os("TC_PDFIUM").map(PathBuf::from).filter(|p| p.exists()));
        // Windows: qpdf.exe + DLL vedle sebe; Linux/macOS: oficiální rozložení bin/qpdf + lib/
        let qpdf_rel = if cfg!(windows) { exe("qpdf") } else { "bin/qpdf".to_string() };
        let qpdf = get("qpdf", QPDF, &qpdf_rel, Some("qpdf"));
        // ffprobe leží ve stejné složce jako ffmpeg (stejný balíček)
        t.ffprobe = ffmpeg
            .as_ref()
            .and_then(|f| f.parent().map(|d| d.to_path_buf()))
            .map(|d| d.join(exe("ffprobe")))
            .filter(|p| p.exists())
            .or_else(|| find_in_path("ffprobe"));
        t.qpdf = qpdf;
        t.ffmpeg = ffmpeg;
        t.magick = magick;
        t.pandoc = pandoc;
        t.typst = typst;
        t.pdfium = pdfium;
        t
    }
}

fn blob_key(blob: &[u8]) -> String {
    // rychlý otisk: délka + vzorky ze začátku a konce
    let mut h: u64 = 0xcbf29ce484222325 ^ blob.len() as u64;
    for b in blob.iter().take(8192).chain(blob.iter().rev().take(8192)) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn extract(name: &str, blob: &[u8], root: &Path) -> Option<PathBuf> {
    let dir = root.join(format!("{name}-{}", blob_key(blob)));
    if dir.join(".ok").exists() {
        return Some(dir);
    }
    // smaž staré verze tohoto nástroje
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&format!("{name}-")) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
    fs::create_dir_all(&dir).ok()?;
    let dec = zstd::Decoder::new(blob).ok()?;
    let mut ar = tar::Archive::new(dec);
    ar.set_preserve_permissions(true);
    ar.unpack(&dir).ok()?;
    fs::write(dir.join(".ok"), b"1").ok()?;
    Some(dir)
}

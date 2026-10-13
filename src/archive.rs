//! Archivy: zip, 7z, tar.*, gz/bz2/xz/zst, rar (čtení). Vše čistě v Rustu (bez 7-Zip/WinRAR).
use crate::{formats::Kind, job::*, opts::Opts, stats::Engine, util::*};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Měří, kolik dat už prošlo, a posouvá průběh v rozsahu [start, start+span].
#[derive(Clone)]
struct Meter {
    shared: Arc<Shared>,
    done: Arc<AtomicU64>,
    total: u64,
    start: f32,
    span: f32,
}

impl Meter {
    fn new(shared: &Arc<Shared>, total: u64, start: f32, span: f32) -> Meter {
        Meter { shared: shared.clone(), done: Arc::new(AtomicU64::new(0)), total: total.max(1), start, span }
    }
    fn add(&self, n: u64) {
        let d = self.done.fetch_add(n, Ordering::Relaxed) + n;
        self.shared.set_frac(self.start + self.span * (d as f32 / self.total as f32).min(1.0));
    }
}

struct MeterRead<R> {
    inner: R,
    m: Meter,
}
impl<R: Read> Read for MeterRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.m.shared.cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED));
        }
        let n = self.inner.read(buf)?;
        self.m.add(n as u64);
        Ok(n)
    }
}

struct MeterWrite<W> {
    inner: W,
    m: Meter,
}
impl<W: Write> Write for MeterWrite<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.m.shared.cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED));
        }
        let n = self.inner.write(buf)?;
        self.m.add(n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn err<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    if s.contains(CANCELLED) {
        CANCELLED.into()
    } else {
        s
    }
}

/// Bezpečná relativní cesta (žádné `..`, žádné absolutní cesty).
fn safe_rel(name: &str) -> Option<PathBuf> {
    let p = Path::new(name.trim_start_matches(['/', '\\']));
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

struct Entry {
    abs: PathBuf,
    rel: String,
    is_dir: bool,
    size: u64,
}

fn collect(input: &Path, include_root: bool) -> Vec<Entry> {
    let mut v = Vec::new();
    if input.is_file() {
        let name = input.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        v.push(Entry { abs: input.to_path_buf(), rel: name, is_dir: false, size: path_size(input) });
        return v;
    }
    let root_name = input.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "archiv".into());
    for e in walkdir::WalkDir::new(input).sort_by_file_name().into_iter().filter_map(Result::ok) {
        let rel_path = e.path().strip_prefix(input).unwrap_or(e.path());
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
        let rel = if include_root {
            if rel_str.is_empty() {
                root_name.clone()
            } else {
                format!("{root_name}/{rel_str}")
            }
        } else {
            rel_str
        };
        if rel.is_empty() {
            continue;
        }
        let is_dir = e.file_type().is_dir();
        let size = if is_dir { 0 } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
        v.push(Entry { abs: e.path().to_path_buf(), rel, is_dir, size });
    }
    v
}

fn tar_into<W: Write>(w: W, entries: &[Entry], m: &Meter) -> Res<W> {
    let mut b = tar::Builder::new(w);
    for e in entries {
        if e.is_dir {
            b.append_dir(&e.rel, &e.abs).map_err(err)?;
        } else {
            let f = File::open(&e.abs).map_err(err)?;
            let mut h = tar::Header::new_gnu();
            h.set_metadata(&f.metadata().map_err(err)?);
            h.set_cksum();
            b.append_data(&mut h, &e.rel, MeterRead { inner: BufReader::new(f), m: m.clone() }).map_err(err)?;
        }
    }
    b.into_inner().map_err(err)
}

pub fn pack(ctx: &JobCtx, input: &Path, ext: &str, out: &Path, o: &Opts, include_root: bool, start: f32, span: f32) -> Res<()> {
    let entries = collect(input, include_root);
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let m = Meter::new(&ctx.shared, total, start, span);
    let lvl = o.arc_level.clamp(0, 9);
    let file = File::create(out).map_err(err)?;
    let file = BufWriter::new(file);
    match ext {
        "zip" => {
            let mut zw = zip::ZipWriter::new(file);
            let base = zip::write::SimpleFileOptions::default()
                .compression_method(if lvl == 0 { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated })
                .compression_level(Some(lvl as i64))
                .large_file(true);
            for e in &entries {
                if e.is_dir {
                    zw.add_directory(&e.rel, base).map_err(err)?;
                    continue;
                }
                if o.password.is_empty() {
                    zw.start_file(&e.rel, base).map_err(err)?;
                } else {
                    zw.start_file(&e.rel, base.with_aes_encryption(zip::AesMode::Aes256, &o.password)).map_err(err)?;
                }
                let f = File::open(&e.abs).map_err(err)?;
                io::copy(&mut MeterRead { inner: BufReader::new(f), m: m.clone() }, &mut zw).map_err(err)?;
            }
            zw.finish().map_err(err)?;
        }
        "7z" => {
            let mut w = sevenz_rust2::ArchiveWriter::new(file).map_err(err)?;
            let mut methods: Vec<sevenz_rust2::EncoderConfiguration> = Vec::new();
            if !o.password.is_empty() {
                methods.push(sevenz_rust2::encoder_options::AesEncoderOptions::new(sevenz_rust2::Password::new(&o.password)).into());
            }
            methods.push(sevenz_rust2::encoder_options::Lzma2Options::from_level(lvl.max(1)).into());
            w.set_content_methods(methods);
            for e in &entries {
                if e.is_dir {
                    w.push_archive_entry::<&[u8]>(sevenz_rust2::ArchiveEntry::new_directory(&e.rel), None).map_err(err)?;
                } else {
                    let f = File::open(&e.abs).map_err(err)?;
                    w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(&e.rel), Some(MeterRead { inner: BufReader::new(f), m: m.clone() }))
                        .map_err(err)?;
                }
            }
            w.finish().map_err(err)?;
        }
        "tar" => {
            let mut w = tar_into(file, &entries, &m)?;
            w.flush().map_err(err)?;
        }
        "tar.gz" => {
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::new(lvl.max(1)));
            tar_into(enc, &entries, &m)?.finish().map_err(err)?;
        }
        "tar.bz2" => {
            let enc = bzip2::write::BzEncoder::new(file, bzip2::Compression::new(lvl.clamp(1, 9)));
            tar_into(enc, &entries, &m)?.finish().map_err(err)?;
        }
        "tar.xz" => {
            let enc = liblzma::write::XzEncoder::new(file, lvl.clamp(1, 9));
            tar_into(enc, &entries, &m)?.finish().map_err(err)?;
        }
        "tar.zst" => {
            let enc = zstd::Encoder::new(file, (lvl.max(1) * 2) as i32).map_err(err)?;
            tar_into(enc, &entries, &m)?.finish().map_err(err)?;
        }
        other => return Err(trf!("Nepodporovaný archiv: {other}", "Unsupported archive: {other}")),
    }
    ctx.shared.set_frac((start + span).min(1.0));
    Ok(())
}

fn write_entry(dest: &Path, rel: &Path, reader: &mut dyn Read, m: &Meter) -> Res<()> {
    let target = dest.join(rel);
    if let Some(p) = target.parent() {
        fs::create_dir_all(p).map_err(err)?;
    }
    let f = File::create(&target).map_err(err)?;
    let mut w = MeterWrite { inner: BufWriter::new(f), m: m.clone() };
    io::copy(reader, &mut w).map_err(err)?;
    w.flush().map_err(err)?;
    Ok(())
}

pub fn extract(ctx: &JobCtx, archive: &Path, ext: &str, dest: &Path, o: &Opts, start: f32, span: f32) -> Res<()> {
    fs::create_dir_all(dest).map_err(err)?;
    let file_size = path_size(archive);
    match ext {
        "zip" | "cbz" | "jar" | "apk" | "war" => {
            let mut z = zip::ZipArchive::new(BufReader::new(File::open(archive).map_err(err)?)).map_err(err)?;
            let mut total = 0u64;
            for i in 0..z.len() {
                total += z.by_index_raw(i).map(|f| f.size()).unwrap_or(0);
            }
            let m = Meter::new(&ctx.shared, total, start, span);
            for i in 0..z.len() {
                let pw = o.password.as_bytes();
                let mut f = if pw.is_empty() {
                    z.by_index(i).map_err(err)?
                } else {
                    z.by_index_decrypt(i, pw).map_err(err)?
                };
                let Some(rel) = f.enclosed_name().and_then(|p| safe_rel(&p.to_string_lossy())) else { continue };
                if f.is_dir() {
                    fs::create_dir_all(dest.join(rel)).map_err(err)?;
                } else {
                    write_entry(dest, &rel, &mut f, &m)?;
                }
            }
        }
        "7z" => {
            let pw = if o.password.is_empty() { sevenz_rust2::Password::empty() } else { sevenz_rust2::Password::new(&o.password) };
            let mut r = sevenz_rust2::ArchiveReader::open(archive, pw).map_err(err)?;
            let total: u64 = r.archive().files.iter().map(|f| f.size()).sum();
            let m = Meter::new(&ctx.shared, total, start, span);
            let shared = ctx.shared.clone();
            r.for_each_entries(|entry, reader| {
                if shared.cancelled() {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED).into());
                }
                if let Some(rel) = safe_rel(entry.name()) {
                    if entry.is_directory() {
                        fs::create_dir_all(dest.join(&rel))?;
                    } else {
                        write_entry(dest, &rel, reader, &m).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                    }
                } else {
                    io::copy(reader, &mut io::sink())?;
                }
                Ok(true)
            })
            .map_err(err)?;
        }
        "rar" | "cbr" => {
            let m = Meter::new(&ctx.shared, 1, start, span);
            let archive_s = archive.to_path_buf();
            let open = if o.password.is_empty() {
                unrar::Archive::new(&archive_s).open_for_processing()
            } else {
                unrar::Archive::with_password(&archive_s, &o.password).open_for_processing()
            };
            let mut cur = open.map_err(err)?;
            let total: u64 = unrar::Archive::new(&archive_s)
                .open_for_listing()
                .map(|l| l.filter_map(Result::ok).map(|e| e.unpacked_size).sum())
                .unwrap_or(file_size);
            let m = Meter { total: total.max(1), ..m };
            while let Some(h) = cur.read_header().map_err(err)? {
                if ctx.shared.cancelled() {
                    return Err(CANCELLED.into());
                }
                let e = h.entry();
                let size = e.unpacked_size;
                let is_file = e.is_file();
                let safe = safe_rel(&e.filename.to_string_lossy()).is_some();
                cur = if is_file && safe {
                    let c = h.extract_with_base(dest).map_err(err)?;
                    m.add(size);
                    c
                } else {
                    h.skip().map_err(err)?
                };
            }
        }
        "tar" | "tar.gz" | "tgz" | "tar.bz2" | "tbz2" | "tbz" | "tar.xz" | "txz" | "tar.zst" | "tzst" => {
            let m = Meter::new(&ctx.shared, file_size, start, span);
            let src = MeterRead { inner: BufReader::new(File::open(archive).map_err(err)?), m };
            let r: Box<dyn Read> = match ext {
                "tar" => Box::new(src),
                "tar.gz" | "tgz" => Box::new(flate2::read::MultiGzDecoder::new(src)),
                "tar.bz2" | "tbz2" | "tbz" => Box::new(bzip2::read::MultiBzDecoder::new(src)),
                "tar.xz" | "txz" => Box::new(liblzma::read::XzDecoder::new(src)),
                _ => Box::new(zstd::Decoder::new(src).map_err(err)?),
            };
            tar::Archive::new(r).unpack(dest).map_err(err)?;
        }
        "gz" | "bz2" | "xz" | "zst" => {
            let m = Meter::new(&ctx.shared, file_size, start, span);
            let src = MeterRead { inner: BufReader::new(File::open(archive).map_err(err)?), m: m.clone() };
            let mut r: Box<dyn Read> = match ext {
                "gz" => Box::new(flate2::read::MultiGzDecoder::new(src)),
                "bz2" => Box::new(bzip2::read::MultiBzDecoder::new(src)),
                "xz" => Box::new(liblzma::read::XzDecoder::new(src)),
                _ => Box::new(zstd::Decoder::new(src).map_err(err)?),
            };
            let name = stem_of(archive);
            let mut f = BufWriter::new(File::create(dest.join(if name.is_empty() { "vystup".into() } else { name })).map_err(err)?);
            io::copy(&mut r, &mut f).map_err(err)?;
            f.flush().map_err(err)?;
        }
        other => return Err(trf!("Nepodporovaný archiv: {other}", "Unsupported archive: {other}")),
    }
    ctx.shared.set_frac((start + span).min(1.0));
    Ok(())
}

pub fn convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let o = &job.opts;
    let target = job.fmt.ext.as_str();
    let bytes_in = path_size(&job.input);
    let stem = stem_of(&job.input);

    // ── rozbalení do složky ──
    if job.fmt.extra == "extract" {
        let dest = unique_path(job.out_dir.join(&stem));
        ctx.shared.set_note(tr!("Rozbaluji…", "Extracting…"));
        if let Err(e) = extract(ctx, &job.input, &job.in_ext, &dest, o, 0.0, 1.0) {
            let _ = fs::remove_dir_all(&dest);
            return Err(e);
        }
        return Ok(Outcome::simple(vec![dest.clone()], bytes_in, path_size(&dest), tr!("rozbaleno do složky", "extracted to a folder")));
    }

    let out = unique_path(job.out_dir.join(format!("{stem}.{target}")));
    let part = part_path(&out);
    let res = if job.kind == Kind::Archive {
        // archiv -> jiný archiv: rozbalit do dočasné složky a zabalit znovu
        let tmp = app_dir().join("tmp").join(format!("repack-{}", unix_now()));
        ctx.shared.set_note(tr!("Rozbaluji…", "Extracting…"));
        let r = extract(ctx, &job.input, &job.in_ext, &tmp, o, 0.0, 0.5).and_then(|_| {
            ctx.shared.set_note(tr!("Balím…", "Packing…"));
            pack(ctx, &tmp, target, &part, o, false, 0.5, 0.5)
        });
        let _ = fs::remove_dir_all(&tmp);
        r
    } else {
        ctx.shared.set_note(tr!("Balím…", "Packing…"));
        pack(ctx, &job.input, target, &part, o, true, 0.0, 1.0)
    };
    if let Err(e) = res {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    finish_part(&part, &out)?;
    Ok(Outcome::simple(vec![out.clone()], bytes_in, path_size(&out), trf!("{} · úroveň {}", "{} · level {}", target.to_uppercase(), o.arc_level)))
}

#[allow(dead_code)]
fn _e(_: Engine) {}

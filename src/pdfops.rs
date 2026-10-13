//! PDF: qpdf (sloučit, rozdělit, stránky, otočit, optimalizace, hesla) + pdfium (PDF -> obrázky/text).
use crate::{formats::*, image, job::*, opts::Opts, stats::Engine, tools::Tools, util::*};
use pdfium_render::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// pdfium-render dovolí navázat knihovnu jen jednou za proces -> držíme jedinou sdílenou instanci.
fn pdfium(ctx: &JobCtx) -> Res<&'static Pdfium> {
    static PDFIUM: std::sync::OnceLock<Result<Pdfium, String>> = std::sync::OnceLock::new();
    let r = PDFIUM.get_or_init(|| {
        let lib = ctx.tools.pdfium.as_ref().ok_or_else(|| tr!("pdfium (renderování PDF) není k dispozici", "pdfium (PDF rendering) is not available").to_string())?;
        let b = Pdfium::bind_to_library(lib).map_err(|e| format!("pdfium: {e}"))?;
        Ok(Pdfium::new(b))
    });
    r.as_ref().map_err(|e| e.clone())
}

pub fn pdf_to_images(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let o = &job.opts;
    let pdfium = pdfium(ctx)?;
    let pw = if o.password.is_empty() { None } else { Some(o.password.as_str()) };
    let doc = pdfium.load_pdf_from_file(&job.input, pw).map_err(|e| trf!("PDF nelze otevřít: {e}", "Cannot open the PDF: {e}"))?;
    let n = doc.pages().len() as usize;
    let stem = stem_of(&job.input);
    let dir = unique_path(job.out_dir.join(&stem));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ext = job.fmt.ext.as_str();
    let coder = if job.fmt.extra.is_empty() { ext } else { job.fmt.extra.as_str() };
    let width = n.to_string().len().max(2);
    let scale = o.dpi.max(36) as f32 / 72.0;
    let tmp_dir = app_dir().join("tmp");
    let _ = fs::create_dir_all(&tmp_dir);
    let mut outputs = Vec::new();
    ctx.shared.set_frac(0.0);
    for (i, page) in doc.pages().iter().enumerate() {
        if ctx.shared.cancelled() {
            let _ = fs::remove_dir_all(&dir);
            return Err(CANCELLED.into());
        }
        ctx.shared.set_note(trf!("Stránka {} z {n}", "Page {} of {n}", i + 1));
        let bmp = page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale)).map_err(|e| format!("render: {e}"))?;
        let img = bmp.as_image().map_err(|e| format!("render: {e}"))?;
        let target = dir.join(format!("{stem}-{:0width$}.{ext}", i + 1));
        if ext == "png" && !o.strip_meta {
            img.save_with_format(&target, ::image::ImageFormat::Png).map_err(|e| e.to_string())?;
        } else {
            let tmp = tmp_dir.join(format!("page-{}-{}.png", unix_now(), i));
            img.save_with_format(&tmp, ::image::ImageFormat::Png).map_err(|e| e.to_string())?;
            let r = image::convert_file(ctx, o, &tmp, &target, coder, ext);
            let _ = fs::remove_file(&tmp);
            r?;
        }
        outputs.push(target);
        ctx.shared.set_frac((i + 1) as f32 / n as f32);
    }
    let bytes_out = path_size(&dir);
    Ok(Outcome { outputs: vec![dir], bytes_in: path_size(&job.input), bytes_out, engine: Engine::Cpu, detail: trf!("{n} stránek · {} DPI · pdfium", "{n} pages · {} DPI · pdfium", o.dpi), media_secs: 0.0 })
}

pub fn pdf_to_text(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let o = &job.opts;
    let pdfium = pdfium(ctx)?;
    let pw = if o.password.is_empty() { None } else { Some(o.password.as_str()) };
    let doc = pdfium.load_pdf_from_file(&job.input, pw).map_err(|e| trf!("PDF nelze otevřít: {e}", "Cannot open the PDF: {e}"))?;
    let n = doc.pages().len() as usize;
    let mut text = String::new();
    for (i, page) in doc.pages().iter().enumerate() {
        if ctx.shared.cancelled() {
            return Err(CANCELLED.into());
        }
        text.push_str(&strip_markers(&page_text(&page)));
        text.push_str("\n\n");
        ctx.shared.set_frac((i + 1) as f32 / n.max(1) as f32);
    }
    let out = unique_path(job.out_dir.join(format!("{}.txt", stem_of(&job.input))));
    fs::write(&out, text).map_err(|e| e.to_string())?;
    Ok(Outcome::simple(vec![out.clone()], path_size(&job.input), path_size(&out), trf!("{n} stránek · text", "{n} pages · text")))
}

/// Znaky povolené v XML 1.0 (EPUB, DOCX, ODT i HTML výstupy jsou XML).
fn xml_ok(c: char) -> bool {
    (!c.is_control() || matches!(c, '\n' | '\r' | '\t')) && !matches!(c as u32, 0xFDD0..=0xFDEF | 0xFFFE | 0xFFFF | 0x1FFFE | 0x1FFFF | 0x2FFFE | 0x2FFFF)
}

fn clean_text(t: &str) -> String {
    // pdfium značí dělení slov znakem U+0002; ostatní řídicí znaky zahodíme
    t.replace('\u{2}', "\u{AD}")
        .chars()
        .filter(|c| (!c.is_control() || matches!(c, '\n' | '\r' | '\t')) && !matches!(*c as u32, 0xFDD0..=0xFDEF | 0xFFFE | 0xFFFF | 0x1FFFE | 0x1FFFF | 0x2FFFE | 0x2FFFF))
        .collect()
}

struct Line {
    top: f32,
    bottom: f32,
    right: f32,
    text: String,
    size: f32,
}

/// Text jedné stránky s rozpoznanými odstavci (prázdný řádek) a nadpisy (značka \u{1}H<úroveň>\u{1}).
///
/// Čte se po jednotlivých znacích v pořadí, v jakém je dává pdfium (včetně jeho zalomení řádků).
/// Dřív se používaly `text.segments()` – pdfium u některých PDF (jiné fonty, wkhtmltopdf, Qt…)
/// rozseká řádek na vodorovné proužky a výsledkem byl nečitelný text typu "Th ld fe h b h".
fn page_text(page: &PdfPage) -> String {
    let Ok(text) = page.text() else { return String::new() };
    let new_line = || Line { top: f32::MIN, bottom: f32::MAX, right: f32::MIN, text: String::new(), size: 0.0 };
    let mut lines: Vec<Line> = Vec::new();
    let mut cur: Option<Line> = None;
    fn flush(cur: &mut Option<Line>, lines: &mut Vec<Line>) {
        if let Some(mut l) = cur.take() {
            l.text = clean_text(&l.text).trim().to_string();
            if !l.text.is_empty() && l.top > f32::MIN && l.bottom < f32::MAX {
                lines.push(l);
            }
        }
    }
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        match c {
            '\r' => continue,
            '\n' => {
                flush(&mut cur, &mut lines);
                continue;
            }
            _ => {}
        }
        let l = cur.get_or_insert_with(new_line);
        l.text.push(c);
        if !c.is_whitespace() && c != '\u{2}' {
            if let Ok(b) = ch.loose_bounds() {
                l.top = l.top.max(b.top().value);
                l.bottom = l.bottom.min(b.bottom().value);
                l.right = l.right.max(b.right().value);
            }
            l.size = l.size.max(ch.scaled_font_size().value);
        }
    }
    flush(&mut cur, &mut lines);
    if lines.is_empty() {
        return clean_text(&text.all());
    }
    // typická velikost písma = nejčastější (vážená délkou)
    let mut by_size: Vec<(i32, usize)> = Vec::new();
    for l in &lines {
        let k = (l.size * 2.0).round() as i32;
        let n = l.text.chars().count();
        match by_size.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += n,
            None => by_size.push((k, n)),
        }
    }
    let body = by_size.iter().max_by_key(|e| e.1).map(|e| e.0 as f32 / 2.0).unwrap_or(0.0);
    // nadpisy: řádky výrazně větší než běžný text; úroveň podle pořadí velikostí v dokumentu (největší = H1)
    let is_heading = |l: &Line| body > 0.0 && l.size / body > 1.12 && l.text.chars().count() < 140;
    let mut head_sizes: Vec<i32> = lines.iter().filter(|l| is_heading(l)).map(|l| (l.size * 2.0).round() as i32).collect();
    head_sizes.sort_unstable_by(|a, b| b.cmp(a));
    head_sizes.dedup();
    let mut out = String::new();
    let mut prev_bottom: Option<f32> = None;
    for l in &lines {
        let h = (l.top - l.bottom).max(1.0);
        if let Some(pb) = prev_bottom {
            if pb - l.top > h * 0.55 {
                out.push('\n');
            }
        }
        if is_heading(l) {
            let rank = head_sizes.iter().position(|k| *k == (l.size * 2.0).round() as i32).unwrap_or(0);
            let level = (rank + 1).min(3);
            out.push('\u{1}');
            out.push_str(&format!("H{level}"));
            out.push('\u{1}');
        }
        out.push_str(&l.text);
        out.push('\n');
        prev_bottom = Some(l.bottom);
    }
    out
}

/// Odstraní značky nadpisů (pro prostý text).
fn strip_markers(t: &str) -> String {
    t.lines()
        .map(|l| match l.strip_prefix('\u{1}') {
            Some(rest) => rest.splitn(2, '\u{1}').nth(1).unwrap_or("").to_string(),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Text z PDF po stránkách (pdfium).
fn extract_pages(ctx: &JobCtx, input: &Path, pw: &str, progress: f32) -> Res<Vec<String>> {
    let pdfium = pdfium(ctx)?;
    let pw = if pw.is_empty() { None } else { Some(pw) };
    let doc = pdfium.load_pdf_from_file(input, pw).map_err(|e| trf!("PDF nelze otevřít: {e}", "Cannot open the PDF: {e}"))?;
    let n = doc.pages().len() as usize;
    let mut out = Vec::with_capacity(n);
    for (i, page) in doc.pages().iter().enumerate() {
        if ctx.shared.cancelled() {
            return Err(CANCELLED.into());
        }
        ctx.shared.set_note(trf!("Čtu text: stránka {} z {n}", "Reading text: page {} of {n}", i + 1));
        out.push(page_text(&page));
        ctx.shared.set_frac(progress * (i + 1) as f32 / n.max(1) as f32);
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Bullet(String),
}

/// Poskládá "řádky z PDF" zpátky do odstavců a odrážek (heuristika – PDF žádnou strukturu neobsahuje).
pub fn reflow(pages: &[String]) -> Vec<Block> {
    let mut lines: Vec<String> = Vec::new();
    for p in pages {
        for l in p.replace('\r', "").lines() {
            lines.push(l.trim().to_string());
        }
        lines.push("\u{0c}".to_string()); // konec stránky
    }
    let mut lens: Vec<usize> = lines.iter().map(|l| l.chars().count()).filter(|n| *n > 0).collect();
    lens.sort_unstable();
    let maxlen = lens.get(lens.len() * 9 / 10).copied().unwrap_or(80).max(20);
    let is_bullet = |l: &str| -> Option<String> {
        for b in ["• ", "◦ ", "▪ ", "● ", "– ", "- ", "* ", "· "] {
            if let Some(r) = l.strip_prefix(b) {
                return Some(r.trim().to_string());
            }
        }
        None
    };
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<(bool, String, usize)> = None; // (je odrážka, text, délka posledního řádku)
    let flush = |cur: &mut Option<(bool, String, usize)>, blocks: &mut Vec<Block>| {
        if let Some((b, t, _)) = cur.take() {
            if !t.trim().is_empty() {
                blocks.push(if b { Block::Bullet(t) } else { Block::Para(t) });
            }
        }
    };
    for l in lines {
        if l == "\u{0c}" {
            // odstavec pokračuje přes stránku, pokud věta nekončí
            if let Some((bullet, t, _)) = &cur {
                if *bullet || t.ends_with(['.', '!', '?', ':', ';']) {
                    flush(&mut cur, &mut blocks);
                }
            }
            continue;
        }
        if l.is_empty() {
            flush(&mut cur, &mut blocks);
            continue;
        }
        if let Some(rest) = l.strip_prefix('\u{1}') {
            // značka nadpisu: \u{1}H2\u{1}text
            flush(&mut cur, &mut blocks);
            let level: u8 = rest.chars().nth(1).and_then(|c| c.to_digit(10)).unwrap_or(2) as u8;
            let text = rest.splitn(2, '\u{1}').nth(1).unwrap_or("").trim().to_string();
            if !text.is_empty() {
                blocks.push(Block::Heading(level, text));
            }
            continue;
        }
        if let Some(b) = is_bullet(&l) {
            flush(&mut cur, &mut blocks);
            let n = l.chars().count();
            cur = Some((true, b, n));
            continue;
        }
        match cur.take() {
            None => cur = Some((false, l.clone(), l.chars().count())),
            Some((bullet, mut text, last_len)) => {
                // měkký spojovník z pdfium: slovo pokračuje na dalším řádku
                if text.ends_with('\u{AD}') {
                    text.pop();
                    if l.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                        text.push_str(&l);
                        cur = Some((bullet, text, l.chars().count()));
                        continue;
                    }
                    text.push('-');
                }
                let ends_sentence = text.ends_with(['.', '!', '?', ':', ';']);
                let starts_upper = l.chars().next().map(|c| c.is_uppercase() || c.is_ascii_digit()).unwrap_or(false);
                let short_prev = last_len * 100 < maxlen * 55;
                let hyphen_join = text.ends_with('-') && l.chars().next().map(|c| c.is_lowercase()).unwrap_or(false);
                if !bullet && !hyphen_join && (short_prev || (ends_sentence && starts_upper && last_len * 100 < maxlen * 85)) {
                    blocks.push(Block::Para(text));
                    cur = Some((false, l.clone(), l.chars().count()));
                } else {
                    if text.ends_with('-') && l.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                        text.pop(); // dělení slov na konci řádku
                    } else {
                        text.push(' ');
                    }
                    text.push_str(&l);
                    cur = Some((bullet, text, l.chars().count()));
                }
            }
        }
    }
    flush(&mut cur, &mut blocks);
    blocks
}

/// V pandoc markdownu lze zpětným lomítkem escapovat každou ASCII interpunkci -> text zůstane doslovný.
pub fn md_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars().filter(|c| *c != '\u{AD}' && xml_ok(*c)) {
        if c.is_ascii_punctuation() {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

pub fn blocks_to_markdown(blocks: &[Block]) -> String {
    let mut o = String::new();
    let mut prev_bullet = false;
    for b in blocks {
        match b {
            Block::Heading(lv, t) => {
                if !o.is_empty() {
                    o.push('\n');
                }
                o.push_str(&"#".repeat((*lv).clamp(1, 6) as usize));
                o.push(' ');
                o.push_str(&md_escape(t));
                o.push_str("\n\n");
                prev_bullet = false;
            }
            Block::Para(t) => {
                if !o.is_empty() {
                    o.push('\n');
                }
                o.push_str(&md_escape(t));
                o.push_str("\n\n");
                prev_bullet = false;
            }
            Block::Bullet(t) => {
                if !prev_bullet && !o.is_empty() {
                    o.push('\n');
                }
                o.push_str("- ");
                o.push_str(&md_escape(t));
                o.push('\n');
                prev_bullet = true;
            }
        }
    }
    o
}

pub fn pdf_to_doc(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let pandoc = ctx.tools.pandoc.as_ref().ok_or(tr!("Pandoc není k dispozici", "Pandoc is not available"))?;
    let o = &job.opts;
    let pages = extract_pages(ctx, &job.input, &o.password, 0.8)?;
    if pages.iter().all(|p| p.trim().is_empty()) {
        return Err(tr!("PDF neobsahuje textovou vrstvu (je to nejspíš sken). OCR zatím neumím – převeď stránky na obrázky.", "The PDF has no text layer (probably a scan). OCR is not supported yet – convert the pages to images instead.").into());
    }
    let md = blocks_to_markdown(&reflow(&pages));
    let tmp_dir = app_dir().join("tmp");
    let _ = fs::create_dir_all(&tmp_dir);
    let tmp = tmp_dir.join(format!("pdf2doc-{}.md", unix_now()));
    fs::write(&tmp, md).map_err(|e| e.to_string())?;
    let ext = job.fmt.ext.as_str();
    let out = unique_path(job.out_dir.join(format!("{}.{}", stem_of(&job.input), ext)));
    let part = part_path(&out);
    ctx.shared.set_note(tr!("Pandoc skládá dokument…", "Pandoc is building the document…"));
    let mut cmd = command(pandoc);
    cmd.args(["-f", "markdown-smart-auto_identifiers", "-t", &job.fmt.extra]).arg(&tmp).arg("-o").arg(&part);
    if matches!(job.fmt.extra.as_str(), "html5" | "latex" | "typst" | "tei") {
        cmd.arg("-s").args(["--metadata", &format!("pagetitle={}", stem_of(&job.input))]);
    }
    if matches!(job.fmt.extra.as_str(), "epub3" | "docx" | "odt" | "fb2") {
        cmd.args(["--metadata", &format!("title={}", stem_of(&job.input))]);
    }
    let r = run_cmd(ctx, cmd, |_, _| {});
    let _ = fs::remove_file(&tmp);
    if let Err(e) = r {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    finish_part(&part, &out)?;
    ctx.shared.set_frac(1.0);
    Ok(Outcome {
        outputs: vec![out.clone()],
        bytes_in: path_size(&job.input),
        bytes_out: path_size(&out),
        engine: Engine::Cpu,
        detail: trf!("{} stránek · text z PDF → {} (bez obrázků a rozvržení)", "{} pages · text from the PDF → {} (without images and layout)", pages.len(), job.fmt.extra),
        media_secs: 0.0,
    })
}

// ───────────────────────────── qpdf ─────────────────────────────

pub fn page_count(tools: &Tools, path: &Path, pw: &str) -> Option<u32> {
    let q = tools.qpdf.as_ref()?;
    let mut c = command(q);
    if !pw.is_empty() {
        c.arg(format!("--password={pw}"));
    }
    let out = c.arg("--show-npages").arg(path).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn qpdf(ctx: &JobCtx, args: Vec<String>) -> Res<()> {
    let q = ctx.tools.qpdf.as_ref().ok_or(tr!("qpdf není k dispozici", "qpdf is not available"))?;
    ctx.shared.set_indeterminate();
    let out = command(q).args(&args).output().map_err(|e| format!("qpdf: {e}"))?;
    // 0 = OK, 3 = hotovo s varováními
    match out.status.code() {
        Some(0) | Some(3) => Ok(()),
        _ => {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if msg.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { msg })
        }
    }
}

fn pwarg(pw: &str) -> Vec<String> {
    if pw.is_empty() {
        vec![]
    } else {
        vec![format!("--password={pw}")]
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Rozparsuje "1-3,7,9-z" (z = poslední) na množinu stránek.
pub fn parse_pages(spec: &str, n: u32) -> Option<Vec<bool>> {
    let mut sel = vec![false; n as usize + 1];
    for part in spec.split(',') {
        let p = part.trim().to_lowercase();
        if p.is_empty() {
            continue;
        }
        let num = |t: &str| -> Option<u32> {
            if t == "z" {
                Some(n)
            } else {
                t.parse().ok()
            }
        };
        let (a, b) = match p.split_once('-') {
            Some((x, y)) => (num(x.trim())?, num(y.trim())?),
            None => {
                let v = num(&p)?;
                (v, v)
            }
        };
        if a == 0 || b == 0 || a > n || b > n {
            return None;
        }
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        for i in lo..=hi {
            sel[i as usize] = true;
        }
    }
    Some(sel)
}

fn ranges_of(sel: &[bool]) -> String {
    let mut parts = Vec::new();
    let mut i = 1;
    while i < sel.len() {
        if sel[i] {
            let start = i;
            while i + 1 < sel.len() && sel[i + 1] {
                i += 1;
            }
            parts.push(if start == i { format!("{start}") } else { format!("{start}-{i}") });
        }
        i += 1;
    }
    parts.join(",")
}

pub fn merge(ctx: &JobCtx, inputs: &[PathBuf], out: &Path, pw: &str) -> Res<()> {
    let mut a = pwarg(pw);
    a.extend([s("--empty"), s("--pages")]);
    for i in inputs {
        a.push(i.to_string_lossy().to_string());
    }
    a.extend([s("--"), out.to_string_lossy().to_string()]);
    qpdf(ctx, a)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageMode {
    Extract,
    Delete,
    Odd,
    Even,
    Reverse,
}

pub enum PdfTask {
    Merge { inputs: Vec<PathBuf>, out_dir: PathBuf, name: String, pw: String },
    ToPdf { inputs: Vec<PathBuf>, merge: bool, out_dir: PathBuf, name: String, opts: Opts, si: std::sync::Arc<crate::hw::SysInfo> },
    SplitEvery { input: PathBuf, n: u32, out_dir: PathBuf, pw: String },
    SplitRanges { input: PathBuf, ranges: String, out_dir: PathBuf, pw: String },
    Pages { input: PathBuf, spec: String, mode: PageMode, out_dir: PathBuf, pw: String },
    Rotate { input: PathBuf, angle: i32, spec: String, out_dir: PathBuf, pw: String },
    Optimize { input: PathBuf, linearize: bool, out_dir: PathBuf, pw: String },
    Encrypt { input: PathBuf, user: String, owner: String, out_dir: PathBuf },
    Decrypt { input: PathBuf, out_dir: PathBuf, pw: String },
}

fn out_named(dir: &Path, input: &Path, suffix: &str) -> PathBuf {
    unique_path(dir.join(format!("{} {suffix}.pdf", stem_of(input))))
}

fn done(out: Vec<PathBuf>, bytes_in: u64, detail: impl Into<String>) -> Res<Outcome> {
    let bytes_out = out.iter().map(|p| path_size(p)).sum();
    Ok(Outcome::simple(out, bytes_in, bytes_out, detail))
}

pub fn run(ctx: &JobCtx, task: PdfTask) -> Res<Outcome> {
    match task {
        PdfTask::Merge { inputs, out_dir, name, pw } => {
            let out = unique_path(out_dir.join(format!("{name}.pdf")));
            let bytes_in = inputs.iter().map(|p| path_size(p)).sum();
            merge(ctx, &inputs, &out, &pw)?;
            ctx.shared.set_frac(1.0);
            done(vec![out], bytes_in, trf!("sloučeno {} PDF", "merged {} PDFs", inputs.len()))
        }
        PdfTask::ToPdf { inputs, merge: do_merge, out_dir, name, opts, si } => {
            let tmp = app_dir().join("tmp").join(format!("topdf-{}", unix_now()));
            fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
            let n = inputs.len();
            let mut parts: Vec<(PathBuf, PathBuf)> = Vec::new(); // (zdroj, pdf)
            let bytes_in = inputs.iter().map(|p| path_size(p)).sum();
            let r: Res<()> = (|| {
                for (i, inp) in inputs.iter().enumerate() {
                    if ctx.shared.cancelled() {
                        return Err(CANCELLED.into());
                    }
                    ctx.shared.set_note(trf!("Soubor {} z {n}: {}", "File {} of {n}: {}", i + 1, inp.file_name().unwrap_or_default().to_string_lossy()));
                    let ext = full_ext(inp);
                    let dst = tmp.join(format!("{i:04}.pdf"));
                    let kind = detect_kind(&ext, &si);
                    match kind {
                        Kind::Pdf => {
                            fs::copy(inp, &dst).map_err(|e| e.to_string())?;
                        }
                        Kind::Image => {
                            let mut args = vec![s("-units"), s("PixelsPerInch"), s("-density"), opts.dpi.to_string()];
                            let (pre, post) = image::magick_args(&opts, &ext, "pdf", "pdf");
                            args.extend(pre);
                            args.push(format!("{}[0]", inp.to_string_lossy()));
                            args.extend(post);
                            args.push(format!("pdf:{}", dst.to_string_lossy()));
                            image::run_magick(ctx, args)?;
                        }
                        Kind::Document => {
                            crate::doc::run_pandoc(ctx, inp, &ext, "typst", &dst, &opts, true, None)?;
                        }
                        _ => return Err(trf!("{}: tento typ souboru nelze převést do PDF", "{}: this file type cannot be converted to PDF", inp.display())),
                    }
                    parts.push((inp.clone(), dst));
                    ctx.shared.set_frac((i + 1) as f32 / n as f32 * if do_merge { 0.9 } else { 1.0 });
                }
                Ok(())
            })();
            if let Err(e) = r {
                let _ = fs::remove_dir_all(&tmp);
                return Err(e);
            }
            let result = if do_merge && parts.len() > 1 {
                let out = unique_path(out_dir.join(format!("{name}.pdf")));
                let list: Vec<PathBuf> = parts.iter().map(|p| p.1.clone()).collect();
                merge(ctx, &list, &out, "").map(|_| vec![out])
            } else {
                let mut outs = Vec::new();
                for (src, pdf) in &parts {
                    let out = unique_path(out_dir.join(format!("{}.pdf", stem_of(src))));
                    fs::copy(pdf, &out).map_err(|e| e.to_string())?;
                    outs.push(out);
                }
                Ok(outs)
            };
            let _ = fs::remove_dir_all(&tmp);
            ctx.shared.set_frac(1.0);
            done(result?, bytes_in, trf!("{} souborů → PDF", "{} files → PDF", n))
        }
        PdfTask::SplitEvery { input, n, out_dir, pw } => {
            let dir = unique_path(out_dir.join(trf!("{} (rozděleno)", "{} (split)", stem_of(&input))));
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut a = pwarg(&pw);
            a.extend([input.to_string_lossy().to_string(), format!("--split-pages={}", n.max(1)), dir.join(format!("{}.pdf", stem_of(&input))).to_string_lossy().to_string()]);
            qpdf(ctx, a)?;
            ctx.shared.set_frac(1.0);
            let files: Vec<PathBuf> = fs::read_dir(&dir).map(|r| r.filter_map(Result::ok).map(|e| e.path()).collect()).unwrap_or_default();
            Ok(Outcome::simple(vec![dir.clone()], path_size(&input), path_size(&dir), trf!("{} souborů po {} stranách", "{} files of {} pages each", files.len(), n)))
        }
        PdfTask::SplitRanges { input, ranges, out_dir, pw } => {
            let dir = unique_path(out_dir.join(trf!("{} (rozděleno)", "{} (split)", stem_of(&input))));
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let parts: Vec<&str> = ranges.split(';').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            for (i, r) in parts.iter().enumerate() {
                let mut a = pwarg(&pw);
                a.extend([input.to_string_lossy().to_string(), s("--pages"), s("."), r.to_string(), s("--"), dir.join(format!("{} - {}.pdf", stem_of(&input), r.replace(',', "_"))).to_string_lossy().to_string()]);
                qpdf(ctx, a)?;
                ctx.shared.set_frac((i + 1) as f32 / parts.len() as f32);
            }
            Ok(Outcome::simple(vec![dir.clone()], path_size(&input), path_size(&dir), trf!("{} částí", "{} parts", parts.len())))
        }
        PdfTask::Pages { input, spec, mode, out_dir, pw } => {
            let n = page_count(&ctx.tools, &input, &pw).ok_or(tr!("Nelze zjistit počet stránek (heslo?)", "Cannot read the page count (password?)"))?;
            let range = match mode {
                PageMode::Extract => {
                    parse_pages(&spec, n).ok_or(tr!("Neplatný výběr stránek (příklad: 1-3,7,9-z)", "Invalid page selection (example: 1-3,7,9-z)"))?;
                    spec.replace(' ', "")
                }
                PageMode::Delete => {
                    let sel = parse_pages(&spec, n).ok_or(tr!("Neplatný výběr stránek (příklad: 2,5-7)", "Invalid page selection (example: 2,5-7)"))?;
                    let keep: Vec<bool> = sel.iter().enumerate().map(|(i, v)| i > 0 && !v).collect();
                    let r = ranges_of(&keep);
                    if r.is_empty() {
                        return Err(tr!("Smazáním by nezbyla žádná stránka", "Deleting would leave no pages").into());
                    }
                    r
                }
                PageMode::Odd => "1-z:odd".into(),
                PageMode::Even => "1-z:even".into(),
                PageMode::Reverse => "z-1".into(),
            };
            let suffix = match mode {
                PageMode::Extract => tr!("(vybrané stránky)", "(selected pages)"),
                PageMode::Delete => tr!("(bez stránek)", "(pages removed)"),
                PageMode::Odd => tr!("(liché)", "(odd)"),
                PageMode::Even => tr!("(sudé)", "(even)"),
                PageMode::Reverse => tr!("(obráceně)", "(reversed)"),
            };
            let out = out_named(&out_dir, &input, suffix);
            let mut a = pwarg(&pw);
            a.extend([input.to_string_lossy().to_string(), s("--pages"), s("."), range.clone(), s("--"), out.to_string_lossy().to_string()]);
            qpdf(ctx, a)?;
            ctx.shared.set_frac(1.0);
            done(vec![out], path_size(&input), trf!("stránky {range}", "pages {range}"))
        }
        PdfTask::Rotate { input, angle, spec, out_dir, pw } => {
            let out = out_named(&out_dir, &input, tr!("(otočeno)", "(rotated)"));
            let sp = spec.replace(' ', "");
            let rot = if sp.is_empty() { format!("--rotate=+{angle}") } else { format!("--rotate=+{angle}:{sp}") };
            let mut a = pwarg(&pw);
            a.extend([rot, input.to_string_lossy().to_string(), out.to_string_lossy().to_string()]);
            qpdf(ctx, a)?;
            ctx.shared.set_frac(1.0);
            done(vec![out], path_size(&input), trf!("otočeno o {angle}°", "rotated by {angle}°"))
        }
        PdfTask::Optimize { input, linearize, out_dir, pw } => {
            let out = out_named(&out_dir, &input, tr!("(optimalizováno)", "(optimised)"));
            let mut a = pwarg(&pw);
            a.extend([s("--object-streams=generate"), s("--compress-streams=y"), s("--recompress-flate"), s("--compression-level=9")]);
            if linearize {
                a.push(s("--linearize"));
            }
            a.extend([input.to_string_lossy().to_string(), out.to_string_lossy().to_string()]);
            qpdf(ctx, a)?;
            ctx.shared.set_frac(1.0);
            done(vec![out], path_size(&input), tr!("optimalizace + komprese", "optimisation + compression"))
        }
        PdfTask::Encrypt { input, user, owner, out_dir } => {
            let out = out_named(&out_dir, &input, tr!("(zamčeno)", "(locked)"));
            let owner = if owner.is_empty() { user.clone() } else { owner };
            qpdf(ctx, vec![s("--encrypt"), user, owner, s("256"), s("--"), input.to_string_lossy().to_string(), out.to_string_lossy().to_string()])?;
            ctx.shared.set_frac(1.0);
            done(vec![out], path_size(&input), "AES-256")
        }
        PdfTask::Decrypt { input, out_dir, pw } => {
            let out = out_named(&out_dir, &input, tr!("(odemčeno)", "(unlocked)"));
            let mut a = pwarg(&pw);
            a.extend([s("--decrypt"), input.to_string_lossy().to_string(), out.to_string_lossy().to_string()]);
            qpdf(ctx, a)?;
            ctx.shared.set_frac(1.0);
            done(vec![out], path_size(&input), tr!("heslo odstraněno", "password removed"))
        }
    }
}

//! Dokumenty přes pandoc (+ typst pro PDF).
use crate::{formats::*, job::*, opts::Opts, stats::Engine, util::*};
use std::path::Path;

const EXTRACT_MEDIA_WRITERS: &[&str] = &["markdown", "gfm", "rst", "org", "asciidoc", "textile", "mediawiki", "typst", "latex", "dokuwiki", "muse"];

/// Lua filtr pro pandoc: odstraní identifikátory a vnitřní odkazy, které typst nezná
/// (EPUB generuje id jako "ch001.xhtml#kapitola", což není platný label v Typstu).
const TYPST_FILTER: &str = r##"
local function noid(e) e.identifier = ""; return e end
return {
  { Header = noid, Div = noid, Span = noid, CodeBlock = noid, Image = noid, Figure = noid,
    Table = function(t) t.attr = pandoc.Attr(); return t end,
    Link = function(l)
      local tg = l.target
      if tg:match("^#") or (not tg:match("^[%a][%w+.-]*:") and tg:match("#")) or tg:match("%.x?html$") then
        return l.content
      end
      l.identifier = ""
      return l
    end },
}
"##;

static WORK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Dokument -> PDF ve dvou krocích: pandoc -> .typ (obrázky jako relativní cesty), potom typst compile.
/// (Pandocí vestavěné volání typstu selhávalo na absolutních cestách typu C:/... u obrázků z EPUB/DOCX.)
fn run_pandoc_pdf(ctx: &JobCtx, input: &Path, reader: &str, out: &Path, o: &Opts) -> Res<()> {
    let pandoc = ctx.tools.pandoc.as_ref().ok_or(tr!("Pandoc není k dispozici", "Pandoc is not available"))?;
    let typst = ctx.tools.typst.as_ref().ok_or(tr!("Typst (pro výstup PDF) není k dispozici", "Typst (needed for PDF output) is not available"))?;
    let work = app_dir().join("tmp").join(format!("typ-{}-{}", std::process::id(), WORK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let typ = work.join("doc.typ");
    let stem = stem_of(input);
    let result = (|| -> Res<()> {
        ctx.shared.set_note(tr!("Pandoc připravuje dokument…", "Pandoc is preparing the document…"));
        ctx.shared.set_indeterminate();
        std::fs::write(work.join("filter.lua"), TYPST_FILTER).map_err(|e| e.to_string())?;
        let mut cmd = command(pandoc);
        cmd.current_dir(&work).arg("--lua-filter=filter.lua").args(["-f", reader, "-t", "typst", "-s"]).arg(input).arg("-o").arg(&typ).arg("--extract-media=media");
        cmd.args(["-V", &format!("papersize={}", o.paper), "-V", &format!("fontsize={}pt", o.font_pt.clamp(6, 24))]);
        // pevně daný font vložený v Typstu (bez něj starší šablony hlásí "font fallback list must not be empty")
        cmd.args(["-V", "mainfont=Libertinus Serif"]);
        cmd.args(["--metadata", &format!("title={stem}")]);
        if o.toc {
            cmd.arg("--toc");
        }
        run_cmd(ctx, cmd, |_, _| {})?;
        ctx.shared.set_note(tr!("Typst sází PDF…", "Typst is typesetting the PDF…"));
        let mut t = command(typst);
        t.current_dir(&work).arg("compile").arg("--root").arg(&work).arg(&typ).arg(out);
        run_cmd(ctx, t, |_, _| {}).map(|_| ())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

pub fn run_pandoc(ctx: &JobCtx, input: &Path, in_ext: &str, writer: &str, out: &Path, o: &Opts, to_pdf: bool, cwd: Option<&Path>) -> Res<()> {
    let reader = if matches!(in_ext, "txt" | "text") { "markdown+hard_line_breaks".to_string() } else { pandoc_reader(in_ext).to_string() };
    if to_pdf {
        return run_pandoc_pdf(ctx, input, &reader, out, o);
    }
    let pandoc = ctx.tools.pandoc.as_ref().ok_or(tr!("Pandoc není k dispozici", "Pandoc is not available"))?;
    let mut cmd = command(pandoc);
    cmd.arg("-f").arg(reader).arg("-t").arg(writer);
    cmd.arg(input).arg("-o").arg(out);
    let stem = stem_of(input);
    if matches!(writer, "html5" | "latex" | "typst" | "tei" | "beamer") {
        cmd.arg("-s");
        cmd.args(["--metadata", &format!("pagetitle={stem}"), "--metadata", &format!("title={stem}")]);
    }
    if o.toc {
        cmd.arg("--toc");
    }
    if EXTRACT_MEDIA_WRITERS.contains(&writer) && matches!(in_ext, "docx" | "odt" | "epub" | "html" | "htm") {
        cmd.arg(format!("--extract-media={stem}_media"));
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    ctx.shared.set_note(tr!("Pandoc převádí…", "Pandoc is converting…"));
    ctx.shared.set_indeterminate();
    run_cmd(ctx, cmd, |_, _| {}).map(|_| ())
}

pub fn convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let ext = job.fmt.ext.as_str();
    let to_pdf = job.fmt.class == Class::Pdf;
    let out = unique_path(job.out_dir.join(format!("{}.{}", stem_of(&job.input), ext)));
    let part = part_path(&out);
    run_pandoc(ctx, &job.input, &job.in_ext, &job.fmt.extra, &part, &job.opts, to_pdf, Some(&job.out_dir)).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        e
    })?;
    finish_part(&part, &out)?;
    ctx.shared.set_frac(1.0);
    Ok(Outcome {
        outputs: vec![out.clone()],
        bytes_in: path_size(&job.input),
        bytes_out: path_size(&out),
        engine: Engine::Cpu,
        detail: if to_pdf { "Pandoc → Typst → PDF".to_string() } else { format!("Pandoc ({})", job.fmt.extra) },
        media_secs: 0.0,
    })
}

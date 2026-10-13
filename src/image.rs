//! Obrázky přes ImageMagick (všechny formáty, které vložený build umí).
use crate::{job::*, opts::Opts, stats::Engine, util::*};
use std::path::Path;

const MULTI: &[&str] = &["gif", "webp", "tiff", "tif", "pdf", "apng", "mng", "ico", "icns", "psd", "heic", "avif", "jxl"];
const NO_ALPHA: &[&str] = &["jpeg", "jpg", "bmp", "ppm", "pgm", "pbm", "pnm", "pcx", "wbmp"];
const LOSSY_Q: &[&str] = &["jpeg", "jpg", "webp", "avif", "heic", "jxl", "jp2", "pdf"];

/// Argumenty ImageMagicku: (před vstupem, po vstupu).
pub fn magick_args(o: &Opts, in_ext: &str, coder: &str, out_ext: &str) -> (Vec<String>, Vec<String>) {
    let mut pre: Vec<String> = Vec::new();
    let mut post: Vec<String> = Vec::new();
    if matches!(in_ext, "svg" | "svgz") {
        pre.extend(["-background".into(), "none".into(), "-density".into(), format!("{}", o.dpi.max(72))]);
    }
    post.push("-auto-orient".into());
    if o.resize_pct > 0 && o.resize_pct != 100 {
        post.extend(["-resize".into(), format!("{}%", o.resize_pct)]);
    } else if o.max_w > 0 || o.max_h > 0 {
        let w = if o.max_w > 0 { o.max_w.to_string() } else { String::new() };
        let h = if o.max_h > 0 { o.max_h.to_string() } else { String::new() };
        post.extend(["-resize".into(), format!("{w}x{h}>")]);
    }
    if NO_ALPHA.contains(&coder) {
        post.extend(["-background".into(), o.img_bg.clone(), "-alpha".into(), "remove".into(), "-alpha".into(), "off".into()]);
    }
    if out_ext == "ico" {
        post.extend(["-resize".into(), "256x256>".into(), "-define".into(), "icon:auto-resize=256,128,64,48,32,16".into()]);
    }
    if LOSSY_Q.contains(&coder) {
        post.extend(["-quality".into(), o.img_quality.clamp(1, 100).to_string()]);
    }
    if o.strip_meta {
        post.push("-strip".into());
    }
    (pre, post)
}

pub fn run_magick(ctx: &JobCtx, args: Vec<String>) -> Res<()> {
    let m = ctx.tools.magick.as_ref().ok_or(tr!("ImageMagick není k dispozici", "ImageMagick is not available"))?;
    let mut cmd = command(m);
    if let Some(dir) = m.parent() {
        cmd.env("MAGICK_HOME", dir).env("MAGICK_CONFIGURE_PATH", dir);
    }
    cmd.env("MAGICK_TEMPORARY_PATH", std::env::temp_dir());
    cmd.arg("-monitor").args(&args);
    ctx.shared.set_indeterminate();
    let shared = ctx.shared.clone();
    run_cmd(ctx, cmd, move |is_err, line| {
        if is_err {
            if let Some(pos) = line.find("% complete") {
                let digits: String = line[..pos].chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
                if let Ok(p) = digits.parse::<f32>() {
                    shared.set_frac(p / 100.0);
                }
            }
        }
    })
    .map(|_| ())
}

pub fn convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    if ctx.tools.magick.is_none() {
        return crate::imgnative::convert(ctx, job);
    }
    let o = &job.opts;
    let ext = job.fmt.ext.as_str();
    let coder = if job.fmt.extra.is_empty() { ext } else { job.fmt.extra.as_str() };
    let out = unique_path(job.out_dir.join(format!("{}.{}", stem_of(&job.input), ext)));
    let part = part_path(&out);
    let (pre, post) = magick_args(o, &job.in_ext, coder, ext);
    let multi = MULTI.contains(&ext) || MULTI.contains(&coder);
    let src = job.input.to_string_lossy().to_string();
    let src_arg = if multi { src } else { format!("{src}[0]") };
    let mut args = pre;
    args.push(src_arg);
    args.extend(post);
    args.push(format!("{coder}:{}", part.to_string_lossy()));
    ctx.shared.set_note(tr!("ImageMagick převádí…", "ImageMagick is converting…"));
    run_magick(ctx, args).map_err(|e| {
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
        detail: format!("ImageMagick{}", if LOSSY_Q.contains(&coder) { trf!(" · kvalita {}", " · quality {}", o.img_quality) } else { String::new() }),
        media_secs: 0.0,
    })
}

/// Pro PDF -> obrázky: převede mezisoubor (PNG) na cílový formát.
pub fn convert_file(ctx: &JobCtx, o: &Opts, src: &Path, dst: &Path, coder: &str, ext: &str) -> Res<()> {
    if ctx.tools.magick.is_none() {
        return crate::imgnative::convert_file(o, src, dst, ext);
    }
    let (pre, post) = magick_args(o, "png", coder, ext);
    let mut args = pre;
    args.push(format!("{}[0]", src.to_string_lossy()));
    args.extend(post);
    args.push(format!("{coder}:{}", dst.to_string_lossy()));
    run_magick(ctx, args)
}

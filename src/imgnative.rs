//! Vestavěný převodník obrázků (čisté Rust, knihovna `image`) – záloha, když není k dispozici ImageMagick
//! (typicky Linux a macOS bez nainstalovaného balíčku). Umí běžné formáty; vše ostatní (HEIC, AVIF, JXL, PSD, SVG…)
//! zůstává na ImageMagicku. Na Windows je ImageMagick vložený, takže se tohle nepoužije.
use crate::{job::*, opts::Opts, stats::Engine, util::*};
use image::{
    codecs::{gif::GifEncoder, jpeg::JpegEncoder, webp::WebPEncoder},
    imageops::FilterType,
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, RgbaImage,
};
use std::{
    collections::BTreeSet,
    fs::File,
    io::BufWriter,
    path::Path,
};

/// Formáty, které umíme číst / zapsat bez ImageMagicku.
pub const READ: &[&str] = &["png", "jpg", "jpeg", "jpe", "gif", "bmp", "tif", "tiff", "webp", "ico", "qoi", "tga", "pnm", "ppm", "pgm", "pbm"];
pub const WRITE: &[&str] = &["png", "jpg", "gif", "bmp", "tiff", "webp", "ico", "qoi", "tga", "ppm"];

pub fn read_set() -> BTreeSet<String> {
    READ.iter().map(|s| s.to_string()).collect()
}
pub fn write_set() -> BTreeSet<String> {
    WRITE.iter().map(|s| s.to_string()).collect()
}

/// "white", "black", "#rrggbb" -> RGB (výchozí bílá).
fn parse_bg(s: &str) -> [u8; 3] {
    let t = s.trim().to_lowercase();
    match t.as_str() {
        "black" => [0, 0, 0],
        "white" | "" => [255, 255, 255],
        "gray" | "grey" => [128, 128, 128],
        _ => {
            let h = t.trim_start_matches('#');
            if h.len() == 6 {
                if let Ok(v) = u32::from_str_radix(h, 16) {
                    return [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                }
            }
            [255, 255, 255]
        }
    }
}

fn flatten(img: &DynamicImage, bg: [u8; 3]) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let m = |c: u8, b: u8| (c as f32 * a + b as f32 * (1.0 - a)).round() as u8;
        out.put_pixel(x, y, image::Rgb([m(p[0], bg[0]), m(p[1], bg[1]), m(p[2], bg[2])]));
    }
    out
}

fn load(src: &Path) -> Res<DynamicImage> {
    let reader = ImageReader::open(src).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut dec = reader.into_decoder().map_err(|e| format!("{}: {e}", src.display()))?;
    let orient = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec).map_err(|e| e.to_string())?;
    img.apply_orientation(orient);
    Ok(img)
}

fn resize(img: DynamicImage, o: &Opts) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if o.resize_pct > 0 && o.resize_pct != 100 {
        let nw = ((w as f64 * o.resize_pct as f64 / 100.0).round() as u32).max(1);
        let nh = ((h as f64 * o.resize_pct as f64 / 100.0).round() as u32).max(1);
        return img.resize_exact(nw, nh, FilterType::Lanczos3);
    }
    if o.max_w > 0 || o.max_h > 0 {
        let (mw, mh) = (if o.max_w > 0 { o.max_w } else { u32::MAX }, if o.max_h > 0 { o.max_h } else { u32::MAX });
        if w > mw || h > mh {
            return img.resize(mw, mh, FilterType::Lanczos3); // zachová poměr stran, jen zmenšuje
        }
    }
    img
}

/// Převede obrázek `src` do `dst` ve formátu `ext` (png, jpg, …).
pub fn convert_file(o: &Opts, src: &Path, dst: &Path, ext: &str) -> Res<()> {
    let mut img = resize(load(src)?, o);
    let file = File::create(dst).map_err(|e| e.to_string())?;
    let mut w = BufWriter::new(file);
    let err = |e: image::ImageError| e.to_string();
    match ext {
        "jpg" | "jpeg" => {
            let rgb = flatten(&img, parse_bg(&o.img_bg));
            JpegEncoder::new_with_quality(&mut w, o.img_quality.clamp(1, 100) as u8).write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8).map_err(err)?;
        }
        "png" => {
            let rgba = img.to_rgba8();
            image::codecs::png::PngEncoder::new(&mut w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8).map_err(err)?;
        }
        "webp" => {
            // knihovna umí jen bezeztrátový WebP (kvalita se neuplatní)
            let rgba = img.to_rgba8();
            WebPEncoder::new_lossless(&mut w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8).map_err(err)?;
        }
        "gif" => {
            let rgba: RgbaImage = img.to_rgba8();
            let mut enc = GifEncoder::new(&mut w);
            enc.encode_frame(image::Frame::new(rgba)).map_err(err)?;
        }
        "bmp" => {
            let rgb = flatten(&img, parse_bg(&o.img_bg));
            image::codecs::bmp::BmpEncoder::new(&mut w).write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8).map_err(err)?;
        }
        "tiff" | "tif" => {
            drop(w);
            img.save_with_format(dst, image::ImageFormat::Tiff).map_err(err)?;
            return Ok(());
        }
        "ico" => {
            if img.width() > 256 || img.height() > 256 {
                img = img.resize(256, 256, FilterType::Lanczos3);
            }
            let rgba = img.to_rgba8();
            image::codecs::ico::IcoEncoder::new(&mut w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8).map_err(err)?;
        }
        "qoi" => {
            let rgba = img.to_rgba8();
            image::codecs::qoi::QoiEncoder::new(&mut w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8).map_err(err)?;
        }
        "tga" => {
            let rgba = img.to_rgba8();
            image::codecs::tga::TgaEncoder::new(&mut w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8).map_err(err)?;
        }
        "ppm" | "pnm" | "pgm" | "pbm" => {
            let rgb = flatten(&img, parse_bg(&o.img_bg));
            image::codecs::pnm::PnmEncoder::new(&mut w).write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8).map_err(err)?;
        }
        other => return Err(trf!("Formát .{other} vestavěný převodník neumí – nainstaluj ImageMagick.", "The built-in converter cannot write .{other} – install ImageMagick.")),
    }
    use std::io::Write;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Hotový krok pro frontu (stejný tvar výsledku jako ImageMagick).
pub fn convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let ext = job.fmt.ext.as_str();
    let out = unique_path(job.out_dir.join(format!("{}.{}", stem_of(&job.input), ext)));
    let part = part_path(&out);
    ctx.shared.set_indeterminate();
    ctx.shared.set_note(tr!("Vestavěný převodník…", "Built-in converter…"));
    if let Err(e) = convert_file(&job.opts, &job.input, &part, ext) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    finish_part(&part, &out)?;
    ctx.shared.set_frac(1.0);
    Ok(Outcome {
        outputs: vec![out.clone()],
        bytes_in: path_size(&job.input),
        bytes_out: path_size(&out),
        engine: Engine::Cpu,
        detail: tr!("vestavěný převodník (bez ImageMagicku)", "built-in converter (no ImageMagick)").to_string(),
        media_secs: 0.0,
    })
}

//! Integrační testy: spouštějí se proti nástrojům v PATH (na Windows buildu jsou vložené).
//! `cargo test --no-default-features`
use crate::{convert::run_convert, formats::*, hw, job::*, opts::*, pdfops::*, probe, tools::Tools, trackops::*, util::*};
use std::{path::PathBuf, process::Command, sync::Arc};

fn ctx() -> (JobCtx, Arc<hw::SysInfo>) {
    let tools = Tools::prepare(|_| {});
    let si = Arc::new(hw::detect(&tools));
    (JobCtx { shared: Shared::new(), si: si.clone(), tools: Arc::new(tools) }, si)
}

fn work(name: &str) -> PathBuf {
    let d = PathBuf::from("/tmp/tc-test").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sh(args: &[&str]) {
    let o = Command::new(args[0]).args(&args[1..]).output().unwrap();
    assert!(o.status.success(), "{:?}: {}", args, String::from_utf8_lossy(&o.stderr));
}

fn job(ctx: &JobCtx, si: &hw::SysInfo, input: &PathBuf, to: &str, opts: Opts) -> Res<Outcome> {
    let ext = full_ext(input);
    let kind = detect_kind(&ext, si);
    let fmt = targets_for(kind, &ext, si).into_iter().find(|f| f.ext == to).unwrap_or_else(|| panic!("cíl {to} není nabízen pro {ext}"));
    let j = ConvertJob { input: input.clone(), in_ext: ext, kind, fmt, opts, out_dir: input.parent().unwrap().to_path_buf() };
    run_convert(ctx, &j)
}

fn make_video(dir: &PathBuf) -> PathBuf {
    let v = dir.join("clip.mkv");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=320x240:r=25:d=2", "-f", "lavfi", "-i", "sine=f=440:d=2", "-f", "lavfi", "-i", "sine=f=880:d=2",
        "-map", "0", "-map", "1", "-map", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-metadata:s:a:0", "language=eng", "-metadata:s:a:1", "language=cze", v.to_str().unwrap()]);
    v
}

#[test]
fn video_audio_image_conversions() {
    let (c, si) = ctx();
    let d = work("media");
    let v = make_video(&d);

    for to in ["mp4", "webm", "avi", "mov"] {
        let mut o = Opts::default();
        o.preset = Preset::Speed;
        let r = job(&c, &si, &v, to, o).unwrap_or_else(|e| panic!("video→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
        assert!(r.bytes_out > 1000);
        println!("video→{to}: {} ({})", r.detail, r.engine.label());
    }
    // video → audio
    for to in ["mp3", "flac", "opus", "m4a", "wav", "ogg", "ac3", "wma"] {
        let r = job(&c, &si, &v, to, Opts::default()).unwrap_or_else(|e| panic!("audio→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
        println!("video→{to}: {}", r.detail);
    }
    // bitrate + výběr 2. stopy
    let mut o = Opts::default();
    o.akbps = 96;
    o.audio_track = 1;
    let r = job(&c, &si, &v, "mp3", o).unwrap();
    let p = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert!(p.audio_streams().next().unwrap().bit_rate > 80_000 && p.audio_streams().next().unwrap().bit_rate < 110_000);
    // animace + snímek
    for to in ["gif", "apng", "webp", "png", "jpg"] {
        let r = job(&c, &si, &v, to, Opts::default()).unwrap_or_else(|e| panic!("anim→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
    }
    // střih
    let mut o = Opts::default();
    o.trim_start = "0:00:00.5".into();
    o.trim_end = "1.5".into();
    o.preset = Preset::Speed;
    let r = job(&c, &si, &v, "mp4", o).unwrap();
    let p = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert!((p.duration - 1.0).abs() < 0.2, "délka po střihu {}", p.duration);
    // CRF a bitrate režimy
    for rc in [Rc::Cqp, Rc::Vbr, Rc::Cbr] {
        let mut o = Opts::default();
        o.rc = rc;
        o.vcodec = "hevc".into();
        o.preset = Preset::Speed;
        job(&c, &si, &v, "mkv", o).unwrap_or_else(|e| panic!("hevc {rc:?}: {e}"));
    }
}

#[test]
fn images_and_documents() {
    let (c, si) = ctx();
    let d = work("img");
    let png = d.join("pic.png");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=200x120:d=1", "-frames:v", "1", png.to_str().unwrap()]);
    for to in ["jpg", "webp", "bmp", "tiff", "gif", "ico", "pdf"] {
        let r = job(&c, &si, &png, to, Opts::default()).unwrap_or_else(|e| panic!("img→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
    }
    let mut o = Opts::default();
    o.resize_pct = 50;
    let r = job(&c, &si, &png, "jpg", o).unwrap();
    let p = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert_eq!(p.video().unwrap().width, 100);

    let md = d.join("doc.md");
    std::fs::write(&md, "# Nadpis\n\nTěšíme se na **Česko** – příliš žluťoučký kůň.\n\n- a\n- b\n").unwrap();
    for to in ["html", "docx", "odt", "epub", "txt", "rst", "tex"] {
        let r = job(&c, &si, &md, to, Opts::default()).unwrap_or_else(|e| panic!("doc→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
    }
    // docx → md zpět
    let docx = d.join("doc.docx");
    let r = job(&c, &si, &docx, "md", Opts::default()).unwrap();
    assert!(std::fs::read_to_string(&r.outputs[0]).unwrap().contains("Nadpis"));
}

#[test]
fn archives_roundtrip() {
    let (c, si) = ctx();
    let d = work("arc");
    let src = d.join("data");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "ahoj".repeat(1000)).unwrap();
    std::fs::write(src.join("sub/b.txt"), "svět".repeat(1000)).unwrap();
    for to in ["zip", "7z", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"] {
        let mut o = Opts::default();
        if to == "zip" || to == "7z" {
            o.password = "heslo123".into();
        }
        let kind = Kind::Folder;
        let fmt = targets_for(kind, "", &si).into_iter().find(|f| f.ext == to).unwrap();
        let j = ConvertJob { input: src.clone(), in_ext: String::new(), kind, fmt, opts: o.clone(), out_dir: d.clone() };
        let r = run_convert(&c, &j).unwrap_or_else(|e| panic!("pack {to}: {e}"));
        let arc = r.outputs[0].clone();
        // rozbalit zpátky
        let ext = full_ext(&arc);
        let k = detect_kind(&ext, &si);
        assert_eq!(k, Kind::Archive, "{ext}");
        let fmt = targets_for(k, &ext, &si).into_iter().find(|f| f.extra == "extract").unwrap();
        let j = ConvertJob { input: arc.clone(), in_ext: ext, kind: k, fmt, opts: o, out_dir: d.join("out") };
        std::fs::create_dir_all(d.join("out")).unwrap();
        let r = run_convert(&c, &j).unwrap_or_else(|e| panic!("unpack {to}: {e}"));
        let out = &r.outputs[0];
        let found: Vec<_> = walkdir::WalkDir::new(out).into_iter().filter_map(Result::ok).filter(|e| e.file_name() == "b.txt").collect();
        assert_eq!(found.len(), 1, "{to}: b.txt nenalezen v {out:?}");
        assert_eq!(std::fs::read_to_string(found[0].path()).unwrap().len(), "svět".len() * 1000);
        println!("{to}: OK ({})", r.detail);
    }
    // archiv → jiný archiv
    let zip = d.join("data.zip");
    let _ = std::fs::remove_file(&zip);
}

#[test]
fn pdf_tools() {
    let (c, si) = ctx();
    let d = work("pdf");
    let mut pdfs = Vec::new();
    for i in 0..3 {
        let p = d.join(format!("p{i}.pdf"));
        // 3 stránky každé
        let mut imgs = Vec::new();
        for j in 0..3 {
            let png = d.join(format!("i{i}{j}.png"));
            sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c=0x{:02x}{:02x}{:02x}:s=300x400", 40 + i * 60, 40 + j * 60, 120), "-frames:v", "1", png.to_str().unwrap()]);
            imgs.push(png);
        }
        let mut a = vec!["convert".to_string()];
        a.extend(imgs.iter().map(|x| x.to_string_lossy().to_string()));
        a.push(p.to_string_lossy().to_string());
        sh(&a.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        pdfs.push(p);
    }
    let n = |p: &PathBuf| page_count(&c.tools, p, "").unwrap();
    assert_eq!(n(&pdfs[0]), 3);
    let r = run(&c, PdfTask::Merge { inputs: pdfs.clone(), out_dir: d.clone(), name: "all".into(), pw: String::new() }).unwrap();
    assert_eq!(n(&r.outputs[0]), 9);
    let merged = r.outputs[0].clone();
    let r = run(&c, PdfTask::SplitEvery { input: merged.clone(), n: 4, out_dir: d.clone(), pw: String::new() }).unwrap();
    assert_eq!(std::fs::read_dir(&r.outputs[0]).unwrap().count(), 3);
    let r = run(&c, PdfTask::SplitRanges { input: merged.clone(), ranges: "1-2; 3-z".into(), out_dir: d.clone(), pw: String::new() }).unwrap();
    assert_eq!(std::fs::read_dir(&r.outputs[0]).unwrap().count(), 2);
    for (mode, spec, expect) in [(PageMode::Extract, "1-3,9", 4), (PageMode::Delete, "2,5-7", 5), (PageMode::Odd, "", 5), (PageMode::Even, "", 4), (PageMode::Reverse, "", 9)] {
        let r = run(&c, PdfTask::Pages { input: merged.clone(), spec: spec.into(), mode, out_dir: d.clone(), pw: String::new() }).unwrap_or_else(|e| panic!("{spec}: {e}"));
        assert_eq!(n(&r.outputs[0]), expect, "{spec}");
    }
    assert!(run(&c, PdfTask::Pages { input: merged.clone(), spec: "99".into(), mode: PageMode::Extract, out_dir: d.clone(), pw: String::new() }).is_err());
    run(&c, PdfTask::Rotate { input: merged.clone(), angle: 90, spec: "1-2".into(), out_dir: d.clone(), pw: String::new() }).unwrap();
    run(&c, PdfTask::Optimize { input: merged.clone(), linearize: true, out_dir: d.clone(), pw: String::new() }).unwrap();
    let r = run(&c, PdfTask::Encrypt { input: merged.clone(), user: "tajne".into(), owner: String::new(), out_dir: d.clone() }).unwrap();
    assert!(page_count(&c.tools, &r.outputs[0], "").is_none());
    let r = run(&c, PdfTask::Decrypt { input: r.outputs[0].clone(), out_dir: d.clone(), pw: "tajne".into() }).unwrap();
    assert_eq!(n(&r.outputs[0]), 9);
    // cokoliv → PDF (obrázky + pdf, bez typstu)
    let r = run(&c, PdfTask::ToPdf { inputs: vec![d.join("i00.png"), pdfs[1].clone(), d.join("i11.png")], merge: true, out_dir: d.clone(), name: "mix".into(), opts: Opts::default(), si: si.clone() }).unwrap();
    assert_eq!(n(&r.outputs[0]), 5);
}

#[test]
fn tracks_remux_and_extract() {
    let (c, _si) = ctx();
    let d = work("tracks");
    let v = make_video(&d);
    let p = probe::run(&c.tools, &v).unwrap();
    assert_eq!(p.streams.len(), 3);
    assert_eq!(p.count(probe::StreamKind::Audio), 2);
    // nech video + 2. audio, do mp4
    let keep = vec![p.streams[0].index, p.streams[2].index];
    let r = remux(&c, RemuxTask { input: v.clone(), probe: p.clone(), out_ext: "mp4".into(), keep, keep_chapters: true, keep_meta: true, out_dir: d.clone() }).unwrap();
    let q = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert_eq!(q.streams.len(), 2);
    assert_eq!(q.audio_streams().next().unwrap().lang, "cze");
    // extrakce audia
    let a = p.streams.iter().find(|s| s.kind == probe::StreamKind::Audio).unwrap().clone();
    let r = extract_track(&c, ExtractTask { input: v.clone(), stream: a, duration: p.duration, subs_to_srt: false, out_dir: d.clone() }).unwrap();
    assert!(r.outputs[0].extension().unwrap() == "m4a");
    // titulky: vyrob mkv se srt
    let srt = d.join("t.srt");
    std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:01,000\nAhoj\n").unwrap();
    let withsub = d.join("sub.mkv");
    sh(&["ffmpeg", "-v", "error", "-y", "-i", v.to_str().unwrap(), "-i", srt.to_str().unwrap(), "-map", "0", "-map", "1", "-c", "copy", "-metadata:s:s:0", "language=cze", withsub.to_str().unwrap()]);
    let p2 = probe::run(&c.tools, &withsub).unwrap();
    let s = p2.streams.iter().find(|s| s.kind == probe::StreamKind::Subtitle).unwrap().clone();
    let r = extract_track(&c, ExtractTask { input: withsub.clone(), stream: s, duration: p2.duration, subs_to_srt: true, out_dir: d.clone() }).unwrap();
    assert!(std::fs::read_to_string(&r.outputs[0]).unwrap().contains("Ahoj"));
    // remux s titulky do mp4 (mov_text)
    let keep: Vec<usize> = p2.streams.iter().map(|s| s.index).collect();
    let r = remux(&c, RemuxTask { input: withsub, probe: p2, out_ext: "mp4".into(), keep, keep_chapters: false, keep_meta: false, out_dir: d.clone() }).unwrap();
    let q = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert_eq!(q.count(probe::StreamKind::Subtitle), 1);
}

#[test]
fn cancel_and_progress() {
    let (c, si) = ctx();
    let d = work("cancel");
    let v = d.join("long.mkv");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=20", "-c:v", "libx264", "-preset", "ultrafast", v.to_str().unwrap()]);
    // průběh
    let sh_ = c.shared.clone();
    let h = std::thread::spawn({
        let c = c.clone();
        let v = v.clone();
        let si = si.clone();
        move || {
            let mut o = Opts::default();
            o.preset = Preset::Speed;
            job(&c, &si, &v, "mp4", o)
        }
    });
    let mut seen = 0.0f32;
    while !h.is_finished() {
        if let Some(f) = sh_.frac() {
            seen = seen.max(f);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(h.join().unwrap().is_ok());
    assert!(seen > 0.3, "průběh se nehlásil: {seen}");
    // zrušení
    let c2 = JobCtx { shared: Shared::new(), si: si.clone(), tools: c.tools.clone() };
    let sh2 = c2.shared.clone();
    // pomalý kodek, ať je co rušit; starší FFmpeg (třeba 4.4 z Ubuntu 22.04) nemá SVT-AV1
    let slow = if si.has_encoder("libsvtav1") { "av1" } else if si.has_encoder("libx265") { "hevc" } else { "h264" };
    let h = std::thread::spawn({
        let v = v.clone();
        let si = si.clone();
        move || {
            let mut o = Opts::default();
            o.vcodec = slow.into();
            job(&c2, &si, &v, "mkv", o)
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(700));
    sh2.cancel();
    let r = h.join().unwrap();
    assert_eq!(r.unwrap_err(), CANCELLED);
    let leftovers: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().contains("tcpart")).collect();
    assert!(leftovers.is_empty(), "zbyly rozpracované soubory: {leftovers:?}");
}

fn probe_of(c: &JobCtx, p: &std::path::Path) -> probe::Probe {
    probe::run(&c.tools, p).unwrap()
}

#[test]
fn custom_values_video_and_audio() {
    let (c, si) = ctx();
    let d = work("custom");
    let v = make_video(&d);

    // vlastní rozlišení (w×h s pruhy), FPS 23.976, 10-bit, GOP, preset ultrafast, extra args
    let mut o = Opts::default();
    o.width = 640;
    o.height = 360;
    o.fps = 23.976;
    o.pix_fmt = "yuv420p10le".into();
    o.x_preset = "ultrafast".into();
    o.gop = 12;
    o.vprofile = "high10".into();
    o.extra_args = "-metadata title=\"Muj titulek\"".into();
    o.vcodec = "h264".into();
    let r = job(&c, &si, &v, "mp4", o).unwrap_or_else(|e| panic!("custom video: {e}"));
    let p = probe_of(&c, &r.outputs[0]);
    let vs = p.video().unwrap();
    assert_eq!((vs.width, vs.height), (640, 360));
    assert!((vs.fps - 23.976).abs() < 0.01, "fps {}", vs.fps);
    assert_eq!(vs.pix_fmt, "yuv420p10le");
    assert_eq!(p.tags.get("title").map(|s| s.as_str()), Some("Muj titulek"));
    assert!(r.detail.contains("10-bit"), "{}", r.detail);

    // jen šířka -> výška se dopočítá (320x240 -> 160x120)
    let mut o = Opts::default();
    o.width = 160;
    o.x_preset = "veryfast".into();
    o.x_tune = "zerolatency".into();
    let r = job(&c, &si, &v, "mkv", o).unwrap();
    let vs = probe_of(&c, &r.outputs[0]).video().unwrap().clone();
    assert_eq!((vs.width, vs.height), (160, 120));

    // vlastní bitrate videa
    let mut o = Opts::default();
    o.rc = Rc::Vbr;
    o.vkbps = 777;
    o.maxrate = 900;
    o.x_preset = "ultrafast".into();
    let r = job(&c, &si, &v, "mp4", o).unwrap();
    let br = probe_of(&c, &r.outputs[0]).video().unwrap().bit_rate;
    println!("vbr 777k -> {br}");

    // audio: vlastní vzorkování, kanály, bitrate, hloubka
    let mut o = Opts::default();
    o.sample_rate = 22050;
    o.channels = 2;
    o.akbps = 123;
    let r = job(&c, &si, &v, "mp3", o).unwrap();
    let a = probe_of(&c, &r.outputs[0]).audio_streams().next().unwrap().clone();
    assert_eq!((a.sample_rate, a.channels), (22050, 2));

    let mut o = Opts::default();
    o.abits = 24;
    o.sample_rate = 96000;
    let r = job(&c, &si, &v, "flac", o).unwrap();
    let a = probe_of(&c, &r.outputs[0]).audio_streams().next().unwrap().clone();
    assert_eq!(a.sample_rate, 96000);
    assert_eq!(a.bit_depth, 24, "{:?}", a.sample_fmt);

    for (to, bits, float, expect) in [("wav", 24, false, 24), ("wav", 0, true, 32), ("aiff", 16, false, 16), ("aiff", 24, false, 24), ("au", 16, false, 16), ("wav", 8, false, 8)] {
        let mut o = Opts::default();
        o.acodec = "auto".into();
        o.abits = bits;
        o.afloat = float;
        let r = job(&c, &si, &v, to, o).unwrap_or_else(|e| panic!("{to} {bits}: {e}"));
        let a = probe_of(&c, &r.outputs[0]).audio_streams().next().unwrap().clone();
        assert_eq!(a.bit_depth, expect, "{to} {bits}/{float}: {}", a.sample_fmt);
    }
    // CPU preset žebříček: všechny názvy musí x264 přijmout
    for p in ["superfast", "faster", "fast", "slow"] {
        let mut o = Opts::default();
        o.x_preset = p.into();
        let mut v2 = v.clone();
        v2.set_extension("mkv");
        job(&c, &si, &v, "mkv", o).unwrap_or_else(|e| panic!("preset {p}: {e}"));
    }
    // SVT-AV1 a VP9 rychlosti
    if si.has_encoder("libsvtav1") {
        let mut o = Opts::default();
        o.vcodec = "av1".into();
        o.svt_preset = 12;
        job(&c, &si, &v, "mkv", o).unwrap_or_else(|e| panic!("av1: {e}"));
    } else {
        eprintln!("přeskakuji av1: tenhle FFmpeg nemá libsvtav1");
    }
    let mut o = Opts::default();
    o.vcodec = "vp9".into();
    o.vp9_speed = 8;
    job(&c, &si, &v, "webm", o).unwrap_or_else(|e| panic!("vp9: {e}"));
}

#[test]
fn split_args_and_fps() {
    assert_eq!(crate::ffmpeg::split_args(r#"-a "b c" 'd e' f"#), vec!["-a", "b c", "d e", "f"]);
    assert_eq!(crate::ffmpeg::fps_arg(23.976), "24000/1001");
    assert_eq!(crate::ffmpeg::fps_arg(59.94), "60000/1001");
    assert_eq!(crate::ffmpeg::fps_arg(24.0), "24");
    assert_eq!(crate::ffmpeg::fps_arg(37.5), "37.5");
}

#[test]
fn pdf_to_text_images_docs() {
    // vyžaduje pdfium (vložené při buildu, nebo TC_PDFIUM=/cesta/libpdfium.so)
    let (c, si) = ctx();
    if c.tools.pdfium.is_none() {
        eprintln!("přeskakuji: pdfium není k dispozici");
        return;
    }
    let d = work("pdf2x");
    let pdf = d.join("doc.pdf");
    std::fs::write(&pdf, &include_bytes!("../testdata/czech-text.pdf")[..]).unwrap();
    // text
    let r = job(&c, &si, &pdf, "txt", Opts::default()).unwrap();
    let t = std::fs::read_to_string(&r.outputs[0]).unwrap();
    assert!(t.contains("Příliš žluťoučký"), "{t}");
    // obrázky
    for to in ["png", "jpg", "webp"] {
        let mut o = Opts::default();
        o.dpi = 72;
        let r = job(&c, &si, &pdf, to, o).unwrap_or_else(|e| panic!("pdf→{to}: {e}"));
        assert_eq!(std::fs::read_dir(&r.outputs[0]).unwrap().count(), 1, "{to}");
    }
    // dokumenty
    for to in ["md", "docx", "html", "odt", "rtf", "epub", "rst", "tex"] {
        let r = job(&c, &si, &pdf, to, Opts::default()).unwrap_or_else(|e| panic!("pdf→{to}: {e}"));
        assert!(r.outputs[0].exists(), "{to}");
    }
    let r = job(&c, &si, &pdf, "md", Opts::default()).unwrap();
    let md = std::fs::read_to_string(&r.outputs[0]).unwrap();
    println!("---- MD ----\n{md}\n------------");
    assert!(md.contains("\n# Příliš žluťoučký kůň") || md.starts_with("# Příliš žluťoučký kůň"), "nadpis: {md}");
    assert!(md.contains("## Druhá kapitola"), "podnadpis: {md}");
    assert!(md.contains("První odrážka") && (md.contains("-   ") || md.contains("- První")), "odrážky: {md}");
    assert!(md.contains("Poslední odstavec"), "odstavec: {md}");
    // odstavce se nesmí slít do jednoho: poslední odstavec je samostatný
    assert!(!md.contains("Druhá odrážka. Poslední"), "odstavce: {md}");
    // docx -> zpátky na md: obsah musí přežít
    let docx = job(&c, &si, &pdf, "docx", Opts::default()).unwrap();
    let back = job(&c, &si, &docx.outputs[0], "md", Opts::default()).unwrap();
    assert!(std::fs::read_to_string(&back.outputs[0]).unwrap().contains("Poslední odstavec"));
}

#[test]
fn reflow_heuristics() {
    use crate::pdfops::*;
    let pages = vec!["Nadpis\n\nPrvní řádek odstavce, který je dlouhý a pokračuje\nna dalším řádku tohoto odstavce a končí tečkou.\nDalší věta začíná.\n• bod jedna\n• bod dva\n".to_string(), "slo-\nva jdou dohromady\n".to_string()];
    let b = reflow(&pages);
    assert_eq!(b[0], Block::Para("Nadpis".into()));
    assert!(matches!(&b[1], Block::Para(t) if t.contains("pokračuje na dalším řádku")));
    assert!(b.contains(&Block::Bullet("bod jedna".into())));
    assert!(b.contains(&Block::Para("slova jdou dohromady".into())));
    assert_eq!(md_escape("a*b_c 1."), "a\\*b\\_c 1\\.");
}

#[test]
fn documents_with_images_to_pdf() {
    // regresní test: EPUB/DOCX s obrázkem -> PDF (dřív padalo na absolutní cestě C:/... v typstu)
    if crate::util::find_in_path("typst").is_none() {
        eprintln!("přeskakuji: typst není v PATH");
        return;
    }
    let (c, si) = ctx();
    let d = work("docpdf");
    let png = d.join("cover.png");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=300x200:d=1", "-frames:v", "1", png.to_str().unwrap()]);
    let md = d.join("kniha.md");
    std::fs::write(&md, "---\ntitle: Kniha\nlang: cs\n---\n\n# Kapitola\n\nPříliš žluťoučký kůň.\n\n![obrázek](cover.png)\n\nDalší text.\n").unwrap();
    for (fmt, name) in [("epub", "kniha.epub"), ("docx", "kniha.docx"), ("html", "kniha.html"), ("odt", "kniha.odt")] {
        let out = d.join(name);
        let mut cmd = vec!["pandoc", md.to_str().unwrap(), "-o", out.to_str().unwrap()];
        if fmt == "epub" {
            cmd.extend(["--epub-cover-image", png.to_str().unwrap()]);
        }
        let o = Command::new(cmd[0]).args(&cmd[1..]).current_dir(&d).output().unwrap();
        assert!(o.status.success(), "{fmt}: {}", String::from_utf8_lossy(&o.stderr));
        let r = job(&c, &si, &out, "pdf", Opts::default()).unwrap_or_else(|e| panic!("{fmt}→pdf: {e}"));
        let pages = crate::pdfops::page_count(&c.tools, &r.outputs[0], "").unwrap();
        assert!(pages >= 1, "{fmt}");
        println!("{fmt} → pdf: {pages} str.");
    }
    // paralelně dva převody (každý má vlastní pracovní složku)
    let a = d.join("kniha.epub");
    let h: Vec<_> = (0..3)
        .map(|_| {
            let (c, si, a) = (c.clone(), si.clone(), a.clone());
            std::thread::spawn(move || job(&c, &si, &a, "pdf", Opts::default()))
        })
        .collect();
    for t in h {
        t.join().unwrap().unwrap();
    }
}

#[test]
fn pdf_with_garbage_chars_to_epub() {
    // PDF s "obfuskovaným" textem (řídicí znaky, nonchars) nesmí rozbít EPUB/XML výstupy
    use crate::pdfops::*;
    let pages = vec!["Hello\u{0}\u{1}\u{FFFE}\u{FDD0} world \u{FFFF}\nDruhý řádek\n".to_string()];
    let md = blocks_to_markdown(&reflow(&pages));
    assert!(!md.contains('\u{FFFE}'));
    let (c, _si) = ctx();
    let d = work("garbage");
    let f = d.join("g.md");
    std::fs::write(&f, &md).unwrap();
    let out = d.join("g.epub");
    let o = Command::new("pandoc").args(["-f", "markdown", "-t", "epub3", f.to_str().unwrap(), "-o", out.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let _ = c;
}

#[test]
fn gpu_failure_falls_back_to_cpu() {
    // simulace: "GPU" enkodér je v seznamu, ale ve skutečnosti nefunguje (tady není NVENC) -> převod musí skončit na CPU
    let (c, si) = ctx();
    let d = work("fallback");
    let v = make_video(&d);
    let mut fake = (*si).clone();
    fake.hw_encoders = vec![(crate::gpu::Backend::Nvenc, "h264_nvenc".into())];
    let fake = Arc::new(fake);
    let c2 = JobCtx { shared: Shared::new(), si: fake.clone(), tools: c.tools.clone() };
    let mut o = Opts::default();
    o.vcodec = "h264".into();
    o.preset = Preset::Speed;
    let r = job(&c2, &fake, &v, "mp4", o).unwrap();
    assert_eq!(r.engine, crate::stats::Engine::Cpu, "po selhání GPU musí převod doběhnout na CPU");
    assert!(r.outputs[0].exists());
}

#[test]
fn avx512_encoders_run_with_real_ffmpeg() {
    // x265 / SVT-AV1 musí přijmout parametry pro AVX-512 (na CPU bez AVX-512 se jen nepoužijí)
    let (c, si) = ctx();
    let d = work("avx512");
    let v = make_video(&d);
    for (codec, ext) in [("hevc", "mkv"), ("av1", "mkv"), ("h264", "mp4")] {
        if codec == "av1" && !si.has_encoder("libsvtav1") {
            eprintln!("přeskakuji av1: tenhle FFmpeg nemá libsvtav1");
            continue;
        }
        let mut o = Opts::default();
        o.hw = HwMode::Cpu;
        o.vcodec = codec.into();
        o.preset = Preset::Speed;
        o.avx512 = true;
        let r = job(&c, &si, &v, ext, o).unwrap_or_else(|e| panic!("{codec}: {e}"));
        println!("{codec}: {} (avx512 detekováno: {})", r.detail, si.avx512);
        assert!(r.outputs[0].exists());
        if si.avx512 && matches!(codec, "hevc" | "av1") {
            assert!(r.detail.contains("AVX-512"), "{}", r.detail);
        }
    }
}


/// Regrese: pdfium u PDF z wkhtmltopdf/Qt rozsekal řádky na jednotlivá písmena/proužky a z PDF vylezl nečitelný text
/// (i při převodu na Markdown). Čtení po znacích musí dát čistý text ve správném pořadí.
#[test]
fn pdf_text_is_clean_for_fragmented_layouts() {
    let (c, si) = ctx();
    if c.tools.pdfium.is_none() {
        eprintln!("přeskakuji: pdfium není k dispozici (TC_PDFIUM=/cesta/libpdfium.so)");
        return;
    }
    let d = work("pdftext-clean");
    for (name, bytes) in [("fragmented.pdf", &include_bytes!("../testdata/fragmented-text.pdf")[..]), ("czech.pdf", &include_bytes!("../testdata/czech-text.pdf")[..])] {
        let pdf = d.join(name);
        std::fs::write(&pdf, bytes).unwrap();
        let txt = job(&c, &si, &pdf, "txt", Opts::default()).unwrap();
        let t = std::fs::read_to_string(&txt.outputs[0]).unwrap();
        assert!(t.contains("Příliš žluťoučký kůň úpěl ďábelské ódy"), "{name}: {t}");
        assert!(t.contains("ěščřžýáíéúůďťň"), "{name}: {t}");
        let md = job(&c, &si, &pdf, "md", Opts::default()).unwrap();
        let m = std::fs::read_to_string(&md.outputs[0]).unwrap();
        assert!(m.contains("Příliš žluťoučký kůň"), "{name}: {m}");
        // žádné rozsekané řádky: v průměru musí mít slovo víc než 2 znaky
        let words: Vec<&str> = m.split_whitespace().collect();
        let avg = words.iter().map(|w| w.chars().count()).sum::<usize>() as f32 / words.len().max(1) as f32;
        assert!(avg > 3.0, "{name}: podezřele krátká slova ({avg}): {m}");
    }
}


/// MP4 / MOV / WebM nesmí dostat víc než jedno video a nesmí dostat kodek, který neumí;
/// zvukových stop zůstane víc a právě jedna je výchozí.
#[test]
fn remux_respects_container_limits() {
    let (c, _si) = ctx();
    let d = work("remux-limits");
    let big = d.join("two-videos.mkv");
    sh(&[
        "ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10:duration=1", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=10:duration=1", "-f", "lavfi", "-i",
        "sine=frequency=440:duration=1", "-f", "lavfi", "-i", "sine=frequency=880:duration=1", "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-c:v", "libx264", "-c:a:0", "aac", "-c:a:1", "libvorbis", "-disposition:a:0", "0",
        "-disposition:a:1", "default", big.to_str().unwrap(),
    ]);
    let p = probe::run(&c.tools, &big).unwrap();
    assert_eq!(p.count(probe::StreamKind::Video), 2);
    let all: Vec<usize> = p.streams.iter().map(|s| s.index).collect();
    // plán: druhé video v MP4 se vynechá, Vorbis v MP4 také
    let pl = plan(&p.streams, &|i| all.contains(&i), "mp4");
    let skipped: Vec<_> = p.streams.iter().zip(&pl).filter(|(_, s)| s.is_some()).map(|(st, s)| (st.codec.clone(), *s)).collect();
    assert!(skipped.iter().any(|(_, s)| *s == Some(Skip::ExtraVideo)), "{skipped:?}");
    assert!(skipped.iter().any(|(c, s)| c == "vorbis" && *s == Some(Skip::Codec)), "{skipped:?}");
    // v MKV projde všechno
    assert!(plan(&p.streams, &|i| all.contains(&i), "mkv").iter().all(|s| s.is_none()));
    // skutečný remux do MP4: 1 video + 1 zvuk (aac)
    let r = remux(&c, RemuxTask { input: big.clone(), probe: p.clone(), out_ext: "mp4".into(), keep: all.clone(), keep_chapters: false, keep_meta: false, out_dir: d.clone() }).unwrap();
    let q = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert_eq!(q.count(probe::StreamKind::Video), 1);
    assert_eq!(q.count(probe::StreamKind::Audio), 1);
    // WebM: video h264 nejde vůbec
    assert!(plan(&p.streams, &|i| all.contains(&i), "webm").iter().zip(&p.streams).filter(|(_, s)| s.kind == probe::StreamKind::Video).all(|(x, _)| x.is_some()));
    // dvě aac stopy do MP4: obě zůstanou, právě jedna výchozí
    let two = d.join("two-audio.mkv");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10:duration=1", "-f", "lavfi", "-i", "sine=duration=1", "-f", "lavfi", "-i", "sine=frequency=700:duration=1", "-map", "0", "-map", "1", "-map", "2", "-c:v", "libx264", "-c:a", "aac", "-disposition:a:0", "default", "-disposition:a:1", "default", two.to_str().unwrap()]);
    let p2 = probe::run(&c.tools, &two).unwrap();
    let keep: Vec<usize> = p2.streams.iter().map(|s| s.index).collect();
    let r = remux(&c, RemuxTask { input: two, probe: p2, out_ext: "mp4".into(), keep, keep_chapters: false, keep_meta: false, out_dir: d.clone() }).unwrap();
    let q = probe::run(&c.tools, &r.outputs[0]).unwrap();
    assert_eq!(q.count(probe::StreamKind::Audio), 2);
    assert_eq!(q.audio_streams().filter(|a| a.default).count(), 1, "právě jedna výchozí zvuková stopa");
}


/// Bez ImageMagicku (Linux/macOS bez balíčku) musí fungovat vestavěný převodník na běžné formáty.
#[test]
fn builtin_image_converter_without_imagemagick() {
    let d = work("native-img");
    let png = d.join("in.png");
    sh(&["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x200:rate=1:duration=1", "-frames:v", "1", png.to_str().unwrap()]);
    let mut o = Opts::default();
    for to in crate::imgnative::WRITE {
        let out = d.join(format!("out.{to}"));
        crate::imgnative::convert_file(&o, &png, &out, to).unwrap_or_else(|e| panic!("png→{to}: {e}"));
        let img = image::open(&out).unwrap_or_else(|e| panic!("{to} se nedá přečíst: {e}"));
        // ICO má limit 256 px na stranu
        let want = if *to == "ico" { (256, 160) } else { (320, 200) };
        assert_eq!((img.width(), img.height()), want, "{to}");
    }
    // zmenšení na polovinu a omezení největší strany
    o.resize_pct = 50;
    let out = d.join("half.jpg");
    crate::imgnative::convert_file(&o, &png, &out, "jpg").unwrap();
    assert_eq!(image::open(&out).unwrap().width(), 160);
    o.resize_pct = 0;
    o.max_w = 100;
    let out = d.join("max.png");
    crate::imgnative::convert_file(&o, &png, &out, "png").unwrap();
    let m = image::open(&out).unwrap();
    assert_eq!((m.width(), m.height()), (100, 63));
    // formát, který neumíme, dá srozumitelnou chybu
    assert!(crate::imgnative::convert_file(&Opts::default(), &png, &d.join("x.heic"), "heic").is_err());
    // celá cesta přes frontu bez magicku
    let (c, si) = ctx();
    let mut tools = (*c.tools).clone();
    tools.magick = None;
    let c2 = JobCtx { shared: Shared::new(), si: si.clone(), tools: Arc::new(tools) };
    let r = job(&c2, &si, &png, "jpg", Opts::default()).unwrap();
    assert!(r.outputs[0].exists());
}

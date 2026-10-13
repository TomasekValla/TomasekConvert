//! Stopy v kontejneru: remux bez překódování (stream copy) a extrakce jedné stopy.
use crate::{ffmpeg, job::*, probe::*, stats::Engine, util::*};
use std::path::PathBuf;

pub struct RemuxTask {
    pub input: PathBuf,
    pub probe: Probe,
    pub out_ext: String,
    pub keep: Vec<usize>,
    pub keep_chapters: bool,
    pub keep_meta: bool,
    pub out_dir: PathBuf,
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Proč se stopa do vybraného kontejneru nedostane.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Skip {
    /// kontejner tenhle druh stopy vůbec neumí (např. přílohy mimo MKV)
    Container,
    /// kontejner druh stopy umí, ale ne tenhle kodek (např. TrueHD v MP4)
    Codec,
    /// kontejner (nebo přehrávače) počítá jen s jedním videem
    ExtraVideo,
}

impl Skip {
    pub fn label(self, st: &Stream, ext: &str) -> String {
        match self {
            Skip::Container => trf!("nejde do .{}", "not in .{}", ext),
            Skip::Codec => trf!("kodek {} nejde do .{}", "codec {} not allowed in .{}", st.codec, ext),
            Skip::ExtraVideo => trf!("v .{} jen 1 video – použij MKV", "only 1 video track in .{} – use MKV", ext),
        }
    }
}

/// Kontejnery, kde druhá obrazová stopa nedává smysl: formálně jde zapsat, ale přehrávače (Windows, macOS/iOS, televize,
/// prohlížeče) přehrají jen první a některé soubor odmítnou. Víc videí patří do MKV.
fn single_video_only(ext: &str) -> bool {
    matches!(ext, "mp4" | "m4v" | "mov" | "webm" | "ts")
}

/// Povolené kodeky v kontejneru (jen to, co `-c copy` opravdu zapíše a běžné přehrávače přehrají).
fn codec_ok(st: &Stream, ext: &str) -> bool {
    let c = st.codec.as_str();
    match (st.kind, ext) {
        (_, "mkv" | "mka" | "mks") => true,
        (StreamKind::Video, "mp4" | "m4v") => matches!(c, "h264" | "hevc" | "av1" | "mpeg4" | "mpeg2video" | "mpeg1video" | "vp9" | "h263" | "mjpeg" | "png" | "jpeg2000"),
        (StreamKind::Video, "mov") => matches!(c, "h264" | "hevc" | "av1" | "mpeg4" | "mpeg2video" | "mpeg1video" | "vp9" | "h263" | "mjpeg" | "png" | "jpeg2000" | "prores" | "dnxhd" | "qtrle" | "rawvideo" | "dvvideo" | "cfhd"),
        (StreamKind::Audio, "mp4" | "m4v") => matches!(c, "aac" | "mp3" | "mp2" | "ac3" | "eac3" | "alac" | "opus" | "flac" | "dts" | "amr_nb" | "amr_wb"),
        (StreamKind::Audio, "mov") => matches!(c, "aac" | "mp3" | "mp2" | "ac3" | "eac3" | "alac" | "opus" | "flac" | "dts" | "amr_nb" | "amr_wb" | "qdm2") || c.starts_with("pcm_") || c.starts_with("adpcm_ima_qt"),
        (StreamKind::Video, "webm") => matches!(c, "vp8" | "vp9" | "av1"),
        (StreamKind::Audio, "webm") => matches!(c, "opus" | "vorbis"),
        (StreamKind::Video, "ts") => matches!(c, "h264" | "hevc" | "mpeg2video" | "mpeg1video" | "mpeg4" | "vc1"),
        (StreamKind::Audio, "ts") => matches!(c, "aac" | "aac_latm" | "mp3" | "mp2" | "ac3" | "eac3" | "dts" | "opus"),
        _ => true,
    }
}

/// Umí daný kontejner tenhle DRUH stopy (bez překódování)?
fn container_ok(st: &Stream, ext: &str) -> bool {
    let mkv = matches!(ext, "mkv" | "mka" | "mks");
    let mp4 = matches!(ext, "mp4" | "mov" | "m4v");
    match st.kind {
        StreamKind::Attachment | StreamKind::Data => mkv,
        StreamKind::Subtitle => !((mp4 && !st.is_text_sub()) || matches!(ext, "avi" | "flv" | "wmv" | "webm" | "ts" | "mka" | "mp3")),
        StreamKind::Video => !matches!(ext, "mka" | "mks") && !(st.attached_pic && matches!(ext, "webm" | "ts")),
        StreamKind::Audio => !matches!(ext, "mks"),
    }
}

/// Stav každé stopy (v pořadí `streams`) pro daný výstupní kontejner: `None` = půjde tam, `Some(důvod)` = vynechá se.
/// `selected` říká, které stopy uživatel zaškrtl – "jen jedno video" se počítá jen z nich (první vybrané vyhrává).
pub fn plan(streams: &[Stream], selected: &dyn Fn(usize) -> bool, ext: &str) -> Vec<Option<Skip>> {
    let single = single_video_only(ext);
    let (mut videos, mut covers) = (0, 0);
    streams
        .iter()
        .map(|st| {
            if !container_ok(st, ext) {
                return Some(Skip::Container);
            }
            if !codec_ok(st, ext) {
                return Some(Skip::Codec);
            }
            if single && st.kind == StreamKind::Video && selected(st.index) {
                let n = if st.attached_pic { &mut covers } else { &mut videos };
                *n += 1;
                if *n > 1 {
                    return Some(Skip::ExtraVideo);
                }
            }
            None
        })
        .collect()
}

pub fn remux(ctx: &JobCtx, t: RemuxTask) -> Res<Outcome> {
    let ext = t.out_ext.as_str();
    let out = unique_path(t.out_dir.join(format!("{} ({}).{ext}", stem_of(&t.input), tr!("upraveno", "edited"))));
    let part = part_path(&out);
    let mp4 = matches!(ext, "mp4" | "mov" | "m4v");
    let mut args = vec![s("-i"), t.input.to_string_lossy().to_string()];
    let mut skipped = 0;
    let mut kept_subs_text = true;
    let mut any_sub = false;
    let mut mapped = 0;
    let st_plan = plan(&t.probe.streams, &|i| t.keep.contains(&i), ext);
    // výstupní pořadí stop podle druhu (kvůli příznaku "výchozí")
    let (mut n_a, mut n_v) = (0usize, 0usize);
    let mut audio_defaults: Vec<bool> = Vec::new();
    let mut video_defaults: Vec<bool> = Vec::new();
    for idx in &t.keep {
        let Some(pos) = t.probe.streams.iter().position(|x| x.index == *idx) else { continue };
        let st = &t.probe.streams[pos];
        // co kontejner neumí, přeskočíme (UI na to upozorňuje)
        if st_plan[pos].is_some() {
            skipped += 1;
            continue;
        }
        if st.kind == StreamKind::Subtitle {
            any_sub = true;
            kept_subs_text &= st.is_text_sub();
        }
        match st.kind {
            StreamKind::Audio => {
                n_a += 1;
                audio_defaults.push(st.default);
            }
            StreamKind::Video if !st.attached_pic => {
                n_v += 1;
                video_defaults.push(st.default);
            }
            _ => {}
        }
        args.extend([s("-map"), format!("0:{idx}")]);
        mapped += 1;
    }
    if mapped == 0 {
        return Err(tr!("Nevybrána žádná použitelná stopa", "No usable track selected").into());
    }
    args.extend([s("-c"), s("copy")]);
    // MKV si příznaky drží, jak jsou. V ostatních kontejnerech musí být právě jedna výchozí zvuková (a obrazová)
    // stopa – jinak si přehrávač vybere podle sebe a třeba spustí komentář místo hlavní řeči.
    if !matches!(ext, "mkv" | "mka" | "mks") {
        for (kind, defs, n) in [("a", &audio_defaults, n_a), ("v", &video_defaults, n_v)] {
            if n > 1 {
                let first = defs.iter().position(|d| *d).unwrap_or(0);
                for i in 0..n {
                    args.extend([format!("-disposition:{kind}:{i}"), s(if i == first { "default" } else { "0" })]);
                }
            }
        }
    }
    if mp4 && any_sub && kept_subs_text {
        args.extend([s("-c:s"), s("mov_text")]);
    }
    args.extend([s("-map_chapters"), s(if t.keep_chapters { "0" } else { "-1" })]);
    args.extend([s("-map_metadata"), s(if t.keep_meta { "0" } else { "-1" })]);
    if mp4 {
        args.extend([s("-movflags"), s("+faststart")]);
    }
    args.extend([s("-ignore_unknown")]);
    args.push(part.to_string_lossy().to_string());
    let total = Some(t.probe.duration).filter(|d| *d > 0.0);
    let r = ffmpeg::run_plain(ctx, {
        let mut a = args;
        a.shrink_to_fit();
        a
    }, total);
    if let Err(e) = r {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    finish_part(&part, &out)?;
    let detail = if skipped > 0 { trf!("bez překódování · {skipped} stop vynecháno (nejdou do {})", "no re-encoding · {skipped} tracks skipped (not allowed in {})", ext.to_uppercase()) } else { tr!("bez překódování (stream copy)", "no re-encoding (stream copy)").into() };
    Ok(Outcome { outputs: vec![out.clone()], bytes_in: path_size(&t.input), bytes_out: path_size(&out), engine: Engine::Cpu, detail, media_secs: t.probe.duration })
}

pub struct ExtractTask {
    pub input: PathBuf,
    pub stream: Stream,
    pub duration: f64,
    pub subs_to_srt: bool,
    pub out_dir: PathBuf,
}

/// Přípona pro surovou stopu podle kodeku.
pub fn track_ext(st: &Stream, to_srt: bool) -> &'static str {
    match st.kind {
        StreamKind::Video => "mkv",
        StreamKind::Audio => match st.codec.as_str() {
            "aac" | "alac" => "m4a",
            "mp3" => "mp3",
            "ac3" => "ac3",
            "eac3" => "eac3",
            "dts" => "dts",
            "flac" => "flac",
            "opus" => "opus",
            "vorbis" => "ogg",
            "truehd" => "thd",
            "mp2" => "mp2",
            "wmav2" | "wmav1" => "wma",
            "amr_nb" => "amr",
            c if c.starts_with("pcm_") => "wav",
            _ => "mka",
        },
        StreamKind::Subtitle => {
            if to_srt && st.is_text_sub() {
                return "srt";
            }
            match st.codec.as_str() {
                "subrip" | "srt" | "mov_text" => "srt",
                "ass" => "ass",
                "ssa" => "ssa",
                "webvtt" => "vtt",
                "hdmv_pgs_subtitle" => "sup",
                _ => "mks",
            }
        }
        StreamKind::Attachment => "bin",
        StreamKind::Data => "bin",
    }
}

pub fn extract_track(ctx: &JobCtx, t: ExtractTask) -> Res<Outcome> {
    let st = &t.stream;
    let stem = stem_of(&t.input);
    let lang = if st.lang.is_empty() { String::new() } else { format!(".{}", st.lang) };
    if st.kind == StreamKind::Attachment {
        let name = if st.filename.is_empty() { format!("{stem}.attachment{}", st.index) } else { st.filename.clone() };
        let out = unique_path(t.out_dir.join(name));
        let args = vec![
            format!("-dump_attachment:{}", st.index),
            out.to_string_lossy().to_string(),
            s("-i"),
            t.input.to_string_lossy().to_string(),
            s("-t"),
            s("0"),
            s("-f"),
            s("null"),
            s("-"),
        ];
        // ffmpeg vrací chybu i při úspěšném dumpu bez výstupu -> kontrolujeme existenci souboru
        let _ = ffmpeg::run_plain(ctx, args, None);
        if !out.exists() {
            return Err(tr!("Přílohu se nepodařilo vytáhnout", "Could not extract the attachment").into());
        }
        return Ok(Outcome::simple(vec![out.clone()], path_size(&t.input), path_size(&out), tr!("příloha", "attachment")));
    }
    let ext = track_ext(st, t.subs_to_srt);
    let tag = match st.kind {
        StreamKind::Video => "video",
        StreamKind::Audio => "audio",
        _ => "titulky",
    };
    let out = unique_path(t.out_dir.join(format!("{stem}.{tag}{}{lang}.{ext}", st.index)));
    let part = part_path(&out);
    let mut args = vec![s("-i"), t.input.to_string_lossy().to_string(), s("-map"), format!("0:{}", st.index)];
    if st.kind == StreamKind::Subtitle && ext == "srt" && st.codec != "subrip" {
        args.extend([s("-c:s"), s("srt")]);
    } else {
        args.extend([s("-c"), s("copy")]);
    }
    args.extend([s("-map_metadata"), s("-1")]);
    args.push(part.to_string_lossy().to_string());
    let total = if st.kind == StreamKind::Subtitle { None } else { Some(t.duration).filter(|d| *d > 0.0) };
    if let Err(e) = ffmpeg::run_plain(ctx, args, total) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    finish_part(&part, &out)?;
    Ok(Outcome { outputs: vec![out.clone()], bytes_in: path_size(&t.input), bytes_out: path_size(&out), engine: Engine::Cpu, detail: trf!("stopa #{} · {}", "track #{} · {}", st.index, st.headline()), media_secs: t.duration })
}

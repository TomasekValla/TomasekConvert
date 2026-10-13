//! Video / audio / animace / titulky přes ffmpeg (s AMD AMF a D3D11VA).
use crate::{
    formats::Class,
    gpu::{self, Backend},
    hw::SysInfo,
    job::*,
    opts::*,
    probe::{self, Probe, StreamKind},
    stats::Engine,
    util::*,
};
use std::path::Path;

pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for part in s.split(':') {
        total = total * 60.0 + part.trim().replace(',', ".").parse::<f64>().ok()?;
    }
    Some(total)
}

fn esc_filter_path(p: &str) -> String {
    p.replace('\\', "/").replace(':', "\\:").replace('\'', "\\'")
}

/// Rozdělí řetězec argumentů jako shell (podporuje "uvozovky" a 'apostrofy').
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                has = true;
            }
            None if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// FPS jako zlomek pro "televizní" hodnoty (23.976 -> 24000/1001).
pub fn fps_arg(f: f32) -> String {
    for (v, r) in [(23.976, "24000/1001"), (29.97, "30000/1001"), (47.952, "48000/1001"), (59.94, "60000/1001"), (119.88, "120000/1001")] {
        if (f - v).abs() < 0.0051 {
            return r.to_string();
        }
    }
    format!("{f}")
}

fn scale_filter(o: &Opts) -> Option<String> {
    let (w, h) = (o.width, o.height);
    if w == 0 && h == 0 {
        return None;
    }
    let algo = if o.scale_algo.is_empty() { "lanczos" } else { o.scale_algo.as_str() };
    Some(if w > 0 && h > 0 {
        if o.keep_aspect {
            format!("scale={w}:{h}:force_original_aspect_ratio=decrease:flags={algo},pad={w}:{h}:(ow-iw)/2:(oh-ih)/2")
        } else {
            format!("scale={w}:{h}:flags={algo}")
        }
    } else if w > 0 {
        format!("scale={w}:-2:flags={algo}")
    } else {
        format!("scale=-2:{h}:flags={algo}")
    })
}

pub struct Plan {
    pub pre: Vec<String>,
    pub post: Vec<String>,
    pub engine: Engine,
    pub detail: String,
}

fn s(v: &str) -> String {
    v.to_string()
}

fn trim_args(o: &Opts) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(ss) = parse_time(&o.trim_start) {
        v.push(s("-ss"));
        v.push(format!("{ss}"));
    }
    if let Some(to) = parse_time(&o.trim_end) {
        v.push(s("-to"));
        v.push(format!("{to}"));
    }
    v
}

fn total_secs(o: &Opts, p: &Option<Probe>) -> Option<f64> {
    let dur = p.as_ref().map(|p| p.duration).filter(|d| *d > 0.0)?;
    let ss = parse_time(&o.trim_start).unwrap_or(0.0);
    let to = parse_time(&o.trim_end).unwrap_or(dur).min(dur);
    Some((to - ss).max(0.1))
}

fn audio_filters(o: &Opts) -> Option<String> {
    let mut f = Vec::new();
    if o.volume_db.abs() > 0.01 {
        f.push(format!("volume={}dB", o.volume_db));
    }
    if o.normalize {
        f.push(s("loudnorm=I=-16:TP=-1.5:LRA=11"));
    }
    if f.is_empty() {
        None
    } else {
        Some(f.join(","))
    }
}

pub fn default_kbps(codec: &str, channels: u32) -> u32 {
    let ch = channels.max(2);
    match codec {
        "mp3" => 192,
        "aac" => 64 * ch.min(6),
        "opus" => 64 * ch.min(6),
        "vorbis" => 160,
        "ac3" => 96 * ch.min(6),
        "eac3" => 96 * ch.min(6),
        "mp2" => 192,
        "wma" => 192,
        "speex" => 32,
        _ => 192,
    }
}

fn pcm_encoder(o: &Opts, id: &str, ext: &str) -> String {
    // AIFF / AU / CAF-BE chtějí big-endian PCM, WAV little-endian
    let be = matches!(ext, "aiff" | "aif" | "aifc" | "au" | "snd");
    let e = if be { "be" } else { "le" };
    if o.afloat {
        return format!("pcm_f32{e}");
    }
    let bits = if o.abits > 0 {
        o.abits
    } else {
        match id {
            "pcm24" => 24,
            "pcm32" => 32,
            _ => 16,
        }
    };
    match bits {
        8 => if be { "pcm_s8".into() } else { "pcm_u8".into() },
        24 => format!("pcm_s24{e}"),
        32 => format!("pcm_s32{e}"),
        _ => format!("pcm_s16{e}"),
    }
}

fn encode_audio_args(o: &Opts, codec_id: &str, in_channels: u32, ext: &str) -> Vec<String> {
    let mut a = Vec::new();
    let Some(c) = acodec(codec_id) else {
        a.extend([s("-c:a"), s("aac")]);
        return a;
    };
    a.push(s("-c:a"));
    if codec_id.starts_with("pcm") {
        a.push(pcm_encoder(o, codec_id, ext));
    } else {
        a.push(s(c.enc));
    }
    if !c.lossless {
        let ch = if o.channels > 0 { o.channels } else { in_channels };
        let k = if o.akbps > 0 { o.akbps } else { default_kbps(codec_id, ch) };
        if codec_id == "amr" {
            a.extend([s("-ar"), s("8000"), s("-ac"), s("1"), s("-b:a"), s("12200")]);
            return a;
        }
        a.extend([s("-b:a"), format!("{k}k")]);
        if codec_id == "opus" {
            a.extend([s("-vbr"), s("on")]);
        }
    } else if o.abits > 0 || o.afloat {
        // bitová hloubka u bezeztrátových kodeků
        match codec_id {
            "flac" => {
                a.extend([s("-sample_fmt"), s(if o.abits <= 16 { "s16" } else { "s32" })]);
                if o.abits == 24 {
                    a.extend([s("-bits_per_raw_sample"), s("24")]);
                }
            }
            "alac" => {
                a.extend([s("-sample_fmt"), s(if o.abits <= 16 { "s16p" } else { "s32p" })]);
                if o.abits == 24 {
                    a.extend([s("-bits_per_raw_sample"), s("24")]);
                }
            }
            "wavpack" => a.extend([s("-sample_fmt"), s(if o.afloat { "fltp" } else if o.abits <= 16 { "s16" } else { "s32" })]),
            _ => {}
        }
    }
    if codec_id == "flac" {
        a.extend([s("-compression_level"), format!("{}", o.flac_level.min(12))]);
    }
    if o.sample_rate > 0 {
        a.extend([s("-ar"), format!("{}", o.sample_rate)]);
    }
    if o.channels > 0 {
        a.extend([s("-ac"), format!("{}", o.channels)]);
    }
    if let Some(f) = audio_filters(o) {
        a.extend([s("-af"), f]);
    }
    a
}

fn build_video(si: &SysInfo, o: &Opts, p: &Option<Probe>, ext: &str, gpu: Option<(Backend, &'static str)>, dec: Option<Vec<String>>) -> Plan {
    let mut pre = trim_args(o);
    let mut post: Vec<String> = Vec::new();
    let codec_id = if o.vcodec == "auto" { default_vcodec(ext) } else { o.vcodec.as_str() };
    let vc = vcodec(codec_id).unwrap_or(&VCODECS[0]);
    let src = p.as_ref().and_then(|p| p.video());
    let src_10bit = src.map(|v| v.bit_depth > 8).unwrap_or(false);
    let mut detail: Vec<String> = vec![vc.label.to_string()];
    let mut engine = Engine::Cpu;

    let avx = o.avx512 && si.avx512 && gpu.is_none() && !o.copy_video;
    if avx && si.ffmpeg_avx512_flag {
        // vlastní ffmpeg (filtry, škálování) – jen když to ten build přijímá
        pre.extend([s("-cpuflags"), s("+avx512")]);
    }
    if let Some(d) = &dec {
        pre.extend(d.clone());
    }
    if let Some((Backend::Vaapi, _)) = gpu {
        if let Some(dev) = gpu::vaapi_device() {
            pre.extend([s("-vaapi_device"), dev]);
        }
    }

    // ── mapování stop ──
    post.extend([s("-map"), s("0:v:0?")]);
    if o.audio_track >= 0 {
        post.extend([s("-map"), format!("0:a:{}?", o.audio_track)]);
    } else {
        post.extend([s("-map"), s("0:a?")]);
    }
    let subs: Vec<_> = p.as_ref().map(|p| p.streams.iter().filter(|x| x.kind == StreamKind::Subtitle).collect()).unwrap_or_default();
    let mut sub_codec: Option<&str> = None;
    if o.keep_subs && !subs.is_empty() {
        match ext {
            "mkv" => sub_codec = Some("copy"),
            "mp4" | "m4v" | "mov" if subs.iter().all(|x| x.is_text_sub()) => sub_codec = Some("mov_text"),
            _ => {}
        }
    }
    if let Some(sc) = sub_codec {
        post.extend([s("-map"), s("0:s?"), s("-c:s"), s(sc)]);
        if ext == "mkv" {
            post.extend([s("-map"), s("0:t?"), s("-c:t"), s("copy")]);
        }
    }
    if o.strip_meta {
        post.extend([s("-map_metadata"), s("-1"), s("-map_chapters"), s("-1")]);
    }

    // ── video ──
    if o.copy_video {
        post.extend([s("-c:v"), s("copy")]);
        detail = vec![tr!("video bez překódování", "video stream copy").to_string()];
    } else {
        let want_pix = o.pix_fmt.trim().to_string();
        let auto_ten = want_pix.is_empty() && src_10bit && matches!(vc.id, "hevc" | "av1" | "prores");
        let ten = auto_ten || want_pix.contains("10le") || want_pix.contains("p010");
        let kbps = o.vkbps.max(50);
        let mut vf: Vec<String> = Vec::new();
        if let Some(f) = scale_filter(o) {
            vf.push(f);
        } else if let Some(v) = src {
            if v.width % 2 == 1 || v.height % 2 == 1 {
                vf.push(s("scale=trunc(iw/2)*2:trunc(ih/2)*2"));
            }
        }
        if o.fps > 0.0 {
            vf.push(format!("fps={}", fps_arg(o.fps)));
        }
        if !o.burn_sub.trim().is_empty() {
            vf.push(format!("subtitles='{}'", esc_filter_path(o.burn_sub.trim())));
        }
        if let Some((Backend::Vaapi, _)) = gpu {
            vf.push(format!("format={},hwupload", if ten { "p010" } else { "nv12" }));
        }
        if !vf.is_empty() {
            post.extend([s("-vf"), vf.join(",")]);
        }
        match gpu {
            Some((b, enc)) => {
                engine = Engine::Gpu;
                post.extend([s("-c:v"), s(enc)]);
                let (lo, hi, def) = b.qp_range(vc.id);
                let (pl, pq) = match o.preset {
                    Preset::Quality => ("Quality", 0),
                    Preset::Balanced => ("Balanced", 1),
                    Preset::Speed => ("Speed", 2),
                };
                match b {
                    Backend::Amf => post.extend([s("-usage"), s("transcoding"), s("-quality"), s(["quality", "balanced", "speed"][pq])]),
                    Backend::Nvenc => post.extend([s("-preset"), s(["p7", "p5", "p2"][pq]), s("-tune"), s("hq")]),
                    Backend::Qsv => post.extend([s("-preset"), s(["veryslow", "medium", "veryfast"][pq])]),
                    Backend::Vaapi | Backend::VideoToolbox => {}
                }
                detail.push(if matches!(b, Backend::Vaapi | Backend::VideoToolbox) { b.short().to_string() } else { format!("{} {pl}", b.short()) });
                match o.rc {
                    Rc::Cqp => {
                        let qp = if o.qp == 0 { def } else { o.qp.clamp(lo, hi) };
                        let q = qp.to_string();
                        match b {
                            Backend::Amf => {
                                post.extend([s("-rc"), s("cqp"), s("-qp_i"), q.clone(), s("-qp_p"), q.clone()]);
                                if vc.id == "h264" {
                                    post.extend([s("-qp_b"), q.clone()]);
                                }
                            }
                            Backend::Nvenc => post.extend([s("-rc"), s("constqp"), s("-qp"), q.clone()]),
                            Backend::Qsv => post.extend([s("-global_quality"), q.clone()]),
                            Backend::Vaapi => post.extend([s("-rc_mode"), s("CQP"), s(if matches!(vc.id, "h264" | "hevc") { "-qp" } else { "-global_quality" }), q.clone()]),
                            Backend::VideoToolbox => post.extend([s("-q:v"), q.clone()]),
                        }
                        detail.push(format!("CQP {qp}"));
                    }
                    Rc::Vbr => {
                        let mx = if o.maxrate > 0 { o.maxrate } else { kbps * 3 / 2 };
                        let (br, mr) = (format!("{kbps}k"), format!("{mx}k"));
                        match b {
                            Backend::Amf => post.extend([s("-rc"), s("vbr_peak"), s("-b:v"), br, s("-maxrate"), mr]),
                            Backend::Nvenc => post.extend([s("-rc"), s("vbr"), s("-b:v"), br, s("-maxrate"), mr]),
                            Backend::Qsv => post.extend([s("-b:v"), br, s("-maxrate"), mr]),
                            Backend::Vaapi => post.extend([s("-rc_mode"), s("VBR"), s("-b:v"), br, s("-maxrate"), mr]),
                            Backend::VideoToolbox => post.extend([s("-b:v"), br]),
                        }
                        detail.push(format!("VBR {} Mb/s", kbps as f32 / 1000.0));
                    }
                    Rc::Cbr => {
                        let br = format!("{kbps}k");
                        match b {
                            Backend::Amf => post.extend([s("-rc"), s("cbr"), s("-b:v"), br.clone(), s("-minrate"), br.clone(), s("-maxrate"), br]),
                            Backend::Nvenc => post.extend([s("-rc"), s("cbr"), s("-b:v"), br.clone(), s("-maxrate"), br, s("-bufsize"), format!("{}k", kbps * 2)]),
                            Backend::Qsv => post.extend([s("-b:v"), br.clone(), s("-maxrate"), br]),
                            Backend::Vaapi => post.extend([s("-rc_mode"), s("CBR"), s("-b:v"), br.clone(), s("-maxrate"), br]),
                            Backend::VideoToolbox => post.extend([s("-b:v"), br]),
                        }
                        detail.push(format!("CBR {} Mb/s", kbps as f32 / 1000.0));
                    }
                }
                if ten {
                    if b != Backend::Vaapi {
                        post.extend([s("-pix_fmt"), s("p010le")]);
                    }
                    if vc.id == "hevc" && o.vprofile.trim().is_empty() {
                        post.extend([s("-profile:v"), s("main10")]);
                    }
                    detail.push(s("10-bit"));
                } else if !want_pix.is_empty() && b != Backend::Vaapi {
                    post.extend([s("-pix_fmt"), want_pix.clone()]);
                }
            }
            None => {
                post.extend([s("-c:v"), s(vc.cpu)]);
                let crf = match vc.crf {
                    Some((lo, hi, d)) => {
                        if o.crf == 0 {
                            d
                        } else {
                            o.crf.clamp(lo, hi)
                        }
                    }
                    None => {
                        if o.crf == 0 {
                            4
                        } else {
                            o.crf.clamp(2, 31)
                        }
                    }
                };
                let bitrate_mode = o.rc != Rc::Cqp;
                let xp = if o.x_preset.trim().is_empty() { "medium".to_string() } else { o.x_preset.trim().to_string() };
                let svt = o.svt_preset.min(13).to_string();
                let cpu_used = o.vp9_speed.min(8).to_string();
                match vc.id {
                    "h264" | "hevc" => {
                        post.extend([s("-preset"), xp.clone()]);
                        if !o.x_tune.trim().is_empty() {
                            post.extend([s("-tune"), o.x_tune.trim().to_string()]);
                        }
                        if bitrate_mode {
                            post.extend([s("-b:v"), format!("{kbps}k")]);
                            if o.rc == Rc::Cbr || o.maxrate > 0 {
                                let mx = if o.maxrate > 0 { o.maxrate } else { kbps };
                                let bs = if o.bufsize > 0 { o.bufsize } else { mx * 2 };
                                post.extend([s("-maxrate"), format!("{mx}k"), s("-bufsize"), format!("{bs}k")]);
                            }
                        } else {
                            post.extend([s("-crf"), crf.to_string()]);
                        }
                        if !want_pix.is_empty() {
                            post.extend([s("-pix_fmt"), want_pix.clone()]);
                            if ten {
                                detail.push(s("10-bit"));
                            }
                        } else if auto_ten {
                            post.extend([s("-pix_fmt"), s("yuv420p10le")]);
                            detail.push(s("10-bit"));
                        } else if !src_10bit {
                            post.extend([s("-pix_fmt"), s("yuv420p")]);
                        }
                        if vc.id == "hevc" {
                            let mut xp265 = vec![s("log-level=error")];
                            if avx {
                                xp265.push(s("asm=avx512")); // x265 ho bez toho nepoužívá
                            }
                            if o.bframes >= 0 {
                                xp265.push(format!("bframes={}", o.bframes));
                            }
                            post.extend([s("-x265-params"), xp265.join(":")]);
                        }
                    }
                    "av1" => {
                        post.extend([s("-preset"), svt.clone()]);
                        if avx {
                            post.extend([s("-svtav1-params"), s("asm=max")]); // výchozí je jen AVX2
                        }
                        if bitrate_mode {
                            post.extend([s("-b:v"), format!("{kbps}k")]);
                        } else {
                            post.extend([s("-crf"), crf.to_string()]);
                        }
                        let pf = if !want_pix.is_empty() { want_pix.clone() } else if ten { s("yuv420p10le") } else { s("yuv420p") };
                        post.extend([s("-pix_fmt"), pf]);
                    }
                    "vp9" => {
                        post.extend([s("-row-mt"), s("1"), s("-cpu-used"), cpu_used.clone(), s("-deadline"), s("good")]);
                        if bitrate_mode {
                            post.extend([s("-b:v"), format!("{kbps}k")]);
                        } else {
                            post.extend([s("-crf"), crf.to_string(), s("-b:v"), s("0")]);
                        }
                    }
                    "vp8" => {
                        post.extend([s("-b:v"), format!("{}k", if bitrate_mode { kbps } else { 4000 }), s("-crf"), crf.to_string()]);
                    }
                    "vvc" => {
                        post.extend([s("-qp"), crf.to_string(), s("-preset"), s("medium")]);
                    }
                    "prores" => {
                        post.extend([s("-profile:v"), s("3"), s("-vendor"), s("apl0"), s("-pix_fmt"), s("yuv422p10le")]);
                    }
                    "dnxhd" => {
                        post.extend([s("-profile:v"), s("dnxhr_hq"), s("-pix_fmt"), s("yuv422p")]);
                    }
                    "ffv1" => {
                        post.extend([s("-level"), s("3"), s("-coder"), s("1"), s("-context"), s("1"), s("-slicecrc"), s("1")]);
                    }
                    "utvideo" | "huffyuv" => {}
                    "wmv1" | "wmv2" | "h263" | "flv1" => {
                        post.extend([s("-b:v"), format!("{kbps}k")]);
                    }
                    _ => {
                        if bitrate_mode {
                            post.extend([s("-b:v"), format!("{kbps}k")]);
                        } else {
                            post.extend([s("-q:v"), crf.to_string()]);
                        }
                    }
                }
                if vc.crf.is_some() && !bitrate_mode {
                    detail.push(format!("CRF {crf}"));
                } else if bitrate_mode {
                    detail.push(format!("{} Mb/s", kbps as f32 / 1000.0));
                }
                detail.push(if avx && matches!(vc.id, "hevc" | "av1") { "CPU · AVX-512".to_string() } else { s("CPU") });
            }
        }
        // ── společné vlastní hodnoty ──
        let amf_now = gpu.is_some();
        if !want_pix.is_empty() && !matches!(vc.id, "h264" | "hevc" | "av1") {
            post.extend([s("-pix_fmt"), want_pix.clone()]);
        }
        if o.gop > 0 {
            post.extend([s("-g"), o.gop.to_string()]);
        }
        if o.bframes >= 0 && !amf_now && vc.id != "hevc" {
            post.extend([s("-bf"), o.bframes.to_string()]);
        }
        if !o.vprofile.trim().is_empty() {
            post.extend([s("-profile:v"), o.vprofile.trim().to_string()]);
        }
        if !o.vlevel.trim().is_empty() {
            post.extend([s("-level"), o.vlevel.trim().to_string()]);
        }
        if dec.is_some() {
            engine = if engine == Engine::Gpu { Engine::Gpu } else { Engine::Mixed };
            detail.push(trf!("HW dekódování ({})", "HW decoding ({})", si.hw_decoder().unwrap_or("?")));
        }
    }
    if vc.id == "hevc" && matches!(ext, "mp4" | "mov" | "m4v") && !o.copy_video {
        post.extend([s("-tag:v"), s("hvc1")]);
    }

    // ── audio ──
    let in_a = p.as_ref().and_then(|p| {
        if o.audio_track >= 0 {
            p.audio_streams().nth(o.audio_track as usize)
        } else {
            p.audio_streams().next()
        }
    });
    if let Some(a) = in_a {
        let allowed = video_container_audio(ext);
        let untouched = o.acodec == "auto"
            && !o.normalize
            && o.volume_db.abs() < 0.01
            && o.sample_rate == 0
            && o.channels == 0
            && o.akbps == 0;
        if untouched && (allowed.is_empty() || allowed.contains(&a.codec.as_str())) {
            post.extend([s("-c:a"), s("copy")]);
        } else {
            let id = if o.acodec == "auto" { video_container_default_audio(ext) } else { o.acodec.as_str() };
            post.extend(encode_audio_args(o, id, a.channels, ext));
        }
    } else {
        post.push(s("-an"));
    }
    if matches!(ext, "mp4" | "mov" | "m4v") {
        post.extend([s("-movflags"), s("+faststart")]);
    }
    Plan { pre, post, engine, detail: detail.join(" · ") }
}

/// Který GPU enkodér se pro tenhle převod použije (None = CPU).
pub fn pick_gpu(si: &SysInfo, o: &Opts, ext: &str) -> Option<(Backend, &'static str)> {
    if o.hw != HwMode::Auto || o.copy_video {
        return None;
    }
    let codec_id = if o.vcodec == "auto" { default_vcodec(ext) } else { o.vcodec.as_str() };
    let want = o.pix_fmt.trim();
    let order: Vec<Backend> = match Backend::from_id(&o.backend) {
        Some(b) => vec![b],
        None => gpu::ALL.to_vec(),
    };
    order.into_iter().find_map(|b| b.encoder(codec_id).filter(|e| si.has_hw_enc(b, e) && b.pix_ok(want, codec_id)).map(|e| (b, e)))
}

/// Seznam pokusů: nejdřív co nejvíc na GPU, pak postupně bezpečnější varianty.
fn video_plans(si: &SysInfo, o: &Opts, p: &Option<Probe>, ext: &str) -> Vec<Plan> {
    let gpu = pick_gpu(si, o, ext);
    let hw_on = o.hw == HwMode::Auto && !o.copy_video;
    let dec_ok = p
        .as_ref()
        .and_then(|p| p.video())
        .map(|v| matches!(v.codec.as_str(), "h264" | "hevc" | "vp9" | "av1") && matches!(v.pix_fmt.as_str(), "yuv420p" | "yuvj420p" | "yuv420p10le" | "nv12" | "p010le"))
        .unwrap_or(false);
    let dec = if hw_on && o.hw_decode && dec_ok { si.hw_decode_args() } else { None };
    let mut modes: Vec<(Option<(Backend, &'static str)>, Option<Vec<String>>)> = vec![(gpu, dec.clone())];
    if dec.is_some() {
        modes.push((gpu, None));
    }
    if gpu.is_some() {
        modes.push((None, None));
    }
    let mut seen: Vec<(Option<Backend>, bool)> = Vec::new();
    let mut plans = Vec::new();
    for (g, d) in modes {
        let key = (g.map(|x| x.0), d.is_some());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        plans.push(build_video(si, o, p, ext, g, d));
    }
    plans
}

fn build_audio(o: &Opts, p: &Option<Probe>, ext: &str) -> Plan {
    let pre = trim_args(o);
    let mut post: Vec<String> = Vec::new();
    let default_id = default_audio_for_ext(ext);
    let choices = audio_choices(ext);
    let id = if o.acodec != "auto" && choices.contains(&o.acodec.as_str()) { o.acodec.as_str() } else { default_id };
    let track = if o.audio_track >= 0 { o.audio_track } else { 0 };
    post.extend([s("-map"), format!("0:a:{track}")]);
    let cover = p.as_ref().and_then(|p| p.streams.iter().find(|x| x.attached_pic)).map(|x| x.index);
    match cover {
        Some(ci) if matches!(ext, "mp3" | "m4a" | "m4b" | "flac" | "aiff" | "aif" | "mka" | "wv") && !o.strip_meta => {
            post.extend([s("-map"), format!("0:{ci}"), s("-c:v"), s("copy"), s("-disposition:v:0"), s("attached_pic")]);
        }
        _ => post.push(s("-vn")),
    }
    post.extend([s("-sn"), s("-dn")]);
    let in_ch = p.as_ref().and_then(|p| p.audio_streams().next()).map(|a| a.channels).unwrap_or(2);
    post.extend(encode_audio_args(o, id, in_ch, ext));
    match ext {
        "mp3" => post.extend([s("-id3v2_version"), s("3")]),
        "dts" | "mlp" => post.extend([s("-strict"), s("-2")]),
        _ => {}
    }
    if o.strip_meta {
        post.extend([s("-map_metadata"), s("-1")]);
    }
    let label = acodec(id).map(|c| c.label).unwrap_or(id);
    let lossless = acodec(id).map(|c| c.lossless).unwrap_or(false);
    let q = if lossless {
        if id == "flac" { format!(" · komprese {}", o.flac_level) } else { String::new() }
    } else {
        let ch = if o.channels > 0 { o.channels } else { in_ch };
        format!(" · {} kb/s", if o.akbps > 0 { o.akbps } else { default_kbps(id, ch) })
    };
    Plan { pre, post, engine: Engine::Cpu, detail: format!("{label}{q}") }
}

fn build_anim(o: &Opts, ext: &str, class: Class) -> Plan {
    let mut pre = trim_args(o);
    let mut post: Vec<String> = vec![s("-an"), s("-sn")];
    let w = if o.anim_width > 0 { o.anim_width } else { 480 };
    let fps = if o.anim_fps > 0 { o.anim_fps } else { 15 };
    if class == Class::FrameImage {
        pre = vec![s("-ss"), format!("{}", o.frame_time.max(0.0))];
        if let Some(f) = scale_filter(o) {
            post.extend([s("-vf"), f]);
        }
        post.extend([s("-frames:v"), s("1"), s("-update"), s("1")]);
        match ext {
            "jpg" => post.extend([s("-q:v"), format!("{}", (31 - (o.img_quality.clamp(1, 100) * 29 / 100)).clamp(2, 31))]),
            "webp" => post.extend([s("-q:v"), format!("{}", o.img_quality)]),
            _ => {}
        }
        return Plan { pre, post, engine: Engine::Cpu, detail: trf!("snímek v {:.1} s", "frame at {:.1} s", o.frame_time) };
    }
    match ext {
        "gif" => post.extend([
            s("-vf"),
            format!("fps={fps},scale={w}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5"),
            s("-loop"),
            s("0"),
        ]),
        "apng" => post.extend([s("-vf"), format!("fps={fps},scale={w}:-1:flags=lanczos"), s("-plays"), s("0"), s("-f"), s("apng")]),
        _ => post.extend([
            s("-vf"),
            format!("fps={fps},scale={w}:-1:flags=lanczos"),
            s("-c:v"),
            s("libwebp"),
            s("-q:v"),
            format!("{}", o.img_quality),
            s("-loop"),
            s("0"),
        ]),
    }
    Plan { pre, post, engine: Engine::Cpu, detail: format!("{fps} fps · {w} px") }
}

fn run_ffmpeg(ctx: &JobCtx, args: Vec<String>, total: Option<f64>) -> Res<()> {
    let ff = ctx.tools.ffmpeg.as_ref().ok_or(tr!("ffmpeg není k dispozici", "ffmpeg is not available"))?;
    let mut cmd = command(ff);
    cmd.args(["-hide_banner", "-nostats", "-loglevel", "error", "-progress", "pipe:1", "-stats_period", "0.25", "-y"]);
    cmd.args(&args);
    match total {
        Some(_) => ctx.shared.set_frac(0.0),
        None => ctx.shared.set_indeterminate(),
    }
    let shared = ctx.shared.clone();
    run_cmd(ctx, cmd, move |is_err, line| {
        if is_err {
            return;
        }
        if let Some(v) = line.strip_prefix("out_time_us=") {
            if let (Ok(us), Some(t)) = (v.trim().parse::<f64>(), total) {
                shared.set_frac((us / 1e6 / t) as f32);
            }
        } else if line.starts_with("progress=end") && total.is_some() {
            shared.set_frac(1.0);
        }
    })
    .map(|_| ())
}

pub fn convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    let o = &job.opts;
    let ext = job.fmt.ext.as_str();
    let probe = probe::run(&ctx.tools, &job.input).ok();
    let out = unique_path(job.out_dir.join(format!("{}.{}", stem_of(&job.input), ext)));
    let part = part_path(&out);
    let bytes_in = path_size(&job.input);
    let total = total_secs(o, &probe);

    let plans: Vec<Plan> = match job.fmt.class {
        Class::Video => video_plans(&ctx.si, o, &probe, ext),
        Class::Audio => vec![build_audio(o, &probe, ext)],
        Class::AnimImage | Class::FrameImage => vec![build_anim(o, ext, job.fmt.class)],
        Class::Subtitle => vec![Plan { pre: vec![], post: vec![s("-map"), s("0:s:0?")], engine: Engine::Cpu, detail: "titulky".into() }],
        _ => return Err(tr!("Neplatný cíl pro ffmpeg", "Invalid target for ffmpeg").into()),
    };
    // snímek / titulky nemají smysluplný průběh
    let total = if matches!(job.fmt.class, Class::FrameImage | Class::Subtitle) { None } else { total };

    let mut last_err = String::new();
    let n = plans.len();
    for (i, plan) in plans.into_iter().enumerate() {
        if i > 0 {
            ctx.shared.set_note(tr!("GPU varianta selhala, zkouším bezpečnější režim…", "The GPU variant failed, trying a safer mode…"));
        }
        let _ = std::fs::remove_file(&part);
        let mut args: Vec<String> = plan.pre.clone();
        args.push(s("-i"));
        args.push(job.input.to_string_lossy().to_string());
        args.extend(plan.post.clone());
        args.extend(split_args(&o.extra_args));
        args.push(part.to_string_lossy().to_string());
        match run_ffmpeg(ctx, args, total) {
            Ok(()) => {
                finish_part(&part, &out)?;
                let bytes_out = path_size(&out);
                return Ok(Outcome {
                    outputs: vec![out],
                    bytes_in,
                    bytes_out,
                    engine: plan.engine,
                    detail: plan.detail,
                    media_secs: total.unwrap_or(0.0),
                });
            }
            Err(e) if e == CANCELLED => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
            Err(e) => {
                last_err = e;
                let _ = std::fs::remove_file(&part);
                if i + 1 == n {
                    break;
                }
            }
        }
    }
    Err(last_err)
}

/// Obecný ffmpeg běh pro nástroje (remux, extrakce stop...).
pub fn run_plain(ctx: &JobCtx, args: Vec<String>, total: Option<f64>) -> Res<()> {
    run_ffmpeg(ctx, args, total)
}

#[allow(dead_code)]
pub fn exists(p: &Path) -> bool {
    p.exists()
}

#[allow(dead_code)]
fn _use(_: &probe::Stream) {}

#[cfg(test)]
mod gpu_tests {
    use super::*;

    fn fake_si() -> SysInfo {
        let mut s = SysInfo::default();
        for b in gpu::ALL {
            for c in b.codecs() {
                if let Some(e) = b.encoder(c) {
                    s.hw_encoders.push((b, e.to_string()));
                }
            }
        }
        s
    }

    fn only(b: Backend) -> SysInfo {
        let mut s = fake_si();
        s.hw_encoders.retain(|(bb, _)| *bb == b);
        s
    }

    fn plan_args(si: &SysInfo, o: &Opts, ext: &str) -> (Vec<String>, Engine) {
        let plans = video_plans(si, o, &None, ext);
        (plans[0].post.clone(), plans[0].engine)
    }

    #[test]
    fn backend_priority_and_cpu_fallback() {
        let si = fake_si();
        let mut o = Opts::default();
        o.vcodec = "hevc".into();
        assert_eq!(pick_gpu(&si, &o, "mkv").unwrap().0, Backend::Nvenc, "automaticky NVENC jako první");
        o.backend = "amf".into();
        assert_eq!(pick_gpu(&si, &o, "mkv").unwrap(), (Backend::Amf, "hevc_amf"));
        o.backend = "auto".into();
        // jen Intel + VA-API: QSV vyhraje
        let mut s2 = fake_si();
        s2.hw_encoders.retain(|(b, _)| matches!(b, Backend::Qsv | Backend::Vaapi));
        assert_eq!(pick_gpu(&s2, &o, "mkv").unwrap().0, Backend::Qsv);
        // kodeky bez GPU enkodéru -> CPU
        for c in ["prores", "ffv1", "mpeg4", "theora"] {
            o.vcodec = c.into();
            let (a, e) = plan_args(&si, &o, "mkv");
            assert!(e == Engine::Cpu && !a.join(" ").contains("nvenc"), "{c}");
        }
        // přepnutí na CPU
        o.vcodec = "h264".into();
        o.hw = HwMode::Cpu;
        assert!(pick_gpu(&si, &o, "mp4").is_none());
        // 10-bit h264 GPU neumí, 4:4:4 také ne
        o.hw = HwMode::Auto;
        o.pix_fmt = "yuv420p10le".into();
        assert!(pick_gpu(&si, &o, "mkv").is_none());
        o.vcodec = "hevc".into();
        assert!(pick_gpu(&si, &o, "mkv").is_some());
        o.pix_fmt = "yuv444p".into();
        assert!(pick_gpu(&si, &o, "mkv").is_none());
        // VideoToolbox umí jen hevc 10-bit
        o.pix_fmt = "yuv420p10le".into();
        o.backend = "videotoolbox".into();
        o.vcodec = "h264".into();
        assert!(pick_gpu(&si, &o, "mkv").is_none());
    }

    #[test]
    fn avx512_is_enabled_only_where_it_helps() {
        let mut si = SysInfo::default();
        si.avx512 = true;
        si.ffmpeg_avx512_flag = true;
        let mut o = Opts::default();
        o.hw = HwMode::Cpu;
        // HEVC (x265): asm=avx512 + ffmpeg cpuflags
        o.vcodec = "hevc".into();
        let plans = video_plans(&si, &o, &None, "mkv");
        assert!(plans[0].pre.join(" ").contains("-cpuflags +avx512"));
        assert!(plans[0].post.join(" ").contains("asm=avx512"), "{:?}", plans[0].post);
        assert!(plans[0].detail.contains("AVX-512"));
        // AV1 (SVT-AV1)
        o.vcodec = "av1".into();
        let p = video_plans(&si, &o, &None, "mkv");
        assert!(p[0].post.join(" ").contains("-svtav1-params asm=max"));
        // H.264 (x264 si AVX-512 vybírá sám): žádné zásahy do x264 parametrů
        o.vcodec = "h264".into();
        let p = video_plans(&si, &o, &None, "mp4");
        assert!(!p[0].post.join(" ").contains("asm="));
        // vypnuto uživatelem / procesor to neumí / jen kopie / GPU
        o.vcodec = "hevc".into();
        o.avx512 = false;
        let p = video_plans(&si, &o, &None, "mkv");
        assert!(!p[0].pre.join(" ").contains("cpuflags") && !p[0].post.join(" ").contains("asm="));
        o.avx512 = true;
        si.avx512 = false;
        let p = video_plans(&si, &o, &None, "mkv");
        assert!(!p[0].post.join(" ").contains("asm="));
        si.avx512 = true;
        o.copy_video = true;
        let p = video_plans(&si, &o, &None, "mkv");
        assert!(!p[0].pre.join(" ").contains("cpuflags"));
        o.copy_video = false;
        o.hw = HwMode::Auto;
        let gpu_si = fake_si();
        let mut g = gpu_si.clone();
        g.avx512 = true;
        g.ffmpeg_avx512_flag = true;
        let p = video_plans(&g, &o, &None, "mkv");
        assert!(!p[0].pre.join(" ").contains("cpuflags"), "na GPU se AVX-512 nepoužívá");
        // starý ffmpeg bez přepínače -cpuflags: jen parametry enkodérů
        si.ffmpeg_avx512_flag = false;
        o.hw = HwMode::Cpu;
        let p = video_plans(&si, &o, &None, "mkv");
        assert!(!p[0].pre.join(" ").contains("cpuflags") && p[0].post.join(" ").contains("asm=avx512"));
    }

    #[test]
    fn rate_control_args_per_backend() {
        for (b, enc, cqp, vbr, cbr) in [
            (Backend::Amf, "h264_amf", "-rc cqp -qp_i 21", "-rc vbr_peak -b:v 8000k", "-rc cbr"),
            (Backend::Nvenc, "h264_nvenc", "-rc constqp -qp 22", "-rc vbr -b:v 8000k", "-rc cbr -b:v 8000k"),
            (Backend::Qsv, "h264_qsv", "-global_quality 22", "-b:v 8000k -maxrate 12000k", "-b:v 8000k -maxrate 8000k"),
            (Backend::Vaapi, "h264_vaapi", "-rc_mode CQP -qp 22", "-rc_mode VBR -b:v 8000k", "-rc_mode CBR"),
            (Backend::VideoToolbox, "h264_videotoolbox", "-q:v 65", "-b:v 8000k", "-b:v 8000k"),
        ] {
            let si = only(b);
            let mut o = Opts::default();
            o.vcodec = "h264".into();
            for (rc, want) in [(Rc::Cqp, cqp), (Rc::Vbr, vbr), (Rc::Cbr, cbr)] {
                o.rc = rc;
                let (a, e) = plan_args(&si, &o, "mp4");
                let j = a.join(" ");
                assert_eq!(e, Engine::Gpu);
                assert!(j.contains(&format!("-c:v {enc}")) && j.contains(want), "{b:?} {rc:?}: {j}");
            }
        }
        // presety
        let mut o = Opts::default();
        o.vcodec = "hevc".into();
        o.preset = Preset::Speed;
        assert!(plan_args(&only(Backend::Nvenc), &o, "mkv").0.join(" ").contains("-preset p2"));
        assert!(plan_args(&only(Backend::Amf), &o, "mkv").0.join(" ").contains("-quality speed"));
        assert!(plan_args(&only(Backend::Qsv), &o, "mkv").0.join(" ").contains("-preset veryfast"));
        o.preset = Preset::Quality;
        assert!(plan_args(&only(Backend::Nvenc), &o, "mkv").0.join(" ").contains("-preset p7"));
        // VA-API: nahrání do GPU + zařízení
        let plans = video_plans(&only(Backend::Vaapi), &Opts::default(), &None, "mp4");
        assert!(plans[0].post.join(" ").contains("format=nv12,hwupload"));
        // fallback žebříček končí na CPU
        assert!(video_plans(&only(Backend::Nvenc), &Opts::default(), &None, "mp4").len() >= 2);
    }

    /// Volby, které posíláme jednotlivým enkodérům, musí v daném ffmpegu opravdu existovat.
    #[test]
    fn private_options_exist_in_ffmpeg() {
        let Some(ff) = crate::util::find_in_path("ffmpeg") else { return };
        let listed = String::from_utf8_lossy(&crate::util::command(&ff).args(["-hide_banner", "-encoders"]).output().unwrap().stdout).to_string();
        for b in gpu::ALL {
            let si = only(b);
            for codec in b.codecs() {
                let Some(enc) = b.encoder(codec) else { continue };
                let help = String::from_utf8_lossy(&crate::util::command(&ff).args(["-hide_banner", "-h", &format!("encoder={enc}")]).output().unwrap().stdout).to_string();
                if !listed.lines().any(|l| l.split_whitespace().nth(1) == Some(enc)) {
                    continue; // tenhle ffmpeg enkodér nemá (např. AMF / VideoToolbox na Linuxu)
                }
                let known: Vec<String> = help.lines().filter_map(|l| l.trim_start().strip_prefix('-').map(|r| r.split_whitespace().next().unwrap_or("").to_string())).collect();
                for rc in [Rc::Cqp, Rc::Vbr, Rc::Cbr] {
                    let mut o = Opts::default();
                    o.vcodec = codec.to_string();
                    o.rc = rc;
                    let (args, _) = plan_args(&si, &o, "mkv");
                    for (i, a) in args.iter().enumerate() {
                        let private = matches!(a.as_str(), "-preset" | "-tune" | "-rc" | "-qp" | "-rc_mode" | "-usage" | "-quality" | "-qp_i" | "-qp_p" | "-qp_b");
                        if private && i > 1 {
                            assert!(known.contains(&a[1..].to_string()), "{enc}: ffmpeg nezná volbu {a} ({rc:?})");
                        }
                    }
                }
            }
        }
    }
}

//! Uživatelská nastavení převodu (kvalita, kodeky, HW...).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum HwMode {
    /// Co jde přes AMF (GPU), jde přes GPU; zbytek přes CPU.
    Auto,
    Cpu,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Rc {
    Cqp,
    Vbr,
    Cbr,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Preset {
    Quality,
    Balanced,
    Speed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Opts {
    // ── video ──
    pub hw: HwMode,
    pub hw_decode: bool,
    /// Použít AVX-512 při enkódování na CPU (jen když ho procesor umí).
    pub avx512: bool,
    pub backend: String, // "auto" nebo id z gpu::Backend
    pub vcodec: String, // "auto" nebo id z VCODECS
    pub rc: Rc,
    pub preset: Preset,
    pub qp: u32, // 0 = výchozí pro kodek
    pub crf: u32,    // 0 = výchozí pro kodek
    pub vkbps: u32,  // cílový bitrate pro VBR/CBR
    pub maxrate: u32, // 0 = automaticky
    pub bufsize: u32, // 0 = automaticky
    pub pix_fmt: String, // "" = podle zdroje
    pub width: u32,  // 0 = automaticky / původní
    pub height: u32, // 0 = automaticky / původní
    pub keep_aspect: bool,
    pub scale_algo: String,
    pub fps: f32,    // 0 = původní
    // CPU rychlost (u AMF se používá Quality/Balanced/Speed)
    pub x_preset: String,
    pub x_tune: String,
    pub svt_preset: u32,
    pub vp9_speed: u32,
    pub gop: u32,
    pub bframes: i32, // -1 = výchozí
    pub vprofile: String,
    pub vlevel: String,
    pub extra_args: String,
    pub copy_video: bool,
    pub keep_subs: bool,
    pub burn_sub: String,
    // ── audio ──
    pub acodec: String, // "auto" nebo id
    pub akbps: u32,     // 0 = výchozí
    pub flac_level: u32,
    pub abits: u32, // 0 = podle zdroje / kodeku
    pub afloat: bool,
    pub sample_rate: u32,
    pub channels: u32,
    pub normalize: bool,
    pub volume_db: f32,
    pub audio_track: i32, // -1 = všechny / výchozí
    // ── střih ──
    pub trim_start: String,
    pub trim_end: String,
    // ── obrázky ──
    pub img_quality: u32,
    pub resize_pct: u32,
    pub max_w: u32,
    pub max_h: u32,
    pub img_bg: String,
    pub dpi: u32,
    pub strip_meta: bool,
    pub frame_time: f32,
    pub anim_fps: u32,
    pub anim_width: u32,
    // ── dokumenty ──
    pub paper: String,
    pub font_pt: u32,
    pub toc: bool,
    // ── archivy ──
    pub arc_level: u32,
    pub password: String,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            hw: HwMode::Auto,
            hw_decode: true,
            avx512: true,
            backend: "auto".into(),
            vcodec: "auto".into(),
            rc: Rc::Cqp,
            preset: Preset::Quality,
            qp: 0,
            crf: 0,
            vkbps: 8000,
            maxrate: 0,
            bufsize: 0,
            pix_fmt: String::new(),
            width: 0,
            height: 0,
            keep_aspect: true,
            scale_algo: "lanczos".into(),
            fps: 0.0,
            x_preset: "medium".into(),
            x_tune: String::new(),
            svt_preset: 6,
            vp9_speed: 2,
            gop: 0,
            bframes: -1,
            vprofile: String::new(),
            vlevel: String::new(),
            extra_args: String::new(),
            copy_video: false,
            keep_subs: true,
            burn_sub: String::new(),
            acodec: "auto".into(),
            akbps: 0,
            flac_level: 8,
            abits: 0,
            afloat: false,
            sample_rate: 0,
            channels: 0,
            normalize: false,
            volume_db: 0.0,
            audio_track: -1,
            trim_start: String::new(),
            trim_end: String::new(),
            img_quality: 90,
            resize_pct: 0,
            max_w: 0,
            max_h: 0,
            img_bg: "white".into(),
            dpi: 200,
            strip_meta: false,
            frame_time: 1.0,
            anim_fps: 15,
            anim_width: 480,
            paper: "a4".into(),
            font_pt: 11,
            toc: false,
            arc_level: 6,
            password: String::new(),
        }
    }
}

pub struct VCodec {
    pub id: &'static str,
    pub label: &'static str,
    pub cpu: &'static str,
    /// (min, max, výchozí) CRF u CPU, nebo None = bez CRF (použije se -q:v / bitrate)
    pub crf: Option<(u32, u32, u32)>,
    pub containers: &'static [&'static str], // prázdné = vše
}

pub const VCODECS: &[VCodec] = &[
    VCodec { id: "h264", label: "H.264 / AVC", cpu: "libx264", crf: Some((0, 51, 20)), containers: &[] },
    VCodec { id: "hevc", label: "H.265 / HEVC", cpu: "libx265", crf: Some((0, 51, 23)), containers: &["mp4", "mkv", "mov", "m4v", "ts", "m2ts", "mts", "3gp"] },
    VCodec { id: "av1", label: "AV1", cpu: "libsvtav1", crf: Some((10, 63, 30)), containers: &["mp4", "mkv", "webm", "mov", "m4v", "ivf"] },
    VCodec { id: "vp9", label: "VP9", cpu: "libvpx-vp9", crf: Some((0, 63, 31)), containers: &["webm", "mkv", "mp4", "ivf"] },
    VCodec { id: "vp8", label: "VP8", cpu: "libvpx", crf: Some((4, 63, 10)), containers: &["webm", "mkv", "ivf"] },
    VCodec { id: "vvc", label: "H.266 / VVC", cpu: "libvvenc", crf: Some((0, 63, 28)), containers: &["mp4", "mkv", "mov"] },
    VCodec { id: "mpeg4", label: "MPEG-4 Part 2", cpu: "mpeg4", crf: None, containers: &["mp4", "mkv", "avi", "mov", "m4v", "3gp", "3g2", "asf", "nut"] },
    VCodec { id: "xvid", label: "Xvid", cpu: "libxvid", crf: None, containers: &["avi", "mp4", "mkv", "mov"] },
    VCodec { id: "mpeg2", label: "MPEG-2", cpu: "mpeg2video", crf: None, containers: &["mpg", "mpeg", "vob", "ts", "m2ts", "mts", "mkv", "mp4", "mov", "mxf"] },
    VCodec { id: "mpeg1", label: "MPEG-1", cpu: "mpeg1video", crf: None, containers: &["mpg", "mpeg", "mkv"] },
    VCodec { id: "theora", label: "Theora", cpu: "libtheora", crf: None, containers: &["ogv", "mkv"] },
    VCodec { id: "wmv2", label: "WMV 8 (wmv2)", cpu: "wmv2", crf: None, containers: &["wmv", "asf", "avi", "mkv"] },
    VCodec { id: "wmv1", label: "WMV 7 (wmv1)", cpu: "wmv1", crf: None, containers: &["wmv", "asf", "avi"] },
    VCodec { id: "flv1", label: "Sorenson (flv1)", cpu: "flv", crf: None, containers: &["flv", "f4v"] },
    VCodec { id: "h263", label: "H.263", cpu: "h263", crf: None, containers: &["3gp", "3g2", "mov", "mp4"] },
    VCodec { id: "mjpeg", label: "Motion JPEG", cpu: "mjpeg", crf: None, containers: &["avi", "mov", "mkv", "mp4"] },
    VCodec { id: "prores", label: "Apple ProRes", cpu: "prores_ks", crf: None, containers: &["mov", "mkv", "mxf"] },
    VCodec { id: "dnxhd", label: "DNxHD / DNxHR", cpu: "dnxhd", crf: None, containers: &["mov", "mxf", "mkv"] },
    VCodec { id: "ffv1", label: "FFV1 (lossless)", cpu: "ffv1", crf: None, containers: &["mkv", "avi", "mov", "nut"] },
    VCodec { id: "utvideo", label: "Ut Video (lossless)", cpu: "utvideo", crf: None, containers: &["avi", "mkv", "mov", "nut"] },
    VCodec { id: "huffyuv", label: "HuffYUV (lossless)", cpu: "huffyuv", crf: None, containers: &["avi", "mkv", "nut"] },
];

pub fn vcodec(id: &str) -> Option<&'static VCodec> {
    VCODECS.iter().find(|c| c.id == id)
}

/// Výchozí video kodek pro kontejner.
pub fn default_vcodec(container: &str) -> &'static str {
    match container {
        "webm" => "vp9",
        "ogv" => "theora",
        "wmv" | "asf" => "wmv2",
        "mpg" | "mpeg" | "vob" | "mxf" => "mpeg2",
        "flv" | "f4v" => "h264",
        "ivf" => "vp9",
        _ => "h264",
    }
}

pub struct ACodec {
    pub id: &'static str,
    pub label: &'static str,
    pub enc: &'static str,
    pub lossless: bool,
}

pub const ACODECS: &[ACodec] = &[
    ACodec { id: "aac", label: "AAC", enc: "aac", lossless: false },
    ACodec { id: "mp3", label: "MP3", enc: "libmp3lame", lossless: false },
    ACodec { id: "opus", label: "Opus", enc: "libopus", lossless: false },
    ACodec { id: "vorbis", label: "Vorbis", enc: "libvorbis", lossless: false },
    ACodec { id: "ac3", label: "Dolby AC-3", enc: "ac3", lossless: false },
    ACodec { id: "eac3", label: "Dolby E-AC-3", enc: "eac3", lossless: false },
    ACodec { id: "mp2", label: "MP2", enc: "mp2", lossless: false },
    ACodec { id: "wma", label: "WMA v2", enc: "wmav2", lossless: false },
    ACodec { id: "flac", label: "FLAC", enc: "flac", lossless: true },
    ACodec { id: "alac", label: "Apple Lossless", enc: "alac", lossless: true },
    ACodec { id: "pcm16", label: "PCM 16-bit", enc: "pcm_s16le", lossless: true },
    ACodec { id: "pcm24", label: "PCM 24-bit", enc: "pcm_s24le", lossless: true },
    ACodec { id: "pcm32", label: "PCM 32-bit float", enc: "pcm_f32le", lossless: true },
    ACodec { id: "wavpack", label: "WavPack", enc: "wavpack", lossless: true },
    ACodec { id: "tta", label: "TTA", enc: "tta", lossless: true },
    ACodec { id: "amr", label: "AMR-NB", enc: "libopencore_amrnb", lossless: false },
    ACodec { id: "speex", label: "Speex", enc: "libspeex", lossless: false },
];

pub fn acodec(id: &str) -> Option<&'static ACodec> {
    ACODECS.iter().find(|c| c.id == id)
}

/// Výchozí audio kodek pro čistě zvukový výstup.
pub fn default_audio_for_ext(ext: &str) -> &'static str {
    match ext {
        "mp3" => "mp3",
        "flac" => "flac",
        "wav" | "w64" | "caf" => "pcm16",
        "aiff" | "aif" | "au" => "pcm16",
        "ogg" | "oga" => "vorbis",
        "opus" => "opus",
        "m4a" | "m4b" | "aac" | "3ga" => "aac",
        "wma" => "wma",
        "ac3" => "ac3",
        "eac3" => "eac3",
        "mp2" => "mp2",
        "mka" => "flac",
        "amr" => "amr",
        "wv" => "wavpack",
        "tta" => "tta",
        "spx" => "speex",
        _ => "aac",
    }
}

/// Kodeky povolené v audio kontejneru (první = výchozí).
pub fn audio_choices(ext: &str) -> Vec<&'static str> {
    match ext {
        "mp3" => vec!["mp3"],
        "flac" => vec!["flac"],
        "wav" | "w64" => vec!["pcm16", "pcm24", "pcm32"],
        "caf" => vec!["pcm16", "pcm24", "alac", "aac"],
        "aiff" | "aif" => vec!["pcm16", "pcm24"],
        "au" => vec!["pcm16", "pcm24"],
        "ogg" | "oga" => vec!["vorbis", "opus", "flac"],
        "opus" => vec!["opus"],
        "m4a" | "m4b" => vec!["aac", "alac", "mp3", "ac3", "eac3"],
        "aac" | "3ga" => vec!["aac"],
        "wma" => vec!["wma"],
        "ac3" => vec!["ac3"],
        "eac3" => vec!["eac3"],
        "mp2" => vec!["mp2"],
        "amr" => vec!["amr"],
        "wv" => vec!["wavpack"],
        "tta" => vec!["tta"],
        "spx" => vec!["speex"],
        "mka" => vec!["flac", "opus", "aac", "mp3", "vorbis", "ac3", "eac3", "alac", "pcm16", "pcm24", "wavpack", "tta"],
        _ => vec!["aac"],
    }
}

/// Audio kodeky povolené uvnitř video kontejneru (pro "copy" rozhodnutí).
pub fn video_container_audio(ext: &str) -> Vec<&'static str> {
    match ext {
        "mp4" | "m4v" | "mov" => vec!["aac", "mp3", "ac3", "eac3", "alac", "opus", "flac"],
        "mkv" => vec![],
        "webm" => vec!["opus", "vorbis"],
        "avi" => vec!["mp3", "ac3", "mp2", "pcm_s16le"],
        "wmv" | "asf" => vec!["wmav2", "wmav1", "mp3"],
        "flv" | "f4v" => vec!["aac", "mp3"],
        "ts" | "m2ts" | "mts" => vec!["aac", "ac3", "eac3", "mp3", "mp2", "dts"],
        "mpg" | "mpeg" | "vob" => vec!["mp2", "ac3", "mp3", "dts"],
        "ogv" => vec!["vorbis", "opus", "flac"],
        "3gp" | "3g2" => vec!["aac", "amr_nb"],
        _ => vec!["aac", "mp3"],
    }
}

pub fn video_container_default_audio(ext: &str) -> &'static str {
    match ext {
        "webm" | "ogv" => "opus",
        "avi" => "mp3",
        "wmv" | "asf" => "wma",
        "mpg" | "mpeg" | "vob" => "ac3",
        _ => "aac",
    }
}

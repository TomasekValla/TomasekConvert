//! ffprobe -> přehledný model (kontejner, stopy, kapitoly, tagy).
use crate::{tools::Tools, util::command};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Attachment,
    Data,
}

impl StreamKind {
    pub fn label(&self) -> &'static str {
        match self {
            StreamKind::Video => "Video",
            StreamKind::Audio => "Audio",
            StreamKind::Subtitle => tr!("Titulky", "Subtitles"),
            StreamKind::Attachment => tr!("Příloha", "Attachment"),
            StreamKind::Data => "Data",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stream {
    pub index: usize,
    pub kind: StreamKind,
    pub codec: String,
    pub codec_long: String,
    pub profile: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub pix_fmt: String,
    pub bit_depth: u32,
    pub color_space: String,
    pub color_transfer: String,
    pub color_primaries: String,
    pub color_range: String,
    pub hdr: String,
    pub bit_rate: u64,
    pub channels: u32,
    pub channel_layout: String,
    pub sample_fmt: String,
    pub sample_rate: u32,
    pub lang: String,
    pub title: String,
    pub default: bool,
    pub forced: bool,
    pub attached_pic: bool,
    pub duration: f64,
    pub nb_frames: u64,
    pub filename: String,
    pub mimetype: String,
    #[allow(dead_code)]
    pub tags: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub raw: String,
    pub format_name: String,
    pub format_long: String,
    /// Srozumitelný název kontejneru (ffprobe vrací např. "mov,mp4,m4a,3gp,3g2,mj2" -> ".mp4").
    pub container: String,
    pub duration: f64,
    pub size: u64,
    pub bit_rate: u64,
    pub tags: BTreeMap<String, String>,
    pub streams: Vec<Stream>,
    pub chapters: Vec<Chapter>,
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn f(v: &Value, k: &str) -> f64 {
    s(v, k).parse().unwrap_or(0.0)
}
fn u(v: &Value, k: &str) -> u64 {
    s(v, k).parse().unwrap_or(0)
}
fn tags(v: &Value) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Some(Value::Object(o)) = v.get("tags") {
        for (k, val) in o {
            if let Some(t) = val.as_str() {
                m.insert(k.clone(), t.to_string());
            }
        }
    }
    m
}
fn tag_ci(m: &BTreeMap<String, String>, key: &str) -> String {
    m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone()).unwrap_or_default()
}
fn fraction(x: &str) -> f64 {
    let mut it = x.split('/');
    let a: f64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let b: f64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

pub fn run(tools: &Tools, path: &Path) -> Result<Probe, String> {
    let ffprobe = tools.ffprobe.as_ref().ok_or(tr!("ffprobe není k dispozici", "ffprobe is not available"))?;
    let out = command(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-show_chapters", "-show_programs"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() && out.stdout.len() < 10 {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let raw = String::from_utf8_lossy(&out.stdout).to_string();
    let mut p = parse(&raw)?;
    p.container = container_label(&p.format_name, &crate::util::full_ext(path));
    Ok(p)
}

/// Ze seznamu názvů formátů vybere ten, který odpovídá příponě souboru (jinak první).
pub fn container_label(format_name: &str, ext: &str) -> String {
    let names: Vec<&str> = format_name.split(',').collect();
    let alias = |e: &str| match e {
        "mkv" | "mka" | "mk3d" | "webm" => vec!["matroska", "webm"],
        "m4a" | "m4v" | "mp4" | "mov" | "3gp" | "3g2" | "m4b" => vec!["mov", "mp4", "m4a", "3gp", "3g2", "mj2"],
        "ts" | "mts" | "m2ts" => vec!["mpegts"],
        "mpg" | "mpeg" => vec!["mpeg", "mpegvideo"],
        "ogg" | "oga" | "ogv" | "opus" => vec!["ogg"],
        "jpg" | "jpeg" => vec!["image2", "jpeg_pipe"],
        _ => vec![],
    };
    if names.iter().any(|n| *n == ext) {
        return ext.to_string();
    }
    if !ext.is_empty() && names.iter().any(|n| alias(ext).contains(n)) {
        return ext.to_string();
    }
    names.first().map(|s| s.to_string()).unwrap_or_default()
}

pub fn parse(raw: &str) -> Result<Probe, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("JSON: {e}"))?;
    let mut p = Probe { raw: serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_string()), ..Default::default() };
    if let Some(fm) = v.get("format") {
        p.format_name = s(fm, "format_name");
        p.format_long = s(fm, "format_long_name");
        p.duration = f(fm, "duration");
        p.size = u(fm, "size");
        p.bit_rate = u(fm, "bit_rate");
        p.tags = tags(fm);
    }
    if let Some(Value::Array(arr)) = v.get("streams") {
        for st in arr {
            let kind = match s(st, "codec_type").as_str() {
                "video" => StreamKind::Video,
                "audio" => StreamKind::Audio,
                "subtitle" => StreamKind::Subtitle,
                "attachment" => StreamKind::Attachment,
                _ => StreamKind::Data,
            };
            let t = tags(st);
            let disp = st.get("disposition");
            let flag = |k: &str| disp.and_then(|d| d.get(k)).and_then(|x| x.as_i64()).unwrap_or(0) == 1;
            let transfer = s(st, "color_transfer");
            let mut hdr = String::new();
            if transfer == "smpte2084" {
                hdr = "HDR10 / PQ".into();
            } else if transfer == "arib-std-b67" {
                hdr = "HLG".into();
            }
            if let Some(Value::Array(sd)) = st.get("side_data_list") {
                if sd.iter().any(|x| s(x, "side_data_type").contains("DOVI")) {
                    hdr = if hdr.is_empty() { "Dolby Vision".into() } else { format!("{hdr} + Dolby Vision") };
                }
            }
            let bit_depth = {
                let b = u(st, "bits_per_raw_sample") as u32;
                if b > 0 {
                    b
                } else if s(st, "pix_fmt").contains("10") {
                    10
                } else if s(st, "pix_fmt").contains("12") {
                    12
                } else if kind == StreamKind::Video {
                    8
                } else {
                    let bps = u(st, "bits_per_sample") as u32;
                    if bps > 0 {
                        bps
                    } else {
                        match s(st, "sample_fmt").trim_end_matches('p') {
                            "u8" => 8,
                            "s16" => 16,
                            "s32" | "flt" => 32,
                            "s64" | "dbl" => 64,
                            _ => 0,
                        }
                    }
                }
            };
            p.streams.push(Stream {
                index: u(st, "index") as usize,
                kind,
                codec: s(st, "codec_name"),
                codec_long: s(st, "codec_long_name"),
                profile: s(st, "profile"),
                width: u(st, "width") as u32,
                height: u(st, "height") as u32,
                fps: {
                    let a = fraction(&s(st, "avg_frame_rate"));
                    if a > 0.0 {
                        a
                    } else {
                        fraction(&s(st, "r_frame_rate"))
                    }
                },
                pix_fmt: s(st, "pix_fmt"),
                bit_depth,
                color_space: s(st, "color_space"),
                color_transfer: transfer,
                color_primaries: s(st, "color_primaries"),
                color_range: s(st, "color_range"),
                hdr,
                bit_rate: u(st, "bit_rate"),
                channels: u(st, "channels") as u32,
                channel_layout: s(st, "channel_layout"),
                sample_fmt: s(st, "sample_fmt"),
                sample_rate: u(st, "sample_rate") as u32,
                lang: tag_ci(&t, "language"),
                title: tag_ci(&t, "title"),
                default: flag("default"),
                forced: flag("forced"),
                attached_pic: flag("attached_pic"),
                duration: f(st, "duration"),
                nb_frames: u(st, "nb_frames"),
                filename: tag_ci(&t, "filename"),
                mimetype: tag_ci(&t, "mimetype"),
                tags: t,
            });
        }
    }
    if let Some(Value::Array(arr)) = v.get("chapters") {
        for c in arr {
            p.chapters.push(Chapter { start: f(c, "start_time"), end: f(c, "end_time"), title: tag_ci(&tags(c), "title") });
        }
    }
    Ok(p)
}

impl Probe {
    pub fn video(&self) -> Option<&Stream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Video && !s.attached_pic)
    }
    pub fn audio_streams(&self) -> impl Iterator<Item = &Stream> {
        self.streams.iter().filter(|s| s.kind == StreamKind::Audio)
    }
    pub fn count(&self, k: StreamKind) -> usize {
        self.streams.iter().filter(|s| s.kind == k).count()
    }
}

impl Stream {
    pub fn headline(&self) -> String {
        match self.kind {
            StreamKind::Video => {
                let mut v = vec![self.codec.to_uppercase()];
                if self.width > 0 {
                    v.push(format!("{}×{}", self.width, self.height));
                }
                if self.fps > 0.0 {
                    v.push(format!("{:.3} fps", self.fps).replace(".000", ""));
                }
                v.join(" · ")
            }
            StreamKind::Audio => {
                let mut v = vec![self.codec.to_uppercase()];
                if !self.channel_layout.is_empty() {
                    v.push(self.channel_layout.clone());
                } else if self.channels > 0 {
                    v.push(format!("{} kan.", self.channels));
                }
                if self.sample_rate > 0 {
                    v.push(format!("{} Hz", self.sample_rate));
                }
                v.join(" · ")
            }
            StreamKind::Subtitle => self.codec.to_uppercase(),
            StreamKind::Attachment => {
                if self.filename.is_empty() {
                    self.codec.to_uppercase()
                } else {
                    self.filename.clone()
                }
            }
            StreamKind::Data => self.codec.to_uppercase(),
        }
    }
    pub fn is_text_sub(&self) -> bool {
        matches!(self.codec.as_str(), "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "mov_text" | "text" | "microdvd" | "subviewer" | "sami" | "ttml")
    }
}

impl Stream {
    /// "24-bit" / "32-bit float" ...
    pub fn depth_label(&self) -> String {
        // u ztrátových kodeků je "bitová hloubka" jen interní formát dekodéru – nezobrazujeme ji
        if matches!(self.codec.as_str(), "aac" | "mp3" | "opus" | "vorbis" | "ac3" | "eac3" | "mp2" | "mp1" | "wmav1" | "wmav2" | "wmapro" | "dts" | "amr_nb" | "amr_wb" | "speex" | "atrac3" | "cook" | "ra_144" | "truehd") {
            return String::new();
        }
        let float = matches!(self.sample_fmt.as_str(), "flt" | "fltp" | "dbl" | "dblp");
        if self.bit_depth == 0 {
            String::new()
        } else if float {
            format!("{}-bit float", self.bit_depth)
        } else {
            format!("{}-bit", self.bit_depth)
        }
    }
}

pub fn fmt_bitrate(b: u64) -> String {
    if b == 0 {
        "—".into()
    } else if b >= 1_000_000 {
        format!("{:.2} Mb/s", b as f64 / 1e6)
    } else {
        format!("{} kb/s", b / 1000)
    }
}

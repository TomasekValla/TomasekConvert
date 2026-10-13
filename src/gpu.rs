//! GPU enkodéry: NVIDIA NVENC, AMD AMF, Apple VideoToolbox, Intel Quick Sync, VA-API.
//! Co stroj opravdu umí, se zjišťuje zkušebním enkódem při startu (viz hw.rs).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Backend {
    Nvenc,
    Amf,
    VideoToolbox,
    Qsv,
    Vaapi,
}

/// Pořadí, v jakém se backendy zkouší při automatické volbě.
pub const ALL: [Backend; 5] = [Backend::Nvenc, Backend::Amf, Backend::VideoToolbox, Backend::Qsv, Backend::Vaapi];

impl Backend {
    pub fn id(self) -> &'static str {
        match self {
            Backend::Nvenc => "nvenc",
            Backend::Amf => "amf",
            Backend::VideoToolbox => "videotoolbox",
            Backend::Qsv => "qsv",
            Backend::Vaapi => "vaapi",
        }
    }
    pub fn from_id(s: &str) -> Option<Backend> {
        ALL.iter().copied().find(|b| b.id() == s)
    }
    /// Plný název.
    pub fn label(self) -> &'static str {
        match self {
            Backend::Nvenc => "NVIDIA NVENC",
            Backend::Amf => "AMD AMF",
            Backend::VideoToolbox => "Apple VideoToolbox",
            Backend::Qsv => "Intel Quick Sync",
            Backend::Vaapi => "VA-API",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Backend::Nvenc => "NVENC",
            Backend::Amf => "AMF",
            Backend::VideoToolbox => "VideoToolbox",
            Backend::Qsv => "QSV",
            Backend::Vaapi => "VA-API",
        }
    }
    /// Názvy ffmpeg enkodérů pro kodek (id z `opts::VCODECS`).
    pub fn encoder(self, codec: &str) -> Option<&'static str> {
        Some(match (self, codec) {
            (Backend::Amf, "h264") => "h264_amf",
            (Backend::Amf, "hevc") => "hevc_amf",
            (Backend::Amf, "av1") => "av1_amf",
            (Backend::Nvenc, "h264") => "h264_nvenc",
            (Backend::Nvenc, "hevc") => "hevc_nvenc",
            (Backend::Nvenc, "av1") => "av1_nvenc",
            (Backend::Qsv, "h264") => "h264_qsv",
            (Backend::Qsv, "hevc") => "hevc_qsv",
            (Backend::Qsv, "av1") => "av1_qsv",
            (Backend::Vaapi, "h264") => "h264_vaapi",
            (Backend::Vaapi, "hevc") => "hevc_vaapi",
            (Backend::Vaapi, "av1") => "av1_vaapi",
            (Backend::VideoToolbox, "h264") => "h264_videotoolbox",
            (Backend::VideoToolbox, "hevc") => "hevc_videotoolbox",
            _ => return None,
        })
    }
    pub fn codecs(self) -> &'static [&'static str] {
        match self {
            Backend::Amf | Backend::Nvenc => &["h264", "hevc", "av1"],
            Backend::Qsv | Backend::Vaapi => &["h264", "hevc", "av1"],
            Backend::VideoToolbox => &["h264", "hevc"],
        }
    }
    /// (min, max, výchozí) hodnota posuvníku kvality v režimu konstantní kvality.
    pub fn qp_range(self, codec: &str) -> (u32, u32, u32) {
        match (self, codec) {
            (Backend::VideoToolbox, _) => (1, 100, 65),
            (Backend::Amf, "h264") => (10, 51, 21),
            (Backend::Amf, "hevc") => (10, 51, 23),
            (Backend::Amf, "av1") => (40, 255, 120),
            (Backend::Nvenc, "av1") | (Backend::Vaapi, "av1") => (0, 255, 110),
            (Backend::Qsv, "av1") => (1, 255, 110),
            (Backend::Nvenc, "h264") | (Backend::Vaapi, "h264") => (0, 51, 22),
            (Backend::Nvenc, "hevc") | (Backend::Vaapi, "hevc") => (0, 51, 24),
            (Backend::Qsv, "h264") => (1, 51, 22),
            (Backend::Qsv, "hevc") => (1, 51, 24),
            _ => (1, 51, 24),
        }
    }
    /// U VideoToolboxu je vyšší číslo = lepší kvalita (opačně než u ostatních).
    pub fn higher_is_better(self) -> bool {
        self == Backend::VideoToolbox
    }
    /// Umí tenhle backend požadovaný pixel format (prázdný = automaticky)?
    pub fn pix_ok(self, want: &str, codec: &str) -> bool {
        match want {
            "" | "yuv420p" | "nv12" => true,
            "yuv420p10le" | "p010le" => matches!(codec, "hevc" | "av1") && self != Backend::VideoToolbox || (self == Backend::VideoToolbox && codec == "hevc"),
            _ => false,
        }
    }
    /// Kodeky, které se při převodu přes tenhle backend neumí 10-bit (kontrola na úrovni kodeku).
    #[allow(dead_code)]
    pub fn needs_hwupload(self) -> bool {
        self == Backend::Vaapi
    }
}

/// Renderovací zařízení pro VA-API (Linux). Lze přepsat proměnnou TC_VAAPI_DEVICE.
pub fn vaapi_device() -> Option<String> {
    if let Ok(d) = std::env::var("TC_VAAPI_DEVICE") {
        return Some(d);
    }
    for n in 128..136 {
        let p = format!("/dev/dri/renderD{n}");
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}

/// Argumenty zkušebního enkódu (0,2 s videa).
pub fn probe_args(b: Backend, enc: &str) -> Vec<String> {
    let s = |x: &str| x.to_string();
    let mut a = vec![s("-hide_banner"), s("-loglevel"), s("error")];
    if b == Backend::Vaapi {
        match vaapi_device() {
            Some(d) => a.extend([s("-vaapi_device"), d]),
            None => return Vec::new(),
        }
    }
    a.extend([s("-f"), s("lavfi"), s("-i"), s("color=c=black:s=1280x720:r=30:d=0.2")]);
    if b == Backend::Vaapi {
        a.extend([s("-vf"), s("format=nv12,hwupload")]);
    }
    a.extend([s("-frames:v"), s("3"), s("-c:v"), s(enc), s("-f"), s("null"), s("-")]);
    a
}

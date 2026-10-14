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

/// Proč zkušební enkód selhal (z chybového výstupu ffmpeg).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fail {
    /// Ovladač je starší, než vyžaduje vložený ffmpeg. Obsahuje nejnižší potřebnou verzi, pokud ji ffmpeg uvedl.
    DriverTooOld(Option<String>),
    /// Chybí knihovna ovladače (nvEncodeAPI64.dll, libnvidia-encode.so, amfrt64.dll, …).
    NoLibrary,
    /// Karta tenhle kodek / enkodér neumí (nebo tu žádná vhodná karta není).
    NoDevice,
    /// Cokoliv jiného – poslední řádek chybového výstupu.
    Other(String),
}

/// Rozpozná důvod selhání podle textu, který ffmpeg vypsal při zkušebním enkódu.
pub fn classify(output: &str) -> Fail {
    let low = output.to_lowercase();
    // NVENC: "Driver does not support the required nvenc API version. Required: 13.0 Found: 12.1"
    //        "The minimum required Nvidia driver for nvenc is 570.0 or newer"
    if low.contains("driver does not support the required nvenc api") || low.contains("minimum required nvidia driver") {
        let min = low.split("minimum required nvidia driver for nvenc is ").nth(1).and_then(|t| t.split_whitespace().next()).map(|v| v.trim_end_matches('.').to_string()).or_else(|| {
            let req = low.split("required: ").nth(1)?.split_whitespace().next()?.to_string();
            nvenc_min_driver(&req).map(|v| v.to_string())
        });
        return Fail::DriverTooOld(min);
    }
    if low.contains("cannot load nvcuda") || low.contains("cannot load libcuda") || low.contains("cannot load nvencodeapi") || low.contains("cannot load libnvidia-encode") || low.contains("amfrt") && low.contains("failed to open") {
        return Fail::NoLibrary;
    }
    if low.contains("no capable devices found")
        || low.contains("unsupported device")
        || low.contains("incompatible client key")
        || low.contains("amf_no_device")
        || low.contains("failed to create amf")
        || low.contains("createcomponent")
        || low.contains("error initializing an mfx session")
        || low.contains("failed to initialise vaapi")
        || low.contains("failed to initialize vaapi")
        || low.contains("no usable encoding profile")
        || low.contains("device creation failed")
    {
        return Fail::NoDevice;
    }
    let last = output.lines().map(str::trim).rev().find(|l| !l.is_empty()).unwrap_or("").to_string();
    Fail::Other(last)
}

/// Nejnižší verze NVIDIA ovladače pro dané NVENC API (podle nv-codec-headers). Windows / Linux.
pub fn nvenc_min_driver(api: &str) -> Option<&'static str> {
    let (win, lin) = match api {
        "13.0" => ("570.00", "570.00"),
        "12.2" => ("551.76", "550.54"),
        "12.1" => ("531.61", "530.41"),
        "12.0" => ("522.25", "520.56"),
        "11.1" => ("471.41", "470.57"),
        _ => return None,
    };
    Some(if cfg!(windows) { win } else { lin })
}

/// Windows hlásí verzi ovladače jako `31.0.15.5123`; skutečná verze NVIDIA je posledních pět číslic (551.23).
pub fn nvidia_driver_from_windows(v: &str) -> Option<String> {
    let parts: Vec<&str> = v.trim().split('.').collect();
    if parts.len() != 4 || !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let digits = format!("{}{}", parts[2], parts[3]);
    if digits.len() < 5 {
        return None;
    }
    let d = &digits[digits.len() - 5..];
    Some(format!("{}.{}", &d[..3], &d[3..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_nvenc_driver_too_old() {
        let out = "[h264_nvenc @ 000001] Driver does not support the required nvenc API version. Required: 13.0 Found: 11.1\n[h264_nvenc @ 000001] The minimum required Nvidia driver for nvenc is 570.0 or newer\nError initializing output stream";
        assert_eq!(classify(out), Fail::DriverTooOld(Some("570.0".into())));
        let out = "Driver does not support the required nvenc API version. Required: 12.1 Found: 11.1";
        assert!(matches!(classify(out), Fail::DriverTooOld(Some(_))));
    }

    #[test]
    fn classifies_missing_device_and_library() {
        assert_eq!(classify("[av1_nvenc @ 0x1] OpenEncodeSessionEx failed: unsupported device (2): (no details)"), Fail::NoDevice);
        assert_eq!(classify("[h264_nvenc @ 0x1] Cannot load nvEncodeAPI64.dll"), Fail::NoLibrary);
        assert!(matches!(classify("something odd\n\nlast line\n"), Fail::Other(l) if l == "last line"));
    }

    #[test]
    fn decodes_windows_nvidia_driver_versions() {
        assert_eq!(nvidia_driver_from_windows("31.0.15.5123").as_deref(), Some("551.23"));
        assert_eq!(nvidia_driver_from_windows("27.21.14.7514").as_deref(), Some("475.14"));
        assert_eq!(nvidia_driver_from_windows("32.0.15.8097").as_deref(), Some("580.97"));
        assert!(nvidia_driver_from_windows("garbage").is_none());
    }
}

//! Tabulky formátů: co je co, do čeho se dá převádět a kterým nástrojem.
use crate::hw::SysInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Document,
    Pdf,
    Archive,
    Folder,
    Subtitle,
    Other,
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Video => "Video",
            Kind::Audio => "Audio",
            Kind::Image => tr!("Obrázek", "Image"),
            Kind::Document => tr!("Dokument", "Document"),
            Kind::Pdf => "PDF",
            Kind::Archive => tr!("Archiv", "Archive"),
            Kind::Folder => tr!("Složka", "Folder"),
            Kind::Subtitle => tr!("Titulky", "Subtitles"),
            Kind::Other => tr!("Neznámé", "Unknown"),
        }
    }
}

/// Jak se výstupní formát vyrábí.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Video,      // ffmpeg, kontejner s videem
    Audio,      // ffmpeg, jen zvuk
    AnimImage,  // ffmpeg: gif / apng / animované webp z videa
    FrameImage, // ffmpeg: jeden snímek z videa
    Image,      // ImageMagick
    Doc,        // pandoc
    Pdf,        // z čehokoliv do PDF
    Archive,
    Subtitle,
    Text,       // prostý text (PDF -> txt)
}

#[derive(Clone, Debug)]
pub struct Fmt {
    pub ext: String,
    pub name: String,
    pub class: Class,
    /// pandoc writer / magick coder – podle třídy.
    pub extra: String,
}

impl Fmt {
    fn new(ext: &str, name: &str, class: Class, extra: &str) -> Fmt {
        Fmt { ext: ext.into(), name: name.into(), class, extra: extra.into() }
    }
}

pub const VIDEO_IN: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "wmv", "flv", "m4v", "mpg", "mpeg", "mpe", "ts", "mts", "m2ts", "m2t", "vob",
    "ogv", "ogx", "3gp", "3g2", "asf", "f4v", "mxf", "rm", "rmvb", "divx", "amv", "nut", "y4m", "mj2", "ivf", "mjpeg",
    "dv", "gxf", "h264", "264", "h265", "265", "hevc", "av1", "obu", "evo", "mod", "tod", "trp", "swf", "wtv", "dvr-ms",
    "bik", "smk", "roq", "qt", "yuv", "m1v", "m2v", "mp4v", "xesc", "lxf", "mk3d", "ismv", "wve",
];
pub const AUDIO_IN: &[&str] = &[
    "mp3", "wav", "flac", "ogg", "oga", "opus", "aac", "m4a", "m4b", "wma", "aiff", "aif", "aifc", "ac3", "eac3", "mp2",
    "mp1", "mka", "caf", "au", "snd", "amr", "awb", "w64", "wv", "tta", "ape", "dts", "dtshd", "mlp", "thd", "spx", "ra",
    "mpc", "mpp", "tak", "oma", "aa3", "at3", "voc", "8svx", "cda", "gsm", "sbc", "sln", "mid", "qoa", "dsf", "dff",
    "dsd", "mogg", "shn", "ircam", "sf", "s3m", "xm", "it", "mod", "wem", "ac4", "adts", "mka", "rmi", "mpa", "m4r", "3ga",
];
pub const IMAGE_IN: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "webp", "gif", "apng", "bmp", "tif", "tiff", "svg", "svgz", "ico", "cur", "ani",
    "icns", "avif", "heic", "heif", "jxl", "jp2", "j2k", "jpf", "jpx", "tga", "psd", "xcf", "exr", "hdr", "pbm", "pgm",
    "ppm", "pnm", "pfm", "pam", "dds", "qoi", "mpo", "nef", "cr2", "cr3", "arw", "dng", "orf", "rw2", "raf", "pef", "srw",
    "x3f", "3fr", "erf", "kdc", "mrw", "sr2", "dcr", "wbmp", "xbm", "xpm", "emf", "wmf", "pcx", "sgi", "sun", "fits",
    "mat", "miff", "mng", "pcd", "pict", "pix", "wpg", "jng", "jbig", "bpg", "flif", "tim", "ras", "rgb", "otb", "farbfeld",
];
pub const DOC_IN: &[&str] = &[
    "docx", "odt", "rtf", "md", "markdown", "html", "htm", "xhtml", "txt", "text", "epub", "fb2", "tex", "latex",
    "rst", "org", "textile", "wiki", "mediawiki", "dokuwiki", "muse", "t2t", "ipynb", "csv", "tsv", "opml", "bib",
    "bibtex", "ris", "enl", "typ", "xml", "jats", "docbook", "man", "creole", "twiki", "tikiwiki", "vimwiki", "djot",
    ];
pub const ARCHIVE_IN: &[&str] = &[
    "zip", "7z", "rar", "tar", "tar.gz", "tgz", "tar.bz2", "tbz2", "tbz", "tar.xz", "txz", "tar.zst", "tzst", "gz", "bz2",
    "xz", "zst", "cbz", "cbr", "jar", "apk", "war",
];
pub const SUB_IN: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "smi", "sami", "sbv", "lrc", "ttml", "dfxp", "stl", "jss", "mpl2"];

pub fn detect_kind(ext: &str, si: &SysInfo) -> Kind {
    let e = ext;
    if e == "pdf" {
        Kind::Pdf
    } else if ARCHIVE_IN.contains(&e) {
        Kind::Archive
    } else if SUB_IN.contains(&e) {
        Kind::Subtitle
    } else if DOC_IN.contains(&e) {
        Kind::Document
    } else if IMAGE_IN.contains(&e) {
        Kind::Image
    } else if VIDEO_IN.contains(&e) {
        Kind::Video
    } else if AUDIO_IN.contains(&e) {
        Kind::Audio
    } else if si.magick_read.contains(e) {
        Kind::Image
    } else {
        Kind::Other
    }
}

pub const IMAGE_OUT_COMMON: &[&str] = &[
    "png", "jpg", "webp", "avif", "heic", "jxl", "gif", "bmp", "tiff", "ico", "svg", "jp2", "tga", "psd", "exr", "hdr",
    "pbm", "pgm", "ppm", "pnm", "dds", "qoi", "icns", "pcx", "xpm", "xbm", "wbmp", "pdf",
];

fn video_containers() -> Vec<Fmt> {
    [
        ("mp4", "MP4"), ("mkv", "Matroska"), ("webm", "WebM"), ("mov", "QuickTime"), ("avi", "AVI"), ("wmv", "WMV"),
        ("flv", "Flash Video"), ("m4v", "M4V"), ("mpg", "MPEG-PS"), ("mpeg", "MPEG"), ("ts", "MPEG-TS"),
        ("m2ts", "Blu-ray M2TS"), ("mts", "AVCHD MTS"), ("vob", "DVD VOB"), ("ogv", "Ogg Video"), ("3gp", "3GPP"),
        ("3g2", "3GPP2"), ("asf", "ASF"), ("f4v", "F4V"), ("mxf", "MXF"), ("nut", "NUT"), ("ivf", "IVF"),
    ]
    .iter()
    .map(|(e, n)| Fmt::new(e, n, Class::Video, ""))
    .collect()
}

fn audio_formats() -> Vec<Fmt> {
    [
        ("mp3", "MP3"), ("flac", "FLAC"), ("wav", "WAV"), ("ogg", "Ogg Vorbis"), ("opus", "Opus"), ("m4a", "M4A (AAC)"),
        ("aac", "AAC (ADTS)"), ("wma", "WMA"), ("aiff", "AIFF"), ("ac3", "Dolby AC-3"), ("eac3", "Dolby E-AC-3"),
        ("mp2", "MP2"), ("mka", "Matroska Audio"), ("oga", "Ogg Audio"), ("m4b", tr!("Audiokniha M4B", "Audiobook M4B")), ("caf", "Core Audio"),
        ("au", "Sun AU"), ("amr", "AMR"), ("w64", "Sony Wave64"), ("wv", "WavPack"), ("tta", "TTA"), ("dts", "DTS"),
        ("mlp", "MLP"), ("spx", "Speex"), ("aif", "AIF"), ("3ga", "3GPP Audio"),
    ]
    .iter()
    .map(|(e, n)| Fmt::new(e, n, Class::Audio, ""))
    .collect()
}

fn doc_formats(si: &SysInfo) -> Vec<Fmt> {
    // (přípona, název, pandoc writer)
    let all: &[(&str, &str, &str)] = &[
        ("md", "Markdown", "markdown"), ("md", "GitHub Markdown", "gfm"), ("html", "HTML", "html5"), ("docx", "Word", "docx"),
        ("odt", "OpenDocument", "odt"), ("rtf", "RTF", "rtf"), ("epub", "EPUB", "epub3"), ("fb2", "FictionBook", "fb2"),
        ("tex", "LaTeX", "latex"), ("txt", tr!("Prostý text", "Plain text"), "plain"), ("rst", "reStructuredText", "rst"), ("org", "Org-mode", "org"),
        ("textile", "Textile", "textile"), ("wiki", "MediaWiki", "mediawiki"), ("adoc", "AsciiDoc", "asciidoc"),
        ("ipynb", "Jupyter Notebook", "ipynb"), ("pptx", "PowerPoint", "pptx"), ("xml", "DocBook", "docbook"),
        ("jats", "JATS XML", "jats"), ("1", "Man page", "man"), ("opml", "OPML", "opml"), ("typ", "Typst", "typst"),
        ("json", "Pandoc JSON", "json"), ("icml", "InDesign ICML", "icml"), ("texi", "Texinfo", "texinfo"),
        ("tei", "TEI", "tei"), ("dokuwiki", "DokuWiki", "dokuwiki"), ("muse", "Muse", "muse"), ("jira", "Jira", "jira"),
        ("bbcode", "BBCode", "bbcode"), ("ans", "ANSI text", "ansi"), ("djot", "Djot", "djot"),
        ("slides.html", tr!("Prezentace (reveal.js)", "Presentation (reveal.js)"), "revealjs"), ("beamer.tex", "Beamer (LaTeX)", "beamer"),
    ];
    all.iter()
        .filter(|(_, _, w)| si.pandoc_out.is_empty() || si.pandoc_out.iter().any(|o| o == w))
        .map(|(e, n, w)| Fmt::new(e, n, Class::Doc, w))
        .collect()
}

/// Pandoc reader pro příponu vstupu.
pub fn pandoc_reader(ext: &str) -> &'static str {
    match ext {
        "docx" => "docx",
        "odt" | "ott" => "odt",
        "rtf" => "rtf",
        "md" | "markdown" => "markdown",
        "html" | "htm" | "xhtml" => "html",
        "epub" => "epub",
        "fb2" => "fb2",
        "tex" | "latex" => "latex",
        "rst" => "rst",
        "org" => "org",
        "textile" => "textile",
        "wiki" | "mediawiki" => "mediawiki",
        "dokuwiki" => "dokuwiki",
        "muse" => "muse",
        "t2t" => "t2t",
        "ipynb" => "ipynb",
        "csv" => "csv",
        "tsv" => "tsv",
        "opml" => "opml",
        "bib" | "bibtex" => "biblatex",
        "ris" => "ris",
        "enl" => "endnotexml",
        "typ" => "typst",
        "xml" | "docbook" => "docbook",
        "jats" => "jats",
        "man" => "man",
        "creole" => "creole",
        "twiki" => "twiki",
        "tikiwiki" => "tikiwiki",
        "vimwiki" => "vimwiki",
        "djot" => "djot",
        "pptx" => "pptx",
        _ => "markdown",
    }
}

pub fn image_formats(si: &SysInfo) -> Vec<Fmt> {
    let mut v: Vec<Fmt> = Vec::new();
    let writable: Vec<String> = if si.magick_write.is_empty() {
        IMAGE_OUT_COMMON.iter().map(|s| s.to_string()).collect()
    } else {
        si.magick_write.iter().cloned().collect()
    };
    // oblíbené nejdřív
    for c in IMAGE_OUT_COMMON {
        if writable.iter().any(|w| w == c) || (*c == "jpg" && writable.iter().any(|w| w == "jpeg")) {
            v.push(Fmt::new(c, &c.to_uppercase(), Class::Image, if *c == "jpg" { "jpeg" } else { c }));
        }
    }
    for w in writable {
        let skip = ["pdf", "jpeg", "jpg", "tif"].contains(&w.as_str()) || v.iter().any(|f| f.ext == w || f.extra == w);
        let junk = w.len() > 6 || w.starts_with(char::is_numeric) && w.len() > 3;
        if !skip && !junk {
            v.push(Fmt::new(&w, &w.to_uppercase(), Class::Image, &w));
        }
    }
    v
}

fn archive_formats() -> Vec<Fmt> {
    [
        ("zip", "ZIP"), ("7z", "7-Zip"), ("tar", "TAR"), ("tar.gz", "TAR + gzip"), ("tar.bz2", "TAR + bzip2"),
        ("tar.xz", "TAR + xz"), ("tar.zst", "TAR + Zstandard"),
    ]
    .iter()
    .map(|(e, n)| Fmt::new(e, n, Class::Archive, ""))
    .collect()
}

fn subtitle_formats() -> Vec<Fmt> {
    [("srt", "SubRip"), ("ass", "ASS"), ("ssa", "SSA"), ("vtt", "WebVTT"), ("sub", "MicroDVD"), ("lrc", "LRC"), ("ttml", "TTML")]
        .iter()
        .map(|(e, n)| Fmt::new(e, n, Class::Subtitle, ""))
        .collect()
}

/// Všechny cíle pro daný vstup (vytvoří se při přidání souboru).
pub fn targets_for(kind: Kind, in_ext: &str, si: &SysInfo) -> Vec<Fmt> {
    let mut v = Vec::new();
    match kind {
        Kind::Video => {
            v.extend(video_containers());
            v.extend(audio_formats());
            v.push(Fmt::new("gif", tr!("Animované GIF", "Animated GIF"), Class::AnimImage, ""));
            v.push(Fmt::new("apng", tr!("Animované PNG", "Animated PNG"), Class::AnimImage, ""));
            v.push(Fmt::new("webp", tr!("Animované WebP", "Animated WebP"), Class::AnimImage, ""));
            for (e, n) in [("png", tr!("PNG snímek", "PNG frame")), ("jpg", tr!("JPG snímek", "JPG frame")), ("webp", tr!("WebP snímek", "WebP frame")), ("bmp", tr!("BMP snímek", "BMP frame")), ("tiff", tr!("TIFF snímek", "TIFF frame"))] {
                v.push(Fmt::new(e, n, Class::FrameImage, ""));
            }
        }
        Kind::Audio => v.extend(audio_formats()),
        Kind::Image => {
            v.extend(image_formats(si));
            v.push(Fmt::new("pdf", "PDF", Class::Pdf, ""));
            if ["gif", "apng", "webp", "avif", "mng", "jxl"].contains(&in_ext) {
                for c in ["mp4", "webm", "mkv", "mov", "avi"] {
                    v.push(Fmt::new(c, &format!("{} (video)", c.to_uppercase()), Class::Video, ""));
                }
            }
        }
        Kind::Document => {
            v.extend(doc_formats(si));
            v.push(Fmt::new("pdf", "PDF", Class::Pdf, ""));
        }
        Kind::Pdf => {
            v.push(Fmt::new("txt", "Text", Class::Text, ""));
            // text z PDF -> libovolný dokumentový formát (přes pandoc)
            v.extend(doc_formats(si).into_iter().filter(|f| f.extra != "plain" && f.extra != "json" && f.extra != "revealjs" && f.extra != "beamer"));
            for f in image_formats(si).into_iter().filter(|f| f.ext != "pdf") {
                v.push(f);
            }
        }
        Kind::Archive => {
            v.push(Fmt::new("folder", tr!("Rozbalit do složky", "Extract to a folder"), Class::Archive, "extract"));
            v.extend(archive_formats());
        }
        Kind::Folder => v.extend(archive_formats()),
        Kind::Subtitle => v.extend(subtitle_formats()),
        Kind::Other => {}
    }
    // nestačí-li pandoc/ffmpeg encodery, nabídka se stejně necenzuruje – chyby jsou čitelné
    v.retain(|f| !(f.class == Class::Archive && f.ext == in_ext));
    v
}

# Third-party notices

TomasekConvert bundles or links the following components. Licences are summarised here; always check the upstream
projects for the authoritative text. Regenerate the Rust crate list with `cargo install cargo-about` /
`cargo-license` before a release.

## Bundled programs (embedded in release binaries, started as separate processes)

| Component | Licence | Source |
|---|---|---|
| FFmpeg + FFprobe (GPL build: x264, x265, libvpx, SVT-AV1, libaom, rav1e, dav1d, opus, lame, vorbis, webp, jxl …) | GPL-3.0-or-later (components under GPL/LGPL/BSD) | https://ffmpeg.org · builds: https://github.com/BtbN/FFmpeg-Builds (macOS: https://ffmpeg.martin-riedl.de) |
| ImageMagick (Windows portable) | ImageMagick License (Apache-2.0-like) | https://imagemagick.org |
| Pandoc | GPL-2.0-or-later | https://pandoc.org |
| Typst | Apache-2.0 | https://github.com/typst/typst |
| qpdf (Windows) | Apache-2.0 | https://github.com/qpdf/qpdf |
| PDFium (via pdfium-binaries) | BSD-3-Clause / Apache-2.0 (with third-party components) | https://github.com/bblanchon/pdfium-binaries |

## Rust crates (non-exhaustive; mostly MIT or MIT/Apache-2.0)

eframe / egui (MIT/Apache-2.0) · egui-phosphor and the Phosphor icon set (MIT) · rfd · sysinfo · serde / serde_json ·
zip · sevenz-rust2 (Apache-2.0) · tar · flate2 · bzip2 · liblzma · zstd · unrar (wraps UnRAR – see
docs/LICENSE_AND_DISTRIBUTION.md) · walkdir · pdfium-render · image · ureq · tray-icon · windows · winresource.

## Fonts and icons

egui default fonts (Ubuntu Light: Ubuntu Font Licence; Hack: MIT; Noto Emoji / emoji-icon-font: OFL / MIT),
Typst embedded fonts (Libertinus Serif etc.: OFL). Segoe UI is only *used* from the Windows installation, never bundled.
The TomasekConvert icon (purple “T”) is original artwork.

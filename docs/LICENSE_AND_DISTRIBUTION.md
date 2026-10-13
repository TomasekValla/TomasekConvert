# Licence and distribution

**Decision: GPL-3.0-or-later** (`LICENSE`, `license` in `Cargo.toml`). The notes below explain why and what to do for releases.

*This is practical guidance, not legal advice. If you distribute widely or commercially, ask a lawyer.*

## What is in a release binary

The release builds embed **FFmpeg built as GPL** (with x264 / x265, which are GPL), **Pandoc** (GPL-2.0-or-later), plus
ImageMagick, Typst, qpdf, PDFium and many Rust crates under permissive licences (see `THIRD_PARTY_NOTICES.md`).
TomasekConvert starts these programs as separate processes and links no GPL code into its own executable
(PDFium is loaded as a library but is BSD/Apache licensed).

## Options for the repository licence

| Option | Pros | Cons |
|---|---|---|
| **GPL-3.0-or-later** (recommended) | Matches the bundled GPL tools, so one clear rule for everything you ship; prevents closed-source forks; simplest to explain | Copyleft – others must share modifications under the same licence |
| **Apache-2.0 or MIT** for your own code | Maximum freedom for others; fine legally while the GPL tools stay *separate programs* | Binaries that **bundle** FFmpeg / Pandoc must still comply with their GPL terms (licence texts, source offer); more paperwork and easier to get wrong |

**Recommendation:** use **GPL-3.0-or-later** for the repository. If you ever want a permissive licence, offer a “lite”
build (`--no-default-features`) that bundles nothing and lets users install FFmpeg themselves.

## What to do when releasing binaries

1. `LICENSE` and `license = "GPL-3.0-or-later"` are in place (holder: “TomasekConvert contributors”; the app itself shows
   no copyright text).
2. Ship/keep `THIRD_PARTY_NOTICES.md` and link to it in each release.
3. GPL “source offer”: the repository (tag = release) plus the exact tool versions/URLs (`build.rs`, release notes) is a
   reasonable way to provide corresponding source; note the FFmpeg build you embedded (BtbN build date).
4. Don't enable FFmpeg `--enable-nonfree` builds (not redistributable). The default BtbN GPL builds are fine.
5. Codec patents (H.264, HEVC, AAC …): software encoders are used for personal conversion; if you ever distribute
   commercially, check the patent-pool situation in your target countries. Hardware encoders (NVENC, AMF, QSV,
   VideoToolbox) are provided by the user's GPU driver and are not shipped with the app.
6. **UnRAR**: the `unrar` crate wraps RARLAB's UnRAR source, whose licence allows extracting RAR but forbids using it to
   re-create the RAR compression algorithm. Extraction only is what this app does.
7. Fonts: the app uses system fonts (Segoe UI on Windows) and egui's bundled fonts (OFL / Ubuntu licence); Typst embeds
   Libertinus Serif and others (OFL).

## Trademarks and names

“TomasekConvert” is your own name. Don't use VERT's name or logo; the layout is merely inspired by it.
Names like NVIDIA NVENC, AMD AMF, Intel Quick Sync and Apple VideoToolbox are used descriptively.

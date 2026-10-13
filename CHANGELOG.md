# Changelog

## 2.2.0
- **First-run setup**: language, theme and accent, startup / tray, shortcut and “Open with”, update preferences.
- **Updates from GitHub, no registration**: opt-in check; one click downloads the release (SHA-256 verified) and installs it –
  Windows runs the installer silently (settings are kept, the app restarts), portable exe / Linux binary is replaced in place.
  Optional automatic install while the app sits in the tray and is idle. Repository: `TomasekValla/TomasekConvert`.
- **Idle CPU ≈ 0**: no periodic repaint, no heartbeat file; single-instance detection uses an OS file lock.
- **Faster batches**: “Parallel jobs = Auto” schedules by weight (small files side by side, heavy video encodes take more of
  the CPU / a GPU slot) instead of a fixed 2.
- **PDF text fixed**: PDF → TXT / MD / DOCX / EPUB … read text character by character; PDFs whose lines pdfium split into
  fragments (e.g. from wkhtmltopdf) no longer produce garbage.
- **Remux**: .mp4 / .mov / .webm / .ts take one video track and only codecs the container really supports (red tags
  say why a track is skipped); several audio tracks are kept with exactly one default.
- **Themes**: two full palettes with checked contrast; switching dark ↔ light repeatedly no longer breaks colours.
- **UI**: top bar really centred (icon-only tabs in narrow windows), real source values (60 fps, 1920 × 1080, 48000 Hz)
  instead of “Source”, working ⓘ info buttons (hover + click), format picker keeps its size while searching,
  “Show in folder” selects the file (Explorer / Finder / file manager).
- **Linux / macOS**: qpdf is bundled (official portable build); built-in Rust converter for common image formats when
  ImageMagick is not installed.
- Licence: GPL-3.0-or-later.

## 2.1.0
- English (default) and Czech UI; dark / light theme and configurable accent colour.
- Neutral hardware support: NVIDIA NVENC, AMD AMF, Intel Quick Sync, VA-API, Apple VideoToolbox (H.264 / HEVC / AV1)
  with automatic CPU fallback; capabilities verified by test encodes and listed in Settings.
- AVX-512 detection and use for CPU encoding (x265, SVT-AV1).
- Basic / Advanced option panels with ⓘ tooltips; “Source” shown instead of 0; source values next to every field;
  full CPU preset ladder; custom resolution, FPS, bitrate, pixel format, sample rate, channels, bit depth.
- PDF → Markdown / DOCX / ODT / HTML / EPUB …; batch “convert all to …”; merge to one PDF.
- Fixed: EPUB / DOCX → PDF failing on images (Typst), format search closing its popup, overlapping file header,
  wrong container label (.mp4 shown as .mov), concurrent jobs writing the same output name.
- Windows / Linux integration: autostart (optionally in tray), “Open with”, context menu, Start-menu entry, tray icon;
  single running instance.
- Opt-in update check; installers (Windows Inno Setup, Linux install.sh / .deb, macOS .dmg); GitHub Actions for 5 targets.
- Tools are bundled for Windows (x64 / ARM64), Linux (x64 / ARM64) and macOS (arm64).

## 2.0.0
- Rewrite from Python to Rust with a native GUI.

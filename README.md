# TomasekConvert

> Beautifully crafted, insanely fast and local conversion. Don't touch the internet when you don't need it.

A fast, private desktop file converter written in Rust. Images, audio, video, documents, PDF, archives and subtitles –
everything is processed on your computer. The release builds ship with the tools they need (FFmpeg, Pandoc, Typst,
PDFium, …) inside a single executable.

*Česká verze: [README.cs.md](README.cs.md)*

## Features

- **Convert (almost) anything** – formats come from the bundled tools: FFmpeg, ImageMagick, Pandoc, Typst, qpdf, PDFium
  plus built-in ZIP / 7z / TAR / GZ / BZ2 / XZ / ZST / RAR support.
- **Hardware encoding** where your machine has it: NVIDIA NVENC, AMD AMF, Intel Quick Sync, VA-API (Linux) and Apple
  VideoToolbox for H.264 / HEVC / AV1. What the machine really supports is verified with a test encode at startup;
  everything else automatically falls back to the CPU – also if a GPU encode fails midway.
- **AVX-512** is detected and used for CPU encoding of HEVC (x265) and AV1 (SVT-AV1), which leave it off by default (can be turned off in Settings).
- **Full control, beginner friendly** – *Basic* mode shows only the essentials, *Advanced* mode exposes every value
  (resolution, FPS, bit depth / pixel format, bitrate, GOP, presets, sample rate, channels, custom ffmpeg arguments …),
  each with an ⓘ tooltip. Next to every field you see what the *source* file has.
- **Batch queue** with progress, elapsed time and ETA, parallel jobs, cancel, and “convert all to …” for mixed files.
- **Tools**: PDF (merge with drag & drop ordering, anything → PDF, split, pages, rotate, optimise, password),
  video → audio, track list with stream-copy remux, file info (ffprobe) with raw JSON.
- **PDF → Markdown / DOCX / ODT / HTML / EPUB / images / text.**
- **Statistics**: lifetime counters, CPU vs GPU, favourite formats, history.
- **English and Czech** UI, dark / light theme and a configurable accent colour.
- **Windows / Linux integration**: autostart (optionally straight to the tray), “Open with”, right-click menu, Start menu
  entry, tray icon (Windows). A second launch hands its files over to the running instance.
- **Private by design**: no telemetry, no network access – except the optional update check (and the update download you confirm).

## Download

Get installers and portable files from the [Releases](../../releases) page:

| Platform | Files |
|---|---|
| Windows x64 / ARM64 | `TomasekConvert-Setup-x64.exe`, `…-arm64.exe` (installer with options) or the portable `.exe` |
| Linux x64 / ARM64 | `.tar.gz` (with `install.sh`) or `.deb` |
| macOS (Apple silicon) | `.dmg` |

> Release binaries are not code-signed yet. Windows SmartScreen may warn on first launch (“More info → Run anyway”);
> on macOS use right-click → Open the first time.

Linux needs a working OpenGL driver; qpdf is bundled, ImageMagick (optional, for rare image formats) comes from your package manager
(the `.deb` pulls them in). Windows builds bundle everything.

## Build from source

See [docs/BUILDING.md](docs/BUILDING.md). Short version:

```bash
cargo build --release          # downloads and embeds the tools on first build (~500 MB, cached in vendor-cache/)
cargo build --release --no-default-features   # no bundled tools: uses ffmpeg, pandoc, … from PATH
cargo test --no-default-features              # integration tests (needs ffmpeg, pandoc, qpdf, imagemagick in PATH)
```

## Project documents

- [docs/BUILDING.md](docs/BUILDING.md) – compiling, features, offline builds
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) – how the code is organised
- [docs/RELEASING.md](docs/RELEASING.md) – releases, installers, updating
- [docs/LICENSE_AND_DISTRIBUTION.md](docs/LICENSE_AND_DISTRIBUTION.md) – licence recommendation and legal notes
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) – bundled components and their licences
- [AGENTS.md](AGENTS.md) – brief for AI coding agents (Antigravity, Claude Code, …)

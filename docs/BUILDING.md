# Building

## Requirements

| Platform | What you need |
|---|---|
| Windows | Rust (`rustup`, MSVC toolchain), *Visual Studio Build Tools* with the “Desktop development with C++” workload (includes the Windows SDK → icon embedding) |
| Linux | Rust, a C compiler (`build-essential`), `pkg-config`. Runtime: OpenGL + `libxkbcommon` |
| macOS | Rust, Xcode command line tools |

The Rust version is pinned only by `rust-version` in `Cargo.toml`; `Cargo.lock` is committed so builds are reproducible.

## Bundled tools

`build.rs` downloads the tools for the **target** platform once, compresses them (zstd) and embeds them in the
executable. They are cached in `vendor-cache/<os>-<arch>/`. At first start the app unpacks them into its data
directory (`%LOCALAPPDATA%\TomasekConvert\tools`, `~/.local/share/TomasekConvert/tools`, …). **The app itself never
downloads anything at run time.**

| Tool | Windows x64 / ARM64 | Linux x64 / ARM64 | macOS arm64 |
|---|---|---|---|
| FFmpeg + ffprobe | BtbN GPL build (native ARM64) | BtbN GPL build | martin-riedl.de static build |
| Pandoc | official (ARM64 uses the x64 build under emulation) | official | official |
| Typst | official | official (musl) | official |
| PDFium | pdfium-binaries | pdfium-binaries | pdfium-binaries |
| ImageMagick, qpdf | bundled (x64 build also on ARM64) | qpdf bundled; ImageMagick **system package** (optional – built-in converter for common formats) | qpdf bundled; ImageMagick **system package** (`brew install imagemagick`, optional) |

If a tool is not bundled the app uses the one found in `PATH`.

### Cargo features

```
default = bundle-ffmpeg, bundle-magick, bundle-pandoc, bundle-typst, bundle-pdfium, bundle-qpdf
```

- `--no-default-features` – nothing is downloaded or embedded (fast, great for development; needs the tools in `PATH`).
- `--no-default-features --features bundle-ffmpeg,bundle-pdfium` – pick only some tools.
- `TC_NO_BUNDLE=1` – disables bundling without touching features.

### Environment variables

| Variable | Meaning |
|---|---|
| `TC_FFMPEG_URL`, `TC_PANDOC_URL`, `TC_TYPST_URL`, `TC_PDFIUM_URL`, `TC_MAGICK_URL`, `TC_QPDF_URL` | use a different archive (mirror / pinned version) |
| `TC_ZSTD_LEVEL` | compression level of the embedded tools (default 12; lower = faster build, bigger exe) |
| `TC_PDFIUM` (run time, development) | path to a `libpdfium` when it is not bundled |
| `TC_VAAPI_DEVICE` (run time) | VA-API render device, default `/dev/dri/renderD128` |

Behind a firewall? Put the downloaded archives into `vendor-cache/<os>-<arch>/downloads/` (file names as in the URLs).

## Tests

```bash
cargo test --no-default-features -- --test-threads=1
```

The integration tests (`src/selftest.rs`) run real conversions: video / audio / image / document / archive / PDF
operations, track remuxing, cancelling, progress, GPU → CPU fallback. They need `ffmpeg`, `pandoc`, `qpdf` and
`imagemagick` in `PATH` (and optionally `typst` and `TC_PDFIUM`). GPU encoders cannot be tested without hardware; their
command lines are unit-tested and their option names are verified against `ffmpeg -h encoder=…`.

## Debug shortcuts

Environment variables read at start-up: `TC_FILES=a;b`, `TC_TAB=queue|tools|info|stats|settings|about`,
`TC_TOOL=pdf|audio|tracks`, `TC_OPEN=1` (open all option panels), `TC_INFO=file`, `TC_TRACKS=file`.

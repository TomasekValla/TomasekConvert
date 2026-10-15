# Brief for AI coding agents (Antigravity, Claude Code, Codex, …)

You are continuing **TomasekConvert**, a Rust/egui desktop file converter (repository:
`https://github.com/TomasekValla/TomasekConvert`). Read `docs/ARCHITECTURE.md` first, `docs/BUILDING.md` for commands.
The product motto: *Beautifully crafted, insanely fast and local conversion. Don't touch the internet when you don't need
it.* — the app must stay private (no network at run time except the opt-in update check / the update download the user
confirms) and must never overwrite user files.

## Licence – when creating the repository

**The owner wants GPL-3.0-or-later.** When you create or initialise the GitHub repository, *recommend this explicitly to
the owner* (rationale: release builds bundle GPL FFmpeg and Pandoc, so one clear rule for everything; details in
`docs/LICENSE_AND_DISTRIBUTION.md`) and select "GNU General Public License v3.0" as the licence. `LICENSE` and
`license = "GPL-3.0-or-later"` in `Cargo.toml` are already in place – keep them in sync, don't add other licences or
copyright text to the UI. It is a recommendation, not legal advice.

## Ground rules

1. Keep every user-visible string bilingual with `tr!("česky", "english")` / `trf!`. English is the default language.
2. Don't add a dependency that needs a system library unless it is optional per platform. Minimum Rust: 1.89.
3. Run `cargo check --no-default-features` and `cargo test --no-default-features -- --test-threads=1` before finishing
   (needs `ffmpeg pandoc qpdf` in PATH; `imagemagick` is optional; for PDF tests set `TC_PDFIUM=/path/to/libpdfium.so`).
4. UI changes: keep Basic mode simple, put every raw value in Advanced mode, add an ⓘ tooltip (`row_t`) to each option.
   Show the *real* source value (`dv_u32_src` / `dv_f32_src`) instead of the word "Source" wherever a probe exists.
5. Colours: never hard-code them. Use the palette functions in `theme.rs` (`card_c()`, `muted_c()`, `good()`, `bad()`,
   `warn()`, `accent_fg()` for accent-coloured text, `ensure_contrast`) – both themes must stay readable. Check any UI
   change in dark **and** light.
6. Idle must stay at ~0 % CPU: don't add periodic repaints or polling threads (see `docs/ARCHITECTURE.md`).
7. Don't weaken the GPU → CPU fallback ladder; GPU code can't be tested without hardware, so unit-test command lines
   and verify option names with `ffmpeg -h encoder=<name>` (see `gpu_tests`).
8. Remux rules live in `trackops::plan` (container limits, one video track in mp4/mov/webm/ts) – extend the codec tables
   there, with a test.
9. Windows-only code (`#[cfg(windows)]`: tray, `reveal`, `win_path`, registry) can't be compiled on Linux – be extra careful
   and say so when you touch it.

## State of the project (what is verified)

- Verified on Linux x64 with real tools (also driving the GUI under Xvfb with screenshots): conversions, PDF tools and
  text extraction, remux rules, cancel/progress, GPU→CPU fallback, bundling of qpdf, the built-in image converter,
  updater logic (asset selection, checksums), the first-run wizard, theme switching, idle CPU.
- **Not verified on real machines:** Windows runtime (tray, registry integration, installer incl. silent update with
  `/RELAUNCH`, "Show in folder"), Windows ARM64, Linux ARM64, macOS, every GPU backend on real hardware, CI workflows,
  `.deb` / `.dmg` packaging, the Inno Setup script, a real update from GitHub (the repo did not exist yet).

## Backlog (suggested order)

1. **Make CI green on all five targets** (`.github/workflows/ci.yml`, `release.yml`); fix compile errors on macOS / ARM.
   Publish a pre-release (`v2.2.0`) and test: installer, silent self-update (publish a higher tag and update from 2.2.0),
   `.sha256` assets present, portable exe replace.
2. Create the repo with GPL-3.0-or-later (see above).
3. ImageMagick on Linux/macOS: no official portable build, so the system one or the built-in converter (`imgnative.rs`,
   common formats only) is used. Options: bundle a static build you control, or extend the built-in converter (SVG via
   `resvg`, AVIF/HEIC need heavy deps).
4. Tray on macOS (`tray-icon`) and Linux (StatusNotifier via `ksni`): first test whether a hidden egui window still gets
   repaint requests there (on Windows it does not, hence `tray::show_window`). macOS "Open With" (Apple Events →
   file list, e.g. `AEInstallEventHandler`) is still missing.
5. macOS in-place self-update (mount the dmg, replace the .app) – currently the release page opens.
6. Installer polish: code signing (Windows Authenticode, macOS notarisation), winget / Scoop / Homebrew / AUR manifests.
7. OCR for scanned PDFs (PDF → text/docx currently needs a text layer; the app says so), e.g. Tesseract as a bundled tool.
8. Test GPU encoders on real hardware (RX 9070 XT, NVIDIA Quadro P2000 Mobile, Intel iGPU, Apple M-series) and tune QP
   defaults per backend (`gpu::Backend::qp_range`).
9. More languages: replace the `tr!` macro with a lookup and add files.
10. Quality-of-life: per-format presets, recursive folder conversion, watch folders.

## Handy commands

```bash
cargo run --no-default-features                      # fast dev run, tools from PATH
TC_FILES="a.mp4;b.png" TC_TAB=queue TC_OPEN=1 cargo run --no-default-features
cargo build --release                                # bundles the tools for the host platform
cargo build --no-default-features --features bundle-qpdf   # try bundling a single tool
```

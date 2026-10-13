# Architecture

Single Rust binary (`TomasekConvert`), GUI with **egui / eframe 0.33** (glow backend). Heavy lifting is done by external
tools started as child processes (FFmpeg, ImageMagick, Pandoc, Typst, qpdf) and by Rust crates (PDFium bindings, archives).

```
src/
  main.rs        entry point: CLI args, single-instance hand-over, window options
  app.rs         App state, task queue (start_task), navigation, settings, update loop
  i18n.rs        tr!("česky", "english") and trf!(…) macros; language is a global (English default)
  theme.rs       two palettes (dark/light) via functions `card_c() muted_c() good() …`, accent, contrast helpers,
                 fonts, progress bar, small widgets. Never hard-code colours – use the palette / `ensure_contrast`.
  ui_*.rs        one file per tab (home, queue, tools, pdf, info, stats, settings) + ui_opts (option panels)
  formats.rs     what is what: extension tables, targets per input kind (Kind / Class)
  opts.rs        Opts (all user-tunable values), codec tables (VCODECS, ACODECS)
  convert.rs     router: (input kind, target class) → engine
  ffmpeg.rs      video / audio / animation / subtitles; builds ffmpeg command lines; GPU → CPU fallback ladder
  gpu.rs         hardware encoder backends (NVENC, AMF, QSV, VA-API, VideoToolbox): names, QP ranges, probe args
  hw.rs          system detection: CPU (incl. AVX-512), GPUs, encoders/decoders that really work (test encodes)
  image.rs       ImageMagick
  imgnative.rs   built-in pure-Rust converter for common formats (used when ImageMagick is missing)
  doc.rs         Pandoc (+ Typst two-step for PDF output)
  pdfops.rs      PDF → images/text/documents (PDFium), PDF tools (qpdf), text re-flow heuristics
  trackops.rs    remux (stream copy) and single-track extraction
  archive.rs     zip / 7z / tar.* / gz / bz2 / xz / zst / rar (read) in pure Rust (+ unrar)
  probe.rs       ffprobe JSON → Probe / Stream
  job.rs         Shared (progress/cancel), JobCtx, run_cmd (child process + progress parsing + cancel)
  stats.rs       persistent lifetime statistics (stats.json)
  tools.rs       unpacks embedded tools (build.rs output) or falls back to PATH
  shell.rs       OS integration: autostart, “Open with”, context menu, Start-menu shortcut
  instance.rs    single instance: lock file + inbox directory (no sockets)
  tray.rs        system tray (Windows) + Win32 helper to restore the window
  update.rs      opt-in update check + download (SHA-256) + install (Windows installer / in-place replace)
  ui_setup.rs    first-run wizard and the update banner
  selftest.rs    integration tests (cfg(test))
build.rs         downloads tools per target, zstd-compresses them, embeds icon/version info (Windows)
```

## Key flows

**Task model.** `App::start_task(meta, closure)` spawns a worker thread; progress and cancel live in `job::Shared`
(atomics). Workers wait for a slot: `Slots` is a weighted scheduler (`parallel` = 0 → Auto: capacity = CPU threads (2–12), small
jobs weigh 1, GPU video 3, CPU video threads/2; a number > 0 means exactly that many jobs). Completion arrives as `Bg::Done` and is recorded in `stats`.
Every converter returns `Outcome { outputs, bytes, engine, detail, media_secs }`.

**Conversion routing.** `formats::targets_for(kind, ext, si)` lists the targets for an input; `convert::run_convert`
picks the engine from `(Kind, Class)`. To add a format: extend the tables in `formats.rs`; to add an engine: add a
module and a match arm in `convert.rs`.

**Hardware encoding.** `hw::detect` runs a 0.2 s test encode per candidate encoder (in parallel). `ffmpeg::pick_gpu`
chooses a backend (auto order NVENC → AMF → VideoToolbox → QSV → VA-API, or the user's choice) if the encoder works and
supports the requested pixel format. `video_plans` returns a *ladder* – GPU encode + HW decode → GPU encode with CPU
decode → full CPU – and `ffmpeg::convert` tries them in order, so a flaky driver never fails a job.
CPU extras: AVX-512 is switched on for x265 (`asm=avx512`) and SVT-AV1 (`asm=max`) because they ignore it by default.

**PDF output.** Documents → PDF is two steps (`doc.rs`): Pandoc → `.typ` (images as relative paths, ids stripped by a Lua
filter), then `typst compile`. PDF → documents goes through PDFium text extraction with line/paragraph/heading
re-flow (`pdfops::page_text`, `reflow`) and Pandoc.

**Single instance.** The first process holds an exclusive OS lock on `instance.lock` (released by the OS even after a
crash – no heartbeat). Later launches write their file list into `inbox/`, a watcher thread (600 ms) in the first process
forwards it to the UI.

**Idle behaviour.** The UI is event driven: `update()` must not schedule repaints unless a task runs (250 ms) or an
animation needs it. Don't add periodic timers – idle CPU should stay ~0 (it was ~9 % in 2.1).

**i18n.** All user-visible strings use `tr!("cs", "en")` / `trf!("cs {x}", "en {x}", args…)`. Adding a language means
changing the macros (e.g. to a lookup table) – see AGENTS.md.

## Conventions

- Never block the UI thread: spawn a thread, send `Bg::…` messages, call `ctx.request_repaint()`.
- Never overwrite user files: use `util::unique_path` (reserves the name process-wide) and write to `*.tcpart.*`,
  then `finish_part`.
- Child processes: always `util::command` (hides the console window on Windows).
- No network access at run time except `update::check`.

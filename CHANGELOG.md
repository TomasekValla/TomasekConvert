# Changelog

## 2.2.4

- **Idle CPU v traji (skutečná příčina)**: po schování okna do tray se smyčka událostí zasekla v `ControlFlow::Poll`
  a točila jedno vlákno CPU na 100 %. Nešlo o vsync ani o repaint z aplikace: knihovna eframe 0.33 po naplánovaném
  překreslení přepne smyčku na `Poll` a zavolá `request_redraw()`, jenže skryté okno ve Windows `RedrawRequested`
  nikdy nedostane, takže se `Poll` už nikdy nevrátil na `Wait`. Naplánované překreslení přitom přijde vždycky
  (při schování přijde `Focused(false)` a egui si pak samo vyžádá další snímek). Oprava je v upstreamu
  (egui#8326) až v novějších verzích, proto je eframe 0.33.3 přibalený ve `vendor/eframe` se záplatou
  (viz `vendor/eframe/PATCHES.md`): skryté okno se vykresluje jen na požádání (max. 1× za 100 ms)
  a smyčka se vždy vrátí do spánku.
- Odstraněno `ShowWindow(SW_HIDE)` z 2.2.3 (dělá totéž co `ViewportCommand::Visible(false)`) i
  `request_repaint_after(Duration::MAX)`, které v eframe 0.33 padalo na přetečení (`Instant + Duration`),
  a časovač 250 ms pro schované okno při převodu.
- Start rovnou do tray (`--tray`): okno se po prvním snímku už zbytečně neukáže.
- **Automatické aktualizace**: kontrola běží při startu **a každých 6 hodin** (appka často leží v tray celé dny
  a dřív se po startu už nikdy nepodívala). Kontrola i automatická instalace jsou ve výchozím nastavení zapnuté
  (jdou vypnout v průvodci i v Nastavení). Stažený update se (ověřený SHA-256) nainstaluje sám, jakmile je okno
  schované v tray a nic se nepřevádí; při otevřeném okně zůstává tlačítko „Restartovat a aktualizovat“.

## 2.2.3
- **Idle CPU v traji**: opravena zbývající příčina ~6% spotřeby CPU při skrytém okně v tray ikonce.
  `ViewportCommand::Visible(false)` nestačilo – winit/eframe stále dostával WM_PAINT události
  a točil event loop. Nyní se při schování do traje volá přímo Win32 `ShowWindow(SW_HIDE)`,
  čímž OS přestane posílat překreslovací zprávy. Zároveň se přidává `request_repaint_after(MAX)`
  jako pojistka – winit uspí event loop a probudí ho jen skutečná OS událost (klik na tray, zpráva
  z vlákna konverze).
- **VCRUNTIME140.dll**: instalátor nyní přibaluje Visual C++ Redistributable 2015–2022 a nainstaluje
  ho tiše, pokud na stroji chybí. Opravuje "System Error: VCRUNTIME140.dll was not found" na čistých
  Windows VM a PC bez Visual Studio / Microsoft Office.
- **Naming instalátorů**: Windows instalátor sjednocen na `TomasekConvert-setup-windows-<arch>.exe`
  (dříve `TomasekConvert-Setup-<arch>.exe`), aby odpovídal přenosné verzi `TomasekConvert-portable-windows-<arch>.exe`
  a bylo na první pohled jasné, pro jaký systém a typ instalace je určen. Updater automaticky rozpoznává
  nový i starý formát.

## 2.2.2
- **Auto-update**: the updater now correctly detects a new version is available. In 2.2.1 the installed version number
  matched the latest release tag exactly, so no update was offered (the version comparison returned "up to date").
  Releasing 2.2.2 resolves this – users on 2.2.1 will be prompted to update to 2.2.2 automatically.

## 2.2.1
- **Idle CPU**: a window hidden in the tray (or minimized) no longer lays out or repaints anything and requests no further
  frames – a hidden window does not wait for vsync, so any per-frame repaint request used to spin a whole core (~16 % on a
  6-core CPU). The top-bar width measurement is capped so it can never loop; the single-instance inbox is polled once a second
  with one cheap `read_dir`. Conversions and updates keep running in the background.
- **GPU detection**: test encodes of one backend run one after another (older GeForce cards allow only 2 concurrent NVENC
  sessions, so h264 + hevc + av1 at once failed randomly), unclear failures are retried once, a stuck test is stopped after 20 s,
  and the reason is now shown: *driver too old for the bundled FFmpeg* (NVENC needs driver 570+; GTX 10-series cards still get
  580.x), *driver library missing*, *GPU cannot do this codec* (e.g. AV1 on older cards). NVIDIA driver version is detected
  and shown next to the GPU; the footer warns when NVENC is unavailable only because of the driver.
- **Portable builds** carry `portable` in the name: `TomasekConvert-portable-windows-<arch>.exe`,
  `TomasekConvert-portable-linux-<arch>.tar.gz`. The updater understands both the new and the old names.
- **Format picker** (and the other drop-down lists) is taller: the popup was capped at 200 px; it now scales with the window.

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

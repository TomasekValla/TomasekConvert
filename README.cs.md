# TomasekConvert

> Beautifully crafted, insanely fast and local conversion. Don't touch the internet when you don't need it.

Inspirováno projektem [VERT.sh](https://vert.sh) ([GitHub](https://github.com/VERT-sh/VERT)) — přináší myšlenku čistě lokální a soukromé konverze souborů do podoby rychlé a plnohodnotné desktopové aplikace v Rustu.

Rychlý a soukromý konvertor souborů pro desktop napsaný v Rustu. Obrázky, audio, video, dokumenty, PDF, archivy a titulky –
všechno se zpracovává přímo na tvém počítači. Release verze obsahují potřebné nástroje (FFmpeg, Pandoc, Typst, PDFium…)
uvnitř jediného spustitelného souboru.

## Co umí

- **Převod (skoro) čehokoliv** – formáty vycházejí z vložených nástrojů (FFmpeg, ImageMagick, Pandoc, Typst, qpdf, PDFium)
  a z vestavěné podpory ZIP / 7z / TAR / GZ / BZ2 / XZ / ZST / RAR.
- **Hardwarové enkódování**: NVIDIA NVENC, AMD AMF, Intel Quick Sync, VA-API (Linux) a Apple VideoToolbox pro H.264 / HEVC / AV1.
  Co stroj opravdu umí, se ověří zkušebním převodem při startu; co GPU neumí (nebo když převod na GPU selže), jde automaticky na CPU.
- **Plná kontrola i pro začátečníky** – *Základní* režim ukazuje jen podstatné, *Pokročilý* všechny hodnoty
  (rozlišení, FPS, bitová hloubka / pixel format, bitrate, GOP, presety, vzorkování, kanály, vlastní ffmpeg argumenty…),
  u každé je ikonka ⓘ s vysvětlením a vedle pole vidíš, co má *zdrojový* soubor.
- **Fronta** s průběhem, uplynulým časem a ETA, souběžné převody, hromadná volba formátu pro různé typy souborů.
- **Nástroje**: PDF (sloučit s řazením přetažením, cokoliv → PDF, rozdělit, stránky, otočit, optimalizace, heslo),
  video → audio, seznam stop a remux bez překódování, info o souboru (ffprobe) s raw JSON.
- **PDF → Markdown / DOCX / ODT / HTML / EPUB / obrázky / text.**
- **Statistiky**, **čeština i angličtina**, tmavý / světlý vzhled a volitelná barva zvýraznění.
- **Integrace**: spuštění s počítačem (i rovnou do tray), „Otevřít v programu“, kontextové menu, položka v nabídce Start,
  tray ikona (Windows). Druhé spuštění předá soubory už běžící instanci.
- **Soukromí**: žádná telemetrie ani síť – kromě volitelného tlačítka *Zkontrolovat aktualizace*.

## Stažení a kompilace

Instalátory a přenosné soubory jsou na stránce [Releases](../../releases). Kompilace ze zdrojů: [docs/BUILDING.md](docs/BUILDING.md).
Pro Windows x64 stačí `cargo build --release` – první build stáhne a vloží všechny nástroje (cca 500 MB, uloží se do `vendor-cache/`).

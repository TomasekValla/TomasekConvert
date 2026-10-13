//! Záložka "Info": ffprobe v hezkých kartách + raw JSON + rychlé akce.
use crate::{app::*, formats::*, probe::*, theme::*, tools::Tools, util::*};
use egui::{Color32, RichText, Ui};
use egui_phosphor::regular as ph;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct InfoPage {
    pub path: Option<PathBuf>,
    probe: Option<Probe>,
    error: Option<String>,
    loading: bool,
    raw: bool,
    size: u64,
    pdf_pages: Option<u32>,
    kind: Option<Kind>,
}

impl InfoPage {
    pub fn begin(&mut self, p: &Path, tools: &Tools) {
        let ext = full_ext(p);
        *self = InfoPage { path: Some(p.to_path_buf()), loading: true, size: path_size(p), ..Default::default() };
        if ext == "pdf" {
            self.pdf_pages = crate::pdfops::page_count(tools, p, "");
        }
    }
    pub fn on_probe(&mut self, path: &Path, p: &Option<Probe>, err: Option<String>) {
        if self.path.as_deref() == Some(path) {
            self.loading = false;
            self.probe = p.clone();
            self.error = err;
        }
    }
}

fn kv(ui: &mut Ui, k: &str, v: impl Into<String>) {
    let v: String = v.into();
    if v.is_empty() || v == "unknown" || v == "0" {
        return;
    }
    ui.label(muted(k.to_string()).size(13.0));
    ui.label(RichText::new(v).font(bold(13.5)));
    ui.end_row();
}

fn info_text(name: &str, p: &Probe) -> String {
    let mut t = trf!("{name}\nKontejner: {} ({})\nDélka: {}\nVelikost: {}\nCelkový bitrate: {}\n", "{name}\nContainer: {} ({})\nDuration: {}\nSize: {}\nOverall bitrate: {}\n", p.format_name, p.format_long, fmt_dur(p.duration), fmt_bytes(p.size), fmt_bitrate(p.bit_rate));
    for s in &p.streams {
        t.push_str(&format!("\n#{} {} – {}", s.index, s.kind.label(), s.headline()));
        match s.kind {
            StreamKind::Video => {
                t.push_str(&format!("\n   profil: {} · pix_fmt: {} · {}-bit · bitrate: {}", s.profile, s.pix_fmt, s.bit_depth, fmt_bitrate(s.bit_rate)));
                if !s.hdr.is_empty() {
                    t.push_str(&format!(" · {}", s.hdr));
                }
            }
            _ => t.push_str(&format!(" · {} · {}", fmt_bitrate(s.bit_rate), s.lang)),
        }
        if !s.title.is_empty() {
            t.push_str(&format!(" · \"{}\"", s.title));
        }
    }
    if !p.chapters.is_empty() {
        t.push_str("\n\nKapitoly:");
        for c in &p.chapters {
            t.push_str(&format!("\n  {}  {}", fmt_dur(c.start), c.title));
        }
    }
    if !p.tags.is_empty() {
        t.push_str("\n\nTagy:");
        for (k, v) in &p.tags {
            t.push_str(&format!("\n  {k}: {v}"));
        }
    }
    t
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let mut page = std::mem::take(&mut app.info);
    let mut load: Option<PathBuf> = None;
    let mut act_convert = false;
    let mut act_tracks = false;
    let mut copy: Option<String> = None;

    ui.horizontal_wrapped(|ui| {
        title(ui, tr!("Info o souboru", "File info"), 30.0);
        ui.add_space(10.0);
        if primary_button(ui, &trf!("{}  Vybrat soubor", "{}  Choose file", ph::FOLDER_OPEN)).clicked() {
            load = rfd::FileDialog::new().set_title(tr!("Vyber soubor", "Select a file")).pick_file();
        }
    });
    ui.label(muted(tr!("Přetáhni sem libovolný soubor – video, audio, obrázek… – a uvidíš všechno, co ffprobe zjistí.", "Drop any file here – video, audio, image… – and see everything ffprobe finds.")));
    ui.add_space(6.0);

    let Some(path) = page.path.clone() else {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new(ph::INFO).size(52.0).color(muted_c()));
                ui.label(muted(tr!("Zatím žádný soubor", "No file yet")));
                ui.add_space(40.0);
            });
        });
        app.info = page;
        if let Some(p) = load {
            app.info_load(p);
        }
        return;
    };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

    // ── hlavička + akce ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&name).font(bold(20.0)));
                ui.label(muted(path.display().to_string()).size(12.5));
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ghost_button(ui, &trf!("{}  Převést", "{}  Convert", ph::ARROWS_CLOCKWISE)).clicked() {
                act_convert = true;
            }
            let has_streams = page.probe.as_ref().map(|p| !p.streams.is_empty()).unwrap_or(false);
            ui.add_enabled_ui(has_streams, |ui| {
                if ghost_button(ui, &trf!("{}  Vytáhnout stopu", "{}  Extract track", ph::DOWNLOAD_SIMPLE)).clicked() {
                    act_tracks = true;
                }
                if ghost_button(ui, &trf!("{}  Stripnout stopy", "{}  Strip tracks", ph::SCISSORS)).clicked() {
                    act_tracks = true;
                }
            });
            if let Some(p) = &page.probe {
                if ghost_button(ui, &trf!("{}  Zkopírovat info", "{}  Copy info", ph::COPY)).clicked() {
                    copy = Some(info_text(&name, p));
                }
                if chip(ui, page.raw, &format!("{}  Raw JSON", ph::BRACKETS_CURLY)).clicked() {
                    page.raw = !page.raw;
                }
            }
        });
    });

    if page.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(muted(tr!("Čtu soubor…", "Reading file…")));
        });
    }

    match page.probe.clone() {
        None if !page.loading => {
            let ext = full_ext(&path);
            let kind = detect_kind(&ext, &app.si);
            page.kind = Some(kind);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                heading_small(ui, tr!("Základní informace", "Basic information"));
                egui::Grid::new("basic").num_columns(2).spacing([20.0, 6.0]).show(ui, |ui| {
                    kv(ui, tr!("Velikost", "Size"), fmt_bytes(page.size));
                    kv(ui, tr!("Přípona", "Extension"), if ext.is_empty() { String::new() } else { format!(".{ext}") });
                    kv(ui, tr!("Rozpoznáno jako", "Detected as"), kind.label());
                    if let Some(n) = page.pdf_pages {
                        kv(ui, tr!("Stránek", "Pages"), n.to_string());
                    }
                });
                if let Some(e) = &page.error {
                    ui.label(muted(tr!("ffprobe tenhle typ souboru nepřečetl (u dokumentů a archivů je to normální).", "ffprobe could not read this file type (normal for documents and archives).")).size(12.5));
                    egui::CollapsingHeader::new(tr!("Chyba ffprobe", "ffprobe error")).show(ui, |ui| {
                        ui.label(RichText::new(e.clone()).monospace().size(12.0));
                    });
                }
            });
        }
        None => {}
        Some(p) => {
            if page.raw {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        heading_small(ui, tr!("Raw výstup ffprobe (JSON)", "Raw ffprobe output (JSON)"));
                        if ui.button(trf!("{}  Kopírovat JSON", "{}  Copy JSON", ph::COPY)).clicked() {
                            copy = Some(p.raw.clone());
                        }
                    });
                    egui::ScrollArea::both().max_height(460.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(p.raw.clone()).monospace().size(12.0)).extend());
                    });
                });
            }
            // ── přehledové dlaždice ──
            let cols = if ui.available_width() > 900.0 { 5 } else { 2 };
            let tiles = [
                (ph::PACKAGE, if p.format_long.is_empty() { p.format_name.clone() } else { p.format_name.split(',').next().unwrap_or("").to_string() }, tr!("kontejner", "container")),
                (ph::TIMER, fmt_dur(p.duration), tr!("délka", "duration")),
                (ph::DATABASE, fmt_bytes(p.size.max(page.size)), tr!("velikost", "size")),
                (ph::GAUGE, fmt_bitrate(p.bit_rate), tr!("celkový bitrate", "overall bitrate")),
                (ph::STACK, format!("{}", p.streams.len()), tr!("stop celkem", "tracks total")),
            ];
            for chunk in tiles.chunks(cols) {
                ui.columns(cols, |c| {
                    for (i, (ic, v, l)) in chunk.iter().enumerate() {
                        card(&mut c[i], |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                badge(ui, ic, accent(), 28.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(v.clone()).font(bold(18.0)));
                                    ui.label(muted(l.to_string()).size(12.0));
                                });
                            });
                        });
                    }
                });
            }
            if !p.format_long.is_empty() {
                ui.label(muted(p.format_long.clone()).size(12.5));
            }

            // ── stopy ──
            for s in &p.streams {
                let (ic, col) = match s.kind {
                    StreamKind::Video => (ph::FILM_STRIP, Color32::from_rgb(255, 140, 160)),
                    StreamKind::Audio => (ph::WAVEFORM, Color32::from_rgb(190, 150, 255)),
                    StreamKind::Subtitle => (ph::SUBTITLES, Color32::from_rgb(120, 220, 230)),
                    StreamKind::Attachment => (ph::PAPERCLIP, Color32::from_rgb(255, 215, 120)),
                    StreamKind::Data => (ph::DATABASE, muted_c()),
                };
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        badge(ui, ic, col, 30.0);
                        ui.label(RichText::new(trf!("Stopa #{} · {}", "Track #{} · {}", s.index, s.kind.label())).font(bold(16.0)));
                        ui.label(muted(s.headline()));
                        if s.default {
                            pill_tag(ui, tr!("výchozí", "default"), good());
                        }
                        if s.forced {
                            pill_tag(ui, "forced", warn());
                        }
                        if !s.hdr.is_empty() {
                            pill_tag(ui, &s.hdr, accent());
                        }
                    });
                    egui::Grid::new(("stream", s.index)).num_columns(4).spacing([24.0, 6.0]).show(ui, |ui| {
                        let mut pairs: Vec<(&str, String)> = vec![(tr!("Kodek", "Codec"), if s.codec_long.is_empty() { s.codec.clone() } else { s.codec_long.clone() }), (tr!("Profil", "Profile"), s.profile.clone())];
                        match s.kind {
                            StreamKind::Video => {
                                pairs.push((tr!("Rozlišení", "Resolution"), if s.width > 0 { format!("{}×{}", s.width, s.height) } else { String::new() }));
                                pairs.push(("FPS", if s.fps > 0.0 { format!("{:.3}", s.fps).trim_end_matches('0').trim_end_matches('.').to_string() } else { String::new() }));
                                pairs.push(("Pixel format", s.pix_fmt.clone()));
                                pairs.push((tr!("Bitová hloubka", "Bit depth"), if s.bit_depth > 0 { format!("{}-bit", s.bit_depth) } else { String::new() }));
                                pairs.push((tr!("Barevný prostor", "Colour space"), s.color_space.clone()));
                                pairs.push((tr!("Přenosová funkce", "Transfer function"), s.color_transfer.clone()));
                                pairs.push(("Primaries", s.color_primaries.clone()));
                                pairs.push((tr!("Rozsah", "Range"), s.color_range.clone()));
                                pairs.push(("HDR", s.hdr.clone()));
                                pairs.push((tr!("Snímků", "Frames"), if s.nb_frames > 0 { s.nb_frames.to_string() } else { String::new() }));
                            }
                            StreamKind::Audio => {
                                pairs.push((tr!("Kanály", "Channels"), if s.channels > 0 { format!("{} ({})", s.channels, s.channel_layout) } else { String::new() }));
                                pairs.push((tr!("Vzorkování", "Sample rate"), if s.sample_rate > 0 { format!("{} Hz", s.sample_rate) } else { String::new() }));
                                pairs.push((tr!("Bitová hloubka", "Bit depth"), if s.bit_depth > 0 { format!("{}-bit", s.bit_depth) } else { String::new() }));
                            }
                            StreamKind::Attachment => {
                                pairs.push((tr!("Soubor", "File"), s.filename.clone()));
                                pairs.push(("MIME", s.mimetype.clone()));
                            }
                            _ => {}
                        }
                        pairs.push(("Bitrate", if s.bit_rate > 0 { fmt_bitrate(s.bit_rate) } else { String::new() }));
                        pairs.push((tr!("Jazyk", "Language"), s.lang.clone()));
                        pairs.push((tr!("Název", "Title"), s.title.clone()));
                        pairs.push((tr!("Délka", "Duration"), if s.duration > 0.0 { fmt_dur(s.duration) } else { String::new() }));
                        let pairs: Vec<_> = pairs.into_iter().filter(|(_, v)| !v.is_empty() && v != "unknown").collect();
                        for (i, (k, v)) in pairs.iter().enumerate() {
                            ui.label(muted(k.to_string()).size(13.0));
                            ui.label(RichText::new(v.clone()).font(bold(13.5)));
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                });
            }

            // ── kapitoly ──
            if !p.chapters.is_empty() {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    heading_small(ui, &trf!("{}  Kapitoly ({})", "{}  Chapters ({})", ph::LIST_BULLETS, p.chapters.len()));
                    egui::ScrollArea::vertical().id_salt("chapters").max_height(220.0).show(ui, |ui| {
                        for c in &p.chapters {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(fmt_dur(c.start)).monospace().color(accent()));
                                ui.label(if c.title.is_empty() { "—".to_string() } else { c.title.clone() });
                                ui.label(muted(format!("({})", fmt_dur(c.end - c.start))).size(12.0));
                            });
                        }
                    });
                });
            }

            // ── tagy ──
            if !p.tags.is_empty() {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    heading_small(ui, &trf!("{}  Metadata a tagy", "{}  Metadata and tags", ph::TAG));
                    egui::Grid::new("tags").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                        for (k, v) in &p.tags {
                            ui.label(muted(k.clone()).size(13.0));
                            ui.add(egui::Label::new(RichText::new(v.clone()).font(bold(13.5))).wrap());
                            ui.end_row();
                        }
                    });
                });
            }
        }
    }

    app.info = page;
    if let Some(t) = copy {
        ui.ctx().copy_text(t);
        app.toast(tr!("Zkopírováno do schránky", "Copied to the clipboard"));
    }
    if act_convert {
        app.add_files(vec![path.clone()]);
    }
    if act_tracks {
        app.tracks_load(path);
    }
    if let Some(p) = load {
        app.info_load(p);
    }
}

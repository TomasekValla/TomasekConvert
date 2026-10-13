//! Záložka "Nahrát": úvod, dropzone a přehled podporovaných formátů.
use crate::{app::*, formats::*, theme::*};
use egui::{pos2, vec2, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, Ui};
use egui_phosphor::regular as ph;
use std::collections::BTreeSet;

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(26.0);
    let total_w = ui.available_width();
    let wide = total_w > 820.0;
    let mut picked: Vec<std::path::PathBuf> = Vec::new();

    let hero = |ui: &mut Ui| {
        ui.add_space(18.0);
        ui.label(RichText::new(tr!("Konvertor", "File")).font(bold(if wide { 58.0 } else { 38.0 })));
        ui.label(RichText::new(tr!("souborů", "converter")).font(bold(if wide { 58.0 } else { 38.0 })).color(accent()));
        ui.add_space(14.0);
        ui.label(muted(tr!("Obrázky, audio, video, dokumenty, PDF, archivy a titulky – všechno se zpracovává přímo na tvém počítači.", "Images, audio, video, documents, PDF, archives and subtitles – everything is processed right on your computer.")).size(17.0));
        ui.add_space(6.0);
        ui.label(muted(tr!("Plná kontrola nad kvalitou a kodeky, hardwarová akcelerace tam, kde je k dispozici. Bez nahrávání na server a bez limitů velikosti.", "Full control over quality and codecs, hardware acceleration where available. No uploads and no size limits.")).size(14.0));
    };

    let dropzone = |ui: &mut Ui, picked: &mut Vec<std::path::PathBuf>| {
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 290.0), Sense::click());
        let fill = if resp.hovered() { card_hi() } else { card_c() };
        ui.painter().rect_filled(rect, CornerRadius::same(24), fill);
        ui.painter().rect_stroke(rect, CornerRadius::same(24), Stroke::new(1.5_f32, if resp.hovered() { accent() } else { Color32::from_white_alpha(14) }), StrokeKind::Inside);
        let c = rect.center() - vec2(0.0, 36.0);
        ui.painter().circle_filled(c, 34.0, accent());
        ui.painter().text(c, egui::Align2::CENTER_CENTER, ph::UPLOAD_SIMPLE, egui::FontId::proportional(32.0), accent_text());
        ui.painter().text(pos2(c.x, c.y + 70.0), egui::Align2::CENTER_CENTER, tr!("Přetáhněte soubory nebo klikněte pro výběr", "Drop files here or click to browse"), bold(20.0), ui.visuals().text_color());
        ui.painter().text(pos2(c.x, c.y + 98.0), egui::Align2::CENTER_CENTER, tr!("Můžeš vybrat víc souborů najednou", "You can select multiple files at once"), egui::FontId::proportional(14.0), muted_c());
        if resp.clicked() {
            if let Some(f) = rfd::FileDialog::new().set_title(tr!("Vyber soubory k převodu", "Select files to convert")).pick_files() {
                picked.extend(f);
            }
        }
        ui.horizontal(|ui| {
            if ghost_button(ui, &format!("{}  {}", ph::FOLDER_OPEN, tr!("Vybrat složku (zabalit)", "Choose a folder (archive)"))).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title(tr!("Vyber složku", "Choose a folder")).pick_folder() {
                    picked.push(d);
                }
            }
        });
    };

    if wide {
        ui.columns(2, |cols| {
            hero(&mut cols[0]);
            dropzone(&mut cols[1], &mut picked);
        });
    } else {
        hero(ui);
        dropzone(ui, &mut picked);
    }

    if !picked.is_empty() {
        app.add_files(picked);
        return;
    }

    ui.add_space(30.0);
    ui.separator();
    ui.add_space(20.0);
    ui.vertical_centered(|ui| title(ui, tr!("Podporované formáty", "Supported formats"), 30.0));
    ui.add_space(14.0);

    let si = app.si.clone();
    let t = app.tools.clone();
    let join = |v: Vec<String>| v.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ");
    let mut images: BTreeSet<String> = IMAGE_IN.iter().map(|s| s.to_string()).collect();
    images.extend(si.magick_read.iter().filter(|e| e.len() <= 5).cloned());
    let hw: Vec<&str> = si.backends().iter().map(|b| b.short()).collect();
    let cards: Vec<(&str, &str, Color32, bool, String, String)> = vec![
        (tr!("Obrázky", "Images"), ph::IMAGE, Color32::from_rgb(130, 160, 255), t.magick.is_some(), trf!("ImageMagick · {} formátů čtení / {} zápis", "ImageMagick · {} read / {} write formats", si.magick_read.len(), si.magick_write.len()), join(images.into_iter().collect())),
        ("Audio", ph::WAVEFORM, Color32::from_rgb(190, 150, 255), t.ffmpeg.is_some(), tr!("FFmpeg · všechny kodeky", "FFmpeg · all codecs").to_string(), join(AUDIO_IN.iter().map(|s| s.to_string()).collect())),
        ("Video", ph::FILM_STRIP, Color32::from_rgb(255, 140, 160), t.ffmpeg.is_some(), format!("FFmpeg · {}: {}", tr!("hardwarové enkodéry", "hardware encoders"), if hw.is_empty() { "—".to_string() } else { hw.join(", ") }), join(VIDEO_IN.iter().map(|s| s.to_string()).collect())),
        (tr!("Dokumenty", "Documents"), ph::FILE_TEXT, Color32::from_rgb(130, 230, 170), t.pandoc.is_some(), "Pandoc + Typst".to_string(), join(DOC_IN.iter().map(|s| s.to_string()).collect())),
        ("PDF", ph::FILE_PDF, Color32::from_rgb(255, 170, 110), t.qpdf.is_some() || t.pdfium.is_some(), "qpdf + pdfium".to_string(), tr!("PDF → obrázky / text / dokumenty, sloučit, rozdělit, stránky, otočit, heslo, optimalizace; cokoliv → PDF", "PDF → images / text / documents, merge, split, pages, rotate, password, optimise; anything → PDF").to_string()),
        (tr!("Archivy", "Archives"), ph::FILE_ZIP, Color32::from_rgb(255, 215, 120), true, tr!("Vestavěné (Rust)", "Built in (Rust)").to_string(), join(ARCHIVE_IN.iter().map(|s| s.to_string()).collect())),
        (tr!("Titulky", "Subtitles"), ph::SUBTITLES, Color32::from_rgb(120, 220, 230), t.ffmpeg.is_some(), "FFmpeg".to_string(), join(SUB_IN.iter().map(|s| s.to_string()).collect())),
    ];
    let cols = if ui.available_width() > 1000.0 { 4 } else if ui.available_width() > 640.0 { 2 } else { 1 };
    for chunk in cards.chunks(cols) {
        ui.columns(cols, |c| {
            for (i, (name, icon, color, ok, engine, formats)) in chunk.iter().enumerate() {
                card(&mut c[i], |ui| {
                    ui.set_min_height(250.0);
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        badge(ui, icon, *color, 30.0);
                        ui.label(RichText::new(*name).font(bold(20.0)));
                    });
                    ui.horizontal(|ui| {
                        if *ok {
                            ui.label(RichText::new(format!("{}  {}", ph::CHECK, tr!("Zpracování na tvém PC", "Processed locally"))).size(13.0));
                        } else {
                            ui.label(RichText::new(format!("{}  {}", ph::WARNING, tr!("Nástroj chybí", "Tool missing"))).color(warn()).size(13.0));
                        }
                    });
                    ui.label(muted(format!("{}: {}", tr!("Stav", "Status"), if *ok { tr!("připraveno", "ready") } else { tr!("nedostupné", "unavailable") })).size(12.5));
                    ui.label(muted(engine.clone()).size(12.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{}:", tr!("Podporované formáty", "Supported formats"))).font(bold(14.0)));
                    egui::ScrollArea::vertical().id_salt(*name).max_height(112.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(formats.clone()).size(12.5)).wrap());
                    });
                });
            }
        });
    }
}

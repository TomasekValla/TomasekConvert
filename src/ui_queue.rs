//! Záložka "Konvertovat": fronta souborů, výběr formátu, nastavení, průběh.
use crate::{app::*, formats::*, theme::*, ui_common::*, ui_opts, util::*};
use egui::{Align, Color32, Layout, RichText, Ui};
use egui_phosphor::regular as ph;

enum Act {
    Convert(usize),
    Cancel(usize),
    Remove(usize),
}

fn kind_style(k: Kind) -> (&'static str, Color32) {
    match k {
        Kind::Image => (ph::IMAGE, Color32::from_rgb(130, 160, 255)),
        Kind::Audio => (ph::WAVEFORM, Color32::from_rgb(190, 150, 255)),
        Kind::Video => (ph::FILM_STRIP, Color32::from_rgb(255, 140, 160)),
        Kind::Document => (ph::FILE_TEXT, Color32::from_rgb(130, 230, 170)),
        Kind::Pdf => (ph::FILE_PDF, Color32::from_rgb(255, 170, 110)),
        Kind::Archive | Kind::Folder => (ph::FILE_ZIP, Color32::from_rgb(255, 215, 120)),
        Kind::Subtitle => (ph::SUBTITLES, Color32::from_rgb(120, 220, 230)),
        Kind::Other => (ph::QUESTION, Color32::GRAY),
    }
}

fn class_heading(c: Class) -> &'static str {
    match c {
        Class::Video => tr!("Video", "Video"),
        Class::Audio => tr!("Audio", "Audio"),
        Class::AnimImage => tr!("Animace", "Animation"),
        Class::FrameImage => tr!("Snímek z videa", "Video frame"),
        Class::Image => tr!("Obrázky", "Images"),
        Class::Doc => tr!("Dokumenty", "Documents"),
        Class::Pdf => "PDF",
        Class::Archive => tr!("Archivy", "Archives"),
        Class::Subtitle => tr!("Titulky", "Subtitles"),
        Class::Text => tr!("Text", "Text"),
    }
}

fn meta_line(it: &Item) -> String {
    let mut meta = format!("{} · {}", it.kind.label(), fmt_bytes(it.size));
    if let Some(p) = &it.probe {
        if p.duration > 0.0 {
            meta.push_str(&format!(" · {}", fmt_dur(p.duration)));
        }
        if let Some(v) = p.video() {
            meta.push_str(&format!(" · {} {}×{}", v.codec.to_uppercase(), v.width, v.height));
            if v.fps > 0.0 {
                meta.push_str(&format!(" @ {} fps", format!("{:.3}", v.fps).trim_end_matches('0').trim_end_matches('.')));
            }
            if !v.pix_fmt.is_empty() {
                meta.push_str(&format!(" · {} ({}-bit)", v.pix_fmt, v.bit_depth));
            }
            if v.bit_rate > 0 {
                meta.push_str(&format!(" · {}", crate::probe::fmt_bitrate(v.bit_rate)));
            }
            if !v.hdr.is_empty() {
                meta.push_str(&format!(" · {}", v.hdr));
            }
        }
        if let Some(a) = p.audio_streams().next() {
            meta.push_str(&format!(" · {} {} {} Hz {}{}", tr!("zvuk", "audio"), a.codec.to_uppercase(), a.sample_rate, a.channels, tr!("kan.", "ch")));
            if !a.depth_label().is_empty() {
                meta.push_str(&format!(" {}", a.depth_label()));
            }
            if a.bit_rate > 0 {
                meta.push_str(&format!(" {}", crate::probe::fmt_bitrate(a.bit_rate)));
            }
        }
    }
    meta
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(8.0);
    // ── horní lišta ──
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if primary_button(ui, &format!("{}  {}", ph::PLUS, tr!("Přidat soubory", "Add files"))).clicked() {
                if let Some(f) = rfd::FileDialog::new().set_title(tr!("Vyber soubory", "Select files")).pick_files() {
                    app.add_files(f);
                }
            }
            let can = app.items.iter().any(|i| i.fmt().is_some());
            ui.add_enabled_ui(can, |ui| {
                if primary_button(ui, &format!("{}  {}", ph::PLAY, tr!("Převést vše", "Convert all"))).clicked() {
                    app.convert_all();
                }
            });
            if app.any_running() && ghost_button(ui, &format!("{}  {}", ph::STOP, tr!("Zrušit vše", "Cancel all"))).clicked() {
                app.cancel_all();
            }
            if ghost_button(ui, &format!("{}  {}", ph::BROOM, tr!("Vyčistit hotové", "Clear finished"))).clicked() {
                let tasks = &app.tasks;
                app.items.retain(|i| !i.task.and_then(|t| tasks.get(&t)).map(|t| matches!(t.state, TaskState::Done(..))).unwrap_or(false));
            }
            ui.separator();
            ui.label(muted(tr!("Výstup:", "Output:")));
            if chip(ui, app.settings.same_dir, tr!("Stejná složka jako zdroj", "Same folder as source")).clicked() {
                app.settings.same_dir = true;
                app.settings.save();
            }
            if chip(ui, !app.settings.same_dir, tr!("Vlastní složka", "Custom folder")).clicked() {
                if app.settings.out_dir.is_empty() {
                    if let Some(d) = rfd::FileDialog::new().set_title(tr!("Výstupní složka", "Output folder")).pick_folder() {
                        app.settings.out_dir = d.to_string_lossy().to_string();
                        app.settings.same_dir = false;
                    }
                } else {
                    app.settings.same_dir = false;
                }
                app.settings.save();
            }
            if !app.settings.same_dir {
                ui.label(muted(app.settings.out_dir.clone()).size(12.5));
                if ui.button(ph::FOLDER_OPEN).on_hover_text(tr!("Změnit výstupní složku", "Change output folder")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title(tr!("Výstupní složka", "Output folder")).pick_folder() {
                        app.settings.out_dir = d.to_string_lossy().to_string();
                        app.settings.save();
                    }
                }
            }
        });
    });
    ui.add_space(6.0);

    if app.items.is_empty() {
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(RichText::new(ph::TRAY).size(48.0).color(muted_c()));
                ui.label(RichText::new(tr!("Fronta je prázdná", "The queue is empty")).font(bold(22.0)));
                ui.label(muted(tr!("Přetáhni sem soubory nebo je přidej tlačítkem nahoře.", "Drop files here or add them with the button above.")));
                ui.add_space(30.0);
            });
        });
        return;
    }

    if app.items.len() >= 2 {
        batch_card(app, ui);
    }

    let mut act: Option<Act> = None;
    let adv_before = app.settings.advanced;
    let App { items, tasks, si, stats, settings, .. } = app;
    let n = items.len();
    for idx in 0..n {
        let it = &mut items[idx];
        let (icon, color) = kind_style(it.kind);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let task = it.task.and_then(|t| tasks.get(&t));
            let running = task.map(|t| t.running()).unwrap_or(false);
            let meta = meta_line(it);
            // hlavička: ovládání vpravo, název vlevo (zkrácený, aby se nepřekrýval)
            ui.horizontal(|ui| {
                badge(ui, icon, color, 40.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !running && ui.button(ph::X).on_hover_text(tr!("Odebrat", "Remove")).clicked() {
                        act = Some(Act::Remove(idx));
                    }
                    if !running && it.fmt().is_some() {
                        if primary_button(ui, &format!("{}  {}", ph::PLAY, tr!("Převést", "Convert"))).clicked() {
                            act = Some(Act::Convert(idx));
                        }
                        if ui.button(format!("{}  {}", ph::SLIDERS, tr!("Nastavení", "Options"))).clicked() {
                            it.open = !it.open;
                        }
                        let cur = it.fmt().map(|f| format!(".{}  {}", f.ext, f.name)).unwrap_or_default();
                        let Item { id, filter, targets, sel, .. } = &mut *it;
                        let focus_flag = egui::Id::new(("fmt-focus", *id));
                        let cb = egui::ComboBox::from_id_salt(("fmt", *id))
                            .selected_text(cur)
                            .width(230.0)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show_ui(ui, |ui| {
                                // pevná velikost okna: dřív se při hledání smrsklo na pár pixelů
                                ui.set_min_width(340.0);
                                let r = ui.add(egui::TextEdit::singleline(filter).hint_text(tr!("Hledat formát…", "Search formats…")).desired_width(f32::INFINITY));
                                if !ui.data(|d| d.get_temp::<bool>(focus_flag).unwrap_or(false)) {
                                    r.request_focus();
                                    ui.data_mut(|d| d.insert_temp(focus_flag, true));
                                }
                                let q = filter.to_lowercase();
                                // výška podle okna aplikace: v malém okně se vejde, ve velkém je seznam dost vysoký
                                let list_h = (ui.ctx().content_rect().height() * 0.5).clamp(240.0, 440.0);
                                egui::ScrollArea::vertical().max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                                    let mut last: Option<Class> = None;
                                    let mut shown = 0;
                                    for (i, f) in targets.iter().enumerate() {
                                        if !q.is_empty() && !f.ext.contains(&q) && !f.name.to_lowercase().contains(&q) {
                                            continue;
                                        }
                                        shown += 1;
                                        if last != Some(f.class) {
                                            last = Some(f.class);
                                            ui.label(muted(class_heading(f.class)).size(12.0));
                                        }
                                        if ui.selectable_label(*sel == i, format!(".{}  {}", f.ext, f.name)).clicked() {
                                            *sel = i;
                                            ui.close();
                                        }
                                    }
                                    if shown == 0 {
                                        ui.label(muted(tr!("Nic nenalezeno", "No match")));
                                    }
                                });
                            });
                        if cb.inner.is_none() {
                            // okno je zavřené -> příště začni s prázdným hledáním a znovu zaostři pole
                            filter.clear();
                            ui.data_mut(|d| d.remove::<bool>(focus_flag));
                        }
                        ui.label(muted(ph::ARROW_RIGHT));
                    } else if it.fmt().is_none() && !running {
                        ui.label(RichText::new(tr!("Tenhle formát zatím neumím", "This format is not supported yet")).color(warn()));
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&it.name).font(bold(16.0))).truncate());
                    });
                });
            });
            ui.add(egui::Label::new(muted(meta).size(12.5)).wrap());
            if let Some(t) = task {
                ui.add_space(4.0);
                match task_view(ui, t, stats) {
                    TA::Cancel => act = Some(Act::Cancel(idx)),
                    TA::Retry => act = Some(Act::Convert(idx)),
                    TA::None => {}
                }
            }
            if it.open && !running {
                ui.add_space(6.0);
                ui.separator();
                if let Some(f) = it.fmt().cloned() {
                    let Item { opts, probe, kind, in_ext, .. } = it;
                    ui_opts::panel(ui, *kind, &f, in_ext, opts, si, probe.as_ref(), &mut settings.advanced);
                }
                row(ui, tr!("Výstupní složka", "Output folder"), |ui| pick_dir_button(ui, &mut it.out_dir));
            }
        });
        ui.add_space(2.0);
    }
    if app.settings.advanced != adv_before {
        app.settings.save();
    }
    match act {
        Some(Act::Convert(i)) => app.convert_item(i),
        Some(Act::Cancel(i)) => {
            if let Some(t) = app.items[i].task.and_then(|t| app.tasks.get(&t)) {
                t.shared.cancel();
            }
        }
        Some(Act::Remove(i)) => {
            app.items.remove(i);
        }
        None => {}
    }
}

fn scope_label(k: Option<Kind>) -> &'static str {
    match k {
        None => tr!("Všechny soubory", "All files"),
        Some(Kind::Image) => tr!("Obrázky", "Images"),
        Some(Kind::Audio) => "Audio",
        Some(Kind::Video) => tr!("Videa", "Videos"),
        Some(Kind::Document) => tr!("Dokumenty", "Documents"),
        Some(Kind::Pdf) => "PDF",
        Some(Kind::Archive) => tr!("Archivy", "Archives"),
        Some(Kind::Folder) => tr!("Složky", "Folders"),
        Some(Kind::Subtitle) => tr!("Titulky", "Subtitles"),
        Some(Kind::Other) => tr!("Ostatní", "Other"),
    }
}

/// Hromadná volba cílového formátu + sloučení do jednoho PDF.
fn batch_card(app: &mut App, ui: &mut Ui) {
    let mut kinds: Vec<Kind> = Vec::new();
    for it in &app.items {
        if !kinds.contains(&it.kind) {
            kinds.push(it.kind);
        }
    }
    let scopes: Vec<Option<Kind>> = if kinds.len() > 1 { std::iter::once(None).chain(kinds.iter().map(|k| Some(*k))).collect() } else { vec![None] };

    // pro každý rozsah: kandidátní formáty (ext, název, počet souborů, kolik jich ho má vybraných)
    struct Scope {
        kind: Option<Kind>,
        total: usize,
        cand: Vec<(String, String, usize, usize)>,
    }
    let mut data: Vec<Scope> = Vec::new();
    for sc in &scopes {
        let items: Vec<&Item> = app.items.iter().filter(|i| sc.map_or(true, |k| i.kind == k) && !i.targets.is_empty()).collect();
        if items.is_empty() {
            continue;
        }
        let mut cand: Vec<(String, String, usize, usize)> = Vec::new();
        for it in &items {
            let mut seen: Vec<&str> = Vec::new();
            for f in &it.targets {
                if seen.contains(&f.ext.as_str()) {
                    continue;
                }
                seen.push(&f.ext);
                let selected = it.fmt().map(|c| c.ext == f.ext).unwrap_or(false) as usize;
                match cand.iter_mut().find(|c| c.0 == f.ext) {
                    Some(c) => {
                        c.2 += 1;
                        c.3 += selected;
                    }
                    None => cand.push((f.ext.clone(), f.name.clone(), 1, selected)),
                }
            }
        }
        const PRIO: [&str; 18] = ["pdf", "png", "jpg", "webp", "mp4", "mkv", "mp3", "flac", "wav", "opus", "md", "docx", "html", "txt", "zip", "7z", "avif", "gif"];
        let prio = |e: &str| PRIO.iter().position(|p| *p == e).unwrap_or(usize::MAX);
        cand.sort_by(|a, b| b.2.cmp(&a.2).then(prio(&a.0).cmp(&prio(&b.0))));
        data.push(Scope { kind: *sc, total: items.len(), cand });
    }

    let pdf_items: Vec<usize> = app
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.kind, Kind::Image | Kind::Document | Kind::Pdf) && i.fmt().map(|f| f.ext == "pdf").unwrap_or(false))
        .map(|(n, _)| n)
        .collect();

    let mut apply: Option<(Option<Kind>, String)> = None;
    let mut do_merge = false;
    let App { merge_name, batch_filter, tasks, stats, merge_task, .. } = app;
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_max_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::STACK, tr!("Hromadně: převést na…", "Batch: convert all to…")));
        ui.label(muted(tr!("Klikni na formát a nastaví se u všech souborů, které ho umí. Např. obrázky + dokumenty → PDF.", "Click a format to set it for every file that supports it – e.g. images + documents → PDF.")).size(12.5));
        for sc in &data {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} ({})", scope_label(sc.kind), sc.total)).font(bold(13.5)));
                for (ext, name, n, selected) in sc.cand.iter().take(14) {
                    let all = *selected == sc.total;
                    let label = if *n < sc.total { format!(".{ext}  {n}/{}", sc.total) } else { format!(".{ext}") };
                    if chip(ui, all, &label).on_hover_text(trf!("{name} – umí {n} z {} souborů", "{name} – supported by {n} of {} files", sc.total)).clicked() {
                        apply = Some((sc.kind, ext.clone()));
                    }
                }
                if sc.cand.len() > 14 {
                    ui.menu_button(tr!("Další…", "More…"), |ui| {
                        ui.add(egui::TextEdit::singleline(batch_filter).hint_text(tr!("Hledat formát…", "Search formats…")).desired_width(180.0));
                        let q = batch_filter.to_lowercase();
                        egui::ScrollArea::vertical().max_height((ui.ctx().content_rect().height() * 0.5).clamp(240.0, 440.0)).show(ui, |ui| {
                            for (ext, name, n, _) in &sc.cand {
                                if !q.is_empty() && !ext.contains(&q) && !name.to_lowercase().contains(&q) {
                                    continue;
                                }
                                if ui.selectable_label(false, format!(".{ext}  {name}  ({n}/{})", sc.total)).clicked() {
                                    apply = Some((sc.kind, ext.clone()));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            });
        }
        if pdf_items.len() >= 2 {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(trf!("{} souborů míří do PDF", "{} files going to PDF", pdf_items.len())).font(bold(13.5)));
                ui.label(muted(tr!("Název:", "Name:")));
                ui.add(egui::TextEdit::singleline(merge_name).desired_width(180.0));
                if primary_button(ui, &format!("{}  {}", ph::FILES, tr!("Sloučit do jednoho PDF", "Merge into one PDF"))).clicked() {
                    do_merge = true;
                }
            });
            if let Some(t) = merge_task.and_then(|id| tasks.get(&id)) {
                if task_view(ui, t, stats) == TA::Cancel {
                    t.shared.cancel();
                }
            }
        }
    });

    if let Some((scope, ext)) = apply {
        for it in app.items.iter_mut().filter(|i| scope.map_or(true, |k| i.kind == k)) {
            if it.task.and_then(|t| app.tasks.get(&t)).map(|t| t.running()).unwrap_or(false) {
                continue;
            }
            if let Some(i) = it.targets.iter().position(|f| f.ext == ext) {
                it.sel = i;
            }
        }
    }
    if do_merge && !pdf_items.is_empty() {
        let first = app.items[pdf_items[0]].path.clone();
        let out_dir = app.items[pdf_items[0]].out_dir.clone().unwrap_or_else(|| app.out_dir_for(&first));
        let _ = std::fs::create_dir_all(&out_dir);
        let task = crate::pdfops::PdfTask::ToPdf {
            inputs: pdf_items.iter().map(|i| app.items[*i].path.clone()).collect(),
            merge: true,
            out_dir,
            name: if app.merge_name.trim().is_empty() { tr!("Sloučené", "Merged").into() } else { app.merge_name.trim().to_string() },
            opts: app.items[pdf_items[0]].opts.clone(),
            si: app.si.clone(),
        };
        let meta = TaskMeta { name: trf!("{} souborů", "{} files", pdf_items.len()), from: tr!("různé", "mixed").into(), to: "pdf".into(), kind: tr!("Sloučení do PDF", "Merge to PDF").into() };
        app.merge_task = Some(app.start_task(meta, move |ctx| crate::pdfops::run(ctx, task)));
    }
}

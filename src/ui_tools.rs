//! Záložka "Nástroje": PDF, Video → Audio, Stopy a remux.
use crate::{
    app::*,
    formats::*,
    job::*,
    opts::*,
    probe::*,
    theme::*,
    trackops::*,
    ui_common::*,
    ui_opts,
    util::*,
};
use egui::{Color32, RichText, Ui};
use egui_phosphor::regular as ph;
use std::path::{Path, PathBuf};

pub fn route_drop(app: &mut App, paths: Vec<PathBuf>) {
    match (app.tab, app.tool_tab) {
        (Tab::Tools, ToolTab::Pdf) => crate::ui_pdf::add_files(app, paths),
        (Tab::Tools, ToolTab::Audio) => app.audio_add(paths),
        (Tab::Tools, ToolTab::Tracks) => {
            if let Some(p) = paths.into_iter().next() {
                app.tracks_load(p);
            }
        }
        (Tab::Info, _) => {
            if let Some(p) = paths.into_iter().next() {
                app.info_load(p);
            }
        }
        _ => app.add_files(paths),
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        title(ui, tr!("Nástroje", "Tools"), 30.0);
        ui.add_space(16.0);
        for (t, icon, l) in [(ToolTab::Pdf, ph::FILE_PDF, "PDF"), (ToolTab::Audio, ph::WAVEFORM, "Video → Audio"), (ToolTab::Tracks, ph::STACK, tr!("Stopy a remux", "Tracks and remux"))] {
            if chip(ui, app.tool_tab == t, &format!("{icon}  {l}")).clicked() {
                app.tool_tab = t;
            }
        }
    });
    ui.add_space(6.0);
    match app.tool_tab {
        ToolTab::Pdf => crate::ui_pdf::show(app, ui),
        ToolTab::Audio => audio_ui(app, ui),
        ToolTab::Tracks => tracks_ui(app, ui),
    }
}

// ───────────────────────────── Video → Audio ─────────────────────────────

pub struct ToolFile {
    pub path: PathBuf,
    pub probe: Option<Probe>,
    pub task: Option<u64>,
}

pub struct AudioPage {
    pub files: Vec<ToolFile>,
    pub ext: String,
    pub opts: Opts,
    pub out_dir: Option<PathBuf>,
}

impl Default for AudioPage {
    fn default() -> Self {
        AudioPage { files: Vec::new(), ext: "mp3".into(), opts: Opts::default(), out_dir: None }
    }
}

impl AudioPage {
    pub fn on_probe(&mut self, path: &Path, p: Option<Probe>) {
        for f in self.files.iter_mut().filter(|f| f.path == path) {
            f.probe = p.clone();
        }
    }
}

impl App {
    pub fn audio_add(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if self.audio.files.iter().any(|f| f.path == p) {
                continue;
            }
            let ext = full_ext(&p);
            if !matches!(detect_kind(&ext, &self.si), Kind::Video | Kind::Audio) {
                continue;
            }
            self.spawn_probe(p.clone());
            self.audio.files.push(ToolFile { path: p, probe: None, task: None });
        }
    }
    pub fn tracks_load(&mut self, p: PathBuf) {
        self.tracks.begin(p.clone());
        self.spawn_probe(p);
        self.tab = Tab::Tools;
        self.tool_tab = ToolTab::Tracks;
    }
    pub fn info_load(&mut self, p: PathBuf) {
        self.info.begin(&p, &self.tools);
        self.spawn_probe(p);
        self.tab = Tab::Info;
    }
}

fn audio_ui(app: &mut App, ui: &mut Ui) {
    let mut a = std::mem::take(&mut app.audio);
    let si = app.si.clone();
    let mut add: Vec<PathBuf> = Vec::new();
    let mut remove: Option<usize> = None;
    let mut run_idx: Vec<usize> = Vec::new();
    let mut cancel: Vec<u64> = Vec::new();

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            heading_small(ui, tr!("Vytáhnout zvuk z videa", "Extract audio from video"));
            ui.label(muted(tr!("Přetáhni videa nebo audio soubory. Výstup dostane stejný název.", "Drop video or audio files. The output keeps the same name.")));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, &trf!("{}  Přidat soubory", "{}  Add files", ph::PLUS)).clicked() {
                    if let Some(f) = rfd::FileDialog::new().set_title(tr!("Vyber videa", "Select videos")).pick_files() {
                        add.extend(f);
                    }
                }
            });
        });
    });

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        // formát
        let fmts: Vec<Fmt> = targets_for(Kind::Video, "mkv", &si).into_iter().filter(|f| f.class == Class::Audio).collect();
        row(ui, tr!("Výstupní formát", "Output format"), |ui| {
            let cur = fmts.iter().find(|f| f.ext == a.ext).map(|f| format!(".{}  {}", f.ext, f.name)).unwrap_or_default();
            egui::ComboBox::from_id_salt("aud-fmt").selected_text(cur).width(240.0).height(380.0).show_ui(ui, |ui| {
                for f in &fmts {
                    if ui.selectable_label(a.ext == f.ext, format!(".{}  {}", f.ext, f.name)).clicked() {
                        a.ext = f.ext.clone();
                        a.opts.acodec = "auto".into();
                        a.opts.akbps = 0;
                    }
                }
            });
            for (l, k) in [("96k", 96), ("160k", 160), ("256k", 256), ("320k", 320)] {
                if ui.small_button(l).clicked() {
                    a.opts.akbps = k;
                }
            }
        });
        let probe = a.files.first().and_then(|f| f.probe.clone());
        ui_opts::audio_rows(ui, &mut a.opts, &a.ext.clone(), &si, probe.as_ref(), false, app.settings.advanced);
        ui_opts::trim_rows(ui, &mut a.opts);
        row(ui, "Metadata", |ui| {
            ui.checkbox(&mut a.opts.strip_meta, tr!("Odstranit tagy a obal alba", "Remove tags and cover art"));
        });
        row(ui, tr!("Výstupní složka", "Output folder"), |ui| pick_dir_button(ui, &mut a.out_dir));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!a.files.is_empty(), |ui| {
                if primary_button(ui, &trf!("{}  Vytáhnout audio ({})", "{}  Extract audio ({})", ph::PLAY, a.files.len())).clicked() {
                    run_idx = (0..a.files.len()).collect();
                }
            });
            if ghost_button(ui, tr!("Vyčistit seznam", "Clear list")).clicked() {
                a.files.clear();
            }
        });
    });

    for (i, f) in a.files.iter().enumerate() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                badge(ui, ph::FILM_STRIP, Color32::from_rgb(255, 140, 160), 32.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(f.path.file_name().unwrap_or_default().to_string_lossy()).font(bold(15.0)));
                    let mut m = fmt_bytes(path_size(&f.path));
                    if let Some(p) = &f.probe {
                        m.push_str(&format!(" · {}", fmt_dur(p.duration)));
                        let n = p.count(StreamKind::Audio);
                        m.push_str(&trf!(" · {n}× audio stopa", " · {n}× audio track"));
                        if let Some(s) = p.audio_streams().next() {
                            m.push_str(&format!(" · {}", s.headline()));
                        }
                    }
                    ui.label(muted(m).size(12.5));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(ph::X).clicked() {
                        remove = Some(i);
                    }
                });
            });
            if let Some(t) = f.task.and_then(|t| app.tasks.get(&t)) {
                match task_view(ui, t, &app.stats) {
                    TA::Cancel => cancel.push(f.task.unwrap()),
                    TA::Retry => run_idx.push(i),
                    TA::None => {}
                }
            }
        });
    }

    for id in cancel {
        if let Some(t) = app.tasks.get(&id) {
            t.shared.cancel();
        }
    }
    if let Some(i) = remove {
        a.files.remove(i);
    }
    for i in run_idx {
        if i >= a.files.len() {
            continue;
        }
        if let Some(t) = a.files[i].task.and_then(|t| app.tasks.get(&t)) {
            if t.running() {
                continue;
            }
        }
        let path = a.files[i].path.clone();
        let out_dir = a.out_dir.clone().unwrap_or_else(|| app.out_dir_for(&path));
        let _ = std::fs::create_dir_all(&out_dir);
        let fmt = Fmt { ext: a.ext.clone(), name: a.ext.to_uppercase(), class: Class::Audio, extra: String::new() };
        let job = ConvertJob { input: path.clone(), in_ext: full_ext(&path), kind: Kind::Video, fmt, opts: a.opts.clone(), out_dir };
        let meta = TaskMeta { name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), from: full_ext(&path), to: a.ext.clone(), kind: "Video → Audio".into() };
        a.files[i].task = Some(app.start_task(meta, move |ctx| crate::ffmpeg::convert(ctx, &job)));
    }
    app.audio = a;
    if !add.is_empty() {
        app.audio_add(add);
    }
}

// ───────────────────────────── Stopy a remux ─────────────────────────────

#[derive(Default)]
pub struct TracksPage {
    pub path: Option<PathBuf>,
    pub probe: Option<Probe>,
    pub loading: bool,
    pub error: Option<String>,
    keep: Vec<(usize, bool)>,
    keep_chapters: bool,
    keep_meta: bool,
    out_ext: String,
    subs_srt: bool,
    out_dir: Option<PathBuf>,
    remux_task: Option<u64>,
    extract_tasks: Vec<(String, u64)>,
}

impl TracksPage {
    pub fn begin(&mut self, p: PathBuf) {
        *self = TracksPage { path: Some(p), loading: true, keep_chapters: true, keep_meta: true, ..Default::default() };
    }
    pub fn on_probe(&mut self, path: &Path, probe: Option<Probe>) {
        if self.path.as_deref() != Some(path) {
            return;
        }
        self.loading = false;
        match probe {
            Some(p) => {
                self.keep = p.streams.iter().map(|s| (s.index, true)).collect();
                let ext = full_ext(path);
                self.out_ext = if matches!(ext.as_str(), "mkv" | "mp4" | "mov" | "webm" | "mka" | "m4v") { ext } else { "mkv".into() };
                self.probe = Some(p);
            }
            None => self.error = Some(tr!("ffprobe tenhle soubor nepřečetl.", "ffprobe could not read this file.").into()),
        }
    }
}

fn tracks_ui(app: &mut App, ui: &mut Ui) {
    let mut t = std::mem::take(&mut app.tracks);
    let mut load: Option<PathBuf> = None;
    let mut do_remux = false;
    let mut extract: Option<usize> = None;

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            heading_small(ui, tr!("Stopy v kontejneru", "Tracks in the container"));
            ui.label(muted(tr!("Vyber, které stopy nechat. Remux běží bez překódování (stream copy) – okamžitě a beze ztráty kvality.", "Choose which tracks to keep. Remux runs without re-encoding (stream copy) – instant and lossless.")));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, &trf!("{}  Otevřít soubor", "{}  Open file", ph::FOLDER_OPEN)).clicked() {
                    load = rfd::FileDialog::new().set_title(tr!("Vyber video", "Select a video")).pick_file();
                }
            });
        });
        if let Some(p) = &t.path {
            ui.label(RichText::new(p.display().to_string()).font(bold(13.5)));
        }
    });

    if t.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(muted(tr!("Čtu stopy…", "Reading tracks…")));
        });
    }
    if let Some(e) = &t.error {
        ui.label(RichText::new(e.clone()).color(bad()));
    }
    if t.path.is_none() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(RichText::new(ph::STACK).size(46.0).color(muted_c()));
                ui.label(muted(tr!("Přetáhni sem MKV / MP4 / MOV… a uvidíš všechny stopy.", "Drop an MKV / MP4 / MOV here… to see all its tracks.")));
                ui.add_space(30.0);
            });
        });
    }

    if let Some(probe) = t.probe.clone() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(muted(tr!("Rychlý výběr:", "Quick select:")));
                if ui.button(tr!("Vše", "All")).clicked() {
                    t.keep.iter_mut().for_each(|k| k.1 = true);
                }
                if ui.button(tr!("Nic", "None")).clicked() {
                    t.keep.iter_mut().for_each(|k| k.1 = false);
                }
                if ui.button(tr!("Video + 1. audio", "Video + first audio")).clicked() {
                    let mut seen_a = false;
                    let mut seen_v = false;
                    for k in t.keep.iter_mut() {
                        let kind = probe.streams.iter().find(|s| s.index == k.0).map(|s| s.kind);
                        k.1 = match kind {
                            Some(StreamKind::Video) if !seen_v => {
                                seen_v = true;
                                true
                            }
                            Some(StreamKind::Audio) if !seen_a => {
                                seen_a = true;
                                true
                            }
                            _ => false,
                        };
                    }
                }
                for (label, kind) in [(tr!("Bez titulků", "No subtitles"), StreamKind::Subtitle), (tr!("Bez příloh", "No attachments"), StreamKind::Attachment), (tr!("Bez dat", "No data"), StreamKind::Data)] {
                    if ui.button(label).clicked() {
                        for k in t.keep.iter_mut() {
                            if probe.streams.iter().find(|s| s.index == k.0).map(|s| s.kind) == Some(kind) {
                                k.1 = false;
                            }
                        }
                    }
                }
            });
            ui.add_space(4.0);
            egui::Grid::new("tracks-grid").num_columns(8).spacing([14.0, 8.0]).striped(true).show(ui, |ui| {
                for h in [tr!("Nechat", "Keep"), "#", tr!("Typ", "Type"), tr!("Popis", "Description"), tr!("Jazyk", "Language"), tr!("Název", "Title"), "Bitrate", ""] {
                    ui.label(RichText::new(h).font(bold(12.5)).color(muted_c()));
                }
                ui.end_row();
                let st_plan = plan(&probe.streams, &|idx| t.keep.iter().any(|k| k.0 == idx && k.1), &t.out_ext);
                for (i, s) in probe.streams.iter().enumerate() {
                    let skip = st_plan[i];
                    if let Some(k) = t.keep.get_mut(i) {
                        ui.checkbox(&mut k.1, "");
                    } else {
                        ui.label("");
                    }
                    ui.label(format!("{}", s.index));
                    ui.horizontal(|ui| {
                        let (ic, col) = match s.kind {
                            StreamKind::Video => (ph::FILM_STRIP, Color32::from_rgb(255, 140, 160)),
                            StreamKind::Audio => (ph::WAVEFORM, Color32::from_rgb(190, 150, 255)),
                            StreamKind::Subtitle => (ph::SUBTITLES, Color32::from_rgb(120, 220, 230)),
                            StreamKind::Attachment => (ph::PAPERCLIP, Color32::from_rgb(255, 215, 120)),
                            StreamKind::Data => (ph::DATABASE, muted_c()),
                        };
                        badge(ui, ic, col, 22.0);
                        ui.label(s.kind.label());
                    });
                    ui.horizontal(|ui| {
                        ui.label(s.headline());
                        if s.default {
                            pill_tag(ui, tr!("výchozí", "default"), good());
                        }
                        if s.forced {
                            pill_tag(ui, "forced", warn());
                        }
                        if s.attached_pic {
                            pill_tag(ui, "obal", muted_c());
                        }
                        if let Some(why) = skip {
                            pill_tag(ui, &why.label(s, &t.out_ext), bad());
                        }
                    });
                    ui.label(if s.lang.is_empty() { "—".into() } else { s.lang.clone() });
                    ui.label(if s.title.is_empty() { "—".into() } else { s.title.clone() });
                    ui.label(fmt_bitrate(s.bit_rate));
                    if ui.small_button(trf!("{}  Vytáhnout", "{}  Extract", ph::DOWNLOAD_SIMPLE)).on_hover_text(tr!("Uloží jen tuhle stopu (bez překódování)", "Saves just this track (no re-encoding)")).clicked() {
                        extract = Some(i);
                    }
                    ui.end_row();
                }
            });
            if !probe.chapters.is_empty() || !probe.tags.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if !probe.chapters.is_empty() {
                        ui.checkbox(&mut t.keep_chapters, trf!("Zachovat kapitoly ({})", "Keep chapters ({})", probe.chapters.len()));
                    }
                    ui.checkbox(&mut t.keep_meta, tr!("Zachovat metadata / tagy", "Keep metadata / tags"));
                    ui.checkbox(&mut t.subs_srt, tr!("Titulky při vytažení převést na SRT", "Convert subtitles to SRT when extracting"));
                });
            }
        });

        card(ui, |ui| {
            ui.set_width(ui.available_width());
            row(ui, tr!("Výstupní kontejner", "Output container"), |ui| {
                for e in ["mkv", "mp4", "mov", "webm", "mka", "ts"] {
                    if chip(ui, t.out_ext == e, &format!(".{e}")).clicked() {
                        t.out_ext = e.to_string();
                    }
                }
            });
            let kept = t.keep.iter().filter(|k| k.1).count();
            let bad = plan(&probe.streams, &|idx| t.keep.iter().any(|k| k.0 == idx && k.1), &t.out_ext).iter().zip(t.keep.iter()).filter(|(p, k)| k.1 && p.is_some()).count();
            ui.label(muted(trf!("Vybráno {kept} z {} stop{}", "Selected {kept} of {} tracks{}", probe.streams.len(), if bad > 0 { trf!(" · {bad} nejde do .{} a vynechá se (viz červené štítky)", " · {bad} not supported by .{} and will be skipped (see the red tags)", t.out_ext) } else { String::new() })).size(12.5));
            row(ui, tr!("Výstupní složka", "Output folder"), |ui| pick_dir_button(ui, &mut t.out_dir));
            let running = t.remux_task.and_then(|id| app.tasks.get(&id)).map(|x| x.running()).unwrap_or(false);
            ui.add_enabled_ui(!running && kept > 0, |ui| {
                if primary_button(ui, &trf!("{}  Uložit vybrané stopy (remux)", "{}  Save selected tracks (remux)", ph::PLAY)).clicked() {
                    do_remux = true;
                }
            });
            let act = match t.remux_task.and_then(|id| app.tasks.get(&id)) {
                Some(x) => task_view(ui, x, &app.stats),
                None => TA::None,
            };
            match act {
                TA::Cancel => {
                    if let Some(x) = t.remux_task.and_then(|id| app.tasks.get(&id)) {
                        x.shared.cancel();
                    }
                }
                TA::Retry => do_remux = true,
                TA::None => {}
            }
            for (label, id) in t.extract_tasks.iter().rev().take(4) {
                if let Some(x) = app.tasks.get(id) {
                    ui.label(RichText::new(label.clone()).font(bold(13.0)));
                    if task_view(ui, x, &app.stats) == TA::Cancel {
                        x.shared.cancel();
                    }
                }
            }
        });

        let path = t.path.clone().unwrap();
        let out_dir = t.out_dir.clone().unwrap_or_else(|| app.out_dir_for(&path));
        let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if do_remux {
            let _ = std::fs::create_dir_all(&out_dir);
            let task = RemuxTask {
                input: path.clone(),
                probe: probe.clone(),
                out_ext: t.out_ext.clone(),
                keep: t.keep.iter().filter(|k| k.1).map(|k| k.0).collect(),
                keep_chapters: t.keep_chapters,
                keep_meta: t.keep_meta,
                out_dir: out_dir.clone(),
            };
            let meta = TaskMeta { name: fname.clone(), from: full_ext(&path), to: t.out_ext.clone(), kind: "Remux".into() };
            t.remux_task = Some(app.start_task(meta, move |ctx| remux(ctx, task)));
        }
        if let Some(i) = extract {
            if let Some(s) = probe.streams.get(i).cloned() {
                let _ = std::fs::create_dir_all(&out_dir);
                let label = format!("#{} {} – {}", s.index, s.kind.label(), s.headline());
                let task = ExtractTask { input: path.clone(), stream: s.clone(), duration: probe.duration, subs_to_srt: t.subs_srt, out_dir };
                let meta = TaskMeta { name: fname, from: full_ext(&path), to: track_ext(&s, t.subs_srt).to_string(), kind: tr!("Extrakce stopy", "Track extraction").into() };
                let id = app.start_task(meta, move |ctx| extract_track(ctx, task));
                t.extract_tasks.push((label, id));
            }
        }
    }
    app.tracks = t;
    if let Some(p) = load {
        app.tracks_load(p);
    }
}

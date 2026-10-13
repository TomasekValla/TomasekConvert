//! Nástroj "PDF": sloučit (drag & drop řazení), cokoliv → PDF, rozdělit, stránky, otočit, optimalizace, heslo.
use crate::{app::*, pdfops::*, theme::*, ui_common::*, util::*};
use egui::{RichText, Ui};
use egui_phosphor::regular as ph;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PdfOp {
    Merge,
    ToPdf,
    Split,
    Pages,
    Rotate,
    Optimize,
    Lock,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitMode {
    Every,
    Ranges,
}

pub struct PdfFile {
    pub path: PathBuf,
    pub pages: Option<u32>,
}

pub struct PdfPage {
    op: PdfOp,
    pub files: Vec<PdfFile>,
    name: String,
    merge_all: bool,
    split: SplitMode,
    split_n: u32,
    ranges: String,
    spec: String,
    mode: PageMode,
    angle: i32,
    rot_spec: String,
    linearize: bool,
    pw: String,
    lock_encrypt: bool,
    user_pw: String,
    owner_pw: String,
    opts: crate::opts::Opts,
    out_dir: Option<PathBuf>,
    task: Option<u64>,
}

impl Default for PdfPage {
    fn default() -> Self {
        PdfPage {
            op: PdfOp::Merge,
            files: Vec::new(),
            name: tr!("Sloučené", "Merged").into(),
            merge_all: true,
            split: SplitMode::Every,
            split_n: 1,
            ranges: "1-3; 4-6".into(),
            spec: String::new(),
            mode: PageMode::Extract,
            angle: 90,
            rot_spec: String::new(),
            linearize: false,
            pw: String::new(),
            lock_encrypt: true,
            user_pw: String::new(),
            owner_pw: String::new(),
            opts: crate::opts::Opts::default(),
            out_dir: None,
            task: None,
        }
    }
}

impl PdfPage {
    pub fn on_pages(&mut self, path: &std::path::Path, n: Option<u32>) {
        for f in self.files.iter_mut().filter(|f| f.path == path) {
            f.pages = n;
        }
    }
}

fn single(op: PdfOp) -> bool {
    !matches!(op, PdfOp::Merge | PdfOp::ToPdf)
}

pub fn add_files(app: &mut App, paths: Vec<PathBuf>) {
    let op = app.pdf.op;
    for p in paths {
        let ext = full_ext(&p);
        let ok = if op == PdfOp::ToPdf { matches!(crate::formats::detect_kind(&ext, &app.si), crate::formats::Kind::Pdf | crate::formats::Kind::Image | crate::formats::Kind::Document) } else { ext == "pdf" };
        if !ok || app.pdf.files.iter().any(|f| f.path == p) {
            continue;
        }
        if single(op) {
            app.pdf.files.clear();
        }
        if ext == "pdf" {
            app.spawn_pages(p.clone());
        }
        app.pdf.files.push(PdfFile { path: p, pages: None });
    }
}

fn build(app: &App, p: &PdfPage) -> Result<(PdfTask, TaskMeta), String> {
    if p.files.is_empty() {
        return Err(tr!("Nejdřív přidej soubor", "Add a file first").into());
    }
    let first = p.files[0].path.clone();
    let out_dir = p.out_dir.clone().unwrap_or_else(|| app.out_dir_for(&first));
    let _ = std::fs::create_dir_all(&out_dir);
    let name = if p.name.trim().is_empty() { "vystup".to_string() } else { p.name.trim().to_string() };
    let pw = p.pw.clone();
    let first_name = first.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mk = move |label: &str| TaskMeta { name: first_name.clone(), from: "pdf".into(), to: label.into(), kind: tr!("PDF nástroj", "PDF tool").into() };
    Ok(match p.op {
        PdfOp::Merge => {
            if p.files.len() < 2 {
                return Err(tr!("Na sloučení potřebuješ aspoň 2 PDF", "You need at least 2 PDFs to merge").into());
            }
            (PdfTask::Merge { inputs: p.files.iter().map(|f| f.path.clone()).collect(), out_dir, name, pw }, TaskMeta { name: trf!("{} souborů", "{} files", p.files.len()), from: "pdf".into(), to: "merge".into(), kind: tr!("PDF nástroj", "PDF tool").into() })
        }
        PdfOp::ToPdf => (
            PdfTask::ToPdf { inputs: p.files.iter().map(|f| f.path.clone()).collect(), merge: p.merge_all, out_dir, name, opts: p.opts.clone(), si: app.si.clone() },
            TaskMeta { name: trf!("{} souborů", "{} files", p.files.len()), from: tr!("různé", "mixed").into(), to: "pdf".into(), kind: tr!("PDF nástroj", "PDF tool").into() },
        ),
        PdfOp::Split => match p.split {
            SplitMode::Every => (PdfTask::SplitEvery { input: first, n: p.split_n, out_dir, pw }, mk("split")),
            SplitMode::Ranges => (PdfTask::SplitRanges { input: first, ranges: p.ranges.clone(), out_dir, pw }, mk("split")),
        },
        PdfOp::Pages => (PdfTask::Pages { input: first, spec: p.spec.clone(), mode: p.mode, out_dir, pw }, mk("pages")),
        PdfOp::Rotate => (PdfTask::Rotate { input: first, angle: p.angle, spec: p.rot_spec.clone(), out_dir, pw }, mk("rotate")),
        PdfOp::Optimize => (PdfTask::Optimize { input: first, linearize: p.linearize, out_dir, pw }, mk("optimize")),
        PdfOp::Lock => {
            if p.lock_encrypt {
                if p.user_pw.is_empty() {
                    return Err(tr!("Zadej heslo", "Enter a password").into());
                }
                (PdfTask::Encrypt { input: first, user: p.user_pw.clone(), owner: p.owner_pw.clone(), out_dir }, mk("encrypt"))
            } else {
                (PdfTask::Decrypt { input: first, out_dir, pw }, mk("decrypt"))
            }
        }
    })
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let mut p = std::mem::take(&mut app.pdf);
    let ops = [
        (PdfOp::Merge, ph::FILES, tr!("Sloučit", "Merge")),
        (PdfOp::ToPdf, ph::FILE_PDF, tr!("Cokoliv → PDF", "Anything → PDF")),
        (PdfOp::Split, ph::SCISSORS, tr!("Rozdělit", "Split")),
        (PdfOp::Pages, ph::LIST_NUMBERS, tr!("Stránky", "Pages")),
        (PdfOp::Rotate, ph::ARROW_CLOCKWISE, tr!("Otočit", "Rotate")),
        (PdfOp::Optimize, ph::LIGHTNING, tr!("Optimalizovat", "Optimise")),
        (PdfOp::Lock, ph::LOCK, tr!("Heslo", "Password")),
    ];
    ui.horizontal_wrapped(|ui| {
        for (o, icon, l) in ops {
            if chip(ui, p.op == o, &format!("{icon}  {l}")).clicked() {
                p.op = o;
                if single(o) && p.files.len() > 1 {
                    p.files.truncate(1);
                }
                p.task = None;
            }
        }
    });
    ui.add_space(4.0);

    let mut pending: Vec<PathBuf> = Vec::new();
    let mut remove: Option<usize> = None;
    let mut mv: Option<(usize, usize)> = None;
    let multi = !single(p.op);

    ui.columns(2, |c| {
        // ── levý sloupec: soubory ──
        card(&mut c[0], |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(300.0);
            ui.horizontal(|ui| {
                heading_small(ui, if multi { tr!("Soubory (přetažením změníš pořadí)", "Files (drag to reorder)") } else { tr!("Soubor", "File") });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, &trf!("{}  Přidat", "{}  Add", ph::PLUS)).clicked() {
                        let mut d = rfd::FileDialog::new().set_title(tr!("Vyber soubory", "Select files"));
                        if p.op != PdfOp::ToPdf {
                            d = d.add_filter("PDF", &["pdf"]);
                        }
                        if let Some(f) = if multi { d.pick_files() } else { d.pick_file().map(|x| vec![x]) } {
                            pending.extend(f);
                        }
                    }
                });
            });
            if p.files.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(ph::FILE_PDF).size(44.0).color(muted_c()));
                    ui.label(muted(tr!("Přetáhni sem soubory nebo klikni na Přidat", "Drop files here or click Add")));
                });
            }
            let n_files = p.files.len();
            let dz = egui::Frame::NONE;
            let (_, _dropped) = ui.dnd_drop_zone::<usize, ()>(dz, |ui| {
                ui.set_width(ui.available_width());
                for i in 0..n_files {
                    let id = egui::Id::new(("pdf-dnd", i));
                    let resp = ui
                        .dnd_drag_source(id, i, |ui| {
                            ui.set_width(ui.available_width() - 6.0);
                            ui.horizontal(|ui| {
                                if multi {
                                    ui.label(muted(ph::DOTS_SIX_VERTICAL).size(18.0));
                                }
                                ui.label(RichText::new(format!("{}.", i + 1)).color(muted_c()));
                                let f = &p.files[i];
                                ui.label(RichText::new(f.path.file_name().unwrap_or_default().to_string_lossy()).font(bold(14.0)));
                                if let Some(n) = f.pages {
                                    pill_tag(ui, &format!("{n} str."), muted_c());
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button(ph::X).clicked() {
                                        remove = Some(i);
                                    }
                                    if multi {
                                        if i + 1 < n_files && ui.small_button(ph::ARROW_DOWN).clicked() {
                                            mv = Some((i, i + 2));
                                        }
                                        if i > 0 && ui.small_button(ph::ARROW_UP).clicked() {
                                            mv = Some((i, i - 1));
                                        }
                                    }
                                });
                            });
                        })
                        .response;
                    if let (Some(pointer), Some(hover)) = (ui.input(|inp| inp.pointer.interact_pos()), resp.dnd_hover_payload::<usize>()) {
                        let rect = resp.rect;
                        let stroke = egui::Stroke::new(2.5_f32, accent());
                        let insert = if *hover == i {
                            ui.painter().hline(rect.x_range(), rect.center().y, stroke);
                            i
                        } else if pointer.y < rect.center().y {
                            ui.painter().hline(rect.x_range(), rect.top(), stroke);
                            i
                        } else {
                            ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                            i + 1
                        };
                        if let Some(dragged) = resp.dnd_release_payload::<usize>() {
                            mv = Some((*dragged, insert));
                        }
                    }
                }
            });
            if multi && !p.files.is_empty() {
                let total: u32 = p.files.iter().filter_map(|f| f.pages).sum();
                ui.label(muted(trf!("{} souborů{}", "{} files{}", p.files.len(), if total > 0 { trf!(" · {total} stránek celkem", " · {total} pages total") } else { String::new() })).size(12.5));
            }
        });

        // ── pravý sloupec: nastavení ──
        card(&mut c[1], |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(300.0);
            let pages = p.files.first().and_then(|f| f.pages);
            let run_label = match p.op {
                PdfOp::Merge => tr!("Sloučit do jednoho PDF", "Merge into one PDF"),
                PdfOp::ToPdf => tr!("Převést do PDF", "Convert to PDF"),
                PdfOp::Split => tr!("Rozdělit", "Split"),
                PdfOp::Pages => tr!("Provést", "Apply"),
                PdfOp::Rotate => tr!("Otočit", "Rotate"),
                PdfOp::Optimize => tr!("Optimalizovat", "Optimise"),
                PdfOp::Lock => if p.lock_encrypt { tr!("Zamknout", "Lock") } else { tr!("Odemknout", "Unlock") },
            };
            match p.op {
                PdfOp::Merge => {
                    heading_small(ui, tr!("Sloučit PDF", "Merge PDFs"));
                    row(ui, tr!("Název výsledku", "Result name"), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(220.0));
                        ui.label(muted(".pdf"));
                    });
                    ui.label(muted(tr!("Pořadí = pořadí v seznamu. Změníš ho přetažením řádků (nebo šipkami).", "Order = order in the list. Change it by dragging rows (or with the arrows).")).size(12.5));
                }
                PdfOp::ToPdf => {
                    heading_small(ui, tr!("Cokoliv → PDF", "Anything → PDF"));
                    ui.label(muted(tr!("Obrázky (všechny formáty ImageMagicku), dokumenty (docx, odt, md, html, txt, epub…) i hotová PDF.", "Images (all ImageMagick formats), documents (docx, odt, md, html, txt, epub…) and existing PDFs.")).size(12.5));
                    ui.checkbox(&mut p.merge_all, tr!("Sloučit vše do jednoho PDF (v zobrazeném pořadí)", "Merge everything into one PDF (in the shown order)"));
                    if p.merge_all {
                        row(ui, tr!("Název výsledku", "Result name"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(220.0));
                        });
                    }
                    row(ui, tr!("Papír / písmo", "Paper / font"), |ui| {
                        egui::ComboBox::from_id_salt("p-paper").selected_text(p.opts.paper.clone()).show_ui(ui, |ui| {
                            for c in ["a4", "a5", "a3", "us-letter", "us-legal"] {
                                ui.selectable_value(&mut p.opts.paper, c.to_string(), c);
                            }
                        });
                        ui.add(egui::Slider::new(&mut p.opts.font_pt, 8..=18).suffix(" pt"));
                    });
                    row(ui, tr!("Kvalita obrázků", "Image quality"), |ui| {
                        ui.add(egui::Slider::new(&mut p.opts.img_quality, 30..=100));
                        ui.add(egui::Slider::new(&mut p.opts.dpi, 72..=600).suffix(" DPI"));
                    });
                }
                PdfOp::Split => {
                    heading_small(ui, tr!("Rozdělit PDF", "Split PDF"));
                    segmented(ui, &mut p.split, &[(SplitMode::Every, tr!("Po N stránkách", "Every N pages")), (SplitMode::Ranges, tr!("Podle rozsahů", "By ranges"))]);
                    if p.split == SplitMode::Every {
                        row(ui, tr!("Stránek v souboru", "Pages per file"), |ui| {
                            ui.add(egui::DragValue::new(&mut p.split_n).range(1..=10000));
                            if let Some(n) = pages {
                                ui.label(muted(trf!("→ {} souborů", "→ {} files", n.div_ceil(p.split_n.max(1)))));
                            }
                        });
                    } else {
                        row(ui, tr!("Rozsahy", "Ranges"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.ranges).desired_width(300.0));
                        });
                        ui.label(muted(tr!("Každá část odděleně středníkem, např.  1-3; 4-6; 7-z   (z = poslední stránka)", "Separate parts with a semicolon, e.g.  1-3; 4-6; 7-z   (z = last page)")).size(12.5));
                    }
                }
                PdfOp::Pages => {
                    heading_small(ui, tr!("Stránky", "Pages"));
                    segmented(ui, &mut p.mode, &[(PageMode::Extract, tr!("Vybrat", "Select")), (PageMode::Delete, tr!("Smazat", "Delete")), (PageMode::Odd, tr!("Liché", "Odd")), (PageMode::Even, tr!("Sudé", "Even")), (PageMode::Reverse, tr!("Obrátit", "Reverse"))]);
                    if matches!(p.mode, PageMode::Extract | PageMode::Delete) {
                        row(ui, tr!("Stránky", "Pages"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.spec).hint_text("1-3,7,9-z").desired_width(260.0));
                            if let Some(n) = pages {
                                ui.label(muted(format!("(z {n})")));
                            }
                        });
                        if p.mode == PageMode::Extract {
                            ui.label(muted(tr!("Pořadí se zachová – \"3,1,2\" stránky přeuspořádá.", "The order is kept – \"3,1,2\" reorders the pages.")).size(12.5));
                        }
                    }
                }
                PdfOp::Rotate => {
                    heading_small(ui, tr!("Otočit stránky", "Rotate pages"));
                    segmented(ui, &mut p.angle, &[(90, "90° →"), (180, "180°"), (270, "90° ←")]);
                    row(ui, tr!("Jen stránky", "Only pages"), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut p.rot_spec).hint_text(tr!("prázdné = všechny, např. 1-3,5", "empty = all, e.g. 1-3,5")).desired_width(260.0));
                    });
                }
                PdfOp::Optimize => {
                    heading_small(ui, tr!("Optimalizace", "Optimisation"));
                    ui.label(muted(tr!("Přepakuje objekty a znovu zkomprimuje streamy (bezztrátově). Často ušetří pár až desítky procent.", "Repacks objects and recompresses streams (losslessly). Often saves a few to tens of percent.")).size(12.5));
                    ui.checkbox(&mut p.linearize, tr!("Linearizovat (rychlé zobrazení na webu)", "Linearise (fast web view)"));
                }
                PdfOp::Lock => {
                    heading_small(ui, tr!("Heslo k PDF", "PDF password"));
                    segmented(ui, &mut p.lock_encrypt, &[(true, tr!("Zamknout (AES-256)", "Lock (AES-256)")), (false, tr!("Odemknout", "Unlock"))]);
                    if p.lock_encrypt {
                        row(ui, tr!("Heslo pro otevření", "Open password"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.user_pw).password(true).desired_width(220.0));
                        });
                        row(ui, tr!("Heslo vlastníka", "Owner password"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.owner_pw).password(true).hint_text(tr!("nepovinné", "optional")).desired_width(220.0));
                        });
                    }
                }
            }
            if !(p.op == PdfOp::Lock && p.lock_encrypt) && !matches!(p.op, PdfOp::ToPdf) {
                row(ui, tr!("Heslo zdroje", "Source password"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.pw).password(true).hint_text(tr!("jen pokud je PDF zamčené", "only if the PDF is locked")).desired_width(220.0));
                });
            }
            row(ui, tr!("Výstupní složka", "Output folder"), |ui| pick_dir_button(ui, &mut p.out_dir));
            ui.add_space(6.0);
            let running = p.task.and_then(|t| app.tasks.get(&t)).map(|t| t.running()).unwrap_or(false);
            ui.add_enabled_ui(!running, |ui| {
                if primary_button(ui, &format!("{}  {run_label}", ph::PLAY)).clicked() {
                    match build(app, &p) {
                        Ok((task, meta)) => p.task = Some(app.start_task(meta, move |ctx| run(ctx, task))),
                        Err(e) => app.toast(e),
                    }
                }
            });
            let act = match p.task.and_then(|t| app.tasks.get(&t)) {
                Some(t) => task_view(ui, t, &app.stats),
                None => TA::None,
            };
            match act {
                TA::Cancel => {
                    if let Some(t) = p.task.and_then(|t| app.tasks.get(&t)) {
                        t.shared.cancel();
                    }
                }
                TA::Retry => match build(app, &p) {
                    Ok((task, meta)) => p.task = Some(app.start_task(meta, move |ctx| run(ctx, task))),
                    Err(e) => app.toast(e),
                },
                TA::None => {}
            }
        });
    });

    if let Some(i) = remove {
        p.files.remove(i);
    }
    if let Some((from, to)) = mv {
        if from < p.files.len() {
            let f = p.files.remove(from);
            let to = if from < to { to - 1 } else { to };
            p.files.insert(to.min(p.files.len()), f);
        }
    }
    app.pdf = p;
    if !pending.is_empty() {
        add_files(app, pending);
    }
}

//! Sdílené UI prvky: výsledek úlohy, výběr složky, řádky nastavení s nápovědou, číselná pole.
use crate::{app::*, stats::Stats, theme::*, util::*};
use egui::{vec2, Align, Layout, RichText, Sense, Ui};
use egui_phosphor::regular as ph;
use std::{ops::RangeInclusive, path::PathBuf};

#[derive(PartialEq, Eq)]
pub enum TA {
    None,
    Cancel,
    Retry,
}

/// Průběh / výsledek / chyba úlohy včetně statistik po dokončení.
pub fn task_view(ui: &mut Ui, t: &Task, stats: &Stats) -> TA {
    let mut act = TA::None;
    match &t.state {
        TaskState::Running => {
            let queued = t.shared.started().is_none();
            progress_bar(ui, t.shared.frac(), t.elapsed(), &t.shared.note(), queued);
            if ui.button(format!("{}  {}", ph::X, tr!("Zrušit", "Cancel"))).clicked() {
                act = TA::Cancel;
            }
        }
        TaskState::Done(o, secs) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(trf!("{}  Hotovo za {}", "{}  Done in {}", ph::CHECK_CIRCLE, fmt_dur(*secs))).color(good()).font(bold(14.5)));
                pill_tag(ui, o.engine.label(), if o.engine == crate::stats::Engine::Cpu { muted_c() } else { accent() });
                let delta = if o.bytes_in > 0 { (o.bytes_out as f64 / o.bytes_in as f64 - 1.0) * 100.0 } else { 0.0 };
                if o.bytes_in > 0 {
                    pill_tag(ui, &format!("{} → {} ({:+.0} %)", fmt_bytes(o.bytes_in), fmt_bytes(o.bytes_out), delta), muted_c());
                }
                if o.media_secs > 1.0 && *secs > 0.05 {
                    pill_tag(ui, &trf!("{:.1}× rychlost", "{:.1}× speed", o.media_secs / secs), good());
                }
                if !o.detail.is_empty() {
                    ui.label(muted(o.detail.clone()).size(12.5));
                }
            });
            ui.horizontal(|ui| {
                if let Some(first) = o.outputs.first() {
                    if ui.button(format!("{}  {}", ph::FOLDER_OPEN, tr!("Ve složce", "Show in folder"))).clicked() {
                        reveal(first);
                    }
                    if first.is_file() && ui.button(format!("{}  {}", ph::ARROW_SQUARE_OUT, tr!("Otevřít", "Open"))).clicked() {
                        open_path(first);
                    }
                }
                ui.label(muted(trf!("Celkem převedeno: {} · čas {}", "Total converted: {} · time {}", stats.total_jobs, fmt_long(stats.secs_total))).size(12.0));
            });
        }
        TaskState::Failed(e) => {
            ui.label(RichText::new(format!("{}  {}", ph::WARNING_CIRCLE, tr!("Chyba při převodu", "Conversion failed"))).color(bad()).font(bold(14.5)));
            egui::CollapsingHeader::new(tr!("Podrobnosti", "Details")).id_salt(e.as_ptr() as usize).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(e.clone()).monospace().size(12.0)).wrap());
                if ui.small_button(format!("{}  {}", ph::COPY, tr!("Kopírovat chybu", "Copy error"))).clicked() {
                    ui.ctx().copy_text(e.clone());
                }
            });
            if ui.button(format!("{}  {}", ph::ARROW_COUNTER_CLOCKWISE, tr!("Zkusit znovu", "Try again"))).clicked() {
                act = TA::Retry;
            }
        }
        TaskState::Cancelled => {
            ui.label(muted(format!("{}  {}", ph::PROHIBIT, tr!("Zrušeno", "Cancelled"))));
            if ui.button(format!("{}  {}", ph::ARROW_COUNTER_CLOCKWISE, tr!("Znovu", "Again"))).clicked() {
                act = TA::Retry;
            }
        }
    }
    act
}

/// Ikonka (i): po najetí ukáže vysvětlení, po kliknutí ho nechá otevřené (funguje i na dotykové obrazovce).
pub fn info_icon(ui: &mut Ui, tip: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::Help);
    let pop_id = egui::Popup::default_response_id(&resp);
    let open = egui::Popup::is_id_open(ui.ctx(), pop_id);
    let col = if resp.hovered() || open { accent_fg() } else { muted_c() };
    if resp.hovered() || open {
        ui.painter().circle_filled(rect.center(), 11.5, col.gamma_multiply(0.14));
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, ph::INFO, egui::FontId::proportional(17.0), col);
    if !open {
        resp.clone().on_hover_ui(|ui| {
            ui.set_max_width(340.0);
            ui.add(egui::Label::new(tip).wrap());
        });
    }
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).width(340.0).show(|ui| {
        ui.set_max_width(340.0);
        ui.add(egui::Label::new(tip).wrap());
    });
}

pub const LABEL_W: f32 = 190.0;

/// Řádek nastavení: popisek (+ ikonka (i) s nápovědou) vlevo, ovládací prvky vpravo.
pub fn row_t(ui: &mut Ui, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(LABEL_W, 30.0), Layout::right_to_left(Align::Center), |ui| {
            if !tip.is_empty() {
                info_icon(ui, tip);
            }
            ui.label(muted(label.to_string()));
        });
        add(ui);
    });
}

pub fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    row_t(ui, label, "", add);
}

/// Nadpis sekce v panelu nastavení.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).font(bold(14.5)).color(accent()));
    ui.add_space(2.0);
}

pub fn pick_dir_button(ui: &mut Ui, current: &mut Option<PathBuf>) {
    ui.horizontal(|ui| {
        let txt = current.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| tr!("(podle nastavení)", "(from settings)").to_string());
        ui.label(muted(txt));
        if ui.button(format!("{}  {}", ph::FOLDER_OPEN, tr!("Změnit…", "Change…"))).clicked() {
            if let Some(d) = rfd::FileDialog::new().set_title(tr!("Výstupní složka", "Output folder")).pick_folder() {
                *current = Some(d);
            }
        }
        if current.is_some() && ui.button(ph::X).clicked() {
            *current = None;
        }
    });
}

fn parse_num(s: &str) -> Option<f64> {
    let num: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-')).collect::<String>().replace(',', ".");
    num.parse::<f64>().ok()
}

/// Celočíselné pole; hodnota 0 se zobrazí jako `zero` (např. "Zdroj" / "Auto"), ne jako číslo.
pub fn dv_u32(ui: &mut Ui, v: &mut u32, range: RangeInclusive<u32>, unit: &str, zero: &str, speed: f64) -> egui::Response {
    let (z, u, zp) = (zero.to_string(), unit.to_string(), zero.to_string());
    ui.add(
        egui::DragValue::new(v)
            .range(range)
            .speed(speed)
            .custom_formatter(move |n, _| {
                if n == 0.0 && !z.is_empty() {
                    z.clone()
                } else if u.is_empty() {
                    format!("{}", n as u32)
                } else {
                    format!("{} {}", n as u32, u)
                }
            })
            .custom_parser(move |s| if s.trim().eq_ignore_ascii_case(&zp) { Some(0.0) } else { parse_num(s) }),
    )
}

pub fn dv_f32(ui: &mut Ui, v: &mut f32, range: RangeInclusive<f32>, unit: &str, zero: &str, speed: f64) -> egui::Response {
    let (z, u, zp) = (zero.to_string(), unit.to_string(), zero.to_string());
    ui.add(
        egui::DragValue::new(v)
            .range(range)
            .speed(speed)
            .custom_formatter(move |n, _| {
                if n == 0.0 && !z.is_empty() {
                    z.clone()
                } else {
                    let t = format!("{:.3}", n);
                    let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
                    if u.is_empty() {
                        t
                    } else {
                        format!("{t} {u}")
                    }
                }
            })
            .custom_parser(move |s| if s.trim().eq_ignore_ascii_case(&zp) { Some(0.0) } else { parse_num(s) }),
    )
}

/// Celočíselné pole, které při hodnotě 0 ("jako zdroj") ukáže SKUTEČNOU hodnotu zdroje (např. "1920 px"),
/// ne slovo "Zdroj". Úprava hodnoty ji nastaví natvrdo; hodnota rovná zdroji zůstane 0 (= beze změny).
pub fn dv_u32_src(ui: &mut Ui, v: &mut u32, range: RangeInclusive<u32>, unit: &str, src: Option<u32>, zero: &str, speed: f64) -> egui::Response {
    match (*v == 0, src) {
        (true, Some(s)) if s > 0 => {
            let mut tmp = s;
            let r = ui
                .scope(|ui| {
                    let col = accent_fg();
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = col;
                    ui.visuals_mut().widgets.hovered.fg_stroke.color = col;
                    dv_u32(ui, &mut tmp, range, unit, "", speed)
                })
                .inner;
            if r.changed() && tmp != s {
                *v = tmp;
            }
            r.on_hover_text(tr!("Hodnota ze zdroje – změň ji, jen pokud chceš jiný výstup.", "Value taken from the source – change it only if you want a different output."))
        }
        _ => dv_u32(ui, v, range, unit, zero, speed),
    }
}

/// Totéž pro desetinná čísla (FPS).
pub fn dv_f32_src(ui: &mut Ui, v: &mut f32, range: RangeInclusive<f32>, unit: &str, src: Option<f32>, zero: &str, speed: f64) -> egui::Response {
    match (*v == 0.0, src) {
        (true, Some(s)) if s > 0.0 => {
            let mut tmp = s;
            let r = ui
                .scope(|ui| {
                    let col = accent_fg();
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = col;
                    ui.visuals_mut().widgets.hovered.fg_stroke.color = col;
                    dv_f32(ui, &mut tmp, range, unit, "", speed)
                })
                .inner;
            if r.changed() && (tmp - s).abs() > 0.0005 {
                *v = tmp;
            }
            r.on_hover_text(tr!("Hodnota ze zdroje – změň ji, jen pokud chceš jiný výstup.", "Value taken from the source – change it only if you want a different output."))
        }
        _ => dv_f32(ui, v, range, unit, zero, speed),
    }
}

/// Malé tlačítko-předvolba (např. "1080p").
pub fn preset_btn(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).size(12.5)).corner_radius(egui::CornerRadius::same(10)).min_size(vec2(0.0, 24.0))).clicked()
}

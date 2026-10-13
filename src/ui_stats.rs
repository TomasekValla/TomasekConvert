//! Záložka "Statistiky": lifetime čísla, CPU vs GPU, oblíbené formáty, historie.
use crate::{app::*, theme::*, util::*};
use egui::{vec2, Color32, CornerRadius, RichText, Sense, Ui};
use egui_phosphor::regular as ph;

fn tile(ui: &mut Ui, icon: &str, value: String, label: &str, color: Color32) {
    card(ui, |ui| {
        ui.set_min_height(92.0);
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            badge(ui, icon, color, 34.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(value).font(bold(24.0)));
                ui.label(muted(label.to_string()).size(12.5));
            });
        });
    });
}

fn bar(ui: &mut Ui, label: &str, frac: f32, right: String, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 22.0], egui::Label::new(RichText::new(label).font(bold(13.5))));
        let w = (ui.available_width() - 150.0).max(80.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::from_black_alpha(80));
        let mut r = rect;
        r.set_width((w * frac.clamp(0.0, 1.0)).max(if frac > 0.0 { 8.0 } else { 0.0 }));
        ui.painter().rect_filled(r, CornerRadius::same(8), color);
        ui.label(muted(right).size(12.5));
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let s = app.stats.clone();
    ui.add_space(6.0);
    title(ui, tr!("Statistiky", "Statistics"), 30.0);
    ui.label(muted(trf!("Od {} · uloženo lokálně, nikam se neodesílá", "Since {} · stored locally, never sent anywhere", format_date(s.first_use))));
    ui.add_space(8.0);

    let saved = s.bytes_in as i64 - s.bytes_out as i64;
    let fav = s.top_targets(1).first().map(|(k, c)| format!(".{k} ({c}×)")).unwrap_or_else(|| "—".into());
    let cols = if ui.available_width() > 900.0 { 4 } else { 2 };
    let tiles: Vec<(&str, String, &str, Color32)> = vec![
        (ph::CHECK_CIRCLE, s.total_jobs.to_string(), tr!("převedeno celkem", "converted in total"), good()),
        (ph::TIMER, fmt_long(s.secs_total), tr!("strávený čas převody", "time spent converting"), accent()),
        (ph::HEART, fav, tr!("nejoblíbenější cíl", "favourite target"), Color32::from_rgb(255, 140, 160)),
        (ph::DATABASE, format!("{}{}", if saved < 0 { "+" } else { "−" }, fmt_bytes(saved.unsigned_abs())), tr!("rozdíl velikosti (vstup → výstup)", "size difference (input → output)"), Color32::from_rgb(130, 160, 255)),
    ];
    for chunk in tiles.chunks(cols) {
        ui.columns(cols, |c| {
            for (i, (icon, v, l, col)) in chunk.iter().enumerate() {
                tile(&mut c[i], icon, v.clone(), l, *col);
            }
        });
    }

    ui.columns(2, |c| {
        card(&mut c[0], |ui| {
            ui.set_width(ui.available_width());
            heading_small(ui, "CPU vs. GPU");
            let total = (s.cpu_jobs + s.gpu_jobs + s.mixed_jobs).max(1) as f32;
            bar(ui, "CPU", s.cpu_jobs as f32 / total, format!("{}× · {}", s.cpu_jobs, fmt_long(s.cpu_secs)), muted_c());
            bar(ui, "GPU (AMF)", s.gpu_jobs as f32 / total, format!("{}× · {}", s.gpu_jobs, fmt_long(s.gpu_secs)), accent());
            bar(ui, tr!("GPU dekód", "GPU decode"), s.mixed_jobs as f32 / total, format!("{}× · {}", s.mixed_jobs, fmt_long(s.mixed_secs)), Color32::from_rgb(190, 150, 255));
            ui.label(muted(trf!("Nejdelší převod: {} · neúspěšných: {}", "Longest conversion: {} · failed: {}", fmt_long(s.longest_job_secs), s.failed_jobs)).size(12.5));
        });
        card(&mut c[1], |ui| {
            ui.set_width(ui.available_width());
            heading_small(ui, tr!("Co převádíš nejraději", "What you convert most"));
            let top = s.top_targets(5);
            let max = top.first().map(|t| t.1).unwrap_or(1).max(1) as f32;
            if top.is_empty() {
                ui.label(muted(tr!("Zatím nic – převeď první soubor!", "Nothing yet – convert your first file!")));
            }
            for (k, n) in &top {
                bar(ui, &format!(".{k}"), *n as f32 / max, format!("{n}×"), Color32::from_rgb(130, 230, 170));
            }
            let kinds = s.top_kinds();
            if !kinds.is_empty() {
                ui.label(muted(kinds.iter().map(|(k, n)| format!("{k}: {n}×")).collect::<Vec<_>>().join("  ·  ")).size(12.5));
            }
        });
    });

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            heading_small(ui, tr!("Poslední převody", "Recent conversions"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(trf!("{}  Vynulovat statistiky", "{}  Reset statistics", ph::TRASH)).clicked() {
                    app.stats = crate::stats::Stats::default();
                    app.stats.first_use = unix_now();
                    app.stats.save();
                }
            });
        });
        if s.history.is_empty() {
            ui.label(muted(tr!("Historie je prázdná.", "History is empty.")));
        }
        for h in s.history.iter().rev().take(25) {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&h.name).font(bold(13.5)));
                pill_tag(ui, &format!(".{} → .{}", h.from, h.to), muted_c());
                pill_tag(ui, h.engine.label(), if h.engine == crate::stats::Engine::Cpu { muted_c() } else { accent() });
                ui.label(muted(format!("{} · {} → {}", fmt_dur(h.secs), fmt_bytes(h.bytes_in), fmt_bytes(h.bytes_out))).size(12.5));
                if !h.output.is_empty() && ui.small_button(ph::FOLDER_OPEN).clicked() {
                    reveal(std::path::Path::new(&h.output));
                }
            });
        }
    });
}

fn format_date(ts: u64) -> String {
    // civil-from-days (bez závislosti na chrono)
    let days = (ts / 86400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{d}. {m}. {y}")
}

// V release buildu na Windows nechceme okno konzole.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod app;
mod appicon;
mod archive;
mod convert;
mod doc;
mod ffmpeg;
mod formats;
mod gpu;
mod hw;
mod instance;
mod image;
mod imgnative;
mod job;
mod opts;
mod pdfops;
mod probe;
#[cfg(test)]
mod selftest;
mod shell;
mod update;
mod stats;
mod theme;
mod tools;
mod tray;
mod trackops;
mod ui_common;
mod ui_home;
mod ui_info;
mod ui_opts;
mod ui_pdf;
mod ui_setup;
mod ui_queue;
mod ui_settings;
mod ui_stats;
mod ui_tools;
mod util;

/// Co přišlo z příkazové řádky.
pub struct Startup {
    pub files: Vec<std::path::PathBuf>,
    pub hidden: bool,
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hidden = args.iter().any(|a| a == "--tray" || a == "--hidden");
    let files: Vec<std::path::PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(std::path::PathBuf::from)
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .filter(|p| p.exists())
        .collect();

    // už běží jiná instance? předej jí soubory a skonči
    if instance::acquire() == instance::Role::Secondary {
        let msg = instance::hand_over(&files);
        if instance::wait_consumed(&msg, std::time::Duration::from_secs(3)) {
            return Ok(());
        }
        // první instance neodpovídá (visí) -> nevnucujeme se, jen skončíme
        return Ok(());
    }

    update::cleanup(); // zbytky předchozí aktualizace (stará exe, stažené soubory)
    let settings = app::Settings::load();
    i18n::set_english(settings.lang != "cs");
    let (rgba, width, height) = appicon::rgba();
    let start_hidden = hidden && tray::supported() && settings.tray;
    let mut vp = egui::ViewportBuilder::default()
        .with_title("TomasekConvert")
        .with_app_id("TomasekConvert")
        .with_icon(egui::IconData { rgba, width, height })
        .with_inner_size([1280.0, 860.0])
        .with_min_inner_size([760.0, 560.0])
        .with_drag_and_drop(true);
    if start_hidden {
        vp = vp.with_visible(false);
    }
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    let startup = Startup { files, hidden };
    let r = eframe::run_native("TomasekConvert", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, startup)))));
    instance::release();
    r
}

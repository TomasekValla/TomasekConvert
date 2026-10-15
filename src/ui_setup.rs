//! Úvodní průvodce nastavením (při prvním spuštění) a lišta s aktualizací.
use crate::{
    app::*,
    theme::*,
    ui_common::*,
    ui_settings::{swatch, PALETTE},
    update::{InstallMode, UpdateState},
};
use egui::{vec2, Margin, RichText, Ui};
use egui_phosphor::regular as ph;

pub struct Wizard {
    pub step: usize,
}

const STEPS: usize = 5;

fn dots(ui: &mut Ui, step: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for i in 0..STEPS {
            let (rect, _) = ui.allocate_exact_size(vec2(if i == step { 26.0 } else { 9.0 }, 9.0), egui::Sense::hover());
            let c = if i == step { accent() } else if i < step { accent().gamma_multiply(0.5) } else { chip_off() };
            ui.painter().rect_filled(rect, egui::CornerRadius::same(5), c);
        }
        ui.add_space(8.0);
        ui.label(muted(trf!("Krok {} z {}", "Step {} of {}", step + 1, STEPS)).size(12.5));
    });
}

fn explain(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(muted(text).size(13.5)).wrap());
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut w) = app.wizard.take() else { return };
    let mut finish = false;
    let mut toast: Option<String> = None;
    let mut changed = false;
    let mut theme_changed = false;
    egui::Modal::new(egui::Id::new("setup-wizard")).frame(card_frame(card_c()).inner_margin(Margin::same(26))).show(ctx, |ui| {
        ui.set_width(580.0);
        ui.set_min_height(330.0);
        dots(ui, w.step);
        ui.add_space(6.0);
        match w.step {
            0 => {
                title(ui, tr!("Vítej v TomasekConvert", "Welcome to TomasekConvert"), 26.0);
                explain(ui, tr!("Pár rychlých otázek a appka bude nastavená podle tebe. Všechno jde později změnit v Nastavení.", "A few quick questions and the app is set up your way. Everything can be changed later in Settings."));
                ui.add_space(6.0);
                row_t(ui, tr!("Jazyk", "Language"), tr!("Jazyk celého rozhraní.", "Language of the whole interface."), |ui| {
                    let mut lang = app.settings.lang.clone();
                    if chip(ui, lang == "en", "English").clicked() {
                        lang = "en".into();
                    }
                    if chip(ui, lang == "cs", "Čeština").clicked() {
                        lang = "cs".into();
                    }
                    if lang != app.settings.lang {
                        app.settings.lang = lang;
                        crate::i18n::set_english(app.settings.lang == "en");
                        changed = true;
                    }
                });
                row_t(ui, tr!("Volby převodu", "Conversion options"), tr!("Základní ukazuje jen to podstatné (kvalita, rozlišení…). Pokročilé všechny hodnoty: vlastní bitrate, CPU presety, pixel formát a další.", "Basic shows only the essentials (quality, resolution…). Advanced shows every value: custom bitrate, CPU presets, pixel format and more."), |ui| {
                    if chip(ui, !app.settings.advanced, tr!("Základní", "Basic")).clicked() {
                        app.settings.advanced = false;
                        changed = true;
                    }
                    if chip(ui, app.settings.advanced, tr!("Pokročilé", "Advanced")).clicked() {
                        app.settings.advanced = true;
                        changed = true;
                    }
                });
            }
            1 => {
                title(ui, tr!("Vzhled", "Look"), 26.0);
                explain(ui, tr!("Vyber režim a barvu zvýraznění. Mění se hned.", "Pick a theme and an accent colour. It changes right away."));
                ui.add_space(6.0);
                row_t(ui, tr!("Režim", "Theme"), tr!("Tmavý nebo světlý vzhled.", "Dark or light look."), |ui| {
                    if chip(ui, app.settings.dark, tr!("Tmavý", "Dark")).clicked() {
                        app.settings.dark = true;
                        theme_changed = true;
                    }
                    if chip(ui, !app.settings.dark, tr!("Světlý", "Light")).clicked() {
                        app.settings.dark = false;
                        theme_changed = true;
                    }
                });
                row_t(ui, tr!("Barva", "Colour"), tr!("Hlavní barva tlačítek, pruhů průběhu a pozadí.", "Main colour of buttons, progress bars and the background."), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.horizontal_wrapped(|ui| {
                        for (cs, en, rgb) in PALETTE {
                            if swatch(ui, rgb, app.settings.accent == rgb, tr!(cs, en)) {
                                app.settings.accent = rgb;
                                theme_changed = true;
                            }
                        }
                    });
                });
            }
            2 => {
                title(ui, tr!("Spouštění a pozadí", "Startup and background"), 26.0);
                explain(ui, tr!("Appka může po přihlášení naskočit sama a tiše čekat v oznamovací oblasti, aby byla po ruce, když přetáhneš soubory.", "The app can start by itself when you sign in and wait quietly in the system tray, so it is at hand when you drop files."));
                ui.add_space(6.0);
                row_t(ui, tr!("Spouštět s počítačem", "Run at startup"), tr!("Spustí TomasekConvert automaticky po přihlášení do systému.", "Starts TomasekConvert automatically when you sign in."), |ui| {
                    if ui.checkbox(&mut app.settings.autostart, tr!("Zapnout", "Enable")).changed() {
                        if let Err(e) = crate::shell::set_autostart(app.settings.autostart, app.settings.start_hidden) {
                            toast = Some(format!("{}: {e}", tr!("Nepodařilo se", "Failed")));
                        }
                        changed = true;
                    }
                });
                if app.settings.autostart {
                    row_t(ui, tr!("Po startu", "On startup"), tr!("Otevřít okno, nebo se tiše spustit na pozadí a nerušit.", "Open the window, or start quietly in the background without bothering you."), |ui| {
                        let mut hidden = app.settings.start_hidden;
                        if chip(ui, !hidden, tr!("Otevřít okno", "Open the window")).clicked() {
                            hidden = false;
                        }
                        if chip(ui, hidden, tr!("Jen na pozadí", "Background only")).clicked() {
                            hidden = true;
                        }
                        if hidden != app.settings.start_hidden {
                            app.settings.start_hidden = hidden;
                            let _ = crate::shell::set_autostart(true, hidden);
                            changed = true;
                        }
                    });
                }
                if crate::tray::supported() {
                    row_t(ui, tr!("Oznamovací oblast", "System tray"), tr!("Křížek okna schová aplikaci do oznamovací oblasti a převody běží dál. Ukončíš ji pravým tlačítkem na ikoně. Změna se projeví po restartu.", "The window's close button hides the app to the tray and conversions keep running. Quit it with a right-click on the icon. Takes effect after a restart."), |ui| {
                        if ui.checkbox(&mut app.settings.tray, tr!("Křížek schová do tray", "Close button hides to tray")).changed() {
                            changed = true;
                        }
                    });
                }
            }
            3 => {
                title(ui, tr!("Integrace se systémem", "System integration"), 26.0);
                explain(ui, tr!("Aby šla appka najít a otevřít přímo ze souborů.", "So the app can be found and opened straight from your files."));
                ui.add_space(6.0);
                let mut shortcut = crate::shell::shortcut_exists();
                row_t(ui, tr!("Zástupce v nabídce", "App launcher entry"), tr!("Najdeš ho ve vyhledávání systému jako TomasekConvert s logem.", "Find it in system search as TomasekConvert with its logo."), |ui| {
                    if ui.checkbox(&mut shortcut, tr!("Přidat", "Add")).changed() {
                        if shortcut {
                            app.make_shortcut();
                        } else if let Err(e) = crate::shell::remove_start_shortcut() {
                            toast = Some(format!("{}: {e}", tr!("Nepodařilo se", "Failed")));
                        }
                    }
                });
                row_t(ui, tr!("„Otevřít v programu“", "“Open with”"), tr!("Přidá TomasekConvert do seznamu „Otevřít v programu“ u souborů.", "Adds TomasekConvert to the “Open with” list for files."), |ui| {
                    if ui.checkbox(&mut app.settings.open_with, tr!("Zaregistrovat", "Register")).changed() {
                        if let Err(e) = crate::shell::set_open_with(app.settings.open_with) {
                            toast = Some(format!("{}: {e}", tr!("Nepodařilo se", "Failed")));
                        }
                        changed = true;
                    }
                });
                if cfg!(windows) {
                    row_t(ui, tr!("Kontextové menu", "Context menu"), tr!("Přidá do pravého tlačítka na souborech a složkách položku „Převést v TomasekConvert“.", "Adds a “Convert with TomasekConvert” entry to the right-click menu of files and folders."), |ui| {
                        if ui.checkbox(&mut app.settings.context_menu, tr!("Přidat", "Add")).changed() {
                            if let Err(e) = crate::shell::set_context_menu(app.settings.context_menu) {
                                toast = Some(format!("{}: {e}", tr!("Nepodařilo se", "Failed")));
                            }
                            changed = true;
                        }
                    });
                }
                explain(ui, tr!("Zástupce i registrace míří na tuhle exe – nejdřív ji dej tam, kde má zůstat.", "The shortcut and registrations point to this executable – put it where it should stay first."));
            }
            _ => {
                title(ui, tr!("Aktualizace", "Updates"), 26.0);
                explain(ui, tr!("Na internet appka sahá jen kvůli aktualizacím: při startu a pak každých pár hodin se podívá na veřejnou stránku vydání na GitHubu (jen číslo poslední verze). Můžeš to kdykoliv vypnout.", "The app only touches the internet for updates: at startup and then every few hours it looks at the public releases page on GitHub (just the latest version number). You can turn this off at any time."));
                ui.add_space(6.0);
                row_t(ui, tr!("Hledat aktualizace", "Check for updates"), tr!("Kontrola při startu. Nic se neinstaluje bez tvého souhlasu, pokud nezapneš automatické aktualizace.", "Checked at startup. Nothing is installed without your consent unless you enable automatic updates."), |ui| {
                    if ui.checkbox(&mut app.settings.check_updates, tr!("Při startu a průběžně", "At startup and periodically")).changed() {
                        if !app.settings.check_updates {
                            app.settings.auto_update = false;
                        }
                        changed = true;
                    }
                });
                ui.add_enabled_ui(app.settings.check_updates, |ui| {
                    row_t(ui, tr!("Automaticky instalovat", "Install automatically"), tr!("Nová verze se stáhne na pozadí a nainstaluje ve chvíli, kdy je appka schovaná v oznamovací oblasti a nic nepřevádí. Nastavení zůstane zachované. Když je okno otevřené, jen se ti ukáže tlačítko.", "A new version is downloaded in the background and installed when the app is hidden in the tray and not converting. Your settings are kept. While the window is open you just get a button."), |ui| {
                        if ui.checkbox(&mut app.settings.auto_update, tr!("Zapnout", "Enable")).changed() {
                            changed = true;
                        }
                    });
                });
                let how = match crate::update::install_mode() {
                    InstallMode::WindowsInstaller => tr!("Aktualizace spustí tichý instalátor, nahradí program a nastavení zůstane.", "An update runs the installer silently, replaces the program and keeps your settings."),
                    InstallMode::ReplaceFile => tr!("Aktualizace vymění program na místě a nastavení zůstane.", "An update replaces the program in place and keeps your settings."),
                    InstallMode::Manual => tr!("Tuhle kopii je potřeba aktualizovat ručně (ukážu ti odkaz ke stažení).", "This copy has to be updated by hand (you will get the download link)."),
                };
                explain(ui, how);
            }
        }
        ui.add_space(14.0);
        ui.separator();
        ui.horizontal(|ui| {
            if w.step > 0 && ghost_button(ui, &format!("{}  {}", ph::ARROW_LEFT, tr!("Zpět", "Back"))).clicked() {
                w.step -= 1;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w.step + 1 < STEPS {
                    if primary_button(ui, &format!("{}  {}", tr!("Další", "Next"), ph::ARROW_RIGHT)).clicked() {
                        w.step += 1;
                    }
                } else if primary_button(ui, &format!("{}  {}", ph::CHECK, tr!("Hotovo", "Finish"))).clicked() {
                    finish = true;
                }
                if ghost_button(ui, tr!("Přeskočit", "Skip")).on_hover_text(tr!("Nechá výchozí nastavení", "Keeps the defaults")).clicked() {
                    finish = true;
                }
            });
        });
    });
    if theme_changed {
        app.apply_theme();
        changed = true;
    }
    if let Some(t) = toast {
        app.toast(t);
    }
    if changed {
        app.max_parallel_store();
        app.settings.save();
    }
    if finish {
        app.settings.setup_done = true;
        app.settings.save();
        if app.settings.check_updates {
            app.check_update();
        }
    } else {
        app.wizard = Some(w);
    }
}

/// Lišta nad obsahem: nová verze je k dispozici / stahuje se / je připravena.
pub fn update_banner(app: &mut App, ui: &mut Ui) {
    if app.update_dismissed {
        return;
    }
    let state = app.update.clone();
    let mut action: Option<&str> = None;
    match &state {
        UpdateState::Available(u) => {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}  {} {}", ph::ARROW_CIRCLE_UP, tr!("Je tu nová verze", "A new version is available:"), u.version)).font(bold(15.0)).color(accent_fg()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ghost_button(ui, tr!("Později", "Later")).clicked() {
                            action = Some("dismiss");
                        }
                        if ghost_button(ui, &format!("{}  {}", ph::LIST_BULLETS, tr!("Co je nového", "What's new"))).clicked() {
                            action = Some("notes");
                        }
                        let label = if u.mode == InstallMode::Manual { format!("{}  {}", ph::DOWNLOAD_SIMPLE, tr!("Stáhnout", "Download")) } else { format!("{}  {}", ph::DOWNLOAD_SIMPLE, tr!("Aktualizovat", "Update")) };
                        if primary_button(ui, &label).clicked() {
                            action = Some("download");
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        UpdateState::Downloading(u, f) => {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("{}  {} {}", ph::DOWNLOAD_SIMPLE, tr!("Stahuji verzi", "Downloading version"), u.version)).font(bold(14.5)));
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::same(5), chip_off());
                let mut r = rect;
                r.set_width((rect.width() * f.clamp(0.0, 1.0)).max(10.0));
                ui.painter().rect_filled(r, egui::CornerRadius::same(5), accent());
            });
            ui.add_space(8.0);
        }
        UpdateState::Ready(u, _) => {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}  {} {}", ph::CHECK_CIRCLE, tr!("Verze připravena k instalaci:", "Ready to install version"), u.version)).font(bold(15.0)).color(good()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ghost_button(ui, tr!("Později", "Later")).clicked() {
                            action = Some("dismiss");
                        }
                        ui.add_enabled_ui(!app.any_running(), |ui| {
                            if primary_button(ui, &format!("{}  {}", ph::ARROWS_CLOCKWISE, tr!("Restartovat a aktualizovat", "Restart and update"))).on_disabled_hover_text(tr!("Počkej, až doběhnou převody.", "Wait until the conversions finish.")).clicked() {
                                action = Some("apply");
                            }
                        });
                    });
                });
            });
            ui.add_space(8.0);
        }
        UpdateState::Installing => {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("Instaluji aktualizaci, appka se za chvíli restartuje…", "Installing the update, the app restarts in a moment…"));
                });
            });
            ui.add_space(8.0);
        }
        _ => {}
    }
    match action {
        Some("dismiss") => app.update_dismissed = true,
        Some("notes") => {
            if let UpdateState::Available(u) = &state {
                crate::util::open_url(&u.url);
            }
        }
        Some("download") => app.start_update_download(),
        Some("apply") => app.apply_update(),
        _ => {}
    }
}

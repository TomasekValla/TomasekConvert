//! Záložky "Nastavení" a "O aplikaci".
use crate::{app::*, gpu, opts::HwMode, theme::*, ui_common::*, update::UpdateState};
use egui::{vec2, Color32, RichText, Sense, Ui};
use egui_phosphor::regular as ph;

pub const PALETTE: [(&str, &str, [u8; 3]); 10] = [
    ("Fialová", "Purple", [168, 85, 247]),
    ("Růžová", "Pink", [244, 114, 182]),
    ("Červená", "Red", [239, 68, 68]),
    ("Oranžová", "Orange", [249, 115, 22]),
    ("Žlutá", "Yellow", [234, 179, 8]),
    ("Zelená", "Green", [34, 197, 94]),
    ("Tyrkysová", "Teal", [20, 184, 166]),
    ("Azurová", "Cyan", [6, 182, 212]),
    ("Modrá", "Blue", [59, 130, 246]),
    ("Šedá", "Gray", [148, 163, 184]),
];

pub fn swatch(ui: &mut Ui, rgb: [u8; 3], selected: bool, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    ui.painter().circle_filled(rect.center(), 11.0, c);
    if selected {
        ui.painter().circle_stroke(rect.center(), 14.0, egui::Stroke::new(2.0_f32, ui.visuals().text_color()));
    } else if resp.hovered() {
        ui.painter().circle_stroke(rect.center(), 13.0, egui::Stroke::new(1.5_f32, muted_c()));
    }
    resp.on_hover_text(tip).clicked()
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(6.0);
    title(ui, tr!("Nastavení", "Settings"), 30.0);
    ui.add_space(6.0);
    let mut changed = false;
    let mut theme_changed = false;

    // ── Obecné ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::PALETTE, tr!("Vzhled a jazyk", "Appearance and language")));
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
        row_t(ui, tr!("Barva zvýraznění", "Accent colour"), tr!("Hlavní barva tlačítek, pruhů průběhu a pozadí. Vyber z palety nebo si zvol vlastní.", "Main colour of buttons, progress bars and the background. Pick from the palette or choose your own."), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (cs, en, rgb) in PALETTE {
                if swatch(ui, rgb, app.settings.accent == rgb, tr!(cs, en)) {
                    app.settings.accent = rgb;
                    theme_changed = true;
                }
            }
            let mut c = app.settings.accent;
            if ui.color_edit_button_srgb(&mut c).changed() {
                app.settings.accent = c;
                theme_changed = true;
            }
        });
        row_t(ui, tr!("Výchozí režim voleb", "Default options mode"), tr!("Základní ukazuje jen to podstatné; Pokročilé všechny hodnoty.", "Basic shows just the essentials; Advanced shows every value."), |ui| {
            if chip(ui, !app.settings.advanced, tr!("Základní", "Basic")).clicked() {
                app.settings.advanced = false;
                changed = true;
            }
            if chip(ui, app.settings.advanced, tr!("Pokročilé", "Advanced")).clicked() {
                app.settings.advanced = true;
                changed = true;
            }
        });
    });

    // ── Hardware ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::CPU, tr!("Hardware", "Hardware")));
        row_t(ui, tr!("Výchozí kodér", "Default encoder"), tr!("Pro nové soubory. GPU používá hardwarový enkodér, kde ho kodek podporuje; jinak jde převod automaticky na CPU. U každého souboru to jde přepnout.", "For new files. GPU uses a hardware encoder where the codec supports it; otherwise the conversion falls back to the CPU automatically. Can be changed per file."), |ui| {
            let mut hw = app.settings.hw;
            segmented(ui, &mut hw, &[(HwMode::Auto, "GPU"), (HwMode::Cpu, "CPU")]);
            if hw != app.settings.hw {
                app.settings.hw = hw;
                for it in app.items.iter_mut() {
                    it.opts.hw = hw;
                }
                changed = true;
            }
        });
        row_t(ui, tr!("Dekódování videa", "Video decoding"), tr!("Dekódovat zdroj na GPU (H.264, HEVC, VP9, AV1; 8/10-bit 4:2:0).", "Decode the source on the GPU (H.264, HEVC, VP9, AV1; 8/10-bit 4:2:0)."), |ui| {
            let label = match app.si.hw_decoder() {
                Some(d) => format!("{} ({d})", tr!("Hardwarové", "Hardware")),
                None => tr!("Hardwarové (nenalezeno)", "Hardware (not found)").to_string(),
            };
            if ui.checkbox(&mut app.settings.hw_decode, label).changed() {
                for it in app.items.iter_mut() {
                    it.opts.hw_decode = app.settings.hw_decode;
                }
                changed = true;
            }
        });
        row_t(ui, "AVX-512", tr!("Zapne AVX-512 cesty enkodérů x265 (HEVC) a SVT-AV1, které je ve výchozím stavu nepoužívají. Typicky o několik až ~15 % rychlejší na CPU s plnou šířkou AVX-512 (AMD Zen 4/5, Intel Ice Lake a novější). Na některých starších Intelech (Skylake-X) může snížit takty a být pomalejší – tam to vypni.", "Turns on the AVX-512 code paths of the x265 (HEVC) and SVT-AV1 encoders, which they leave off by default. Typically a few to ~15 % faster on CPUs with full-width AVX-512 (AMD Zen 4/5, Intel Ice Lake and newer). On some older Intel CPUs (Skylake-X) it can lower clock speeds and be slower – switch it off there."), |ui| {
            if app.si.avx512 {
                if ui.checkbox(&mut app.settings.avx512, tr!("Používat při enkódování na CPU", "Use for CPU encoding")).changed() {
                    for it in app.items.iter_mut() {
                        it.opts.avx512 = app.settings.avx512;
                    }
                    changed = true;
                }
            } else {
                ui.label(muted(tr!("Tenhle procesor AVX-512 nepodporuje.", "This processor does not support AVX-512.")));
            }
        });
        ui.separator();
        ui.label(RichText::new(tr!("Co tenhle počítač umí", "What this computer supports")).font(bold(14.0)));
        ui.label(format!("CPU: {} ({} {} / {} {})", app.si.cpu, app.si.cores, tr!("jader", "cores"), app.si.threads, tr!("vláken", "threads")));
        ui.label(format!("GPU: {}", app.si.gpu_label()));
        ui.label(muted(format!("AVX-512: {}", if app.si.avx512 { app.si.avx512_features.join(" · ") } else { tr!("nenalezeno", "not detected").to_string() })));
        ui.add_space(4.0);
        ui.label(muted(tr!("Hardwarové enkodéry (ověřené zkušebním převodem):", "Hardware encoders (verified with a test encode):")).size(13.0));
        for b in gpu::ALL {
            let encs = app.si.encoders_of(b);
            if encs.is_empty() {
                ui.label(muted(format!("{}  {} – {}", ph::X_CIRCLE, b.label(), tr!("nenalezeno", "not found"))));
            } else {
                let names: Vec<String> = encs.iter().map(|e| e.split('_').next().unwrap_or(e).to_uppercase()).collect();
                ui.label(RichText::new(format!("{}  {} – {}", ph::CHECK_CIRCLE, b.label(), names.join(", "))).color(good()));
            }
        }
        ui.label(muted(format!(
            "{}: {}",
            tr!("HW dekódování", "HW decoding"),
            app.si.hw_decoder().unwrap_or(tr!("nenalezeno", "not found"))
        )));
        if app.si.hw_encoders.is_empty() {
            ui.label(muted(tr!("Žádný GPU enkodér nenalezen – převody poběží na CPU. Zkontroluj ovladače grafické karty.", "No GPU encoder found – conversions will run on the CPU. Check your graphics drivers.")).size(12.5));
        }
    });

    // ── Výstup a výkon ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::FOLDER_OPEN, tr!("Výstup a výkon", "Output and performance")));
        row_t(ui, tr!("Výstupní složka", "Output folder"), tr!("Kam se ukládají výsledky.", "Where results are saved."), |ui| {
            if chip(ui, app.settings.same_dir, tr!("Stejná jako zdroj", "Same as source")).clicked() {
                app.settings.same_dir = true;
                changed = true;
            }
            if chip(ui, !app.settings.same_dir, tr!("Vlastní složka", "Custom folder")).clicked() {
                app.settings.same_dir = false;
                if app.settings.out_dir.is_empty() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        app.settings.out_dir = d.to_string_lossy().to_string();
                    }
                }
                changed = true;
            }
        });
        if !app.settings.same_dir {
            row(ui, "", |ui| {
                ui.label(muted(app.settings.out_dir.clone()));
                if ui.button(format!("{}  {}", ph::FOLDER_OPEN, tr!("Vybrat…", "Choose…"))).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        app.settings.out_dir = d.to_string_lossy().to_string();
                        changed = true;
                    }
                }
            });
        }
        ui.label(muted(tr!("Existující soubory se nikdy nepřepisují – přidá se číslo (1), (2)…", "Existing files are never overwritten – a number (1), (2)… is added.")).size(12.5));
        row_t(ui, tr!("Souběžné převody", "Parallel jobs"), tr!("Auto = appka sama spustí tolik převodů najednou, kolik unese procesor: spousta malých souborů (obrázky, zvuk, dokumenty) jede vedle sebe, těžké video na CPU si vezme polovinu vláken. Ruční číslo = přesně tolik souborů najednou.", "Auto = the app runs as many conversions at once as your processor can handle: lots of small files (images, audio, documents) run side by side, a heavy CPU video encode takes half of the threads. A number = exactly that many files at once."), |ui| {
            let resp = ui.add(
                egui::Slider::new(&mut app.settings.parallel, 0..=16)
                    .custom_formatter(|v, _| if v as usize == 0 { "Auto".to_string() } else { format!("{}", v as usize) })
                    .custom_parser(|t| if t.trim().eq_ignore_ascii_case("auto") { Some(0.0) } else { t.trim().parse::<f64>().ok() }),
            );
            if resp.changed() {
                changed = true;
            }
        });
    });

    // ── Integrace se systémem ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::APP_WINDOW, tr!("Integrace se systémem", "System integration")));
        let mut toast: Option<String> = None;
        let mut report = |what: &str, r: Result<(), String>| {
            toast = Some(match r {
                Ok(()) => what.to_string(),
                Err(e) => format!("{}: {e}", tr!("Nepodařilo se", "Failed")),
            });
        };
        row_t(ui, tr!("Spouštět se s počítačem", "Run at startup"), tr!("Spustí TomasekConvert automaticky po přihlášení do systému.", "Starts TomasekConvert automatically when you sign in."), |ui| {
            if ui.checkbox(&mut app.settings.autostart, tr!("Automaticky spustit při startu PC", "Automatically run on PC startup")).changed() {
                let r = crate::shell::set_autostart(app.settings.autostart, app.settings.start_hidden);
                report(if app.settings.autostart { tr!("Automatické spuštění zapnuto", "Autostart enabled") } else { tr!("Automatické spuštění vypnuto", "Autostart disabled") }, r);
                changed = true;
            }
        });
        row_t(ui, tr!("Po startu", "On startup"), tr!("Otevřít okno, nebo se tiše spustit na pozadí (v oznamovací oblasti) a nerušit.", "Open the window, or start quietly in the background (system tray) without bothering you."), |ui| {
            let mut hidden = app.settings.start_hidden;
            if chip(ui, !hidden, tr!("Otevřít okno", "Open the window")).clicked() {
                hidden = false;
            }
            if chip(ui, hidden, tr!("Jen na pozadí (tray)", "Background only (tray)")).clicked() {
                hidden = true;
            }
            if hidden != app.settings.start_hidden {
                app.settings.start_hidden = hidden;
                if app.settings.autostart {
                    let r = crate::shell::set_autostart(true, hidden);
                    report(tr!("Uloženo", "Saved"), r);
                }
                changed = true;
            }
        });
        row_t(ui, tr!("Oznamovací oblast (tray)", "System tray"), tr!("Křížek okna schová aplikaci do oznamovací oblasti a převody běží dál. Ukončíš ji pravým tlačítkem na ikoně. Změna se projeví po restartu.", "The window's close button hides the app to the tray and conversions keep running. Quit it with a right-click on the icon. Takes effect after a restart."), |ui| {
            if ui.checkbox(&mut app.settings.tray, tr!("Křížek schová do tray", "Close button hides to tray")).changed() {
                changed = true;
            }
            if !cfg!(windows) {
                ui.label(muted(tr!("(zatím jen Windows)", "(Windows only for now)")).size(12.0));
            }
        });
        let exists = crate::shell::shortcut_exists();
        row_t(ui, tr!("Zástupce v nabídce Start", "Start menu shortcut"), tr!("Aby šla appka najít ve vyhledávání systému jako TomasekConvert s logem.", "So the app can be found in system search as TomasekConvert with its logo."), |ui| {
            if ui.button(format!("{}  {}", ph::PUSH_PIN, if exists { tr!("Obnovit", "Refresh") } else { tr!("Přidat", "Add") })).clicked() {
                app.make_shortcut();
            }
            if exists && ui.button(format!("{}  {}", ph::TRASH, tr!("Odebrat", "Remove"))).clicked() {
                let r = crate::shell::remove_start_shortcut();
                report(tr!("Zástupce odebrán", "Shortcut removed"), r);
            }
        });
        row_t(ui, tr!("Nabídka „Otevřít v programu“", "“Open with” menu"), tr!("Přidá TomasekConvert do seznamu „Otevřít v programu“ u souborů.", "Adds TomasekConvert to the “Open with” list for files."), |ui| {
            if ui.checkbox(&mut app.settings.open_with, tr!("Zaregistrovat", "Register")).changed() {
                let r = crate::shell::set_open_with(app.settings.open_with);
                report(if app.settings.open_with { tr!("Zaregistrováno", "Registered") } else { tr!("Odregistrováno", "Unregistered") }, r);
                changed = true;
            }
        });
        row_t(ui, tr!("Kontextové menu", "Context menu"), tr!("Přidá do pravého tlačítka na souborech a složkách položku „Převést v TomasekConvert“.", "Adds a “Convert with TomasekConvert” entry to the right-click menu of files and folders."), |ui| {
            if ui.checkbox(&mut app.settings.context_menu, tr!("Přidat do pravého tlačítka", "Add to right-click menu")).changed() {
                let r = crate::shell::set_context_menu(app.settings.context_menu);
                report(if app.settings.context_menu { tr!("Přidáno do kontextového menu", "Added to the context menu") } else { tr!("Odebráno z kontextového menu", "Removed from the context menu") }, r);
                changed = true;
            }
            if !cfg!(windows) {
                ui.label(muted(tr!("(zatím jen Windows)", "(Windows only for now)")).size(12.0));
            }
        });
        ui.label(muted(tr!("Zástupce i registrace míří na tuhle exe – před zapnutím ji zkopíruj tam, kde má zůstat.", "The shortcut and registrations point to this executable – copy it where it should stay before enabling them.")).size(12.5));
        if let Some(t) = toast {
            app.toast(t);
        }
    });

    // ── Aktualizace ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::ARROW_CIRCLE_UP, tr!("Aktualizace", "Updates")));
        update_rows(app, ui, &mut changed);
    });

    // ── Data ──
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::DATABASE, tr!("Data aplikace", "App data")));
        ui.label(muted(format!("{}: {}", tr!("Složka", "Folder"), crate::util::app_dir().display())));
        ui.label(muted(tr!("Obsahuje rozbalené nástroje, settings.json a stats.json. Můžeš ji kdykoliv smazat.", "Contains the unpacked tools, settings.json and stats.json. You can delete it at any time.")));
        if ui.button(format!("{}  {}", ph::FOLDER_OPEN, tr!("Otevřít složku", "Open folder"))).clicked() {
            crate::util::open_path(&crate::util::app_dir());
        }
    });

    if theme_changed {
        app.apply_theme();
        changed = true;
    }
    if changed {
        app.max_parallel_store();
        app.settings.save();
    }
}

fn update_rows(app: &mut App, ui: &mut Ui, changed: &mut bool) {
    row_t(ui, tr!("Kontrola aktualizací", "Check for updates"), tr!("Appka sama od sebe nikdy nesahá na internet. Aktualizace se kontroluje jen na požádání (nebo při startu, pokud to zapneš) a stáhne se pouze informace o nejnovější verzi z GitHubu.", "The app never touches the internet on its own. Updates are only checked on request (or at startup if you enable it) and only the latest-release info is fetched from GitHub."), |ui| {
        let busy = matches!(app.update, UpdateState::Checking | UpdateState::Downloading(..) | UpdateState::Installing);
        ui.add_enabled_ui(!busy, |ui| {
            if ghost_button(ui, &format!("{}  {}", ph::ARROWS_CLOCKWISE, tr!("Zkontrolovat teď", "Check now"))).clicked() {
                app.update_dismissed = false;
                app.check_update();
            }
        });
        if ui.checkbox(&mut app.settings.check_updates, tr!("Kontrolovat při startu", "Check at startup")).changed() {
            if !app.settings.check_updates {
                app.settings.auto_update = false;
            }
            *changed = true;
        }
    });
    ui.add_enabled_ui(app.settings.check_updates, |ui| {
        row_t(ui, tr!("Automatická instalace", "Automatic install"), tr!("Nová verze se stáhne na pozadí a nainstaluje ve chvíli, kdy je appka schovaná v oznamovací oblasti a nic nepřevádí. Nastavení zůstane zachované. Když je okno otevřené, jen se ti ukáže tlačítko.", "A new version is downloaded in the background and installed when the app is hidden in the tray and not converting. Your settings are kept. While the window is open you just get a button."), |ui| {
            if ui.checkbox(&mut app.settings.auto_update, tr!("Instalovat aktualizace samo", "Install updates automatically")).changed() {
                *changed = true;
            }
        });
    });
    match app.update.clone() {
        UpdateState::Idle => {}
        UpdateState::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(tr!("Kontroluji…", "Checking…")));
            });
        }
        UpdateState::UpToDate => {
            ui.label(RichText::new(format!("{}  {} ({})", ph::CHECK_CIRCLE, tr!("Máš nejnovější verzi", "You are up to date"), env!("CARGO_PKG_VERSION"))).color(good()));
        }
        UpdateState::Available(u) => {
            ui.label(RichText::new(format!("{}  {} {}", ph::ARROW_CIRCLE_UP, tr!("Je dostupná nová verze", "A new version is available:"), u.version)).color(accent_fg()).font(bold(14.5)));
            if !u.notes.is_empty() {
                ui.add(egui::Label::new(muted(u.notes.chars().take(600).collect::<String>()).size(12.5)).wrap());
            }
            ui.horizontal(|ui| {
                if u.mode != crate::update::InstallMode::Manual {
                    if primary_button(ui, &format!("{}  {}", ph::DOWNLOAD_SIMPLE, tr!("Stáhnout a nainstalovat", "Download and install"))).clicked() {
                        app.start_update_download();
                    }
                }
                if ghost_button(ui, &format!("{}  {}", ph::GLOBE, tr!("Stránka vydání", "Release page"))).clicked() {
                    crate::util::open_url(&u.url);
                }
            });
        }
        UpdateState::Downloading(u, f) => {
            ui.label(muted(format!("{} {} – {:.0} %", tr!("Stahuji verzi", "Downloading version"), u.version, f * 100.0)));
        }
        UpdateState::Ready(u, _) => {
            ui.label(RichText::new(format!("{}  {} {}", ph::CHECK_CIRCLE, tr!("Připraveno k instalaci:", "Ready to install:"), u.version)).color(good()));
            ui.add_enabled_ui(!app.any_running(), |ui| {
                if primary_button(ui, &format!("{}  {}", ph::ARROWS_CLOCKWISE, tr!("Restartovat a aktualizovat", "Restart and update"))).clicked() {
                    app.apply_update();
                }
            });
        }
        UpdateState::Installing => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(tr!("Instaluji…", "Installing…")));
            });
        }
        UpdateState::Error(e) => {
            ui.label(RichText::new(format!("{}  {}", ph::WARNING, e)).color(warn()).size(12.5));
            if ghost_button(ui, &format!("{}  {}", ph::GLOBE, tr!("Stránka vydání", "Release page"))).clicked() {
                crate::util::open_url(&format!("https://github.com/{}/releases/latest", crate::update::REPO));
            }
        }
    }
}

pub fn about(app: &mut App, ui: &mut Ui) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("TomasekConvert").font(bold(44.0)).color(accent()));
        ui.add_space(6.0);
        ui.label(muted("Beautifully crafted, insanely fast and local conversion.").size(18.0));
        ui.label(muted("Don't touch the internet when you don't need it.").size(18.0));
        ui.add_space(6.0);
        ui.label(muted(format!("{} {}", tr!("Verze", "Version"), env!("CARGO_PKG_VERSION"))));
    });
    ui.add_space(14.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::SHIELD_CHECK, tr!("Soukromí", "Privacy")));
        ui.label(tr!("Převody probíhají kompletně na tvém počítači. Aplikace nenavazuje žádná síťová spojení – s jedinou výjimkou: kontrola aktualizací, kterou spustíš sám (nebo zapneš v Nastavení).", "Conversions run entirely on your computer. The app makes no network connections – with one exception: the update check, which you start yourself (or enable in Settings)."));
    });
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::PACKAGE, tr!("Použité nástroje", "Bundled tools")));
        for (n, v) in &app.si.versions {
            ui.label(format!("{n}  –  {v}"));
        }
        ui.label(format!("pdfium: {}", if app.tools.pdfium.is_some() { tr!("ano", "yes") } else { tr!("ne", "no") }));
        ui.label(muted(tr!("Archivy zip / 7z / tar / gz / bz2 / xz / zst / rar jsou zakompilované přímo v aplikaci (Rust).", "Zip / 7z / tar / gz / bz2 / xz / zst / rar archives are built into the app (Rust).")));
        ui.label(muted(tr!("Licence jednotlivých nástrojů najdeš v souboru THIRD_PARTY_NOTICES.md v repozitáři.", "The licences of the individual tools are listed in THIRD_PARTY_NOTICES.md in the repository.")).size(12.5));
    });
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        heading_small(ui, &format!("{}  {}", ph::ARROW_CIRCLE_UP, tr!("Aktualizace", "Updates")));
        let mut ch = false;
        update_rows(app, ui, &mut ch);
        if ch {
            app.settings.save();
        }
    });
}

#![cfg_attr(not(windows), allow(dead_code))]
//! Ikona v oznamovací oblasti Windows (tray) + obnovení skrytého okna.
//! Okno se ukazuje přímo přes Win32, takže funguje i když egui zrovna nekreslí (okno je skryté).
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMsg {
    Show,
    Quit,
}

/// Umí tahle platforma ikonu v oznamovací oblasti?
pub fn supported() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
pub struct Tray {
    _icon: tray_icon::TrayIcon,
}

#[cfg(not(windows))]
pub struct Tray;

/// Zobrazí a přivede do popředí hlavní okno (nalezne ho podle názvu).
#[cfg(windows)]
pub fn show_window() {
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW},
    };
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), w!("TomasekConvert")) {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn show_window() {}

#[cfg(windows)]
pub fn create(ctx: egui::Context, tx: Sender<TrayMsg>) -> Option<Tray> {
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    };
    let (rgba, w, h) = crate::appicon::rgba();
    let icon = Icon::from_rgba(rgba, w, h).ok()?;
    let menu = Menu::new();
    let open = MenuItem::new(tr!("Otevřít TomasekConvert", "Open TomasekConvert"), true, None);
    let quit = MenuItem::new(tr!("Ukončit", "Quit"), true, None);
    menu.append(&open).ok()?;
    menu.append(&quit).ok()?;
    let open_id = open.id().clone();
    let quit_id = quit.id().clone();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("TomasekConvert")
        .with_icon(icon)
        .build()
        .ok()?;
    {
        let tx = tx.clone();
        let ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| match e {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } | TrayIconEvent::DoubleClick { .. } => {
                show_window();
                let _ = tx.send(TrayMsg::Show);
                ctx.request_repaint();
            }
            _ => {}
        }));
    }
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == open_id {
            show_window();
            let _ = tx.send(TrayMsg::Show);
            ctx.request_repaint();
        } else if e.id == quit_id {
            show_window();
            let _ = tx.send(TrayMsg::Quit);
            ctx.request_repaint();
        }
    }));
    Some(Tray { _icon: tray })
}

#[cfg(not(windows))]
pub fn create(_ctx: egui::Context, _tx: Sender<TrayMsg>) -> Option<Tray> {
    None
}

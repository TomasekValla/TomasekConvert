//! Vzhled inspirovaný vert.sh: tmavě fialový přechod, "pill" navigace, zaoblené karty.
//! Dvě plnohodnotné palety (tmavá / světlá). Barvy se berou přes funkce, ne konstanty,
//! takže se po přepnutí motivu přebarví úplně všechno a kontrast sedí v obou režimech.
use egui::{
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
    Arc,
};

static ACCENT_RGB: AtomicU32 = AtomicU32::new(0xA855F7);
static DARK: AtomicBool = AtomicBool::new(true);

/// Barva zvýraznění (nastavitelná v Nastavení).
pub fn set_accent(c: [u8; 3]) {
    ACCENT_RGB.store(((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32, Relaxed);
}
pub fn accent_rgb() -> [u8; 3] {
    let v = ACCENT_RGB.load(Relaxed);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}
pub fn accent() -> Color32 {
    let c = accent_rgb();
    Color32::from_rgb(c[0], c[1], c[2])
}
pub fn is_dark() -> bool {
    DARK.load(Relaxed)
}

// ───────────── kontrast ─────────────

fn lin(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.03928 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}
pub fn luminance(c: Color32) -> f32 {
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}
/// Poměr kontrastu podle WCAG (1 = žádný, 21 = černá na bílé).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}
fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}
/// Upraví barvu textu tak, aby na pozadí `bg` měla aspoň kontrast `min` (zachová odstín, jen ztmaví/zesvětlí).
pub fn ensure_contrast(fg: Color32, bg: Color32, min: f32) -> Color32 {
    if contrast(fg, bg) >= min {
        return fg;
    }
    let target = if luminance(bg) > 0.4 { Color32::BLACK } else { Color32::WHITE };
    for i in 1..=24 {
        let c = lerp_c(fg, target, i as f32 / 24.0);
        if contrast(c, bg) >= min {
            return c;
        }
    }
    target
}
/// Barva textu na zvýrazněném podkladu (bílá nebo tmavá – podle toho, co má lepší kontrast).
pub fn accent_text() -> Color32 {
    let a = accent();
    let dark = Color32::from_rgb(24, 16, 32);
    if contrast(Color32::WHITE, a) >= contrast(dark, a) {
        Color32::WHITE
    } else {
        dark
    }
}

// ───────────── palety ─────────────

pub struct Pal {
    pub card: Color32,
    pub card_hi: Color32,
    pub border: Color32,
    pub muted: Color32,
    pub good: Color32,
    pub bad: Color32,
    pub warn: Color32,
    pub chip_off: Color32,
    pub footer: Color32,
    pub text: Color32,
    pub field: Color32,
    pub window: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const DARK_PAL: Pal = Pal {
    card: rgb(58, 58, 63),
    card_hi: rgb(72, 72, 78),
    border: Color32::from_rgba_premultiplied(14, 14, 14, 14),
    muted: rgb(182, 174, 188),
    good: rgb(120, 225, 160),
    bad: rgb(255, 128, 138),
    warn: rgb(255, 200, 110),
    chip_off: rgb(74, 74, 82),
    footer: rgb(33, 32, 37),
    text: rgb(244, 240, 248),
    field: rgb(38, 36, 44),
    window: rgb(48, 46, 54),
};

const LIGHT_PAL: Pal = Pal {
    card: rgb(255, 255, 255),
    card_hi: rgb(241, 236, 249),
    border: rgb(216, 208, 230),
    muted: rgb(92, 82, 108),
    good: rgb(16, 118, 58),
    bad: rgb(186, 28, 48),
    warn: rgb(150, 86, 0),
    chip_off: rgb(234, 229, 243),
    footer: rgb(236, 231, 245),
    text: rgb(30, 22, 42),
    field: rgb(255, 255, 255),
    window: rgb(252, 250, 255),
};

pub fn pal() -> &'static Pal {
    if is_dark() {
        &DARK_PAL
    } else {
        &LIGHT_PAL
    }
}
pub fn card_c() -> Color32 {
    pal().card
}
pub fn card_hi() -> Color32 {
    pal().card_hi
}
pub fn muted_c() -> Color32 {
    pal().muted
}
pub fn good() -> Color32 {
    pal().good
}
pub fn bad() -> Color32 {
    pal().bad
}
pub fn warn() -> Color32 {
    pal().warn
}
pub fn chip_off() -> Color32 {
    pal().chip_off
}
pub fn footer_bg() -> Color32 {
    pal().footer
}
pub fn border() -> Color32 {
    pal().border
}
/// Zvýraznění použitelné jako barva TEXTU / ikon na kartě (v obou režimech čitelné).
pub fn accent_fg() -> Color32 {
    ensure_contrast(accent(), card_c(), 4.5)
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> Color32 {
    let f = |i: usize| (a[i] as f32 * t + b[i] as f32 * (1.0 - t)).round() as u8;
    Color32::from_rgb(f(0), f(1), f(2))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let load = |names: &[&str]| -> Option<Vec<u8>> {
        for n in names {
            for dir in ["C:\\Windows\\Fonts", "/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/truetype/ubuntu"] {
                if let Ok(b) = std::fs::read(std::path::Path::new(dir).join(n)) {
                    return Some(b);
                }
            }
        }
        None
    };
    if let Some(b) = load(&["segoeui.ttf"]) {
        fonts.font_data.insert("ui".into(), Arc::new(FontData::from_owned(b)));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "ui".into());
    }
    let has_bold = if let Some(b) = load(&["segoeuib.ttf", "DejaVuSans-Bold.ttf"]) {
        fonts.font_data.insert("ui-bold".into(), Arc::new(FontData::from_owned(b)));
        true
    } else {
        false
    };
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    let mut bold_list: Vec<String> = Vec::new();
    if has_bold {
        bold_list.push("ui-bold".into());
    }
    bold_list.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts.families.insert(FontFamily::Name("bold".into()), bold_list);
    ctx.set_fonts(fonts);
}

fn visuals(dark: bool) -> egui::Visuals {
    let p = if dark { &DARK_PAL } else { &LIGHT_PAL };
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(12);
    let a = accent();
    let a_fg = ensure_contrast(a, p.card, 4.5);
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = p.window;
    v.extreme_bg_color = p.field;
    v.faint_bg_color = if dark { rgb(66, 64, 74) } else { rgb(245, 241, 251) };
    v.override_text_color = None;
    v.window_stroke = Stroke::new(1.0_f32, if dark { Color32::from_white_alpha(26) } else { p.border });
    v.window_corner_radius = CornerRadius::same(16);

    let (n_bg, i_bg, h_bg, act_bg, o_bg) = if dark {
        (p.card, rgb(80, 80, 88), rgb(100, 100, 110), mix(accent_rgb(), [58, 58, 63], 0.45), rgb(90, 90, 100))
    } else {
        (p.card, rgb(236, 231, 244), rgb(224, 217, 238), mix(accent_rgb(), [255, 255, 255], 0.30), rgb(228, 221, 240))
    };
    let (n_fg, i_fg, h_fg) = if dark { (rgb(236, 232, 242), rgb(244, 240, 248), Color32::WHITE) } else { (p.text, p.text, rgb(16, 8, 26)) };
    let line = if dark { Color32::from_white_alpha(22) } else { p.border };

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = n_bg;
    w.noninteractive.weak_bg_fill = n_bg;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, n_fg);
    w.inactive.bg_fill = i_bg;
    w.inactive.weak_bg_fill = i_bg;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, if dark { Color32::TRANSPARENT } else { p.border });
    w.inactive.fg_stroke = Stroke::new(1.0_f32, i_fg);
    w.hovered.bg_fill = h_bg;
    w.hovered.weak_bg_fill = h_bg;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, a.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5_f32, h_fg);
    w.active.bg_fill = act_bg;
    w.active.weak_bg_fill = act_bg;
    w.active.bg_stroke = Stroke::new(1.0_f32, a);
    w.active.fg_stroke = Stroke::new(2.0_f32, if contrast(h_fg, act_bg) > 3.0 { h_fg } else { accent_text() });
    w.open.bg_fill = o_bg;
    w.open.weak_bg_fill = o_bg;
    w.open.bg_stroke = Stroke::new(1.0_f32, a.gamma_multiply(0.7));
    w.open.fg_stroke = Stroke::new(1.0_f32, i_fg);
    for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        x.corner_radius = r;
    }

    v.selection.bg_fill = a.gamma_multiply(if dark { 0.55 } else { 0.35 });
    v.selection.stroke = Stroke::new(1.0_f32, a_fg);
    v.hyperlink_color = a_fg;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.bad;
    v
}

/// Nastaví obě sady vzhledu (tmavou i světlou) a přepne na zvolenou. Dřív se měnil jen aktuální
/// styl a po pár přepnutích zůstala druhá sada ve výchozích barvách egui -> špatný kontrast.
pub fn apply_visuals(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, Relaxed);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(false));
    ctx.set_theme(if dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(14.0, 8.0);
        s.spacing.interact_size.y = 34.0;
        s.spacing.slider_width = 220.0;
        s.interaction.tooltip_delay = 0.2;
    });
}

/// Přechod na pozadí + jemné diagonální čáry jako na vert.sh.
pub fn paint_background(ctx: &egui::Context, dark: bool) {
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let a = accent_rgb();
    let (top, bot) = if dark { (mix(a, [22, 20, 28], 0.22), mix(a, [16, 15, 21], 0.06)) } else { (mix(a, [255, 255, 255], 0.16), mix(a, [246, 244, 250], 0.06)) };
    let mut m = Mesh::default();
    let uv = egui::epaint::WHITE_UV;
    m.vertices.push(Vertex { pos: rect.left_top(), uv, color: top });
    m.vertices.push(Vertex { pos: rect.right_top(), uv, color: top });
    m.vertices.push(Vertex { pos: rect.right_bottom(), uv, color: bot });
    m.vertices.push(Vertex { pos: rect.left_bottom(), uv, color: bot });
    m.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(m);
    let line = Stroke::new(1.0_f32, if dark { Color32::from_white_alpha(10) } else { Color32::from_white_alpha(70) });
    for (x0, x1) in [(0.10, 0.34), (0.30, 0.52), (0.64, 0.88)] {
        painter.line_segment([pos2(rect.left() + rect.width() * x0, rect.top()), pos2(rect.left() + rect.width() * x1, rect.bottom())], line);
    }
}

pub fn card_frame(fill: Color32) -> egui::Frame {
    let mut f = egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(18)).inner_margin(Margin::same(16));
    if !is_dark() {
        f = f.stroke(Stroke::new(1.0_f32, border())).shadow(egui::epaint::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(14) });
    }
    f
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(card_c()).show(ui, add).inner
}

pub fn title(ui: &mut Ui, text: &str, size: f32) {
    ui.label(RichText::new(text).font(bold(size)));
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(muted_c())
}

pub fn heading_small(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(bold(15.0)));
}

/// Hlavní akce (barevné kulaté tlačítko).
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).font(bold(14.5)).color(accent_text()))
            .fill(accent())
            .corner_radius(CornerRadius::same(14))
            .min_size(vec2(0.0, 36.0)),
    )
}

pub fn ghost_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).font(bold(14.0))).corner_radius(CornerRadius::same(14)).min_size(vec2(0.0, 34.0)))
}

/// Přepínací "pill" (aktivní = barevný).
pub fn chip(ui: &mut Ui, selected: bool, text: &str) -> egui::Response {
    let (fill, col) = if selected { (accent(), accent_text()) } else { (chip_off(), ui.visuals().widgets.inactive.fg_stroke.color) };
    ui.add(
        egui::Button::new(RichText::new(text).font(bold(13.5)).color(col))
            .fill(fill)
            .corner_radius(CornerRadius::same(20))
            .min_size(vec2(0.0, 30.0)),
    )
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (v, label) in options {
            if chip(ui, *value == *v, label).clicked() {
                *value = *v;
            }
        }
    });
}

pub fn badge(ui: &mut Ui, icon: &str, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, FontId::proportional(size * 0.52), Color32::from_rgb(30, 24, 40));
}

/// Malý štítek; barva textu se sama upraví, aby byl čitelný na světlém i tmavém podkladu.
pub fn pill_tag(ui: &mut Ui, text: &str, color: Color32) {
    let tint = color.gamma_multiply(0.18);
    // skutečné pozadí = průhledný nádech přes kartu
    let base = card_c();
    let a = 0.18_f32;
    let bg = Color32::from_rgb(
        (color.r() as f32 * a + base.r() as f32 * (1.0 - a)) as u8,
        (color.g() as f32 * a + base.g() as f32 * (1.0 - a)) as u8,
        (color.b() as f32 * a + base.b() as f32 * (1.0 - a)) as u8,
    );
    let fg = ensure_contrast(color, bg, 4.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), fg);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(10), tint);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
}

/// Pruh průběhu: procenta, uplynulý čas a ETA. `frac = None` => neznámý průběh (animace).
pub fn progress_bar(ui: &mut Ui, frac: Option<f32>, elapsed: f64, note: &str, queued: bool) {
    let h = 14.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let track = if is_dark() { Color32::from_black_alpha(90) } else { Color32::from_black_alpha(28) };
    ui.painter().rect_filled(rect, CornerRadius::same(7), track);
    match frac {
        Some(f) if !queued => {
            let mut r = rect;
            r.set_width((rect.width() * f.clamp(0.0, 1.0)).max(h));
            ui.painter().rect_filled(r, CornerRadius::same(7), accent());
        }
        _ if !queued => {
            // animace jen u běžící úlohy bez známého průběhu (a jen ~25 snímků/s, ne na plný výkon)
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.28;
            let x = rect.left() + ((t * 0.9).fract()) * (rect.width() + seg) - seg;
            let r = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + seg).min(rect.right()), rect.bottom()));
            if r.width() > 0.0 {
                ui.painter().rect_filled(r, CornerRadius::same(7), accent().gamma_multiply(0.8));
            }
        }
        _ => {}
    }
    ui.painter().rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0_f32, border()), StrokeKind::Inside);
    let text = if queued {
        tr!("Čeká ve frontě…", "Waiting in the queue…").to_string()
    } else {
        match frac {
            Some(f) => {
                let eta = if f > 0.02 && elapsed > 1.0 { Some(elapsed / f as f64 * (1.0 - f as f64)) } else { None };
                trf!(
                    "{:.0} %   ·   uplynulo {}   ·   zbývá {}{}",
                    "{:.0} %   ·   elapsed {}   ·   remaining {}{}",
                    f * 100.0,
                    crate::util::fmt_dur(elapsed),
                    eta.map(|e| format!("~{}", crate::util::fmt_dur(e))).unwrap_or_else(|| tr!("počítám…", "estimating…").into()),
                    if note.is_empty() { String::new() } else { format!("   ·   {note}") }
                )
            }
            None => trf!(
                "pracuji…   ·   uplynulo {}{}",
                "working…   ·   elapsed {}{}",
                crate::util::fmt_dur(elapsed),
                if note.is_empty() { String::new() } else { format!("   ·   {note}") }
            ),
        }
    };
    ui.label(muted(text).size(12.5));
}

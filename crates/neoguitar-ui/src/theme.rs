//! Tema visual de NeoGuitar: paleta oscura sobria, tipografía y pequeños widgets propios.

use egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response, RichText, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, vec2,
};
use std::sync::Arc;

pub const SCORE_BG: Color32 = Color32::from_rgb(17, 19, 24);
pub const PANEL: Color32 = Color32::from_rgb(24, 27, 34);
pub const CARD: Color32 = Color32::from_rgb(32, 36, 45);
pub const CARD_HOVER: Color32 = Color32::from_rgb(40, 45, 56);
pub const BORDER: Color32 = Color32::from_rgb(46, 51, 63);
pub const TEXT: Color32 = Color32::from_rgb(230, 233, 239);
pub const MUTED: Color32 = Color32::from_rgb(140, 148, 163);
pub const ACCENT: Color32 = Color32::from_rgb(96, 165, 250);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(34, 58, 96);
pub const DANGER: Color32 = Color32::from_rgb(244, 114, 120);
/// Color de las líneas de la partitura (pentagrama y cuerdas).
pub const SCORE_LINE: Color32 = Color32::from_rgb(100, 109, 126);

fn radius(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

/// Aplica fuentes, colores y espaciado a todo el programa.
pub fn apply(ctx: &egui::Context) {
    // Segoe UI (Windows) como fuente principal si existe; las de egui quedan de respaldo (emojis, símbolos).
    let mut fonts = FontDefinitions::default();
    for (name, file) in [("segoe", "C:/Windows/Fonts/segoeui.ttf"), ("segoe-bold", "C:/Windows/Fonts/segoeuib.ttf")] {
        if let Ok(bytes) = std::fs::read(file) {
            fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
        }
    }
    if fonts.font_data.contains_key("segoe") {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".to_owned());
    }
    // La música (Bravura) se registra aparte en `staff::install_font`, que lee estas definiciones.
    crate::staff::add_music_font(&mut fonts);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::proportional(21.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        let sp = &mut style.spacing;
        sp.item_spacing = vec2(8.0, 6.0);
        sp.button_padding = vec2(12.0, 5.0);
        sp.interact_size = vec2(34.0, 30.0);
        sp.menu_margin = Margin::same(8);
        sp.slider_rail_height = 4.0;
        sp.icon_width = 14.0;
        sp.icon_spacing = 6.0;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = PANEL;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(14, 16, 20);
        v.faint_bg_color = CARD;
        v.text_edit_bg_color = None;
        v.weak_text_color = Some(MUTED);
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = radius(12);
        v.menu_corner_radius = radius(10);
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.corner_radius = radius(8);
        w.inactive.weak_bg_fill = CARD;
        w.inactive.bg_fill = Color32::from_rgb(54, 60, 74);
        w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = radius(8);
        w.hovered.weak_bg_fill = CARD_HOVER;
        w.hovered.bg_fill = Color32::from_rgb(70, 78, 96);
        w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(70, 77, 94));
        w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.hovered.corner_radius = radius(8);
        w.hovered.expansion = 0.0;
        w.active.weak_bg_fill = ACCENT_DIM;
        w.active.bg_fill = ACCENT;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.active.corner_radius = radius(8);
        w.active.expansion = 0.0;
        w.open.weak_bg_fill = CARD_HOVER;
        w.open.bg_fill = CARD_HOVER;
        w.open.bg_stroke = Stroke::new(1.0, BORDER);
        w.open.corner_radius = radius(8);
    });
}

/// Contenedor tipo tarjeta.
pub fn card() -> egui::Frame {
    egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER)).corner_radius(radius(10)).inner_margin(Margin::same(10))
}

/// Marco de un panel (barra superior, lateral o inferior).
pub fn panel_frame(margin: Margin) -> egui::Frame {
    egui::Frame::new().fill(PANEL).inner_margin(margin)
}

/// Encabezado de sección en minúsculas pequeñas.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).strong().color(MUTED));
}

/// Etiqueta redondeada (formato, tempo…).
pub fn pill(ui: &mut Ui, text: &str) {
    egui::Frame::new().fill(Color32::from_rgb(40, 45, 56)).corner_radius(radius(20)).inner_margin(Margin::symmetric(9, 3)).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(Color32::from_rgb(190, 197, 210))).extend());
    });
}

/// Botón de alternancia con icono: resaltado cuando está activo.
pub fn chip(ui: &mut Ui, icon: Icon, on: bool, text: &str) -> Response {
    icon_label(ui, Some(icon), text, on, true, false)
}

pub fn chip_enabled(ui: &mut Ui, icon: Icon, on: bool, text: &str, enabled: bool) -> Response {
    icon_label(ui, Some(icon), text, on, enabled, false)
}

/// Botón principal (relleno de color).
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    icon_label(ui, Some(Icon::Folder), text, false, true, true)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Speaker,
    SpeakerOff,
    Eye,
    Plus,
    Minus,
    Folder,
    Clock,
    Staff,
    Layers,
    Sidebar,
    Metronome,
    Loop,
    Timer,
    Trend,
    Follow,
    Dots,
    Headphones,
    Fretboard,
    Split,
    Fullscreen,
    Lyrics,
    Playlist,
    Transpose,
    Sliders,
}

/// Botón con un icono dibujado. `style`: `Round` es el botón grande de reproducir.
#[derive(Clone, Copy, PartialEq)]
pub enum IconStyle {
    Round,
    Flat,
    /// Plano, pero resaltado (activo).
    FlatOn,
    /// Plano, en rojo (silenciado).
    Danger,
}

pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, style: IconStyle, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = resp.hovered() && enabled;
    let (bg, fg) = match style {
        IconStyle::Round => (if hovered { Color32::from_rgb(130, 185, 255) } else { ACCENT }, Color32::from_rgb(10, 20, 40)),
        IconStyle::Flat => (if hovered { CARD_HOVER } else { Color32::TRANSPARENT }, if hovered { Color32::WHITE } else { Color32::from_rgb(200, 206, 218) }),
        IconStyle::FlatOn => (ACCENT_DIM, Color32::from_rgb(190, 220, 255)),
        IconStyle::Danger => (Color32::from_rgb(68, 34, 40), DANGER),
    };
    let (bg, fg) = if enabled { (bg, fg) } else { (Color32::TRANSPARENT, Color32::from_rgb(80, 86, 98)) };
    let p = ui.painter();
    if style == IconStyle::Round {
        p.circle_filled(rect.center(), size / 2.0, bg);
    } else {
        p.rect_filled(rect, radius(8), bg);
    }
    let u = size * if style == IconStyle::Round { 0.2 } else { 0.24 };
    draw_icon(p, icon, rect.center(), u, fg);
    resp
}

/// Botón con icono de línea y texto. `on`: activo; `primary`: relleno de acento.
pub fn icon_label(ui: &mut Ui, icon: Option<Icon>, text: &str, on: bool, enabled: bool, primary: bool) -> Response {
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
    let gap = if icon.is_some() && !text.is_empty() { 7.0 } else { 0.0 };
    let size = vec2(11.0 * 2.0 + icon_w + gap + galley.size().x, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = resp.hovered() && enabled;
    let (fill, stroke, fg) = if !enabled {
        (CARD, Stroke::new(1.0, BORDER), Color32::from_rgb(90, 96, 108))
    } else if primary {
        (if hovered { Color32::from_rgb(130, 185, 255) } else { ACCENT }, Stroke::NONE, Color32::from_rgb(10, 20, 40))
    } else if on {
        (ACCENT_DIM, Stroke::new(1.0, ACCENT), Color32::from_rgb(200, 224, 255))
    } else {
        (if hovered { CARD_HOVER } else { CARD }, Stroke::new(1.0, if hovered { Color32::from_rgb(70, 77, 94) } else { BORDER }), TEXT)
    };
    let p = ui.painter();
    p.rect(rect, radius(8), fill, stroke, StrokeKind::Inside);
    let mut x = rect.left() + 11.0;
    if let Some(ic) = icon {
        draw_icon(p, ic, egui::pos2(x + icon_w / 2.0, rect.center().y), 4.0, fg);
        x += icon_w + gap;
    }
    if !text.is_empty() {
        p.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    }
    resp
}

/// Dibuja un icono de línea centrado en `c`; `u` es la unidad (el icono ocupa unas ±2u).
pub fn draw_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, u: f32, fg: Color32) {
    let st = Stroke::new((u * 0.38).clamp(1.2, 2.0), fg);
    let pt = |x: f32, y: f32| c + vec2(x * u, y * u);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    match icon {
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![pt(-0.7, -1.1), pt(-0.7, 1.1), pt(1.15, 0.0)], fg, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-0.65, 0.65] {
                p.rect_filled(egui::Rect::from_center_size(pt(dx, 0.0), vec2(u * 0.7, u * 2.2)), radius(1), fg);
            }
        }
        Icon::Stop => {
            p.rect_filled(egui::Rect::from_center_size(c, Vec2::splat(u * 1.9)), radius(2), fg);
        }
        Icon::Speaker | Icon::SpeakerOff => {
            p.add(Shape::convex_polygon(vec![pt(-1.3, -0.55), pt(-0.5, -0.55), pt(0.4, -1.2), pt(0.4, 1.2), pt(-0.5, 0.55), pt(-1.3, 0.55)], fg, Stroke::NONE));
            if icon == Icon::Speaker {
                for k in [1.0f32, 1.7] {
                    let pts: Vec<_> = (-4..=4)
                        .map(|i| {
                            let a = i as f32 * 0.17;
                            pt(0.55 + a.cos() * k * 0.7, a.sin() * k * 0.85)
                        })
                        .collect();
                    p.add(Shape::line(pts, st));
                }
            } else {
                line(&[(0.9, -0.8), (1.9, 0.8)]);
                line(&[(0.9, 0.8), (1.9, -0.8)]);
            }
        }
        Icon::Eye => {
            p.add(Shape::ellipse_stroke(c, vec2(1.9 * u, 1.1 * u), st));
            p.circle_filled(c, 0.6 * u, fg);
        }
        Icon::Plus => {
            line(&[(-1.0, 0.0), (1.0, 0.0)]);
            line(&[(0.0, -1.0), (0.0, 1.0)]);
        }
        Icon::Minus => line(&[(-1.0, 0.0), (1.0, 0.0)]),
        Icon::Folder => {
            line(&[(-1.9, -1.2), (-0.7, -1.2), (-0.2, -0.6), (1.9, -0.6), (1.9, 1.5), (-1.9, 1.5), (-1.9, -1.2)]);
        }
        Icon::Clock => {
            p.circle_stroke(c, 1.9 * u, st);
            line(&[(0.0, -1.0), (0.0, 0.0), (0.8, 0.5)]);
        }
        Icon::Staff => {
            for k in -2..=2 {
                line(&[(-2.0, k as f32 * 0.7), (2.0, k as f32 * 0.7)]);
            }
        }
        Icon::Layers => {
            for (y, w) in [(-1.4f32, 2.0f32), (0.0, 2.0), (1.4, 1.2)] {
                p.rect_filled(egui::Rect::from_min_max(pt(-2.0, y - 0.3), pt(-2.0 + w * 2.0 * 0.9, y + 0.3)), radius(1), fg);
            }
        }
        Icon::Sidebar => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-2.0, -1.6), pt(2.0, 1.6)), radius(2), st, StrokeKind::Inside);
            p.rect_filled(egui::Rect::from_min_max(pt(-2.0, -1.6), pt(-0.7, 1.6)), radius(1), fg);
        }
        Icon::Split => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-2.0, -1.6), pt(2.0, 1.6)), radius(2), st, StrokeKind::Inside);
            line(&[(0.0, -1.6), (0.0, 1.6)]);
        }
        Icon::Lyrics => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-2.0, -1.8), pt(2.0, 0.8)), radius(2), st, StrokeKind::Inside);
            line(&[(-0.9, 0.8), (-1.3, 1.9), (0.2, 0.8)]);
            line(&[(-1.2, -0.8), (1.2, -0.8)]);
            line(&[(-1.2, 0.0), (0.4, 0.0)]);
        }
        Icon::Playlist => {
            line(&[(-2.0, -1.4), (0.3, -1.4)]);
            line(&[(-2.0, -0.1), (0.3, -0.1)]);
            line(&[(-2.0, 1.2), (-0.4, 1.2)]);
            line(&[(0.7, 0.2), (2.0, 1.0), (0.7, 1.8), (0.7, 0.2)]);
        }
        Icon::Fullscreen => {
            line(&[(-2.0, -0.8), (-2.0, -2.0), (-0.8, -2.0)]);
            line(&[(0.8, -2.0), (2.0, -2.0), (2.0, -0.8)]);
            line(&[(2.0, 0.8), (2.0, 2.0), (0.8, 2.0)]);
            line(&[(-0.8, 2.0), (-2.0, 2.0), (-2.0, 0.8)]);
        }
        Icon::Metronome => {
            line(&[(-1.2, 1.8), (-0.6, -1.8), (0.6, -1.8), (1.2, 1.8), (-1.2, 1.8)]);
            line(&[(0.0, 1.0), (0.9, -1.3)]);
        }
        Icon::Loop => {
            line(&[(-1.8, -0.9), (1.5, -0.9)]);
            line(&[(0.8, -1.7), (1.8, -0.9), (0.8, -0.1)]);
            line(&[(1.8, 0.9), (-1.5, 0.9)]);
            line(&[(-0.8, 0.1), (-1.8, 0.9), (-0.8, 1.7)]);
        }
        Icon::Timer => {
            p.circle_stroke(pt(0.0, 0.3), 1.6 * u, st);
            line(&[(-0.5, -1.8), (0.5, -1.8)]);
            line(&[(0.0, 0.3), (0.0, -0.6)]);
            line(&[(0.0, 0.3), (0.6, 0.7)]);
        }
        Icon::Trend => {
            line(&[(-2.0, 1.4), (-0.4, -0.2), (0.6, 0.8), (1.9, -1.0)]);
            line(&[(0.7, -1.2), (1.9, -1.2), (1.9, 0.0)]);
        }
        Icon::Follow => {
            line(&[(0.0, -1.8), (0.0, 0.9)]);
            line(&[(-1.0, -0.1), (0.0, 0.9), (1.0, -0.1)]);
            line(&[(-1.7, 1.8), (1.7, 1.8)]);
        }
        Icon::Dots => {
            for x in [-1.3f32, 0.0, 1.3] {
                p.circle_filled(pt(x, 0.0), 0.35 * u + 0.4, fg);
            }
        }
        Icon::Sliders => {
            for (y, x) in [(-1.3f32, -0.8f32), (0.0, 0.9), (1.3, -0.3)] {
                line(&[(-2.0, y), (2.0, y)]);
                p.circle_filled(pt(x, y), 0.55 * u + 0.6, fg);
            }
        }
        Icon::Transpose => {
            line(&[(-0.9, 1.7), (-0.9, -1.7)]);
            line(&[(-1.8, -0.8), (-0.9, -1.7), (0.0, -0.8)]);
            line(&[(0.9, -1.7), (0.9, 1.7)]);
            line(&[(0.0, 0.8), (0.9, 1.7), (1.8, 0.8)]);
        }
        Icon::Fretboard => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-2.0, -1.5), pt(2.0, 1.5)), radius(2), st, StrokeKind::Inside);
            for x in [-0.7f32, 0.6] {
                line(&[(x, -1.5), (x, 1.5)]);
            }
            line(&[(-2.0, 0.0), (2.0, 0.0)]);
            p.circle_filled(pt(0.0, -0.75), 0.45 * u + 0.5, fg);
        }
        Icon::Headphones => {
            let pts: Vec<_> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
                    pt(a.cos() * 1.7, 0.3 + a.sin() * 1.7)
                })
                .collect();
            p.add(Shape::line(pts, st));
            for x in [-1.7f32, 1.7] {
                p.rect_filled(egui::Rect::from_min_max(pt(x - 0.45, 0.3), pt(x + 0.45, 1.8)), radius(2), fg);
            }
        }
    }
}

/// Barra de progreso de la canción, con la zona de bucle resaltada. Devuelve la fracción (0..1) si el usuario la mueve.
pub fn seek_bar(ui: &mut Ui, frac: f32, loop_frac: Option<(f32, f32)>, enabled: bool) -> Option<f32> {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let p = ui.painter();
    let rail = egui::Rect::from_center_size(rect.center(), vec2(rect.width() - 10.0, 5.0));
    p.rect_filled(rail, radius(3), Color32::from_rgb(52, 58, 72));
    if let Some((a, b)) = loop_frac {
        let r = egui::Rect::from_min_max(egui::pos2(rail.left() + rail.width() * a, rect.center().y - 8.0), egui::pos2(rail.left() + rail.width() * b, rect.center().y + 8.0));
        p.rect_filled(r, radius(3), Color32::from_rgba_unmultiplied(96, 165, 250, 46));
        p.rect_stroke(r, radius(3), Stroke::new(1.0, Color32::from_rgba_unmultiplied(96, 165, 250, 120)), StrokeKind::Inside);
    }
    let x = rail.left() + rail.width() * frac.clamp(0.0, 1.0);
    p.rect_filled(egui::Rect::from_min_max(rail.left_top(), egui::pos2(x, rail.bottom())), radius(3), ACCENT);
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(egui::pos2(x, rect.center().y), if hot { 7.0 } else { 5.5 }, if enabled { Color32::WHITE } else { MUTED });
    if enabled && (resp.dragged() || resp.clicked()) {
        if let Some(pos) = resp.interact_pointer_pos() {
            return Some(((pos.x - rail.left()) / rail.width()).clamp(0.0, 1.0));
        }
    }
    None
}

/// Color de pista legible sobre fondo oscuro: si es muy oscuro se aclara.
pub fn track_color([r, g, b]: [u8; 3]) -> Color32 {
    let lum = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    let c = Color32::from_rgb(r, g, b);
    if lum >= 110.0 {
        return c;
    }
    let t = ((110.0 - lum) / 110.0).clamp(0.0, 1.0) * 0.85 + 0.15;
    let mix = |x: u8| (x as f32 + (200.0 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(r), mix(g), mix(b))
}

/// Contents of the count-in menu, shared by the apps: on/off, seconds to get ready and measures of clicks.
/// Returns `true` if anything changed.
pub fn count_in_controls(ui: &mut Ui, on: &mut bool, measures: &mut u32, delay: &mut f32) -> bool {
    let mut changed = false;
    ui.set_min_width(250.0);
    changed |= ui.checkbox(on, "Count in before playing").changed();
    ui.add_space(4.0);
    ui.label(RichText::new("Time to get ready").size(12.0).color(MUTED));
    ui.spacing_mut().slider_width = 190.0;
    changed |= ui.add(egui::Slider::new(delay, 0.0..=10.0).step_by(0.5).suffix(" s").custom_formatter(|v, _| if v < 0.05 { "none".to_string() } else { format!("{v:.1} s") })).changed();
    ui.add_space(2.0);
    ui.label(RichText::new("Clicks").size(12.0).color(MUTED));
    ui.horizontal(|ui| {
        for (n, label) in [(1u32, "1 measure"), (2, "2 measures")] {
            if ui.selectable_label(*measures == n, label).clicked() {
                *measures = n;
                changed = true;
            }
        }
    });
    changed
}

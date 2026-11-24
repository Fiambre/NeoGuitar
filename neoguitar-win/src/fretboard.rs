//! Mástil virtual: dibuja el brazo de la guitarra con las notas que suenan en el instante actual.

use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, vec2};
use neoguitar_core::model::{midi_note_name, Track};

const FRETS: usize = 24;
/// Marcas del mástil: trastes con un punto, y el 12 y el 24 con dos.
const DOTS: [usize; 8] = [3, 5, 7, 9, 15, 17, 19, 21];
const DOUBLE_DOTS: [usize; 2] = [12, 24];

pub const HEIGHT: f32 = 132.0;

/// Posición horizontal (0..1) del traste `n`: separación decreciente como en un mástil real.
fn fret_pos(n: usize) -> f32 {
    let f = |x: f32| 1.0 - 2f32.powf(-x / 12.0);
    f(n as f32) / f(FRETS as f32)
}

/// Dibuja el mástil de `track` marcando las notas `(cuerda 1-based, traste)` que suenan ahora.
pub fn show(ui: &mut Ui, track: &Track, active: &[(u8, u8)]) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::hover());
    let p = ui.painter_at(rect);

    if track.is_percussion || track.strings.is_empty() {
        p.text(rect.center(), Align2::CENTER_CENTER, "The percussion track has no fretboard", FontId::proportional(13.0), theme::MUTED);
        return;
    }
    let strings = track.strings.len().clamp(1, 8);
    let left = rect.left() + 44.0;
    let right = rect.right() - 14.0;
    let top = rect.top() + 14.0;
    let bottom = rect.bottom() - 26.0;
    let gap = if strings > 1 { (bottom - top) / (strings as f32 - 1.0) } else { 0.0 };
    let x_at = |n: usize| left + (right - left) * fret_pos(n);
    let y_at = |s: usize| if strings > 1 { top + gap * s as f32 } else { (top + bottom) / 2.0 };

    // Brazo.
    let neck = Rect::from_min_max(Pos2::new(left, top - 8.0), Pos2::new(right, bottom + 8.0));
    p.rect_filled(neck, 6.0, Color32::from_rgb(30, 34, 44));
    p.rect_stroke(neck, 6.0, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);

    // Marcas de posición (puntos) y números de traste.
    let mid = (top + bottom) / 2.0;
    for &n in &DOTS {
        p.circle_filled(Pos2::new((x_at(n - 1) + x_at(n)) / 2.0, mid), 4.0, Color32::from_rgb(52, 58, 74));
    }
    for &n in &DOUBLE_DOTS {
        let x = (x_at(n - 1) + x_at(n)) / 2.0;
        p.circle_filled(Pos2::new(x, mid - gap * 0.9), 4.0, Color32::from_rgb(52, 58, 74));
        p.circle_filled(Pos2::new(x, mid + gap * 0.9), 4.0, Color32::from_rgb(52, 58, 74));
    }
    for n in 1..=FRETS {
        let x = x_at(n);
        p.line_segment([Pos2::new(x, top - 8.0), Pos2::new(x, bottom + 8.0)], Stroke::new(1.0, Color32::from_rgb(78, 86, 104)));
        if [1, 3, 5, 7, 9, 12, 15, 17, 19, 21, 24].contains(&n) {
            p.text(Pos2::new((x_at(n - 1) + x) / 2.0, rect.bottom() - 4.0), Align2::CENTER_BOTTOM, n.to_string(), FontId::proportional(10.5), theme::MUTED);
        }
    }
    // Cejuela.
    p.line_segment([Pos2::new(left, top - 8.0), Pos2::new(left, bottom + 8.0)], Stroke::new(3.5, Color32::from_rgb(200, 206, 218)));

    // Cuerdas (las graves, más gruesas) y nombre de la cuerda al aire.
    for s in 0..strings {
        let y = y_at(s);
        let thick = 0.8 + 1.6 * s as f32 / strings as f32;
        p.line_segment([Pos2::new(left, y), Pos2::new(right, y)], Stroke::new(thick, Color32::from_rgb(150, 158, 174)));
        if let Some(&midi) = track.strings.get(s) {
            p.text(Pos2::new(rect.left() + 8.0, y), Align2::LEFT_CENTER, midi_note_name(midi), FontId::proportional(11.5), theme::MUTED);
        }
    }

    // Notas que suenan.
    for &(string, fret) in active {
        let s = (string as usize).saturating_sub(1);
        if s >= strings {
            continue;
        }
        let y = y_at(s);
        let (x, r) = if fret == 0 {
            (left - 11.0, 8.0)
        } else {
            let f = (fret as usize).min(FRETS);
            ((x_at(f - 1) + x_at(f)) / 2.0, 9.0)
        };
        p.circle_filled(Pos2::new(x, y), r + 3.0, Color32::from_rgba_unmultiplied(96, 165, 250, 60));
        p.circle_filled(Pos2::new(x, y), r, theme::ACCENT);
        p.text(Pos2::new(x, y), Align2::CENTER_CENTER, fret.to_string(), FontId::proportional(11.0), Color32::from_rgb(10, 20, 40));
    }
}

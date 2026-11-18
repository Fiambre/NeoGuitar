//! Pentagrama de notación estándar (con la fuente musical Bravura / SMuFL).
//!
//! La guitarra se escribe una octava por encima de lo que suena. Se dibuja una voz por
//! plica: con dos voces, la primera lleva plica hacia arriba y la segunda hacia abajo.

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, vec2};
use neoguitar::model::*;
use std::collections::HashMap;
use std::sync::Arc;

const MUSIC_FAMILY: &str = "music";
const BRAVURA: &[u8] = include_bytes!("../assets/fonts/Bravura.otf");

/// Registra la fuente musical en egui.
/// Registra Bravura como la familia "music".
pub fn add_music_font(fonts: &mut egui::FontDefinitions) {
    fonts.font_data.insert("bravura".to_owned(), Arc::new(egui::FontData::from_static(BRAVURA)));
    fonts.families.insert(FontFamily::Name(MUSIC_FAMILY.into()), vec!["bravura".to_owned()]);
}

pub struct StaffColors {
    pub fg: Color32,
    pub line: Color32,
    pub voice2: Color32,
}

/// Altura total del bloque del pentagrama (en unidades de zoom 1).
pub const STAFF_BLOCK: f32 = 84.0;
/// Distancia entre líneas del pentagrama.
const SPACE: f32 = 7.0;
/// Margen sobre la primera línea dentro del bloque (para líneas adicionales).
pub const STAFF_TOP_MARGIN: f32 = 32.0;

// Códigos SMuFL.
const G_CLEF: char = '\u{E050}';
const F_CLEF: char = '\u{E062}';
const SHARP: char = '\u{E262}';
const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const NOTE_WHOLE: char = '\u{E0A2}';
const NOTE_HALF: char = '\u{E0A3}';
const NOTE_BLACK: char = '\u{E0A4}';
const NOTE_X: char = '\u{E0A9}';
const AUG_DOT: char = '\u{E1E7}';

fn rest_glyph(value: u32) -> char {
    match value {
        1 => '\u{E4E3}',
        2 => '\u{E4E4}',
        4 => '\u{E4E5}',
        8 => '\u{E4E6}',
        16 => '\u{E4E7}',
        32 => '\u{E4E8}',
        _ => '\u{E4E9}',
    }
}

fn flag_glyph(value: u32, up: bool) -> Option<char> {
    let base = match value {
        8 => 0xE240,
        16 => 0xE242,
        32 => 0xE244,
        v if v >= 64 => 0xE246,
        _ => return None,
    };
    char::from_u32(if up { base } else { base + 1 })
}

fn time_digits(n: u8) -> String {
    n.to_string().chars().filter_map(|d| d.to_digit(10).and_then(|v| char::from_u32(0xE080 + v))).collect()
}

fn music_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MUSIC_FAMILY.into()))
}

/// Dibuja texto de la fuente musical con su línea base en `baseline`; `anchor_x` es el
/// centro (`center`), la izquierda o la derecha (`right`).
fn glyphs(p: &Painter, text: String, size: f32, x: f32, baseline: f32, color: Color32, anchor: Anchor) {
    let galley = p.layout_no_wrap(text, music_font(size), color);
    let Some(row) = galley.rows.first() else { return };
    let base = row.pos.y + row.glyphs.first().map(|g| g.pos.y).unwrap_or(0.0);
    let w = galley.size().x;
    let left = match anchor {
        Anchor::Left => x,
        Anchor::Center => x - w / 2.0,
        Anchor::Right => x - w,
    };
    p.galley(Pos2::new(left, baseline - base), galley, color);
}

#[derive(Clone, Copy)]
enum Anchor {
    Left,
    Center,
    Right,
}

/// Letra (0 = do ... 6 = si) y alteración de una nota MIDI según la tonalidad.
fn spell(midi: i32, key: i8) -> (i32, i8) {
    const SHARPS: [(i32, i8); 12] = [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)];
    const FLATS: [(i32, i8); 12] = [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    let (letter, acc) = if key < 0 { FLATS[pc] } else { SHARPS[pc] };
    (octave * 7 + letter, acc)
}

/// Alteración que la armadura impone a cada letra.
fn key_accidental(key: i8, letter: i32) -> i8 {
    const SHARP_ORDER: [i32; 7] = [3, 0, 4, 1, 5, 2, 6];
    const FLAT_ORDER: [i32; 7] = [6, 2, 5, 1, 4, 0, 3];
    let n = key.unsigned_abs() as usize;
    if key > 0 && SHARP_ORDER[..n.min(7)].contains(&letter) {
        1
    } else if key < 0 && FLAT_ORDER[..n.min(7)].contains(&letter) {
        -1
    } else {
        0
    }
}

/// Nota diatónica de la línea superior del pentagrama (F5 en sol, A3 en fa).
fn top_line(clef: Clef) -> i32 {
    match clef {
        Clef::Treble => 5 * 7 + 3,
        Clef::Bass => 3 * 7 + 5,
    }
}

pub struct Geometry {
    pub top: f32,
    pub zoom: f32,
}

impl Geometry {
    fn space(&self) -> f32 {
        SPACE * self.zoom
    }

    fn y(&self, pos: i32) -> f32 {
        self.top + pos as f32 * self.space() / 2.0
    }

    fn bottom(&self) -> f32 {
        self.y(8)
    }
}

/// Clave y armadura al inicio de un sistema.
pub fn draw_head(p: &Painter, c: &StaffColors, g: &Geometry, clef: Clef, key: i8, x: f32, x_end: f32) {
    let sp = g.space();
    for k in 0..5 {
        let y = g.y(k * 2);
        p.line_segment([Pos2::new(x, y), Pos2::new(x_end, y)], Stroke::new(1.0, c.line));
    }
    let (glyph, line_pos) = match clef {
        Clef::Treble => (G_CLEF, 6),
        Clef::Bass => (F_CLEF, 2),
    };
    glyphs(p, glyph.to_string(), 4.0 * sp, x + 2.0 * g.zoom, g.y(line_pos), c.fg, Anchor::Left);

    // Posiciones (en medios espacios) de las alteraciones de la armadura.
    const TREBLE_SHARPS: [i32; 7] = [0, 3, -1, 2, 5, 1, 4];
    const TREBLE_FLATS: [i32; 7] = [4, 1, 5, 2, 6, 3, 7];
    const BASS_SHARPS: [i32; 7] = [2, -1, 3, 0, 4, 1, 5];
    const BASS_FLATS: [i32; 7] = [6, 3, 7, 4, 8, 5, 9];
    let n = key.unsigned_abs().min(7) as usize;
    let table = match (clef, key > 0) {
        (Clef::Treble, true) => TREBLE_SHARPS,
        (Clef::Treble, false) => TREBLE_FLATS,
        (Clef::Bass, true) => BASS_SHARPS,
        (Clef::Bass, false) => BASS_FLATS,
    };
    let glyph = if key > 0 { SHARP } else { FLAT };
    for (i, &pos) in table.iter().take(n).enumerate() {
        glyphs(p, glyph.to_string(), 4.0 * sp, x + 30.0 * g.zoom + i as f32 * 5.5 * g.zoom, g.y(pos), c.fg, Anchor::Left);
    }
}

/// Un compás de pentagrama entre `x0` y `x1`; `xs` son las posiciones de cada pulso.
#[allow(clippy::too_many_arguments)]
pub fn draw_measure(
    p: &Painter,
    c: &StaffColors,
    g: &Geometry,
    track: &Track,
    header: &MeasureHeader,
    measure: &Measure,
    xs: &[f32],
    x0: f32,
    x1: f32,
    show_signature: bool,
) {
    let sp = g.space();
    let zoom = g.zoom;
    for k in 0..5 {
        let y = g.y(k * 2);
        p.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.0, c.line));
    }
    p.line_segment([Pos2::new(x1, g.y(0)), Pos2::new(x1, g.bottom())], Stroke::new(1.0, c.fg));
    p.line_segment([Pos2::new(x0, g.y(0)), Pos2::new(x0, g.bottom())], Stroke::new(1.0, c.fg));

    if show_signature {
        let cx = x0 + 18.0 * zoom;
        glyphs(p, time_digits(header.numerator), 4.0 * sp, cx, g.y(2), c.fg, Anchor::Center);
        glyphs(p, time_digits(header.denominator), 4.0 * sp, cx, g.y(6), c.fg, Anchor::Center);
    }
    if track.is_percussion {
        return;
    }

    let d_top = top_line(track.clef);
    let key = header.key_signature;
    // Alteraciones vigentes en el compás, por nota diatónica.
    let mut current: HashMap<i32, i8> = HashMap::new();
    let note_w = 1.18 * sp;
    let mid_y = g.y(4);

    // Datos de cada pulso y voz para poder agrupar las barras.
    struct Stem {
        beat: usize,
        x: f32,
        up: bool,
        /// Punta natural de la plica (sin barras).
        tip: f32,
        /// Extremo de las notas que quedan del lado contrario de la punta.
        root: f32,
        level: u32,
        quarter: u64,
        voice: usize,
        color: Color32,
    }
    let mut stems: Vec<Stem> = Vec::new();
    let both_voices = |beat: &Beat| beat.voices.iter().filter(|v| !v.empty).count() > 1;

    for (i, beat) in measure.beats.iter().enumerate() {
        let Some(&x) = xs.get(i) else { break };
        for (vi, voice) in beat.voices.iter().enumerate() {
            if voice.empty {
                continue;
            }
            let color = if vi == 0 { c.fg } else { c.voice2 };
            let value = voice.duration.value;
            if voice.notes.is_empty() {
                // Silencio.
                let pos = if value == 1 { 2 } else { 4 };
                glyphs(p, rest_glyph(value).to_string(), 4.0 * sp, x, g.y(pos), color, Anchor::Center);
                continue;
            }

            // Posiciones de las notas de esta voz.
            let mut heads: Vec<(i32, Option<i8>, &Note)> = Vec::new();
            for n in &voice.notes {
                let Some(&open) = track.strings.get(n.string as usize - 1) else { continue };
                let sounding = open + track.capo + n.fret as i32;
                let written = sounding + 12;
                let (d, acc) = spell(written, key);
                let letter = d.rem_euclid(7);
                let known = *current.get(&d).unwrap_or(&key_accidental(key, letter));
                let shown = if acc != known { Some(acc) } else { None };
                current.insert(d, acc);
                heads.push((d_top - d, shown, n));
            }
            if heads.is_empty() {
                continue;
            }
            heads.sort_by_key(|h| h.0);

            let (min_pos, max_pos) = (heads[0].0, heads[heads.len() - 1].0);
            let avg = heads.iter().map(|h| h.0).sum::<i32>() as f32 / heads.len() as f32;
            let up = if both_voices(beat) { vi == 0 } else { avg >= 4.0 };

            // Segundas: la nota contigua a otra del mismo lado pasa al otro lado de la plica.
            let n_heads = heads.len();
            let mut shift = vec![0.0f32; n_heads];
            {
                let order: Vec<usize> = if up { (0..n_heads).rev().collect() } else { (0..n_heads).collect() };
                let mut prev: Option<(i32, bool)> = None;
                for &k in &order {
                    let pos = heads[k].0;
                    let displaced = matches!(prev, Some((pp, false)) if (pos - pp).abs() == 1);
                    if displaced {
                        shift[k] = if up { note_w } else { -note_w };
                    }
                    prev = Some((pos, displaced));
                }
            }
            // Alteraciones en columnas escalonadas para que no se pisen entre sí.
            let mut acc_col = vec![0usize; n_heads];
            {
                let mut last: Vec<i32> = Vec::new();
                for (k, h) in heads.iter().enumerate() {
                    if h.1.is_some() {
                        let col = last.iter().position(|&lp| h.0 - lp >= 6).unwrap_or(last.len());
                        if col == last.len() {
                            last.push(h.0);
                        } else {
                            last[col] = h.0;
                        }
                        acc_col[k] = col;
                    }
                }
            }
            let left_extra = if shift.iter().any(|&d| d < 0.0) { note_w } else { 0.0 };

            for (k, (pos, shown, note)) in heads.iter().enumerate() {
                let y = g.y(*pos);
                let hx = x + shift[k];
                // Líneas adicionales.
                if *pos <= -2 {
                    let first = if pos % 2 == 0 { *pos } else { pos + 1 };
                    for lp in (first..=-2).step_by(2) {
                        p.line_segment([Pos2::new(hx - note_w * 0.8, g.y(lp)), Pos2::new(hx + note_w * 0.8, g.y(lp))], Stroke::new(1.0, c.fg));
                    }
                } else if *pos >= 10 {
                    let last = if pos % 2 == 0 { *pos } else { pos - 1 };
                    for lp in (10..=last).step_by(2) {
                        p.line_segment([Pos2::new(hx - note_w * 0.8, g.y(lp)), Pos2::new(hx + note_w * 0.8, g.y(lp))], Stroke::new(1.0, c.fg));
                    }
                }
                // Cabeza de nota.
                let head = if note.effect.dead {
                    NOTE_X
                } else {
                    match value {
                        1 => NOTE_WHOLE,
                        2 => NOTE_HALF,
                        _ => NOTE_BLACK,
                    }
                };
                glyphs(p, head.to_string(), 4.0 * sp, hx, y, color, Anchor::Center);
                if let Some(acc) = shown {
                    let a = match acc {
                        1 => SHARP,
                        -1 => FLAT,
                        _ => NATURAL,
                    };
                    glyphs(p, a.to_string(), 4.0 * sp, x - note_w * 0.5 - 2.0 * zoom - left_extra - acc_col[k] as f32 * 6.5 * zoom, y, color, Anchor::Right);
                }
                // Puntillos (en el espacio, no sobre la línea).
                let dots = if voice.duration.dotted { 1 } else if voice.duration.double_dotted { 2 } else { 0 };
                for d in 0..dots {
                    let dy = if pos % 2 == 0 { y - sp * 0.5 } else { y };
                    glyphs(p, AUG_DOT.to_string(), 4.0 * sp, hx + note_w * 0.5 + (2.0 + 4.0 * d as f32) * zoom, dy, color, Anchor::Left);
                }
                // Ligadura con la nota anterior de la misma altura.
                if note.tied {
                    let from = if i > 0 { xs[i - 1] + note_w * 0.5 } else { x0 + 3.0 * zoom };
                    let to = x - note_w * 0.5;
                    if to > from + 2.0 {
                        let sign = if up { 1.0 } else { -1.0 };
                        let base = y + sign * sp * 0.6;
                        let pts: Vec<Pos2> = (0..=10)
                            .map(|s| {
                                let f = s as f32 / 10.0;
                                Pos2::new(from + (to - from) * f, base + sign * (1.0 - (2.0 * f - 1.0).powi(2)) * sp * 0.7)
                            })
                            .collect();
                        p.add(egui::Shape::line(pts, Stroke::new(1.0, color)));
                    }
                }
            }

            // Plica.
            if value >= 2 {
                let (root, tip, sx) = if up {
                    (g.y(max_pos), (g.y(min_pos) - 3.5 * sp).min(mid_y), x + note_w * 0.5 - 0.5)
                } else {
                    (g.y(min_pos), (g.y(max_pos) + 3.5 * sp).max(mid_y), x - note_w * 0.5 + 0.5)
                };
                let level = if value >= 8 { (value / 4).ilog2() } else { 0 };
                stems.push(Stem {
                    beat: i,
                    x: sx,
                    up,
                    tip,
                    root,
                    level,
                    quarter: beat.start.saturating_sub(header.start) / QUARTER_TIME,
                    voice: vi,
                    color,
                });
                if voice.duration.enters != 1 {
                    let ty = if up { tip - 0.6 * sp } else { tip + 1.6 * sp };
                    p.text(Pos2::new(x, ty), Align2::CENTER_CENTER, voice.duration.enters.to_string(), FontId::proportional(8.0 * zoom), color);
                }
            }
        }
    }

    // Barras: pulsos consecutivos de la misma voz y negra con figuras de corchea o menores.
    let mut idx = 0;
    while idx < stems.len() {
        let s = &stems[idx];
        let mut end = idx + 1;
        if s.level >= 1 {
            while end < stems.len()
                && stems[end].voice == s.voice
                && stems[end].level >= 1
                && stems[end].quarter == s.quarter
                && stems[end].up == s.up
            {
                end += 1;
            }
        }
        let group = &stems[idx..end];
        if group.len() >= 2 {
            // Altura común de la barra: la punta más lejana del grupo.
            let tip = if s.up { group.iter().map(|m| m.tip).fold(f32::MAX, f32::min) } else { group.iter().map(|m| m.tip).fold(f32::MIN, f32::max) };
            let dir = if s.up { 1.0 } else { -1.0 };
            for m in group {
                p.line_segment([Pos2::new(m.x, m.root), Pos2::new(m.x, tip)], Stroke::new(1.0, m.color));
            }
            for (gi, m) in group.iter().enumerate() {
                for l in 1..=m.level {
                    let y = tip + (l - 1) as f32 * 0.8 * sp * dir;
                    let next = group.get(gi + 1).map(|n| n.level).unwrap_or(0);
                    let prev = if gi > 0 { group[gi - 1].level } else { 0 };
                    let stroke = Stroke::new(0.5 * sp, m.color);
                    if next >= l {
                        p.line_segment([Pos2::new(m.x, y), Pos2::new(group[gi + 1].x, y)], stroke);
                    } else if prev < l {
                        p.line_segment([Pos2::new(m.x, y), Pos2::new(m.x + 1.2 * sp, y)], stroke);
                    } else if prev >= l && next < l {
                        // El último con un nivel que no continúa: el trazo ya llegó desde el anterior.
                    }
                }
            }
            let _ = group[0].beat;
        } else {
            let m = &stems[idx];
            p.line_segment([Pos2::new(m.x, m.root), Pos2::new(m.x, m.tip)], Stroke::new(1.0, m.color));
            // Bandera suelta.
            let value = if m.level == 0 { 4 } else { 4 << m.level };
            if let Some(flag) = flag_glyph(value, m.up) {
                glyphs(p, flag.to_string(), 4.0 * sp, m.x - 0.4, m.tip, m.color, Anchor::Left);
            }
        }
        idx = end;
    }
    let _ = vec2(0.0, 0.0);
    let _ = Rect::NOTHING;
}

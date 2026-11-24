//! Dibujo de la tablatura de una pista.

use egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Ui, vec2};
use crate::staff;
use neoguitar_core::model::*;
use std::collections::HashMap;

const STRING_GAP: f32 = 16.0;
const TOP_MARGIN: f32 = 44.0;
const BOTTOM_MARGIN: f32 = 44.0;
/// Margen superior de los bloques que no son el primero (no llevan números ni marcadores).
const COMPACT_TOP_MARGIN: f32 = 34.0;
/// Separación entre las pistas apiladas de un sistema.
const BLOCK_GAP: f32 = 10.0;
const LEFT_LABEL: f32 = 38.0;
/// Con pentagrama hace falta más lugar a la izquierda para la clave y la armadura.
const LEFT_LABEL_STAFF: f32 = 74.0;

fn left_label(staff: bool) -> f32 {
    if staff { LEFT_LABEL_STAFF } else { LEFT_LABEL }
}

/// Sílaba de la letra que corresponde a cada pulso: (compás, pulso) -> sílaba.
fn lyric_map(song: &Song, track: &Track) -> HashMap<(usize, usize), String> {
    let mut map = HashMap::new();
    let Some(lyrics) = song.lyrics.as_ref().filter(|l| l.track + 1 == track.number) else { return map };
    let mut syllables = lyrics.syllables().into_iter();
    for (mi, measure) in track.measures.iter().enumerate().skip(lyrics.from_measure) {
        for (bi, beat) in measure.beats.iter().enumerate() {
            if beat.voices.iter().any(|v| !v.empty && !v.notes.is_empty()) {
                match syllables.next() {
                    Some(s) => {
                        map.insert((mi, bi), s);
                    }
                    None => return map,
                }
            }
        }
    }
    map
}
const SYSTEM_GAP: f32 = 12.0;

struct Colors {
    fg: Color32,
    line: Color32,
    weak: Color32,
    bg: Color32,
    accent: Color32,
    voice2: Color32,
}

impl Colors {
    fn new(ui: &Ui) -> Self {
        let _ = ui;
        Colors {
            fg: crate::theme::TEXT,
            line: crate::theme::SCORE_LINE,
            weak: crate::theme::MUTED,
            bg: crate::theme::SCORE_BG,
            accent: crate::theme::ACCENT,
            voice2: Color32::from_rgb(90, 160, 235),
        }
    }
}

/// Posición horizontal calculada para un compás dentro de un sistema.
struct MeasureLayout {
    index: usize,
    width: f32,
    show_signature: bool,
    /// Posición de cada pulso (relativa al inicio del compás), por pista mostrada.
    beat_x: Vec<Vec<f32>>,
}

/// Acción pedida por el usuario sobre un compás (índices desde 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAction {
    /// Saltar la reproducción a este compás.
    Seek(usize),
    /// Empezar un bucle en este compás.
    LoopFrom(usize),
    /// Terminar el bucle en este compás.
    LoopTo(usize),
    ClearLoop,
}

/// Altura de un bloque (una pista dentro de un sistema).
fn block_height(track: &Track, staff_on: bool, lyrics: bool, primary: bool, zoom: f32) -> f32 {
    let top = if primary { TOP_MARGIN } else { COMPACT_TOP_MARGIN };
    let extra_top = if staff_on { staff::STAFF_BLOCK } else { 0.0 };
    let extra_bottom = if lyrics { 14.0 } else { 0.0 };
    let strings = track.strings.len().max(1);
    (top + BOTTOM_MARGIN + extra_top + extra_bottom) * zoom + (strings - 1) as f32 * STRING_GAP * zoom
}

/// Dibuja la tablatura de una o varias pistas (la primera es la principal: lleva los números de
/// compás, marcadores y tempos). `playhead` = (compás, tick absoluto) de la reproducción y
/// `loop_range` = compases (desde, hasta) del bucle.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    song: &Song,
    tracks: &[&Track],
    zoom: f32,
    focus_measure: Option<usize>,
    playhead: Option<(usize, u64)>,
    loop_range: Option<(usize, usize)>,
    show_staff: bool,
    transpose: i32,
) -> Option<ViewAction> {
    let colors = Colors::new(ui);
    let mut action = None;
    if tracks.is_empty() {
        return None;
    }
    let staff_flags: Vec<bool> = tracks.iter().map(|t| show_staff && !t.is_percussion).collect();
    let any_staff = staff_flags.iter().any(|&s| s);
    let lyric_maps: Vec<HashMap<(usize, usize), String>> = tracks.iter().map(|t| lyric_map(song, t)).collect();
    let measures_count = tracks.iter().map(|t| t.measures.len()).min().unwrap_or(0);
    let heights: Vec<f32> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| block_height(t, staff_flags[i], !lyric_maps[i].is_empty(), i == 0, zoom))
        .collect();
    let height = heights.iter().sum::<f32>() + BLOCK_GAP * zoom * (tracks.len() - 1) as f32;

    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let avail = ui.available_width().max(300.0);
        let usable = (avail - left_label(any_staff) * zoom - 16.0).max(200.0);

        let layouts: Vec<MeasureLayout> = (0..measures_count).map(|i| layout_measure(song, tracks, i, zoom)).collect();

        // Reparto de compases en sistemas (líneas).
        let mut systems: Vec<Vec<&MeasureLayout>> = vec![Vec::new()];
        let mut used = 0.0;
        for l in &layouts {
            if used + l.width > usable && !systems.last().unwrap().is_empty() {
                systems.push(Vec::new());
                used = 0.0;
            }
            used += l.width;
            systems.last_mut().unwrap().push(l);
        }

        for system in &systems {
            if system.is_empty() {
                continue;
            }
            let (rect, response) = ui.allocate_exact_size(vec2(avail, height), egui::Sense::click());
            // Con pentagrama se deja más aire entre sistemas: así se ve a qué tablatura pertenece cada uno.
            ui.add_space((SYSTEM_GAP + if any_staff { 26.0 } else { 0.0 }) * zoom);
            if focus_measure.is_some_and(|m| system.iter().any(|l| l.index + 1 == m)) {
                ui.scroll_to_rect(rect, Some(egui::Align::TOP));
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let ctx = SystemContext { c: &colors, song, tracks, staff_flags: &staff_flags, lyric_maps: &lyric_maps, heights: &heights, zoom, playhead, loop_range, any_staff, transpose };
            let extents = draw_system(ui, &ctx, system, rect);
            let measure_at = |pos: Pos2| extents.iter().find(|(_, x0, x1)| pos.x >= *x0 && pos.x < *x1).map(|e| e.0);
            if response.clicked() {
                if let Some(m) = response.interact_pointer_pos().and_then(measure_at) {
                    action = Some(ViewAction::Seek(m));
                }
            }
            // Clic derecho: recuerda el compás y abre el menú.
            if response.secondary_clicked() {
                if let Some(m) = response.interact_pointer_pos().and_then(measure_at) {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("ctx_measure"), m));
                }
            }
            response.context_menu(|ui| {
                let m: Option<usize> = ui.data(|d| d.get_temp(egui::Id::unique("ctx_measure")));
                if let Some(m) = m {
                    ui.label(format!("Measure {}", m + 1));
                    if ui.button("▶ Play from here").clicked() {
                        action = Some(ViewAction::Seek(m));
                        ui.close();
                    }
                    if ui.button("🔁 Loop: start here").clicked() {
                        action = Some(ViewAction::LoopFrom(m));
                        ui.close();
                    }
                    if ui.button("🔁 Loop: end here").clicked() {
                        action = Some(ViewAction::LoopTo(m));
                        ui.close();
                    }
                }
                if ui.button("✖ Clear loop").clicked() {
                    action = Some(ViewAction::ClearLoop);
                    ui.close();
                }
            });
        }
    });
    action
}

fn beat_time(beat: &Beat) -> u64 {
    beat.voices
        .iter()
        .filter(|v| !v.empty)
        .map(|v| v.duration.time())
        .min()
        .unwrap_or(QUARTER_TIME)
}

fn layout_measure(song: &Song, tracks: &[&Track], index: usize, zoom: f32) -> MeasureLayout {
    let header = &song.headers[index];
    let show_signature = index == 0 || {
        let prev = &song.headers[index - 1];
        prev.numerator != header.numerator || prev.denominator != header.denominator
    };

    // Cada pista calcula su propia distribución; el compás toma el ancho mayor y las demás se estiran.
    let own: Vec<(Vec<f32>, f32)> = tracks
        .iter()
        .map(|track| {
            let measure = &track.measures[index];
            let mut x = 12.0 + if show_signature { 22.0 } else { 0.0 };
            let mut beat_x = Vec::with_capacity(measure.beats.len());
            for beat in &measure.beats {
                let q = (beat_time(beat) as f32 / QUARTER_TIME as f32).min(4.0);
                let w = 22.0 + 22.0 * q;
                beat_x.push((x + w * 0.5) * zoom);
                x += w;
            }
            (beat_x, (x.max(60.0) + 6.0) * zoom)
        })
        .collect();
    let width = own.iter().map(|(_, w)| *w).fold(0.0, f32::max);
    let beat_x = own.into_iter().map(|(bx, w)| bx.into_iter().map(|v| v * width / w).collect()).collect();
    MeasureLayout { index, width, show_signature, beat_x }
}

fn font(size: f32, zoom: f32) -> FontId {
    FontId::proportional(size * zoom)
}

/// Datos comunes a todos los bloques de un sistema.
struct SystemContext<'a> {
    c: &'a Colors,
    song: &'a Song,
    tracks: &'a [&'a Track],
    staff_flags: &'a [bool],
    lyric_maps: &'a [HashMap<(usize, usize), String>],
    heights: &'a [f32],
    zoom: f32,
    playhead: Option<(usize, u64)>,
    loop_range: Option<(usize, usize)>,
    any_staff: bool,
    /// Semitonos de transposición del sonido (el pentagrama se escribe a la altura que suena).
    transpose: i32,
}

/// Dibuja un sistema: una pista tras otra, apiladas, con un corchete que las agrupa.
/// Devuelve los límites horizontales de cada compás (del bloque principal).
fn draw_system(ui: &Ui, ctx: &SystemContext, system: &[&MeasureLayout], rect: Rect) -> Vec<(usize, f32, f32)> {
    let mut extents = Vec::new();
    let mut y = rect.top();
    let (mut first_top, mut last_bottom) = (rect.top(), rect.bottom());
    for (ti, track) in ctx.tracks.iter().enumerate() {
        let block = Rect::from_min_size(Pos2::new(rect.left(), y), vec2(rect.width(), ctx.heights[ti]));
        let lyrics = if ctx.lyric_maps[ti].is_empty() { None } else { Some(&ctx.lyric_maps[ti]) };
        let (ext, bracket_top, bottom) = draw_block(ui, ctx, track, ti, system, block, ctx.staff_flags[ti], lyrics);
        if ti == 0 {
            extents = ext;
            first_top = bracket_top;
        }
        last_bottom = bottom;
        y += ctx.heights[ti] + BLOCK_GAP * ctx.zoom;
    }
    if ctx.tracks.len() > 1 {
        // Corchete de grupo entre la primera y la última pista.
        let p = ui.painter_at(rect);
        // En el borde izquierdo, para no pisar la clave ni las letras de las cuerdas.
        let x = rect.left() + 1.5 * ctx.zoom;
        p.line_segment([Pos2::new(x, first_top), Pos2::new(x, last_bottom)], Stroke::new(2.0 * ctx.zoom, ctx.c.weak));
    }
    extents
}

/// Un bloque = una pista dentro de un sistema. Devuelve (límites de compases, arriba del corchete, abajo).
#[allow(clippy::too_many_arguments)]
fn draw_block(
    ui: &Ui,
    ctx: &SystemContext,
    track: &Track,
    track_pos: usize,
    system: &[&MeasureLayout],
    rect: Rect,
    staff_on: bool,
    lyrics: Option<&HashMap<(usize, usize), String>>,
) -> (Vec<(usize, f32, f32)>, f32, f32) {
    let (c, song, zoom, playhead, loop_range) = (ctx.c, ctx.song, ctx.zoom, ctx.playhead, ctx.loop_range);
    let primary = track_pos == 0;
    let top_margin = if primary { TOP_MARGIN } else { COMPACT_TOP_MARGIN };
    let mut extents = Vec::with_capacity(system.len());
    let p = ui.painter_at(rect);
    let strings = track.strings.len().max(1);
    let gap = STRING_GAP * zoom;
    let staff_offset = if staff_on { staff::STAFF_BLOCK * zoom } else { 0.0 };
    let top = rect.top() + staff_offset + top_margin * zoom;
    let staff_colors = staff::StaffColors { fg: c.fg, line: c.line, voice2: c.voice2 };
    let staff_geo = staff::Geometry { top: rect.top() + staff::STAFF_TOP_MARGIN * zoom, zoom };
    let bottom = top + (strings - 1) as f32 * gap;
    let left = rect.left() + left_label(ctx.any_staff) * zoom;
    // Nombre de la pista (solo cuando hay varias a la vez).
    if ctx.tracks.len() > 1 {
        let [r, g, b] = track.color;
        let name = if track.name.trim().is_empty() { format!("Track {}", track.number) } else { track.name.trim().to_string() };
        p.text(
            Pos2::new(rect.left() + 3.0 * zoom, rect.top() + 2.0 * zoom),
            Align2::LEFT_TOP,
            name,
            font(9.5, zoom),
            crate::theme::track_color([r, g, b]),
        );
    }
    if staff_on {
        if let Some(first) = system.first() {
            staff::draw_head(&p, &staff_colors, &staff_geo, track.clef, staff::transposed_key(song.headers[first.index].key_signature, ctx.transpose), rect.left(), left);
            // Corchete que une el pentagrama con su tablatura.
            let bx = left - 5.0 * zoom;
            p.line_segment([Pos2::new(bx, staff_geo.top), Pos2::new(bx, bottom)], Stroke::new(2.5 * zoom, c.fg));
        }
    }
    let total_w: f32 = system.iter().map(|l| l.width).sum();
    // Se estira el sistema para llenar el ancho disponible, salvo que sea el último y quede corto.
    let avail = rect.right() - left - 12.0;
    let stretch = if total_w > avail * 0.6 { (avail / total_w).min(1.6) } else { 1.0 };

    let string_y = |s: u8| top + (s as f32 - 1.0) * gap;

    // Afinación a la izquierda.
    for (i, &midi) in track.strings.iter().enumerate() {
        if track.is_percussion {
            break; // en percusión el número de "traste" es la nota MIDI; la cuerda no tiene afinación
        }
        let name = midi_note_name(midi);
        let name = name.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '-').to_string();
        p.text(
            Pos2::new(left - 6.0 * zoom, top + i as f32 * gap),
            Align2::RIGHT_CENTER,
            name,
            font(10.0, zoom),
            c.weak,
        );
    }

    let mut x0 = left;
    for (si, l) in system.iter().enumerate() {
        let w = l.width * stretch;
        let header = &song.headers[l.index];
        let measure = &track.measures[l.index];
        let x1 = x0 + w;
        extents.push((l.index, x0, x1));

        // Compás que suena.
        let cursor_tick = playhead.filter(|(m, _)| *m == l.index).map(|(_, tick)| tick);
        if cursor_tick.is_some() {
            let tint_top = if staff_on { rect.top() + 4.0 * zoom } else { top - 20.0 * zoom };
            let tint = Rect::from_min_max(Pos2::new(x0, tint_top), Pos2::new(x1, bottom + 40.0 * zoom));
            p.rect_filled(tint, 3.0, c.accent.gamma_multiply(0.13));
        }

        // Compases dentro del bucle: franja bajo el número de compás.
        if primary && loop_range.is_some_and(|(a, b)| (a..=b).contains(&l.index)) {
            let y = top - 17.0 * zoom;
            p.line_segment([Pos2::new(x0 + 1.0, y), Pos2::new(x1 - 1.0, y)], Stroke::new(3.0 * zoom, c.voice2));
        }

        // Cuerdas.
        for s in 1..=strings as u8 {
            p.line_segment([Pos2::new(x0, string_y(s)), Pos2::new(x1, string_y(s))], Stroke::new(1.0, c.line));
        }

        // Barra inicial (con repetición) y número de compás.
        if si == 0 || header.repeat_open {
            bar(&p, c, x0, top, bottom, header.repeat_open, false, zoom);
        } else {
            p.line_segment([Pos2::new(x0, top), Pos2::new(x0, bottom)], Stroke::new(1.0, c.fg));
        }
        if primary {
            p.text(
                Pos2::new(x0 + 3.0 * zoom, top - 22.0 * zoom),
                Align2::LEFT_BOTTOM,
                format!("{}", header.number),
                font(9.0, zoom),
                c.weak,
            );

            // Marcador y tempo.
            let mut label_x = x0 + 22.0 * zoom;
            if let Some(marker) = &header.marker {
                let r = p.text(Pos2::new(label_x, top - 34.0 * zoom), Align2::LEFT_BOTTOM, marker, font(11.0, zoom), c.accent);
                label_x = r.right() + 8.0 * zoom;
            }
            let tempo_changed = l.index == 0 || song.headers[l.index - 1].tempo != header.tempo;
            if tempo_changed {
                p.text(
                    Pos2::new(label_x, top - 34.0 * zoom),
                    Align2::LEFT_BOTTOM,
                    format!("{} BPM", header.tempo),
                    font(10.0, zoom),
                    c.weak,
                );
            }
            if header.repeat_alternative != 0 {
                let endings: Vec<String> =
                    (0..8).filter(|i| header.repeat_alternative & (1 << i) != 0).map(|i| format!("{}.", i + 1)).collect();
                p.text(
                    Pos2::new(x0 + 4.0 * zoom, top - 10.0 * zoom),
                    Align2::LEFT_BOTTOM,
                    endings.join(" "),
                    font(9.0, zoom),
                    c.accent,
                );
            }

            // Armadura (se muestra al inicio y cuando cambia).
            let prev_key = l.index.checked_sub(1).map(|j| song.headers[j].key_signature).unwrap_or(0);
            if header.key_signature != prev_key || (l.index == 0 && header.key_signature != 0) {
                let k = staff::transposed_key(header.key_signature, ctx.transpose);
                let label = if k > 0 { format!("{k}#") } else { format!("{}b", -k) };
                p.text(Pos2::new(x0 + 22.0 * zoom, top - 10.0 * zoom), Align2::LEFT_BOTTOM, format!("Key: {label}"), font(8.5, zoom), c.weak);
            }

        }

        // Compás.
        if l.show_signature {
            let cx = x0 + 18.0 * zoom;
            let mid = (top + bottom) / 2.0;
            p.text(Pos2::new(cx, mid - 1.0), Align2::CENTER_BOTTOM, header.numerator.to_string(), font(15.0, zoom), c.fg);
            p.text(Pos2::new(cx, mid + 1.0), Align2::CENTER_TOP, header.denominator.to_string(), font(15.0, zoom), c.fg);
        }

        // Pulsos.
        let xs: Vec<f32> = l.beat_x[track_pos].iter().map(|bx| x0 + bx * stretch).collect();
        if staff_on {
            staff::draw_measure(&p, &staff_colors, &staff_geo, track, header, measure, &xs, x0, x1, l.show_signature, ctx.transpose);
        }
        draw_beats(&p, c, track, measure, &xs, top, bottom, gap, zoom, cursor_tick, &header.tempo_changes, lyrics, l.index);

        // Barra final.
        let closing = header.repeat_close > 0;
        let last = si + 1 == system.len() && l.index + 1 == track.measures.len();
        if closing || last {
            bar(&p, c, x1, top, bottom, closing, true, zoom);
        } else {
            p.line_segment([Pos2::new(x1, top), Pos2::new(x1, bottom)], Stroke::new(1.0, c.fg));
        }
        if primary && closing && header.repeat_close > 1 {
            p.text(
                Pos2::new(x1 - 4.0 * zoom, top - 10.0 * zoom),
                Align2::RIGHT_BOTTOM,
                format!("x{}", header.repeat_close),
                font(9.0, zoom),
                c.accent,
            );
        }
        x0 = x1;
    }
    let bracket_top = if staff_on { staff_geo.top } else { top };
    (extents, bracket_top, bottom + (BOTTOM_MARGIN - 8.0) * zoom)
}

/// Barra de compás; `repeat` agrega la barra gruesa y los puntos.
fn bar(p: &egui::Painter, c: &Colors, x: f32, top: f32, bottom: f32, repeat: bool, closing: bool, zoom: f32) {
    if !repeat {
        // Barra final simple (doble barra fina + gruesa).
        let thick = 2.5 * zoom;
        p.line_segment([Pos2::new(x - 3.0 * zoom, top), Pos2::new(x - 3.0 * zoom, bottom)], Stroke::new(1.0, c.fg));
        p.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(thick, c.fg));
        return;
    }
    let dir = if closing { -1.0 } else { 1.0 };
    let thick = 2.5 * zoom;
    p.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(thick, c.fg));
    p.line_segment(
        [Pos2::new(x + dir * 4.0 * zoom, top), Pos2::new(x + dir * 4.0 * zoom, bottom)],
        Stroke::new(1.0, c.fg),
    );
    let mid = (top + bottom) / 2.0;
    for dy in [-5.0, 5.0] {
        p.circle_filled(Pos2::new(x + dir * 9.0 * zoom, mid + dy * zoom), 1.8 * zoom, c.fg);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_beats(
    p: &egui::Painter,
    c: &Colors,
    track: &Track,
    measure: &Measure,
    xs: &[f32],
    top: f32,
    bottom: f32,
    gap: f32,
    zoom: f32,
    cursor_tick: Option<u64>,
    tempo_changes: &[(u64, i32)],
    lyrics: Option<&HashMap<(usize, usize), String>>,
    measure_index: usize,
) {
    // Cambios de tempo dentro del compás: etiqueta sobre el primer pulso que empieza en ese tick o después.
    for &(tick, bpm) in tempo_changes {
        if let Some((i, _)) = measure.beats.iter().enumerate().find(|(_, b)| b.start >= tick) {
            if let Some(&x) = xs.get(i) {
                p.text(Pos2::new(x, top - 27.0 * zoom), Align2::CENTER_BOTTOM, format!("{bpm} BPM"), font(8.5, zoom), c.weak);
            }
        }
    }
    // Pulso que suena: el último que empezó antes del tick actual.
    let current_beat = cursor_tick.and_then(|tick| measure.beats.iter().rposition(|b| b.start <= tick));
    let levels = |beat: &Beat| -> u32 {
        let v = main_voice(beat);
        match v {
            Some(v) if v.duration.value >= 8 => (v.duration.value / 4).ilog2(),
            _ => 0,
        }
    };

    for (i, beat) in measure.beats.iter().enumerate() {
        let Some(&x) = xs.get(i) else { break };
        if current_beat == Some(i) {
            let r = Rect::from_min_max(Pos2::new(x - 11.0 * zoom, top - 6.0 * zoom), Pos2::new(x + 11.0 * zoom, bottom + 36.0 * zoom));
            p.rect_filled(r, 3.0, c.accent.gamma_multiply(0.35));
        }

        // Texto de efectos sobre el pentagrama.
        let mut tags: Vec<&str> = Vec::new();
        for voice in &beat.voices {
            for n in &voice.notes {
                let e = &n.effect;
                let harmonic = match e.harmonic {
                    Some(Harmonic::Natural) => "N.H.",
                    Some(Harmonic::Artificial) => "A.H.",
                    Some(Harmonic::Pinch) => "P.H.",
                    Some(Harmonic::Tapped) => "T.H.",
                    Some(Harmonic::Semi) => "S.H.",
                    None => "",
                };
                let flags = [
                    (e.palm_mute, "P.M."),
                    (e.let_ring, "L.R."),
                    (e.staccato, "."),
                    (e.heavy_accent, "^"),
                    (e.accent, ">"),
                    (!harmonic.is_empty(), harmonic),
                    (e.tapping, "T"),
                    (e.slapping, "S"),
                    (e.popping, "P"),
                    (e.fade_in, "<"),
                    (e.tremolo_picking.is_some(), "TP"),
                    (e.trill.is_some(), "tr"),
                ];
                for (on, tag) in flags {
                    if on && !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        if !tags.is_empty() {
            p.text(Pos2::new(x, top - 4.0 * zoom), Align2::CENTER_BOTTOM, tags.join(" "), font(8.5, zoom), c.accent);
        }
        if let Some(chord) = &beat.chord {
            p.text(Pos2::new(x, top - 14.0 * zoom), Align2::CENTER_BOTTOM, &chord.name, font(10.0, zoom), c.fg);
        }
        if let Some(syllable) = lyrics.and_then(|m| m.get(&(measure_index, i))) {
            p.text(Pos2::new(x, bottom + 46.0 * zoom), Align2::CENTER_TOP, syllable, font(10.0, zoom), c.fg);
        }
        if let Some(text) = &beat.text {
            p.text(Pos2::new(x, bottom + 30.0 * zoom), Align2::CENTER_TOP, text, font(8.5, zoom), c.weak);
        }

        // Notas.
        for (vi, voice) in beat.voices.iter().enumerate() {
            let color = if vi == 0 { c.fg } else { c.voice2 };
            for n in &voice.notes {
                if n.string == 0 || n.string as usize > track.strings.len() {
                    continue;
                }
                let y = top + (n.string as f32 - 1.0) * gap;
                let text = if track.is_percussion {
                    drum_label(n.fret)
                } else if n.effect.dead {
                    "x".to_string()
                } else if n.tied || n.effect.ghost {
                    format!("({})", n.fret)
                } else if n.effect.harmonic.is_some() {
                    format!("<{}>", n.fret)
                } else {
                    n.fret.to_string()
                };
                let galley = p.layout_no_wrap(text, font(11.0, zoom), color);
                let r = Rect::from_center_size(Pos2::new(x, y), galley.size() + vec2(3.0, 0.0));
                // El fondo de la nota imita el resaltado del compás/pulso que suena.
                let bg = if current_beat == Some(i) {
                    c.bg.lerp_to_gamma(c.accent, 0.40)
                } else if cursor_tick.is_some() {
                    c.bg.lerp_to_gamma(c.accent, 0.13)
                } else {
                    c.bg
                };
                p.rect_filled(r, 2.0, bg);
                p.galley(r.center() - galley.size() / 2.0, galley, color);
                if let Some(bend) = &n.effect.bend {
                    draw_bend(p, c, bend, r.right(), y, gap, zoom);
                }
                if n.effect.vibrato {
                    draw_vibrato(p, c, r.right() + 2.0 * zoom, y, zoom);
                }
                if let Some(g) = n.effect.grace {
                    let label = if g.dead { "x".to_string() } else { g.fret.to_string() };
                    p.text(Pos2::new(r.left() - 3.0 * zoom, y - 4.0 * zoom), Align2::RIGHT_CENTER, label, font(8.0, zoom), c.accent);
                }
                // Siguiente nota en la misma cuerda (slides y ligaduras).
                let next_note = measure.beats.get(i + 1).zip(xs.get(i + 1)).and_then(|(b, &nx)| {
                    b.voices.iter().flat_map(|v| &v.notes).find(|m| m.string == n.string).map(|m| (m.fret, nx))
                });
                if let Some(kind) = n.effect.slide {
                    draw_slide(p, c, kind, n.fret, &r, y, next_note, zoom);
                }
                if n.effect.hammer {
                    if let Some((next_fret, nx)) = next_note {
                        draw_hammer(p, c, r.right(), nx - 7.0 * zoom, y - 7.0 * zoom, next_fret > n.fret, zoom);
                    }
                }
            }
        }

        // Barra de trémolo: curva sobre el pentagrama.
        if let Some(bar) = beat.voices.iter().flat_map(|v| &v.notes).find_map(|n| n.effect.tremolo_bar.as_ref()) {
            draw_tremolo_bar(p, c, bar, x - 4.0 * zoom, top - 26.0 * zoom, zoom);
        }

        // Ritmo bajo el pentagrama.
        let Some(voice) = main_voice(beat) else { continue };
        let stem_top = bottom + 10.0 * zoom;
        let stem_bottom = stem_top + 16.0 * zoom;
        let is_rest = voice.notes.is_empty();
        if is_rest {
            // Silencio: marca corta, más ancha cuanto más larga la duración.
            let w = match voice.duration.value {
                1 | 2 => 8.0,
                _ => 5.0,
            } * zoom;
            let y = stem_top + 8.0 * zoom;
            p.line_segment([Pos2::new(x - w, y), Pos2::new(x + w, y)], Stroke::new(2.5 * zoom, c.weak));
            continue;
        }
        if voice.duration.value >= 2 {
            p.line_segment([Pos2::new(x, stem_top), Pos2::new(x, stem_bottom)], Stroke::new(1.0, c.fg));
        } else {
            p.circle_stroke(Pos2::new(x, stem_top + 4.0 * zoom), 3.0 * zoom, Stroke::new(1.0, c.fg));
        }
        let lv = levels(beat);
        let prev = i.checked_sub(1).map(|j| levels(&measure.beats[j])).unwrap_or(0);
        let next = measure.beats.get(i + 1).map(levels).unwrap_or(0);
        for l in 1..=lv {
            let y = stem_bottom - (l - 1) as f32 * 3.5 * zoom;
            if next >= l && xs.get(i + 1).is_some() {
                p.line_segment([Pos2::new(x, y), Pos2::new(xs[i + 1], y)], Stroke::new(1.8 * zoom, c.fg));
            } else if prev < l {
                p.line_segment([Pos2::new(x, y), Pos2::new(x + 6.0 * zoom, y)], Stroke::new(1.8 * zoom, c.fg));
            } else if prev >= l && next < l {
                // El último de una barra que continúa desde el anterior ya fue dibujado por éste.
            }
        }
        let dots = if voice.duration.dotted { 1 } else if voice.duration.double_dotted { 2 } else { 0 };
        for d in 0..dots {
            p.circle_filled(Pos2::new(x + (5.0 + 4.0 * d as f32) * zoom, stem_bottom - 2.0 * zoom), 1.2 * zoom, c.fg);
        }
        if voice.duration.enters != 1 {
            p.text(
                Pos2::new(x, stem_bottom + 2.0 * zoom),
                Align2::CENTER_TOP,
                voice.duration.enters.to_string(),
                font(8.0, zoom),
                c.weak,
            );
        }
    }
}

/// Voz que define el ritmo del pulso (la primera con contenido).
fn main_voice(beat: &Beat) -> Option<&Voice> {
    beat.voices.iter().find(|v| !v.empty)
}

/// Curva de bend a la derecha de la nota, con la altura máxima escrita (½, 1, 1½...).
fn draw_bend(p: &egui::Painter, c: &Colors, bend: &Bend, x: f32, y: f32, gap: f32, zoom: f32) {
    let width = 22.0 * zoom;
    let per_semitone = gap * 0.8;
    let max = bend.max_semitones();
    let to_pos = |(pos, semis): (f32, f32)| Pos2::new(x + 2.0 * zoom + pos * width, y - semis * per_semitone);
    let pts: Vec<Pos2> = bend.points.iter().copied().map(to_pos).collect();
    if pts.len() >= 2 {
        p.add(egui::Shape::line(pts.clone(), Stroke::new(1.3 * zoom, c.accent)));
    }
    if max > 0.01 {
        let top = to_pos((1.0, max));
        // Altura en tonos: 1/4 tono = 0,5 semitono; 1 = bend completo (2 semitonos).
        let label = match (max * 2.0).round() as i32 {
            1 => "¼".to_string(),
            2 => "½".to_string(),
            3 => "¾".to_string(),
            4 => "1".to_string(),
            6 => "1½".to_string(),
            8 => "2".to_string(),
            n => format!("{}", n as f32 / 4.0),
        };
        p.text(top + vec2(2.0 * zoom, 0.0), Align2::LEFT_CENTER, label, font(8.5, zoom), c.accent);
    }
}

/// Vibrato: línea ondulada a la derecha de la nota.
fn draw_vibrato(p: &egui::Painter, c: &Colors, x: f32, y: f32, zoom: f32) {
    let n = 14;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let f = i as f32 / n as f32;
            Pos2::new(x + f * 20.0 * zoom, y - 7.0 * zoom + (f * std::f32::consts::TAU * 2.5).sin() * 2.0 * zoom)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.0 * zoom, c.accent)));
}

/// Slide: línea entre la nota y la siguiente de la misma cuerda, o un trazo corto de entrada/salida.
fn draw_slide(p: &egui::Painter, c: &Colors, kind: SlideKind, fret: u8, r: &Rect, y: f32, next: Option<(u8, f32)>, zoom: f32) {
    let stroke = Stroke::new(1.3 * zoom, c.accent);
    let d = 4.0 * zoom;
    let line = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], stroke);
    };
    match kind {
        SlideKind::Shift | SlideKind::Legato => {
            let (end_x, up) = match next {
                Some((next_fret, nx)) => (nx - 8.0 * zoom, next_fret >= fret),
                None => (r.right() + 11.0 * zoom, true),
            };
            let (y0, y1) = if up { (y + d, y - d) } else { (y - d, y + d) };
            line(Pos2::new(r.right() + 1.0, y0), Pos2::new(end_x, y1));
        }
        SlideKind::OutDown => line(Pos2::new(r.right() + 1.0, y - d), Pos2::new(r.right() + 11.0 * zoom, y + d)),
        SlideKind::OutUp => line(Pos2::new(r.right() + 1.0, y + d), Pos2::new(r.right() + 11.0 * zoom, y - d)),
        SlideKind::InBelow => line(Pos2::new(r.left() - 11.0 * zoom, y + d), Pos2::new(r.left() - 1.0, y - d)),
        SlideKind::InAbove => line(Pos2::new(r.left() - 11.0 * zoom, y - d), Pos2::new(r.left() - 1.0, y + d)),
    }
}

/// Ligadura de hammer-on / pull-off: arco sobre las dos notas con "H" o "P".
fn draw_hammer(p: &egui::Painter, c: &Colors, x0: f32, x1: f32, y: f32, hammer_on: bool, zoom: f32) {
    if x1 <= x0 + 2.0 {
        return;
    }
    let n = 12;
    let rise = 5.0 * zoom;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let f = i as f32 / n as f32;
            Pos2::new(x0 + (x1 - x0) * f, y - (1.0 - (2.0 * f - 1.0).powi(2)) * rise)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.0 * zoom, c.accent)));
    p.text(
        Pos2::new((x0 + x1) / 2.0, y - rise - 1.0),
        Align2::CENTER_BOTTOM,
        if hammer_on { "H" } else { "P" },
        font(8.0, zoom),
        c.accent,
    );
}

/// Barra de trémolo: polilínea de la altura (negativa = hacia abajo) durante el pulso.
fn draw_tremolo_bar(p: &egui::Painter, c: &Colors, bar: &Bend, x: f32, y: f32, zoom: f32) {
    let width = 28.0 * zoom;
    let per = 3.0 * zoom;
    let pts: Vec<Pos2> = bar.points.iter().map(|&(pos, semis)| Pos2::new(x + pos * width, y - semis * per)).collect();
    if pts.len() >= 2 {
        p.add(egui::Shape::line(pts, Stroke::new(1.2 * zoom, c.voice2)));
    }
    p.text(Pos2::new(x - 2.0 * zoom, y), Align2::RIGHT_CENTER, "tr.bar", font(7.5, zoom), c.voice2);
}

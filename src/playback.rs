//! Línea de tiempo y secuenciador. No depende de la salida de audio: `Engine::render`
//! produce muestras, de modo que se puede probar sin placa de sonido.
//!
//! Expresividad: las notas con bend, slide, vibrato o barra de trémolo se envían por un
//! canal MIDI auxiliar (para que el pitch bend no afecte a las demás notas de la pista).

use crate::model::*;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// Valor de `Event::track` para los clics del metrónomo.
pub const METRONOME_TRACK: u8 = 250;
/// Valor de la mezcla que significa "sin cambios".
pub const MIX_FILE: u8 = 255;
/// Envío de reverb predeterminado del sintetizador.
pub const DEFAULT_REVERB: u8 = 40;
const BEND_CENTER: i32 = 8192;
/// Rango de pitch bend configurado en los canales (semitonos hacia cada lado).
const BEND_RANGE: f32 = 12.0;

/// Un tramo de tiempo con tempo constante (los compases repetidos aparecen varias veces).
#[derive(Debug, Clone)]
pub struct Seg {
    pub t0: f64,
    pub t1: f64,
    pub measure: usize,
    /// Rango de ticks de la partitura que cubre este tramo.
    pub tick0: u64,
    pub tick1: u64,
    /// Tiempos del compás y duración de cada uno en ticks (para la cuenta regresiva).
    pub beats: u32,
    pub beat_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8 },
    Program(u8),
    /// Valor de 14 bits (8192 = centro).
    Bend(u16),
    /// Controlador MIDI (7 = volumen, 10 = balance).
    Control { cc: u8, value: u8 },
}

#[derive(Debug, Clone, Copy)]
pub struct Event {
    /// Segundos a velocidad 100 %.
    pub time: f64,
    pub channel: u8,
    pub track: u8,
    pub kind: Kind,
}

impl Event {
    fn priority(&self) -> u8 {
        match self.kind {
            Kind::NoteOff { .. } => 0,
            Kind::Program(_) | Kind::Control { .. } => 1,
            Kind::Bend(_) => 2,
            Kind::NoteOn { .. } => 3,
        }
    }
}

#[derive(Debug, Default)]
pub struct Timeline {
    pub events: Vec<Event>,
    pub segs: Vec<Seg>,
    /// Mensajes MIDI (canal, comando, dato1, dato2) a aplicar antes de empezar.
    pub setup: Vec<(u8, u8, u8, u8)>,
    pub total: f64,
    /// Canal MIDI principal de cada pista.
    pub channels: Vec<u8>,
}

/// Orden de reproducción de los compases con repeticiones y finales alternativos.
pub fn play_order(headers: &[MeasureHeader]) -> Vec<usize> {
    let mut order = Vec::new();
    let (mut i, mut start, mut pass) = (0usize, 0usize, 1i32);
    let mut jumped = false;
    let mut guard = 0;
    while i < headers.len() && guard < 100_000 {
        guard += 1;
        let h = &headers[i];
        if h.repeat_open && !jumped {
            start = i;
            pass = 1;
        }
        jumped = false;
        if h.repeat_alternative != 0 {
            let bit = (pass - 1).clamp(0, 7);
            if h.repeat_alternative & (1 << bit) == 0 {
                i += 1;
                continue;
            }
        }
        order.push(i);
        if h.repeat_close > 0 {
            if pass <= h.repeat_close {
                pass += 1;
                i = start;
                jumped = true;
                continue;
            }
            pass = 1;
            start = i + 1;
        }
        i += 1;
    }
    order
}

/// Velocidad MIDI a partir del código de dinámica de GP (1 = ppp ... 8 = fff).
fn velocity_of(dynamic: i32) -> i32 {
    if (1..=8).contains(&dynamic) { 16 * dynamic - 1 } else { 95 }
}

/// Semitonos sobre la cuerda al aire de un armónico natural, según el traste.
fn natural_harmonic_offset(fret: i32) -> i32 {
    match fret {
        12 => 12,
        7 | 19 => 19,
        5 | 24 => 24,
        4 | 9 | 16 => 28,
        3 | 6 | 10 | 13 => 31,
        2 | 14 => 34,
        _ => 12,
    }
}

/// Tramos de tempo constante dentro de un compás: (tick de inicio, segundos en ese tick, segundos por tick).
struct TempoMap {
    points: Vec<(u64, f64, f64)>,
}

impl TempoMap {
    fn new(h: &MeasureHeader, t0: f64) -> TempoMap {
        let spt = |bpm: i32| 60.0 / (bpm.clamp(20, 400) as f64 * QUARTER_TIME as f64);
        let mut points = vec![(h.start, t0, spt(h.tempo))];
        for &(tick, bpm) in &h.tempo_changes {
            let &(pt, pt_time, pt_spt) = points.last().unwrap();
            let tick = tick.max(pt);
            let time = pt_time + (tick - pt) as f64 * pt_spt;
            points.push((tick, time, spt(bpm)));
        }
        TempoMap { points }
    }

    fn secs(&self, tick: u64) -> f64 {
        let i = self.points.partition_point(|p| p.0 <= tick).saturating_sub(1);
        let (pt, time, spt) = self.points[i];
        time + tick.saturating_sub(pt) as f64 * spt
    }

    fn spt_at(&self, tick: u64) -> f64 {
        let i = self.points.partition_point(|p| p.0 <= tick).saturating_sub(1);
        self.points[i].2
    }
}

/// Nota ya resuelta a tiempos en segundos, antes de generar los eventos MIDI.
struct Spec {
    track: usize,
    string: u8,
    channel: u8,
    program: u8,
    volume: u8,
    pan: u8,
    percussion: bool,
    key: i32,
    start: f64,
    end: f64,
    velocity: i32,
    spt: f64,
    fx: Effects,
    /// No se ataca (la nota anterior se desliza hasta aquí en legato).
    silent: bool,
    /// Destino de una ligadura (hammer-on/pull-off): se ataca más suave.
    legato: bool,
    /// Desplazamiento en semitonos hasta la nota siguiente (slides).
    slide_target: Option<f32>,
}

/// Canales auxiliares libres para notas con modulación de altura.
struct ChannelPool {
    free_at: Vec<(u8, f64)>,
}

impl ChannelPool {
    fn take(&mut self, from: f64, until: f64) -> Option<u8> {
        let slot = self.free_at.iter_mut().find(|(_, t)| *t <= from)?;
        slot.1 = until;
        Some(slot.0)
    }
}

fn bend_value(semitones: f32) -> u16 {
    let v = BEND_CENTER as f32 + semitones / BEND_RANGE * BEND_CENTER as f32;
    v.round().clamp(0.0, 16383.0) as u16
}

/// Altura (en semitonos) de una curva en la fracción `x` (0..=1) de la nota.
fn curve_at(curve: &Bend, x: f32) -> f32 {
    let pts = &curve.points;
    if pts.is_empty() {
        return 0.0;
    }
    if x <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b.0 {
            let span = (b.0 - a.0).max(1e-6);
            return a.1 + (b.1 - a.1) * ((x - a.0) / span);
        }
    }
    pts[pts.len() - 1].1
}

impl Timeline {
    pub fn build(song: &Song) -> Timeline {
        let mut tl = Timeline::default();

        // Canales MIDI: la percusión usa el 9; el resto se reparte entre los otros 15.
        let melodic: Vec<u8> = (0..16u8).filter(|&c| c != 9).collect();
        let mut channels = Vec::with_capacity(song.tracks.len());
        let mut next = 0;
        for t in &song.tracks {
            if t.is_percussion {
                channels.push(9u8);
            } else {
                channels.push(melodic[next % melodic.len()]);
                next += 1;
                let ch = *channels.last().unwrap();
                tl.setup.push((ch, 0xC0, t.program.clamp(0, 127) as u8, 0));
                tl.setup.push((ch, 0xB0, 7, t.volume.min(127)));
                tl.setup.push((ch, 0xB0, 10, t.pan.min(127)));
            }
        }
        tl.channels = channels.clone();
        if let Some(t) = song.tracks.iter().find(|t| t.is_percussion) {
            tl.setup.push((9, 0xB0, 7, t.volume.min(127)));
            tl.setup.push((9, 0xB0, 10, t.pan.min(127)));
        }
        // Rango de pitch bend ±12 semitonos en todos los canales melódicos (RPN 0).
        for &c in &melodic {
            for (cc, v) in [(101u8, 0u8), (100, 0), (6, BEND_RANGE as u8), (38, 0), (101, 127), (100, 127)] {
                tl.setup.push((c, 0xB0, cc, v));
            }
        }
        let used = song.tracks.iter().filter(|t| !t.is_percussion).count().min(melodic.len());
        let mut pool = ChannelPool { free_at: melodic.iter().skip(used).map(|&c| (c, 0.0)).collect() };

        // ---------------------------------------------------------------- 1.ª pasada
        let mut specs: Vec<Spec> = Vec::new();
        let mut open: HashMap<(usize, u8), usize> = HashMap::new(); // (pista, cuerda) -> spec
        let mut time = 0.0f64;
        let mut metronome: Vec<Event> = Vec::new();
        for m in play_order(&song.headers) {
            let h = &song.headers[m];
            let length = h.length();
            let tm = TempoMap::new(h, time);
            let measure_end = tm.secs(h.start + length);

            // Tramos de tempo constante para ubicar la posición.
            for (i, &(tick, t, _)) in tm.points.iter().enumerate() {
                let (tick1, t1) = match tm.points.get(i + 1) {
                    Some(&(nt, nt_time, _)) => (nt, nt_time),
                    None => (h.start + length, measure_end),
                };
                if tick1 > tick {
                    tl.segs.push(Seg { t0: t, t1, measure: m, tick0: tick, tick1, beats: h.numerator as u32, beat_ticks: QUARTER_TIME * 4 / h.denominator.max(1) as u64 });
                }
            }

            // Metrónomo: un clic por tiempo del compás.
            let beat_ticks = QUARTER_TIME * 4 / h.denominator.max(1) as u64;
            for k in 0..h.numerator as u64 {
                let t = tm.secs(h.start + k * beat_ticks);
                let (key, velocity) = if k == 0 { (76, 115) } else { (77, 90) };
                metronome.push(Event { time: t, channel: 9, track: METRONOME_TRACK, kind: Kind::NoteOn { key, velocity } });
                metronome.push(Event { time: t + 0.05, channel: 9, track: METRONOME_TRACK, kind: Kind::NoteOff { key } });
            }

            for (ti, track) in song.tracks.iter().enumerate() {
                let Some(measure) = track.measures.get(m) else { continue };
                for beat in &measure.beats {
                    for voice in beat.voices.iter().filter(|v| !v.empty) {
                        // Swing ("triplet feel"): las corcheas (o semicorcheas) por pares se tocan 2:1.
                        let mut offset = beat.start.saturating_sub(h.start);
                        let mut dur_ticks = voice.duration.time();
                        let plain = !voice.duration.dotted && !voice.duration.double_dotted && voice.duration.enters == 1;
                        match (h.triplet_feel, voice.duration.value) {
                            (TripletFeel::Eighth, 8) if plain => match offset % 960 {
                                0 => dur_ticks = 640,
                                480 => {
                                    offset += 160;
                                    dur_ticks = 320;
                                }
                                _ => {}
                            },
                            (TripletFeel::Sixteenth, 16) if plain => match offset % 480 {
                                0 => dur_ticks = 320,
                                240 => {
                                    offset += 80;
                                    dur_ticks = 160;
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                        let start_tick = h.start + offset;
                        let start = tm.secs(start_tick);
                        let dur = tm.secs(start_tick + dur_ticks) - start;
                        let spt = tm.spt_at(start_tick);

                        for note in &voice.notes {
                            let open_key = if track.is_percussion {
                                0
                            } else {
                                match track.strings.get(note.string as usize - 1) {
                                    Some(&s) => s + track.capo,
                                    None => continue,
                                }
                            };
                            let mut key = if track.is_percussion { note.fret as i32 } else { open_key + note.fret as i32 };
                            if !track.is_percussion {
                                key = match note.effect.harmonic {
                                    Some(Harmonic::Natural) => open_key + natural_harmonic_offset(note.fret as i32),
                                    Some(_) => key + 12,
                                    None => key,
                                };
                            }
                            if !(0..=127).contains(&key) {
                                continue;
                            }
                            let e = &note.effect;
                            let mut velocity = velocity_of(note.velocity);
                            let mut length = dur;
                            if e.ghost {
                                velocity = velocity * 6 / 10;
                            }
                            if e.accent {
                                velocity += 15;
                            }
                            if e.heavy_accent {
                                velocity += 25;
                            }
                            if e.palm_mute {
                                length *= 0.6;
                                velocity = velocity * 8 / 10;
                            }
                            if e.staccato {
                                length *= 0.5;
                            }
                            if e.dead {
                                length = length.min(0.08);
                                velocity = velocity * 7 / 10;
                            }
                            if e.slapping {
                                velocity += 18;
                                length *= 0.6;
                            }
                            if e.popping {
                                velocity += 22;
                                length *= 0.5;
                            }
                            if e.tapping {
                                velocity = velocity * 9 / 10;
                            }
                            if e.let_ring {
                                length = length.max(measure_end - start).min(6.0);
                            }
                            let key_id = (ti, note.string);
                            // Una nota ligada alarga la anterior en la misma cuerda.
                            if note.tied {
                                if let Some(&idx) = open.get(&key_id) {
                                    let prev: &mut Spec = &mut specs[idx];
                                    if (prev.end - start).abs() < 0.003 && prev.key == key {
                                        prev.end = start + length.max(0.02);
                                        continue;
                                    }
                                }
                            }
                            let mut start_note = start;
                            // Notas de adorno.
                            if let (Some(g), false) = (e.grace, track.is_percussion) {
                                let gdur = (g.ticks as f64 * spt).clamp(0.02, 0.25);
                                let (gs, ge) = if g.on_beat {
                                    start_note = (start + gdur).min(start + length * 0.6);
                                    (start, start_note)
                                } else {
                                    (start - gdur, start)
                                };
                                let gkey = open_key + g.fret as i32;
                                if (0..=127).contains(&gkey) {
                                    specs.push(Spec {
                                        track: ti,
                                        string: note.string,
                                        channel: channels[ti],
                                        program: track.program.clamp(0, 127) as u8,
                                        volume: track.volume,
                                        pan: track.pan,
                                        percussion: false,
                                        key: gkey,
                                        start: gs.max(0.0),
                                        end: ge.max(gs + 0.015),
                                        velocity: if g.dead { 45 } else { (velocity * 85 / 100).clamp(1, 127) },
                                        spt,
                                        fx: Effects::default(),
                                        silent: false,
                                        legato: false,
                                        slide_target: None,
                                    });
                                }
                            }
                            specs.push(Spec {
                                track: ti,
                                string: note.string,
                                channel: channels[ti],
                                program: track.program.clamp(0, 127) as u8,
                                volume: track.volume,
                                pan: track.pan,
                                percussion: track.is_percussion,
                                key,
                                start: start_note,
                                end: start_note + (length - (start_note - start)).max(0.02),
                                velocity: velocity.clamp(1, 127),
                                spt,
                                fx: note.effect.clone(),
                                silent: false,
                                legato: false,
                                slide_target: None,
                            });
                            open.insert(key_id, specs.len() - 1);
                        }
                    }
                }
            }
            time = measure_end;
        }
        tl.total = time;

        // ---------------------------------------------------------------- 2.ª pasada
        // Relaciones entre notas consecutivas de una misma cuerda: ligaduras y slides.
        let mut by_string: HashMap<(usize, u8), Vec<usize>> = HashMap::new();
        for (i, s) in specs.iter().enumerate() {
            if !s.percussion {
                by_string.entry((s.track, s.string)).or_default().push(i);
            }
        }
        for list in by_string.values_mut() {
            list.sort_by(|&a, &b| specs[a].start.total_cmp(&specs[b].start));
            for w in list.windows(2) {
                let (a, b) = (w[0], w[1]);
                let gap = specs[b].start - specs[a].end;
                if gap > 0.35 {
                    continue;
                }
                if specs[a].fx.hammer {
                    specs[b].legato = true;
                    specs[a].end = specs[a].end.max(specs[b].start);
                }
                if let Some(kind @ (SlideKind::Shift | SlideKind::Legato)) = specs[a].fx.slide {
                    specs[a].slide_target = Some((specs[b].key - specs[a].key) as f32);
                    if kind == SlideKind::Legato {
                        specs[b].silent = true;
                        let b_end = specs[b].end;
                        specs[a].end = specs[a].end.max(b_end);
                    }
                }
            }
        }

        // ---------------------------------------------------------------- 3.ª pasada
        let mut events: Vec<Event> = Vec::new();
        let mut order: Vec<usize> = (0..specs.len()).collect();
        order.sort_by(|&a, &b| specs[a].start.total_cmp(&specs[b].start));
        for i in order {
            let s = &specs[i];
            if s.silent {
                continue;
            }
            emit_note(s, &mut pool, &mut events);
        }
        events.extend(metronome);
        events.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.priority().cmp(&b.priority())));
        tl.events = events;
        tl
    }

    /// Compás y tick absoluto de la partitura que suenan en el instante `t`.
    /// Copia con todas las notas (salvo la percusión y el metrónomo) desplazadas `semitones` semitonos.
    pub fn transposed(&self, semitones: i32) -> Timeline {
        let shift = |key: u8| (key as i32 + semitones).clamp(0, 127) as u8;
        let events = self
            .events
            .iter()
            .map(|e| {
                let mut e = *e;
                if e.channel != 9 && e.track != METRONOME_TRACK {
                    e.kind = match e.kind {
                        Kind::NoteOn { key, velocity } => Kind::NoteOn { key: shift(key), velocity },
                        Kind::NoteOff { key } => Kind::NoteOff { key: shift(key) },
                        other => other,
                    };
                }
                e
            })
            .collect();
        Timeline { events, segs: self.segs.clone(), setup: self.setup.clone(), total: self.total, channels: self.channels.clone() }
    }

    pub fn position(&self, t: f64) -> Option<(usize, u64)> {
        let i = self.segs.partition_point(|s| s.t1 <= t).min(self.segs.len().checked_sub(1)?);
        let s = &self.segs[i];
        let f = if s.t1 > s.t0 { ((t - s.t0) / (s.t1 - s.t0)).clamp(0.0, 1.0) } else { 0.0 };
        Some((s.measure, s.tick0 + ((s.tick1 - s.tick0) as f64 * f) as u64))
    }

    /// Cuenta regresiva para empezar en `t`: (cantidad de tiempos, segundos por tiempo).
    pub fn count_in(&self, t: f64) -> Option<(u32, f64)> {
        let i = self.segs.partition_point(|s| s.t1 <= t).min(self.segs.len().checked_sub(1)?);
        let s = &self.segs[i];
        let ticks = (s.tick1 - s.tick0).max(1) as f64;
        Some((s.beats.max(1), s.beat_ticks as f64 * (s.t1 - s.t0) / ticks))
    }

    /// Instante en que empieza por primera vez el compás `measure`.
    pub fn time_of_measure(&self, measure: usize) -> Option<f64> {
        self.segs.iter().find(|s| s.measure == measure).map(|s| s.t0)
    }

    /// Instante en que termina la primera vez que suena el compás `measure`.
    pub fn first_end_of_measure(&self, measure: usize) -> Option<f64> {
        let i = self.segs.iter().position(|s| s.measure == measure)?;
        let mut end = self.segs[i].t1;
        for s in &self.segs[i + 1..] {
            if s.measure == measure && (s.t0 - end).abs() < 1e-6 {
                end = s.t1;
            } else {
                break;
            }
        }
        Some(end)
    }

    /// Instante en que termina por última vez el compás `measure`.
    pub fn end_of_measure(&self, measure: usize) -> Option<f64> {
        self.segs.iter().rev().find(|s| s.measure == measure).map(|s| s.t1)
    }

    /// Archivo MIDI estándar (formato 0) con las pistas activas según `mask`.
    pub fn to_midi(&self, mask: u64, with_metronome: bool) -> Vec<u8> {
        const PPQ: f64 = 960.0;
        // Tempo fijo de 120 BPM: 1 segundo = 2 negras.
        let to_ticks = |t: f64| (t.max(0.0) * 2.0 * PPQ).round() as u64;
        let mut track: Vec<(u64, u8, Vec<u8>)> = Vec::new();
        for &(ch, cmd, d1, d2) in &self.setup {
            let data = if cmd == 0xC0 { vec![cmd | ch, d1] } else { vec![cmd | ch, d1, d2] };
            track.push((0, 0, data));
        }
        for ev in &self.events {
            let enabled = if ev.track == METRONOME_TRACK {
                with_metronome
            } else {
                ev.track >= 64 || mask & (1u64 << ev.track) != 0
            };
            let data = match ev.kind {
                Kind::NoteOn { key, velocity } if enabled => vec![0x90 | ev.channel, key, velocity],
                Kind::NoteOn { .. } => continue,
                Kind::NoteOff { key } => vec![0x80 | ev.channel, key, 0],
                Kind::Program(p) => vec![0xC0 | ev.channel, p],
                Kind::Bend(v) => vec![0xE0 | ev.channel, (v & 0x7f) as u8, ((v >> 7) & 0x7f) as u8],
                Kind::Control { cc, value } => vec![0xB0 | ev.channel, cc, value],
            };
            track.push((to_ticks(ev.time), ev.priority(), data));
        }
        track.sort_by_key(|e| (e.0, e.1));

        let mut body = Vec::new();
        let vlq = |out: &mut Vec<u8>, mut v: u64| {
            let mut bytes = vec![(v & 0x7f) as u8];
            v >>= 7;
            while v > 0 {
                bytes.push(((v & 0x7f) as u8) | 0x80);
                v >>= 7;
            }
            bytes.reverse();
            out.extend(bytes);
        };
        // Tempo 120 BPM = 500000 µs por negra.
        body.extend([0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        let mut last = 0;
        for (tick, _, data) in &track {
            vlq(&mut body, tick - last);
            last = *tick;
            body.extend(data);
        }
        body.extend([0x00, 0xFF, 0x2F, 0x00]);

        let mut out = Vec::new();
        out.extend(b"MThd");
        out.extend(6u32.to_be_bytes());
        out.extend(0u16.to_be_bytes()); // formato 0
        out.extend(1u16.to_be_bytes());
        out.extend((PPQ as u16).to_be_bytes());
        out.extend(b"MTrk");
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
        out
    }
}

/// Genera los eventos de una nota, con su modulación de altura si la tiene.
fn emit_note(s: &Spec, pool: &mut ChannelPool, out: &mut Vec<Event>) {
    let dur = (s.end - s.start).max(0.02);
    let fx = &s.fx;
    let track = s.track.min(249) as u8;
    let ev = |time: f64, channel: u8, kind: Kind| Event { time, channel, track, kind };
    let mut velocity = s.velocity;
    if s.legato {
        velocity = velocity * 75 / 100;
    }

    // ¿Hay que modular la altura?
    let slide_shape = match fx.slide {
        Some(SlideKind::OutDown) => Some((-4.0f32, 0.35f64, false)), // (semitonos, fracción, al inicio)
        Some(SlideKind::OutUp) => Some((4.0, 0.35, false)),
        Some(SlideKind::InBelow) => Some((-4.0, 0.25, true)),
        Some(SlideKind::InAbove) => Some((4.0, 0.25, true)),
        _ => None,
    };
    let modulated = !s.percussion
        && (fx.bend.is_some() || fx.tremolo_bar.is_some() || fx.vibrato || fx.fade_in || slide_shape.is_some() || s.slide_target.is_some());

    let mut channel = s.channel;
    let mut dedicated = false;
    if modulated {
        if let Some(c) = pool.take(s.start - 0.01, s.end + 0.3) {
            channel = c;
            dedicated = true;
            out.push(ev(s.start - 0.004, c, Kind::Program(s.program)));
            out.push(ev(s.start - 0.004, c, Kind::Control { cc: 7, value: s.volume.min(127) }));
            out.push(ev(s.start - 0.004, c, Kind::Control { cc: 10, value: s.pan.min(127) }));
        }
    }

    // Altura en función del tiempo relativo a la nota.
    let pitch_at = |t: f64| -> f32 {
        let x = ((t - s.start) / dur).clamp(0.0, 1.0) as f32;
        let mut semis = 0.0;
        if let Some(b) = &fx.bend {
            semis += curve_at(b, x);
        }
        if let Some(b) = &fx.tremolo_bar {
            semis += curve_at(b, x);
        }
        if let Some((amount, fraction, at_start)) = slide_shape {
            let f = fraction as f32;
            semis += if at_start {
                if x < f { amount * (1.0 - x / f) } else { 0.0 }
            } else if x > 1.0 - f {
                amount * (x - (1.0 - f)) / f
            } else {
                0.0
            };
        }
        if let Some(target) = s.slide_target {
            // Se desliza durante el último 40 % de la nota y se mantiene.
            let f = 0.4f32;
            semis += if x > 1.0 - f { target * (x - (1.0 - f)) / f } else { 0.0 };
        }
        if fx.vibrato && !s.percussion {
            // LFO de ~5,5 Hz que entra tras 0,15 s.
            let rel = (t - s.start - 0.15).max(0.0);
            let depth = (rel / 0.25).min(1.0) as f32;
            semis += 0.28 * depth * (std::f64::consts::TAU * 5.5 * rel).sin() as f32;
        }
        semis
    };

    if modulated {
        let mut last = i32::MIN;
        let mut last_expr = -1i32;
        let mut t = s.start - 0.002;
        let step = 0.012;
        while t <= s.end + 1e-9 {
            // Fade-in: rampa de expresión (CC11) de casi nada a todo durante la nota.
            if fx.fade_in {
                let x = ((t - s.start) / dur).clamp(0.0, 1.0);
                let e = (12.0 + x * 115.0).round() as i32;
                if (e - last_expr).abs() >= 2 {
                    out.push(ev(t, channel, Kind::Control { cc: 11, value: e.clamp(0, 127) as u8 }));
                    last_expr = e;
                }
            }
            let v = bend_value(pitch_at(t.max(s.start))) as i32;
            if (v - last).abs() >= 3 {
                out.push(ev(t, channel, Kind::Bend(v as u16)));
                last = v;
            }
            t += step;
        }
        // Se vuelve al centro cuando termina el release.
        out.push(ev(s.end + 0.28, channel, Kind::Bend(BEND_CENTER as u16)));
        if fx.fade_in {
            out.push(ev(s.end + 0.28, channel, Kind::Control { cc: 11, value: 127 }));
        }
        let _ = dedicated;
    }

    let key = s.key.clamp(0, 127) as u8;
    let vel = velocity.clamp(1, 127) as u8;

    if let (Some(tr), false) = (fx.trill, s.percussion) {
        // Alterna la nota con la del trino.
        let step = (tr.ticks as f64 * s.spt).max(0.03);
        let alt = (s.key + tr.semitones).clamp(0, 127) as u8;
        let mut t = s.start;
        let mut high = false;
        while t < s.end - 1e-6 {
            let k = if high { alt } else { key };
            let off = (t + step * 0.96).min(s.end);
            out.push(ev(t, channel, Kind::NoteOn { key: k, velocity: vel }));
            out.push(ev(off, channel, Kind::NoteOff { key: k }));
            t += step;
            high = !high;
        }
    } else if let Some(value) = fx.tremolo_picking {
        // Repite la nota con la figura indicada.
        let step = (QUARTER_TIME as f64 * 4.0 / value as f64 * s.spt).max(0.025);
        let mut t = s.start;
        let mut n = 0;
        while t < s.end - 1e-6 {
            let off = (t + step * 0.92).min(s.end);
            let v = if n == 0 { vel } else { (vel as i32 * 85 / 100).clamp(1, 127) as u8 };
            out.push(ev(t, channel, Kind::NoteOn { key, velocity: v }));
            out.push(ev(off, channel, Kind::NoteOff { key }));
            t += step;
            n += 1;
        }
    } else {
        out.push(ev(s.start, channel, Kind::NoteOn { key, velocity: vel }));
        out.push(ev(s.end, channel, Kind::NoteOff { key }));
    }
}

pub enum Cmd {
    SetTimeline(Arc<Timeline>),
    /// Cambia la línea de tiempo sin mover la posición ni detener la reproducción (p. ej. al transponer).
    ReplaceTimeline(Arc<Timeline>),
    Play,
    /// Empieza tras `beats` clics separados por `beat_secs` segundos de la canción.
    PlayCountIn { beats: u32, beat_secs: f64 },
    Pause,
    Stop,
    Seek(f64),
    SetSpeed(f32),
    SetVolume(f32),
}

/// Estado compartido con la interfaz.
pub struct Shared {
    /// Posición en segundos (bits de un f64).
    pub pos: AtomicU64,
    pub playing: AtomicBool,
    /// Bit i activo = la pista i suena.
    pub mask: AtomicU64,
    pub metronome: AtomicBool,
    pub loop_on: AtomicBool,
    /// Límites del bucle en segundos (bits de un f64).
    pub loop_start: AtomicU64,
    pub loop_end: AtomicU64,
    /// Ganancia de cada pista en porcentaje (100 = sin cambio), hasta 64 pistas.
    pub gains: Vec<AtomicU8>,
    /// Velocidad de reproducción en porcentaje; el motor la cambia solo con el entrenador.
    pub speed_pct: AtomicU32,
    pub trainer_on: AtomicBool,
    pub trainer_step: AtomicU32,
    pub trainer_max: AtomicU32,
    /// Mezcla por pista; 255 = el valor del archivo (o el predeterminado, en el reverb).
    pub mix_pan: Vec<AtomicU8>,
    pub mix_program: Vec<AtomicU8>,
    pub mix_reverb: Vec<AtomicU8>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            pos: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            mask: AtomicU64::new(0),
            metronome: AtomicBool::new(false),
            loop_on: AtomicBool::new(false),
            loop_start: AtomicU64::new(0),
            loop_end: AtomicU64::new(0),
            gains: (0..64).map(|_| AtomicU8::new(100)).collect(),
            speed_pct: AtomicU32::new(100),
            trainer_on: AtomicBool::new(false),
            trainer_step: AtomicU32::new(5),
            trainer_max: AtomicU32::new(100),
            mix_pan: (0..64).map(|_| AtomicU8::new(MIX_FILE)).collect(),
            mix_program: (0..64).map(|_| AtomicU8::new(MIX_FILE)).collect(),
            mix_reverb: (0..64).map(|_| AtomicU8::new(MIX_FILE)).collect(),
        }
    }
}

impl Shared {
    pub fn position(&self) -> f64 {
        f64::from_bits(self.pos.load(Ordering::Relaxed))
    }

    pub fn set_loop(&self, start: f64, end: f64) {
        self.loop_start.store(start.to_bits(), Ordering::Relaxed);
        self.loop_end.store(end.to_bits(), Ordering::Relaxed);
    }

    pub fn loop_range(&self) -> (f64, f64) {
        (f64::from_bits(self.loop_start.load(Ordering::Relaxed)), f64::from_bits(self.loop_end.load(Ordering::Relaxed)))
    }
}

pub struct Engine {
    synth: Synthesizer,
    rx: Receiver<Cmd>,
    shared: Arc<Shared>,
    timeline: Arc<Timeline>,
    sample_rate: f64,
    pos: f64,
    next: usize,
    playing: bool,
    speed: f64,
    lead: Option<Lead>,
    /// Última mezcla (pan, programa, reverb) enviada al sintetizador por pista.
    applied: Vec<[u8; 3]>,
}

/// Cuenta regresiva en curso.
struct Lead {
    beats: u32,
    beat_secs: f64,
    elapsed: f64,
    next: u32,
    /// Clic encendido que hay que apagar: (instante, tecla).
    off: Option<(f64, i32)>,
}

const BLOCK: usize = 64;

impl Engine {
    pub fn new(font: Arc<SoundFont>, sample_rate: u32, shared: Arc<Shared>, rx: Receiver<Cmd>) -> Result<Engine, String> {
        let settings = SynthesizerSettings::new(sample_rate as i32);
        let synth = Synthesizer::new(&font, &settings).map_err(|e| format!("{e}"))?;
        Ok(Engine {
            synth,
            rx,
            shared,
            timeline: Arc::new(Timeline::default()),
            sample_rate: sample_rate as f64,
            pos: 0.0,
            next: 0,
            playing: false,
            speed: 1.0,
            lead: None,
            applied: vec![[MIX_FILE; 3]; 64],
        })
    }

    /// Silencia todo y deja el pitch bend de cada canal en el centro.
    fn silence(&mut self, immediate: bool) {
        self.synth.note_off_all(immediate);
        for ch in 0..16 {
            self.synth.process_midi_message(ch, 0xE0, 0, 64);
        }
    }

    /// Envía la configuración inicial de los canales (programa, volumen, pan) y restablece la mezcla aplicada.
    fn apply_setup(&mut self) {
        for ch in 0..16 {
            self.synth.process_midi_message(ch, 0xB0, 91, DEFAULT_REVERB as i32);
        }
        for &(ch, cmd, d1, d2) in &self.timeline.setup {
            self.synth.process_midi_message(ch as i32, cmd as i32, d1 as i32, d2 as i32);
        }
        // La próxima sincronización vuelve a aplicar lo que el usuario haya cambiado.
        self.applied = vec![[MIX_FILE; 3]; 64];
    }

    /// Valor del archivo para un canal (de la configuración inicial).
    fn file_value(&self, ch: u8, cmd: u8, cc: u8) -> Option<u8> {
        self.timeline.setup.iter().find(|&&(c, k, d1, _)| c == ch && k == cmd && (cmd == 0xC0 || d1 == cc)).map(|&(_, _, d1, d2)| if cmd == 0xC0 { d1 } else { d2 })
    }

    /// Aplica los cambios de mezcla (instrumento, paneo, reverb) hechos desde la interfaz.
    fn sync_mix(&mut self) {
        let n = self.timeline.channels.len().min(64);
        for t in 0..n {
            let cur = [
                self.shared.mix_pan[t].load(Ordering::Relaxed),
                self.shared.mix_program[t].load(Ordering::Relaxed),
                self.shared.mix_reverb[t].load(Ordering::Relaxed),
            ];
            if cur == self.applied[t] {
                continue;
            }
            let ch = self.timeline.channels[t];
            if cur[0] != self.applied[t][0] {
                let v = if cur[0] != MIX_FILE { Some(cur[0]) } else { self.file_value(ch, 0xB0, 10) };
                if let Some(v) = v {
                    self.synth.process_midi_message(ch as i32, 0xB0, 10, v as i32);
                }
            }
            if cur[1] != self.applied[t][1] && ch != 9 {
                let v = if cur[1] != MIX_FILE { Some(cur[1]) } else { self.file_value(ch, 0xC0, 0) };
                if let Some(v) = v {
                    self.synth.process_midi_message(ch as i32, 0xC0, v as i32, 0);
                }
            }
            if cur[2] != self.applied[t][2] {
                let v = if cur[2] != MIX_FILE { cur[2] } else { DEFAULT_REVERB };
                self.synth.process_midi_message(ch as i32, 0xB0, 91, v as i32);
            }
            self.applied[t] = cur;
        }
    }

    fn seek_to(&mut self, t: f64) {
        self.pos = t.clamp(0.0, self.timeline.total);
        self.next = self.timeline.events.partition_point(|e| e.time < self.pos);
        self.silence(true);
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::SetTimeline(tl) => {
                self.timeline = tl;
                self.silence(true);
                self.apply_setup();
                self.pos = 0.0;
                self.next = 0;
                self.playing = false;
            }
            Cmd::ReplaceTimeline(tl) => {
                let (pos, playing) = (self.pos, self.playing);
                self.timeline = tl;
                self.silence(true);
                self.apply_setup();
                self.seek_to(pos);
                self.playing = playing;
            }
            Cmd::Play => {
                if self.pos >= self.timeline.total {
                    self.pos = 0.0;
                    self.next = 0;
                }
                self.lead = None;
                self.playing = true;
            }
            Cmd::PlayCountIn { beats, beat_secs } => {
                if self.pos >= self.timeline.total {
                    self.pos = 0.0;
                    self.next = 0;
                }
                self.lead = Some(Lead { beats: beats.clamp(1, 16), beat_secs: beat_secs.clamp(0.05, 5.0), elapsed: 0.0, next: 0, off: None });
                self.playing = true;
            }
            Cmd::Pause => {
                self.lead = None;
                self.playing = false;
                self.synth.note_off_all(false);
            }
            Cmd::Stop => {
                self.lead = None;
                self.playing = false;
                self.pos = 0.0;
                self.next = 0;
                self.silence(true);
            }
            Cmd::Seek(t) => self.seek_to(t),
            Cmd::SetSpeed(s) => {
                self.speed = s.clamp(0.1, 4.0) as f64;
                self.shared.speed_pct.store((self.speed * 100.0).round() as u32, Ordering::Relaxed);
            }
            Cmd::SetVolume(v) => self.synth.set_master_volume(v.clamp(0.0, 2.0)),
        }
    }

    fn fire(&mut self, ev: Event, mask: u64, metronome: bool) {
        let ch = ev.channel as i32;
        match ev.kind {
            Kind::NoteOn { key, velocity } => {
                let enabled = if ev.track == METRONOME_TRACK { metronome } else { ev.track >= 64 || mask & (1u64 << ev.track) != 0 };
                if enabled {
                    // Ganancia por pista (mixer de la interfaz), en porcentaje.
                    let gain = self.shared.gains.get(ev.track as usize).map(|g| g.load(Ordering::Relaxed) as i32).unwrap_or(100);
                    let v = (velocity as i32 * gain / 100).clamp(1, 127);
                    self.synth.note_on(ch, key as i32, v);
                }
            }
            Kind::NoteOff { key } => self.synth.note_off(ch, key as i32),
            Kind::Program(p) => {
                let over = self.shared.mix_program.get(ev.track as usize).map(|a| a.load(Ordering::Relaxed)).unwrap_or(MIX_FILE);
                self.synth.process_midi_message(ch, 0xC0, if over != MIX_FILE { over } else { p } as i32, 0)
            }
            Kind::Bend(v) => self.synth.process_midi_message(ch, 0xE0, (v & 0x7f) as i32, ((v >> 7) & 0x7f) as i32),
            Kind::Control { cc, value } => {
                let over = if cc == 10 { self.shared.mix_pan.get(ev.track as usize).map(|a| a.load(Ordering::Relaxed)).unwrap_or(MIX_FILE) } else { MIX_FILE };
                self.synth.process_midi_message(ch, 0xB0, cc as i32, if over != MIX_FILE { over } else { value } as i32)
            }
        }
    }

    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.handle(cmd);
        }
        self.sync_mix();
        let n = left.len().min(right.len());
        let mut done = 0;
        while done < n {
            let chunk = (n - done).min(BLOCK);
            if self.playing && self.lead.is_some() {
                // Cuenta regresiva: solo suenan los clics; la canción no avanza.
                let dt = chunk as f64 / self.sample_rate * self.speed;
                let mut finished = false;
                if let Some(lead) = self.lead.as_mut() {
                    lead.elapsed += dt;
                    if let Some((at, key)) = lead.off {
                        if lead.elapsed >= at {
                            self.synth.note_off(9, key);
                            lead.off = None;
                        }
                    }
                    if lead.next < lead.beats && lead.elapsed >= lead.next as f64 * lead.beat_secs {
                        let (key, vel) = if lead.next == 0 { (76, 120) } else { (77, 100) };
                        self.synth.note_on(9, key, vel);
                        lead.off = Some((lead.elapsed + 0.06, key));
                        lead.next += 1;
                    }
                    finished = lead.next >= lead.beats && lead.elapsed >= lead.beats as f64 * lead.beat_secs;
                }
                if finished {
                    self.lead = None;
                }
            } else if self.playing {
                let end = self.pos + chunk as f64 / self.sample_rate * self.speed;
                let mask = self.shared.mask.load(Ordering::Relaxed);
                let metronome = self.shared.metronome.load(Ordering::Relaxed);
                while let Some(ev) = self.timeline.events.get(self.next).copied() {
                    if ev.time >= end {
                        break;
                    }
                    self.next += 1;
                    self.fire(ev, mask, metronome);
                }
                self.pos = end;
                if self.shared.loop_on.load(Ordering::Relaxed) {
                    let (a, b) = self.shared.loop_range();
                    if b > a && self.pos >= b {
                        self.seek_to(a);
                        // Entrenador de velocidad: más rápido en cada vuelta hasta el máximo.
                        if self.shared.trainer_on.load(Ordering::Relaxed) {
                            let step = self.shared.trainer_step.load(Ordering::Relaxed);
                            let max = self.shared.trainer_max.load(Ordering::Relaxed);
                            let pct = ((self.speed * 100.0).round() as u32).saturating_add(step).min(max.max(30));
                            self.speed = pct as f64 / 100.0;
                            self.shared.speed_pct.store(pct, Ordering::Relaxed);
                        }
                    }
                }
                if self.pos >= self.timeline.total + 1.0 {
                    // Pasó la cola de release: termina y vuelve al inicio.
                    self.playing = false;
                    self.pos = 0.0;
                    self.next = 0;
                    self.silence(true);
                }
            }
            self.synth.render(&mut left[done..done + chunk], &mut right[done..done + chunk]);
            done += chunk;
        }
        self.shared.pos.store(self.pos.to_bits(), Ordering::Relaxed);
        self.shared.playing.store(self.playing, Ordering::Relaxed);
    }
}

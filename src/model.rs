//! Modelo interno de una partitura, independiente del formato de archivo.

/// Ticks por negra (misma convención que TuxGuitar).
pub const QUARTER_TIME: u64 = 960;

#[derive(Debug, Clone, Default)]
pub struct Song {
    pub format: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub album: String,
    pub words: String,
    pub music: String,
    pub copyright: String,
    pub tab_by: String,
    pub instructions: String,
    pub comments: String,
    /// Letra de la canción (GP4/GP5), si la hay.
    pub lyrics: Option<Lyrics>,
    /// Tempo inicial (BPM).
    pub tempo: i32,
    pub headers: Vec<MeasureHeader>,
    pub tracks: Vec<Track>,
}

/// Letra: el texto se reparte por sílabas entre los pulsos con notas de `track`, desde `from_measure`.
#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub track: usize,
    pub from_measure: usize,
    pub text: String,
}

impl Lyrics {
    /// Sílabas: se separa por espacios y guiones (el guion se conserva al final de la sílaba).
    pub fn syllables(&self) -> Vec<String> {
        let mut out = Vec::new();
        for word in self.text.split_whitespace() {
            let mut cur = String::new();
            for ch in word.chars() {
                cur.push(ch);
                if ch == '-' {
                    out.push(std::mem::take(&mut cur));
                }
            }
            if !cur.is_empty() {
                out.push(cur);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct MeasureHeader {
    pub number: usize,
    /// Inicio en ticks (el compás 1 empieza en `QUARTER_TIME`).
    pub start: u64,
    pub numerator: u8,
    pub denominator: u8,
    pub repeat_open: bool,
    /// Número de repeticiones (0 = no cierra repetición).
    pub repeat_close: i32,
    /// Máscara de bits de finales alternativos (bit0 = "1.", bit1 = "2."...).
    pub repeat_alternative: u8,
    pub marker: Option<String>,
    /// Armadura: -7..=-1 bemoles, 0 Do, 1..=7 sostenidos.
    pub key_signature: i8,
    /// Tempo al inicio del compás (BPM).
    pub tempo: i32,
    /// Cambios de tempo dentro del compás: (tick absoluto, BPM), ordenados.
    pub tempo_changes: Vec<(u64, i32)>,
    pub triplet_feel: TripletFeel,
}

impl MeasureHeader {
    pub fn length(&self) -> u64 {
        self.numerator as u64 * (QUARTER_TIME * 4 / self.denominator.max(1) as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TripletFeel {
    #[default]
    None,
    Eighth,
    Sixteenth,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub number: usize,
    pub name: String,
    /// Afinación por cuerda (nota MIDI), de la 1ª (más aguda) a la última.
    pub strings: Vec<i32>,
    pub capo: i32,
    pub color: [u8; 3],
    pub is_percussion: bool,
    pub program: i32,
    /// Volumen y balance del canal (0..=127, balance 64 = centro).
    pub volume: u8,
    pub pan: u8,
    pub clef: Clef,
    pub measures: Vec<Measure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clef {
    #[default]
    Treble,
    Bass,
}

#[derive(Debug, Clone, Default)]
pub struct Measure {
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, Default)]
pub struct Beat {
    /// Inicio absoluto en ticks.
    pub start: u64,
    pub voices: Vec<Voice>,
    pub text: Option<String>,
    pub chord: Option<Chord>,
}

#[derive(Debug, Clone)]
pub struct Voice {
    pub empty: bool,
    pub duration: Duration,
    pub notes: Vec<Note>,
}

impl Default for Voice {
    fn default() -> Self {
        Voice { empty: true, duration: Duration::default(), notes: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration {
    /// 1 redonda, 2 blanca, 4 negra, 8 corchea, 16, 32, 64.
    pub value: u32,
    pub dotted: bool,
    pub double_dotted: bool,
    /// Tresillos y similares: `enters` notas en el tiempo de `times`.
    pub enters: u32,
    pub times: u32,
}

impl Default for Duration {
    fn default() -> Self {
        Duration { value: 4, dotted: false, double_dotted: false, enters: 1, times: 1 }
    }
}

impl Duration {
    pub fn time(&self) -> u64 {
        let mut t = QUARTER_TIME * 4 / self.value.max(1) as u64;
        if self.dotted {
            t += t / 2;
        } else if self.double_dotted {
            t += (t / 4) * 3;
        }
        t * self.times as u64 / self.enters.max(1) as u64
    }
}

#[derive(Debug, Clone, Default)]
pub struct Chord {
    pub name: String,
    pub first_fret: i32,
    pub frets: Vec<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct Note {
    /// Cuerda, 1-based (1 = la más aguda).
    pub string: u8,
    pub fret: u8,
    pub velocity: i32,
    pub tied: bool,
    pub effect: Effects,
}

#[derive(Debug, Clone, Default)]
pub struct Effects {
    pub dead: bool,
    pub ghost: bool,
    pub accent: bool,
    pub heavy_accent: bool,
    pub hammer: bool,
    pub slide: Option<SlideKind>,
    pub let_ring: bool,
    pub palm_mute: bool,
    pub staccato: bool,
    pub vibrato: bool,
    pub fade_in: bool,
    pub bend: Option<Bend>,
    /// Barra de trémolo (afecta a todo el pulso).
    pub tremolo_bar: Option<Bend>,
    pub grace: Option<Grace>,
    pub trill: Option<Trill>,
    /// Trémolo de púa: valor de la figura de repetición (8, 16 o 32).
    pub tremolo_picking: Option<u32>,
    pub harmonic: Option<Harmonic>,
    pub tapping: bool,
    pub slapping: bool,
    pub popping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideKind {
    /// Desliza hasta la nota siguiente, que se vuelve a atacar.
    Shift,
    /// Desliza hasta la nota siguiente sin atacarla de nuevo.
    Legato,
    OutDown,
    OutUp,
    InBelow,
    InAbove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trill {
    /// Semitonos entre la nota y la nota del trino.
    pub semitones: i32,
    /// Duración (ticks) de cada alternancia.
    pub ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grace {
    pub fret: u8,
    pub ticks: u64,
    /// Suena sobre el tiempo (true) o antes de él (false).
    pub on_beat: bool,
    pub dead: bool,
}

/// Curva de bend: puntos (posición 0..=1 dentro de la nota, altura en semitonos).
#[derive(Debug, Clone, PartialEq)]
pub struct Bend {
    pub points: Vec<(f32, f32)>,
}

impl Bend {
    /// Mayor altura alcanzada, en semitonos.
    pub fn max_semitones(&self) -> f32 {
        self.points.iter().map(|p| p.1).fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harmonic {
    Natural,
    Artificial,
    Tapped,
    Pinch,
    Semi,
}

/// Nombre de nota MIDI (p. ej. 64 -> "E4").
pub fn midi_note_name(midi: i32) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let n = midi.rem_euclid(12) as usize;
    format!("{}{}", NAMES[n], midi.div_euclid(12) - 1)
}

/// Abreviatura de un instrumento de percusión General MIDI (o el número si no se conoce).
pub fn drum_label(midi: u8) -> String {
    let name = match midi {
        35 => "B2",
        36 => "BD",
        37 => "SS",
        38 => "SN",
        39 => "CP",
        40 => "S2",
        41 => "T6",
        42 => "HH",
        43 => "T5",
        44 => "PH",
        45 => "T4",
        46 => "OH",
        47 => "T3",
        48 => "T2",
        49 => "CC",
        50 => "T1",
        51 => "RD",
        52 => "CH",
        53 => "RB",
        54 => "TB",
        55 => "SP",
        56 => "CB",
        57 => "C2",
        59 => "R2",
        _ => return midi.to_string(),
    };
    name.to_string()
}

/// Nombre completo para leyendas.
pub fn drum_name(midi: u8) -> Option<&'static str> {
    Some(match midi {
        35 => "Bombo acústico",
        36 => "Bombo",
        37 => "Side stick",
        38 => "Caja",
        39 => "Palmas",
        40 => "Caja eléctrica",
        41 => "Tom piso bajo",
        42 => "Hi-hat cerrado",
        43 => "Tom piso alto",
        44 => "Hi-hat pedal",
        45 => "Tom bajo",
        46 => "Hi-hat abierto",
        47 => "Tom medio-bajo",
        48 => "Tom medio-alto",
        49 => "Crash",
        50 => "Tom alto",
        51 => "Ride",
        52 => "China",
        53 => "Campana de ride",
        54 => "Pandereta",
        55 => "Splash",
        56 => "Cencerro",
        57 => "Crash 2",
        59 => "Ride 2",
        _ => return None,
    })
}

impl Track {
    /// Notas `(cuerda 1-based, traste)` que suenan en el tick absoluto `tick` dentro del compás `measure`.
    pub fn sounding_notes(&self, measure: usize, tick: u64) -> Vec<(u8, u8)> {
        let Some(m) = self.measures.get(measure) else { return Vec::new() };
        let mut out = Vec::new();
        for beat in &m.beats {
            if beat.start > tick {
                break;
            }
            for v in beat.voices.iter().filter(|v| !v.empty) {
                if tick < beat.start + v.duration.time() {
                    out.extend(v.notes.iter().filter(|n| !n.effect.dead).map(|n| (n.string, n.fret)));
                }
            }
        }
        out
    }
}

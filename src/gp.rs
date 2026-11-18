//! Lector de archivos Guitar Pro 3, 4 y 5 (`.gp3`, `.gp4`, `.gp5`).
//!
//! Es un port del lector de TuxGuitar (`GP3InputStream`, `GP4InputStream`,
//! `GP5InputStream`). Los efectos que no se muestran igual se leen y se
//! descartan para mantener la sincronía con el flujo binario.

use crate::model::*;
use std::fmt;

#[derive(Debug)]
pub enum GpError {
    /// Cabecera que no corresponde a GP3/GP4/GP5.
    UnsupportedVersion(String),
    /// El archivo termina antes de lo esperado.
    UnexpectedEof { pos: usize },
    /// Valor fuera de rango o estructura inconsistente.
    Invalid(String),
}

impl fmt::Display for GpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpError::UnsupportedVersion(v) => write!(f, "versión no soportada: {v:?}"),
            GpError::UnexpectedEof { pos } => write!(f, "fin de archivo inesperado (offset {pos})"),
            GpError::Invalid(m) => write!(f, "archivo inválido: {m}"),
        }
    }
}

impl std::error::Error for GpError {}

type Res<T> = Result<T, GpError>;

/// Unidades de las curvas: posición 0..60; altura en cuartos de tono de 25 unidades
/// (100 = un tono entero = 2 semitonos), como interpreta TuxGuitar al reproducir.
const GP_BEND_POSITION: f32 = 60.0;
pub(crate) const GP_BEND_UNITS_PER_SEMITONE: f32 = 50.0;

/// Cursor sobre los bytes del archivo (little-endian).
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(GpError::UnexpectedEof { pos: self.pos }),
        }
    }

    fn skip(&mut self, n: usize) -> Res<()> {
        self.take(n).map(|_| ())
    }

    /// Relleno que algunos archivos GP5 omiten al final del archivo.
    fn skip_trailing(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }

    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> Res<i8> {
        Ok(self.u8()? as i8)
    }

    fn bool(&mut self) -> Res<bool> {
        Ok(self.u8()? == 1)
    }

    fn i32(&mut self) -> Res<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Equivalente a `readString(size, len)` de TuxGuitar: se leen `size`
    /// bytes (o `len` si `size <= 0`) y se usan los primeros `len`.
    fn string_raw(&mut self, size: i32, len: i32) -> Res<String> {
        let n = if size > 0 { size } else { len };
        let n = usize::try_from(n).map_err(|_| GpError::Invalid(format!("largo de texto negativo ({n})")))?;
        let bytes = self.take(n)?;
        let used = if len >= 0 && (len as usize) <= n { len as usize } else { n };
        Ok(decode(&bytes[..used]))
    }

    /// `readStringByteSizeOfInteger`: int = largo+1, luego byte de largo y el texto.
    fn str_bsi(&mut self) -> Res<String> {
        let size = self.i32()?.wrapping_sub(1);
        let len = self.u8()? as i32;
        self.string_raw(size, len)
    }

    /// `readStringByte(size)`: byte de largo, bloque fijo de `size` bytes.
    fn str_byte(&mut self, size: i32) -> Res<String> {
        let len = self.u8()? as i32;
        self.string_raw(size, len)
    }

    /// `readStringInteger`: int de largo y el texto.
    fn str_int(&mut self) -> Res<String> {
        let n = self.i32()?;
        self.string_raw(n, n)
    }
}

/// Los textos vienen en UTF-8 o en Latin-1 según el programa que guardó el archivo.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    Gp3,
    Gp4,
    Gp5,
}

/// Lee un archivo Guitar Pro 3/4/5 desde memoria.
pub fn parse(data: &[u8]) -> Result<Song, GpError> {
    let mut r = Reader::new(data);
    let len = r.u8()? as usize;
    let raw = r.take(30)?;
    let version = decode(&raw[..len.min(30)]);

    let (fmt, ver, name) = match version.as_str() {
        "FICHIER GUITAR PRO v3.00" => (Format::Gp3, 0, "Guitar Pro 3"),
        "FICHIER GUITAR PRO v4.00" => (Format::Gp4, 0, "Guitar Pro 4.00"),
        "FICHIER GUITAR PRO v4.06" => (Format::Gp4, 1, "Guitar Pro 4.06"),
        "FICHIER GUITAR PRO L4.06" => (Format::Gp4, 2, "Guitar Pro 4.06 (L)"),
        "FICHIER GUITAR PRO v5.00" => (Format::Gp5, 0, "Guitar Pro 5.00"),
        "FICHIER GUITAR PRO v5.10" => (Format::Gp5, 1, "Guitar Pro 5.10"),
        _ => return Err(GpError::UnsupportedVersion(version)),
    };

    let mut p = Parser { r, fmt, ver, key_sigs: Vec::new(), ties: Vec::new(), tempo: 120, tempo_changes: Vec::new() };
    let mut song = Song { format: name.to_string(), ..Default::default() };
    p.read_song(&mut song)?;
    Ok(song)
}

struct Parser<'a> {
    r: Reader<'a>,
    fmt: Format,
    /// Sub-versión dentro del formato (en GP5: 0 = 5.00, 1 = 5.10).
    ver: u8,
    key_sigs: Vec<i8>,
    /// Último traste leído por pista y cuerda (para resolver notas ligadas).
    ties: Vec<[i32; 8]>,
    tempo: i32,
    /// Cambios de tempo del compás en curso: (tick, BPM).
    tempo_changes: Vec<(u64, i32)>,
}

impl<'a> Parser<'a> {
    fn read_song(&mut self, song: &mut Song) -> Res<()> {
        self.read_info(song)?;

        let triplet = if self.fmt != Format::Gp5 { self.r.bool()? } else { false };
        let triplet_feel = if triplet { TripletFeel::Eighth } else { TripletFeel::None };

        if self.fmt != Format::Gp3 {
            let lyric_track = self.r.i32()?;
            if let Some((from, text)) = self.read_lyrics()? {
                if lyric_track >= 1 {
                    song.lyrics = Some(Lyrics { track: lyric_track as usize - 1, from_measure: from.max(1) as usize - 1, text });
                }
            }
        }
        if self.fmt == Format::Gp5 {
            self.read_page_setup()?;
        }

        let tempo = self.r.i32()?;
        song.tempo = tempo;
        if self.fmt == Format::Gp5 && self.ver > 0 {
            self.r.skip(1)?;
        }

        let key = self.r.i8()?;
        self.r.skip(3)?;
        if self.fmt != Format::Gp3 {
            self.r.skip(1)?; // octava
        }

        let programs = self.read_channels()?;
        if self.fmt == Format::Gp5 {
            self.r.skip(42)?;
        }

        let measures = self.count("compases", 1 << 14)?;
        let tracks = self.count("pistas", 256)?;

        self.key_sigs = vec![0; measures];
        if measures > 0 {
            self.key_sigs[0] = key;
        }

        self.read_measure_headers(song, measures, triplet_feel)?;
        self.read_tracks(song, tracks, &programs)?;
        self.read_measures(song, measures, tracks, tempo)?;
        Ok(())
    }

    fn count(&mut self, what: &str, max: i32) -> Res<usize> {
        let n = self.r.i32()?;
        if !(0..=max).contains(&n) {
            return Err(GpError::Invalid(format!("cantidad de {what} fuera de rango ({n})")));
        }
        Ok(n as usize)
    }

    fn read_info(&mut self, song: &mut Song) -> Res<()> {
        let r = &mut self.r;
        song.title = r.str_bsi()?;
        song.subtitle = r.str_bsi()?;
        song.artist = r.str_bsi()?;
        song.album = r.str_bsi()?;
        song.words = r.str_bsi()?;
        if self.fmt == Format::Gp5 {
            song.music = r.str_bsi()?;
        }
        song.copyright = r.str_bsi()?;
        song.tab_by = r.str_bsi()?;
        song.instructions = r.str_bsi()?;
        let n = r.i32()?;
        if !(0..=4096).contains(&n) {
            return Err(GpError::Invalid(format!("cantidad de comentarios fuera de rango ({n})")));
        }
        let mut lines = Vec::new();
        for _ in 0..n {
            lines.push(r.str_bsi()?);
        }
        song.comments = lines.join("\n");
        Ok(())
    }

    /// Devuelve (compás de inicio, texto) de la primera línea de letra que no esté vacía.
    fn read_lyrics(&mut self) -> Res<Option<(i32, String)>> {
        let mut first = None;
        for _ in 0..5 {
            let from = self.r.i32()?;
            let text = self.r.str_int()?;
            if first.is_none() && !text.trim().is_empty() {
                first = Some((from, text));
            }
        }
        Ok(first)
    }

    fn read_page_setup(&mut self) -> Res<()> {
        self.r.skip(if self.ver > 0 { 49 } else { 30 })?;
        for _ in 0..11 {
            self.r.skip(4)?;
            self.r.str_byte(0)?;
        }
        Ok(())
    }

    /// Devuelve (programa MIDI, volumen, balance) de cada uno de los 64 canales.
    fn read_channels(&mut self) -> Res<Vec<(i32, u8, u8)>> {
        let to_midi = |b: i8| ((b as i32) * 8 - 1).clamp(0, 127) as u8;
        let mut channels = Vec::with_capacity(64);
        for _ in 0..64 {
            let program = self.r.i32()?;
            let volume = self.r.i8()?;
            let balance = self.r.i8()?;
            self.r.skip(4)?; // chorus, reverb, phaser, tremolo
            self.r.skip(2)?;
            channels.push((program.max(0), to_midi(volume), to_midi(balance)));
        }
        Ok(channels)
    }

    // ---------------------------------------------------------------- compases

    fn read_measure_headers(&mut self, song: &mut Song, count: usize, triplet: TripletFeel) -> Res<()> {
        let (mut num, mut den) = (4u8, 4u8);
        for i in 0..count {
            if self.fmt == Format::Gp5 && i > 0 {
                self.r.skip(1)?;
            }
            let header = if self.fmt == Format::Gp5 {
                self.read_header_gp5(i, &mut num, &mut den)?
            } else {
                self.read_header_gp34(i, &mut num, &mut den, triplet, &song.headers)?
            };
            song.headers.push(header);
            if i + 1 < count {
                self.key_sigs[i + 1] = self.key_sigs[i];
            }
        }
        Ok(())
    }

    fn new_header(&self, index: usize, num: u8, den: u8) -> MeasureHeader {
        MeasureHeader {
            number: index + 1,
            start: 0,
            numerator: num,
            denominator: den,
            repeat_open: false,
            repeat_close: 0,
            repeat_alternative: 0,
            marker: None,
            key_signature: 0,
            tempo: 120,
            tempo_changes: Vec::new(),
            triplet_feel: TripletFeel::None,
        }
    }

    fn read_marker(&mut self) -> Res<String> {
        let title = self.r.str_bsi()?;
        self.r.skip(4)?; // color RGB + 1
        Ok(title)
    }

    fn read_time_signature_part(&mut self, flags: u8, num: &mut u8, den: &mut u8) -> Res<()> {
        if flags & 0x01 != 0 {
            *num = self.r.i8()?.max(1) as u8;
        }
        if flags & 0x02 != 0 {
            let d = self.r.i8()?;
            if d <= 0 {
                return Err(GpError::Invalid(format!("denominador de compás inválido ({d})")));
            }
            *den = d as u8;
        }
        Ok(())
    }

    fn read_header_gp34(
        &mut self,
        index: usize,
        num: &mut u8,
        den: &mut u8,
        triplet: TripletFeel,
        previous: &[MeasureHeader],
    ) -> Res<MeasureHeader> {
        let flags = self.r.u8()?;
        let mut h = self.new_header(index, *num, *den);
        h.triplet_feel = triplet;
        h.repeat_open = flags & 0x04 != 0;
        self.read_time_signature_part(flags, num, den)?;
        h.numerator = *num;
        h.denominator = *den;
        if flags & 0x08 != 0 {
            h.repeat_close = self.r.i8()? as i32;
        }
        if flags & 0x10 != 0 {
            let value = self.r.u8()?;
            h.repeat_alternative = parse_repeat_alternative(previous, value);
        }
        if flags & 0x20 != 0 {
            h.marker = Some(self.read_marker()?);
        }
        if flags & 0x40 != 0 {
            self.key_sigs[index] = self.r.i8()?;
            self.r.skip(1)?;
        }
        h.key_signature = self.key_sigs[index];
        Ok(h)
    }

    fn read_header_gp5(&mut self, index: usize, num: &mut u8, den: &mut u8) -> Res<MeasureHeader> {
        let flags = self.r.u8()?;
        let mut h = self.new_header(index, *num, *den);
        h.repeat_open = flags & 0x04 != 0;
        self.read_time_signature_part(flags, num, den)?;
        h.numerator = *num;
        h.denominator = *den;
        if flags & 0x08 != 0 {
            h.repeat_close = self.r.u8()? as i32 - 1;
        }
        if flags & 0x20 != 0 {
            h.marker = Some(self.read_marker()?);
        }
        if flags & 0x40 != 0 {
            self.key_sigs[index] = self.r.i8()?;
            self.r.skip(1)?;
        }
        if flags & 0x03 != 0 {
            self.r.skip(4)?;
        }
        if flags & 0x10 != 0 {
            h.repeat_alternative = self.r.u8()?;
        } else {
            self.r.skip(1)?;
        }
        h.triplet_feel = match self.r.i8()? {
            1 => TripletFeel::Eighth,
            2 => TripletFeel::Sixteenth,
            _ => TripletFeel::None,
        };
        h.key_signature = self.key_sigs[index];
        Ok(h)
    }

    // ------------------------------------------------------------------ pistas

    fn read_tracks(&mut self, song: &mut Song, count: usize, programs: &[(i32, u8, u8)]) -> Res<()> {
        for number in 1..=count {
            let t = self.read_track(number, programs)?;
            song.tracks.push(t);
            self.ties.push([-1; 8]);
        }
        if self.fmt == Format::Gp5 {
            self.r.skip(if self.ver == 0 { 2 } else { 1 })?;
        }
        Ok(())
    }

    fn read_track(&mut self, number: usize, programs: &[(i32, u8, u8)]) -> Res<Track> {
        let flags = self.r.u8()?;
        if self.fmt == Format::Gp5 && (number == 1 || self.ver == 0) {
            self.r.skip(1)?;
        }
        let name = self.r.str_byte(40)?;
        let string_count = self.r.i32()?;
        if !(0..=7).contains(&string_count) {
            return Err(GpError::Invalid(format!("cantidad de cuerdas fuera de rango ({string_count})")));
        }
        let mut strings = Vec::new();
        for i in 0..7 {
            let tuning = self.r.i32()?;
            if (i as i32) < string_count {
                strings.push(tuning);
            }
        }
        self.r.skip(4)?; // puerto
        let gm1 = self.r.i32()? - 1;
        let _gm2 = self.r.i32()? - 1;
        self.r.skip(4)?; // cantidad de trastes
        let capo = self.r.i32()?;
        let color = {
            let c = self.r.take(4)?;
            [c[0], c[1], c[2]]
        };

        let mut clef = Clef::Treble;
        if self.fmt == Format::Gp5 {
            self.r.skip(5)?;
            if self.r.i32()? == 12 {
                clef = Clef::Bass;
            }
            self.r.skip(if self.ver > 0 { 40 } else { 35 })?;
            if self.ver > 0 {
                self.r.str_bsi()?;
                self.r.str_bsi()?;
            }
        }

        // La percusión se reconoce por el canal MIDI 10 (como hace TuxGuitar): el byte de flags
        // no es fiable (algunos archivos GP 5.00 lo traen en 0xFF). Solo se usa si el canal es inválido.
        let is_percussion = gm1 == 9 || (gm1 < 0 && flags & 0x01 != 0);
        if self.fmt != Format::Gp5 && !is_percussion && strings.iter().any(|&s| s <= 34) {
            clef = Clef::Bass;
        }
        let (program, volume, pan) = usize::try_from(gm1).ok().and_then(|i| programs.get(i)).copied().unwrap_or((0, 100, 64));

        Ok(Track { number, name, strings, capo, color, is_percussion, program, volume, pan, clef, measures: Vec::new() })
    }

    // ------------------------------------------------------- contenido de compases

    fn read_measures(&mut self, song: &mut Song, measures: usize, tracks: usize, tempo: i32) -> Res<()> {
        self.tempo = tempo;
        let mut start = QUARTER_TIME;
        for i in 0..measures {
            song.headers[i].start = start;
            let start_tempo = self.tempo;
            self.tempo_changes.clear();
            for j in 0..tracks {
                let string_count = song.tracks[j].strings.len();
                let m = self.read_measure(start, j, string_count)?;
                song.tracks[j].measures.push(m);
                if self.fmt == Format::Gp5 {
                    self.r.skip_trailing(1);
                }
            }
            let mut changes = std::mem::take(&mut self.tempo_changes);
            changes.sort();
            changes.dedup();
            song.headers[i].tempo = start_tempo;
            song.headers[i].tempo_changes = changes;
            start += song.headers[i].length();
        }
        Ok(())
    }

    fn read_measure(&mut self, measure_start: u64, track: usize, strings: usize) -> Res<Measure> {
        let mut m = Measure::default();
        let voices = if self.fmt == Format::Gp5 { 2 } else { 1 };
        for voice in 0..voices {
            let mut start = measure_start;
            let beats = self.count("pulsos", 1 << 16)?;
            for _ in 0..beats {
                start += self.read_beat(&mut m, start, track, strings, voice)?;
            }
        }
        // Se descartan los pulsos sin voces con contenido y se ordena por inicio.
        m.beats.retain(|b| b.voices.iter().any(|v| !v.empty));
        m.beats.sort_by_key(|b| b.start);
        Ok(m)
    }

    fn read_beat(&mut self, m: &mut Measure, start: u64, track: usize, strings: usize, voice: usize) -> Res<u64> {
        let gp5 = self.fmt == Format::Gp5;
        let flags = self.r.u8()?;

        // Los pulsos de GP3/GP4 son siempre nuevos y no vacíos; en GP5 se
        // fusionan por tiempo de inicio entre las dos voces.
        let beat_idx = if gp5 {
            match m.beats.iter().position(|b| b.start == start) {
                Some(i) => i,
                None => {
                    m.beats.push(Beat { start, voices: vec![Voice::default(), Voice::default()], ..Default::default() });
                    m.beats.len() - 1
                }
            }
        } else {
            m.beats.push(Beat { start, voices: vec![Voice::default()], ..Default::default() });
            m.beats.len() - 1
        };
        if !gp5 {
            m.beats[beat_idx].voices[0].empty = false;
        }

        if flags & 0x40 != 0 {
            let kind = self.r.u8()?;
            if gp5 {
                let v = &mut m.beats[beat_idx].voices[voice];
                v.empty = kind & 0x02 == 0;
                if v.empty {
                    v.notes.clear();
                }
            }
        }

        let duration = self.read_duration(flags)?;
        let mut effect = Effects::default();
        if flags & 0x02 != 0 {
            m.beats[beat_idx].chord = self.read_chord()?;
        }
        if flags & 0x04 != 0 {
            m.beats[beat_idx].text = Some(self.r.str_bsi()?);
        }
        if flags & 0x08 != 0 {
            self.read_beat_effects(&mut effect)?;
        }
        if flags & 0x10 != 0 {
            self.read_mix_change(start)?;
        }

        let string_flags = self.r.u8()?;
        for i in (0..=6).rev() {
            if string_flags & (1 << i) != 0 && (6 - i) < strings {
                let note = self.read_note((6 - i) as u8 + 1, track, effect.clone())?;
                let v = &mut m.beats[beat_idx].voices[voice];
                v.notes.push(note);
                v.empty = false;
            }
        }
        m.beats[beat_idx].voices[voice].duration = duration;

        if gp5 {
            self.r.skip(1)?;
            let extra = self.r.i8()?;
            if extra & 0x08 != 0 {
                self.r.skip(1)?;
            }
            let empty = m.beats[beat_idx].voices[voice].empty;
            Ok(if empty { 0 } else { duration.time() })
        } else {
            Ok(duration.time())
        }
    }

    fn read_duration(&mut self, flags: u8) -> Res<Duration> {
        let b = self.r.i8()? as i32;
        let shift = b + 4;
        if !(2..=9).contains(&shift) {
            return Err(GpError::Invalid(format!("duración inválida ({b})")));
        }
        let mut d = Duration { value: (1u32 << shift) / 4, dotted: flags & 0x01 != 0, double_dotted: false, enters: 1, times: 1 };
        if flags & 0x20 != 0 {
            let division = self.r.i32()?;
            let (enters, times) = match division {
                3 => (3, 2),
                5 => (5, 4),
                6 => (6, 4),
                7 => (7, 4),
                9..=13 => (division as u32, 8),
                _ => (1, 1),
            };
            d.enters = enters;
            d.times = times;
        }
        Ok(d)
    }

    fn read_chord(&mut self) -> Res<Option<Chord>> {
        let mut chord = Chord::default();
        match self.fmt {
            Format::Gp5 => {
                self.r.skip(17)?;
                chord.name = self.r.str_byte(21)?;
                self.r.skip(4)?;
                chord.first_fret = self.r.i32()?;
                for _ in 0..7 {
                    chord.frets.push(self.r.i32()?);
                }
                self.r.skip(32)?;
            }
            Format::Gp4 | Format::Gp3 => {
                if self.r.u8()? & 0x01 == 0 {
                    chord.name = self.r.str_bsi()?;
                    chord.first_fret = self.r.i32()?;
                    if chord.first_fret != 0 {
                        for _ in 0..6 {
                            chord.frets.push(self.r.i32()?);
                        }
                    }
                } else if self.fmt == Format::Gp3 {
                    self.r.skip(25)?;
                    chord.name = self.r.str_byte(34)?;
                    chord.first_fret = self.r.i32()?;
                    for _ in 0..6 {
                        chord.frets.push(self.r.i32()?);
                    }
                    self.r.skip(36)?;
                } else {
                    self.r.skip(16)?;
                    chord.name = self.r.str_byte(21)?;
                    self.r.skip(4)?;
                    chord.first_fret = self.r.i32()?;
                    for _ in 0..7 {
                        chord.frets.push(self.r.i32()?);
                    }
                    self.r.skip(32)?;
                }
            }
        }
        // Un acorde sin ninguna cuerda pisada no se conserva.
        Ok(if chord.frets.iter().any(|&f| f >= 0) { Some(chord) } else { None })
    }

    /// Curva de bend/tremolo bar: 5 bytes de cabecera, cantidad y (posición, valor, vibrato) por punto.
    fn read_curve(&mut self) -> Res<Vec<(i32, i32)>> {
        self.r.skip(5)?;
        let points = self.count("puntos", 1 << 12)?;
        let mut v = Vec::with_capacity(points);
        for _ in 0..points {
            let position = self.r.i32()?;
            let value = self.r.i32()?;
            self.r.skip(1)?;
            v.push((position, value));
        }
        Ok(v)
    }

    fn read_bend(&mut self) -> Res<Option<Bend>> {
        let points: Vec<(f32, f32)> = self
            .read_curve()?
            .into_iter()
            .map(|(pos, val)| ((pos as f32 / GP_BEND_POSITION).clamp(0.0, 1.0), val as f32 / GP_BEND_UNITS_PER_SEMITONE))
            .collect();
        Ok(if points.is_empty() { None } else { Some(Bend { points }) })
    }

    fn read_beat_effects(&mut self, effect: &mut Effects) -> Res<()> {
        if self.fmt == Format::Gp3 {
            let flags = self.r.u8()?;
            effect.vibrato = flags & 0x03 != 0;
            effect.fade_in = flags & 0x10 != 0;
            if flags & 0x20 != 0 {
                let kind = self.r.u8()?;
                if kind == 0 {
                    let value = self.r.i32()? as f32 / GP_BEND_UNITS_PER_SEMITONE;
                    effect.tremolo_bar = Some(Bend { points: vec![(0.0, 0.0), (0.5, -value), (1.0, 0.0)] });
                } else {
                    effect.tapping = kind == 1;
                    effect.slapping = kind == 2;
                    effect.popping = kind == 3;
                    self.r.skip(4)?;
                }
            }
            if flags & 0x40 != 0 {
                self.r.skip(2)?;
            }
            if flags & 0x04 != 0 {
                effect.harmonic = Some(Harmonic::Natural);
            }
            if flags & 0x08 != 0 {
                effect.harmonic = Some(Harmonic::Artificial);
            }
            return Ok(());
        }

        let flags1 = self.r.u8()?;
        let flags2 = self.r.u8()?;
        effect.fade_in = flags1 & 0x10 != 0;
        effect.vibrato = flags1 & 0x02 != 0;
        if flags1 & 0x20 != 0 {
            let kind = self.r.u8()?;
            effect.tapping = kind == 1;
            effect.slapping = kind == 2;
            effect.popping = kind == 3;
        }
        if flags2 & 0x04 != 0 {
            effect.tremolo_bar = self.read_bend()?;
        }
        if flags1 & 0x40 != 0 {
            self.r.skip(2)?; // rasgueo
        }
        if flags2 & 0x02 != 0 {
            self.r.skip(1)?; // dirección de la púa
        }
        Ok(())
    }

    fn read_mix_change(&mut self, beat_start: u64) -> Res<()> {
        let gp5 = self.fmt == Format::Gp5;
        self.r.skip(1)?; // instrumento
        if gp5 {
            self.r.skip(16)?;
        }
        let mut changes = [0i8; 6]; // volumen, pan, chorus, reverb, phaser, tremolo
        for c in changes.iter_mut() {
            *c = self.r.i8()?;
        }
        if gp5 {
            self.r.str_bsi()?; // nombre del tempo
        }
        let tempo = self.r.i32()?;
        for c in changes {
            if c >= 0 {
                self.r.skip(1)?;
            }
        }
        if tempo >= 0 {
            self.tempo = tempo;
            self.tempo_changes.push((beat_start, tempo));
            self.r.skip(1)?;
            if gp5 && self.ver > 0 {
                self.r.skip(1)?;
            }
        }
        match self.fmt {
            Format::Gp3 => {}
            Format::Gp4 => self.r.skip(1)?,
            Format::Gp5 => {
                self.r.skip(2)?;
                if self.ver > 0 {
                    self.r.str_bsi()?;
                    self.r.str_bsi()?;
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------- notas

    fn read_note(&mut self, string: u8, track: usize, effect: Effects) -> Res<Note> {
        let gp5 = self.fmt == Format::Gp5;
        let flags = self.r.u8()?;
        let mut note = Note { string, effect, ..Default::default() };
        note.effect.accent = flags & 0x40 != 0;
        note.effect.heavy_accent = gp5 && flags & 0x02 != 0;
        note.effect.ghost = flags & 0x04 != 0;

        if flags & 0x20 != 0 {
            let kind = self.r.u8()?;
            note.tied = kind == 0x02;
            note.effect.dead = kind == 0x03;
        }
        if !gp5 && flags & 0x01 != 0 {
            self.r.skip(2)?;
        }
        if flags & 0x10 != 0 {
            note.velocity = self.r.i8()? as i32;
        }
        if flags & 0x20 != 0 {
            let fret = self.r.i8()? as i32;
            let value = if note.tied { self.ties[track][string as usize] } else { fret };
            note.fret = if (0..100).contains(&value) { value as u8 } else { 0 };
        }
        if flags & 0x80 != 0 {
            self.r.skip(2)?;
        }
        if gp5 {
            if flags & 0x01 != 0 {
                self.r.skip(8)?;
            }
            self.r.skip(1)?;
        }
        if flags & 0x08 != 0 {
            self.read_note_effects(&mut note.effect, note.fret as i32)?;
        }
        self.ties[track][string as usize] = note.fret as i32;
        Ok(note)
    }

    /// Nota de adorno: traste, dinámica, transición, duración (en 64avos) y, en GP5, banderas.
    fn read_grace(&mut self, gp5: bool) -> Res<Grace> {
        let fret = self.r.u8()?;
        self.r.skip(2)?; // dinámica, transición
        let duration = self.r.u8()?;
        let (mut dead, mut on_beat) = (fret == 255, false);
        if gp5 {
            let flags = self.r.u8()?;
            dead = flags & 0x01 != 0;
            on_beat = flags & 0x02 != 0;
        }
        Ok(Grace { fret: if fret == 255 { 0 } else { fret }, ticks: duration.max(1) as u64 * 60, on_beat, dead })
    }

    fn read_note_effects(&mut self, e: &mut Effects, note_fret: i32) -> Res<()> {
        if self.fmt == Format::Gp3 {
            let flags = self.r.u8()?;
            e.hammer = flags & 0x02 != 0;
            if flags & 0x04 != 0 {
                e.slide = Some(SlideKind::Shift);
            }
            e.let_ring = flags & 0x08 != 0;
            if flags & 0x01 != 0 {
                e.bend = self.read_bend()?;
            }
            if flags & 0x10 != 0 {
                e.grace = Some(self.read_grace(false)?);
            }
            return Ok(());
        }

        let gp5 = self.fmt == Format::Gp5;
        let flags1 = self.r.u8()?;
        let flags2 = self.r.u8()?;
        e.hammer = flags1 & 0x02 != 0;
        e.let_ring = flags1 & 0x08 != 0;
        e.vibrato = e.vibrato || flags2 & 0x40 != 0;
        e.palm_mute = flags2 & 0x02 != 0;
        e.staccato = flags2 & 0x01 != 0;
        if flags1 & 0x01 != 0 {
            e.bend = self.read_bend()?;
        }
        if flags1 & 0x10 != 0 {
            e.grace = Some(self.read_grace(gp5)?);
        }
        if flags2 & 0x04 != 0 {
            e.tremolo_picking = match self.r.u8()? {
                1 => Some(8),
                2 => Some(16),
                3 => Some(32),
                _ => None,
            };
        }
        if flags2 & 0x08 != 0 {
            let kind = self.r.i8()?;
            let found = if gp5 {
                // GP5: máscara de bits.
                [
                    (0x01u8, SlideKind::Shift),
                    (0x02, SlideKind::Legato),
                    (0x04, SlideKind::OutDown),
                    (0x08, SlideKind::OutUp),
                    (0x10, SlideKind::InBelow),
                    (0x20, SlideKind::InAbove),
                ]
                .iter()
                .find(|(bit, _)| kind as u8 & bit != 0)
                .map(|(_, k)| *k)
            } else {
                match kind {
                    2 => Some(SlideKind::Legato),
                    3 => Some(SlideKind::OutDown),
                    4 => Some(SlideKind::OutUp),
                    -1 => Some(SlideKind::InBelow),
                    -2 => Some(SlideKind::InAbove),
                    _ => Some(SlideKind::Shift),
                }
            };
            e.slide = found.or(Some(SlideKind::Shift));
        }
        if flags2 & 0x10 != 0 {
            let kind = self.r.i8()?;
            if gp5 {
                e.harmonic = match kind {
                    1 => Some(Harmonic::Natural),
                    2 => {
                        self.r.skip(3)?;
                        Some(Harmonic::Artificial)
                    }
                    3 => {
                        self.r.skip(1)?;
                        Some(Harmonic::Tapped)
                    }
                    4 => Some(Harmonic::Pinch),
                    5 => Some(Harmonic::Semi),
                    _ => None,
                };
            } else {
                e.harmonic = match kind {
                    1 => Some(Harmonic::Natural),
                    3 => Some(Harmonic::Tapped),
                    4 => Some(Harmonic::Pinch),
                    5 => Some(Harmonic::Semi),
                    15 | 17 | 22 => Some(Harmonic::Artificial),
                    _ => None,
                };
            }
        }
        if flags2 & 0x20 != 0 {
            let fret = self.r.i8()? as i32;
            let period = self.r.i8()?;
            let ticks = match period {
                1 => Some(240),
                2 => Some(120),
                3 => Some(60),
                _ => None,
            };
            e.trill = ticks.map(|ticks| Trill { semitones: fret - note_fret, ticks });
        }
        Ok(())
    }
}

/// Convierte el número de "finales" de GP3/GP4 a máscara de bits, saltando los ya usados.
fn parse_repeat_alternative(previous: &[MeasureHeader], value: u8) -> u8 {
    let mut existing = 0u8;
    for h in previous {
        if h.repeat_open {
            existing = 0;
        }
        existing |= h.repeat_alternative;
    }
    let mut alt = 0u8;
    for i in 0..8 {
        if value > i && existing & (1 << i) == 0 {
            alt |= 1 << i;
        }
    }
    alt
}

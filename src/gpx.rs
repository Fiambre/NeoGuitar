//! Lector de Guitar Pro 6 (`.gpx`) y Guitar Pro 7+ (`.gp`).
//!
//! Ambos guardan la partitura como XML (`score.gpif`). GP6 la envuelve en un
//! sistema de archivos propio (BCFZ comprimido -> BCFS) y GP7 en un ZIP.
//! Port de `GPXFileSystem`, `GPXDocumentReader` y `GPXDocumentParser` de TuxGuitar.

use crate::gp::GpError;
use crate::model::*;
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::Read;

type Res<T> = Result<T, GpError>;

fn invalid<T>(msg: impl Into<String>) -> Res<T> {
    Err(GpError::Invalid(msg.into()))
}

// ============================================================ contenedores

pub fn parse_gpx6(data: &[u8]) -> Res<Song> {
    let files = bcfs_files(data)?;
    let score = files
        .iter()
        .find(|(name, _)| name == "score.gpif")
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| GpError::Invalid("no se encontró score.gpif dentro del .gpx".into()))?;
    parse_gpif(&String::from_utf8_lossy(score), "Guitar Pro 6")
}

pub fn parse_gp7(data: &[u8]) -> Res<Song> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| GpError::Invalid(format!("ZIP inválido: {e}")))?;
    let mut xml = String::new();
    {
        let mut entry = zip
            .by_name("Content/score.gpif")
            .map_err(|_| GpError::Invalid("no se encontró Content/score.gpif dentro del .gp".into()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| GpError::Invalid(format!("no se pudo descomprimir score.gpif: {e}")))?;
        xml.push_str(&String::from_utf8_lossy(&bytes));
    }
    let mut version = String::new();
    if let Ok(mut v) = zip.by_name("VERSION") {
        let _ = v.read_to_string(&mut version);
    }
    let name = match version.trim().chars().next() {
        Some(c) if c.is_ascii_digit() => format!("Guitar Pro {}", version.trim()),
        _ => "Guitar Pro 7".to_string(),
    };
    parse_gpif(&xml, &name)
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let b = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(b as u32)
    }

    /// Bits de mayor a menor peso.
    fn read(&mut self, count: u32) -> Option<u32> {
        let mut v = 0;
        for i in (0..count).rev() {
            v |= self.bit()? << i;
        }
        Some(v)
    }

    /// Bits de menor a mayor peso.
    fn read_reversed(&mut self, count: u32) -> Option<u32> {
        let mut v = 0;
        for i in 0..count {
            v |= self.bit()? << i;
        }
        Some(v)
    }
}

const MAX_BCFS: usize = 512 << 20;

fn bcfz_decompress(src: &[u8]) -> Res<Vec<u8>> {
    if src.len() < 4 {
        return invalid("BCFZ truncado");
    }
    let expected = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;
    if expected > MAX_BCFS {
        return invalid(format!("tamaño descomprimido excesivo ({expected})"));
    }
    let mut bits = Bits { data: &src[4..], pos: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    while out.len() < expected {
        let Some(flag) = bits.bit() else { break };
        if flag == 1 {
            let Some(width) = bits.read(4) else { break };
            let (Some(offs), Some(size)) = (bits.read_reversed(width), bits.read_reversed(width)) else { break };
            let (offs, size) = (offs as usize, size as usize);
            let Some(pos) = out.len().checked_sub(offs) else {
                return invalid("referencia inválida en BCFZ");
            };
            for i in 0..size.min(offs) {
                out.push(out[pos + i]);
            }
        } else {
            let Some(size) = bits.read_reversed(2) else { break };
            for _ in 0..size {
                let Some(b) = bits.read(8) else { return Ok(out) };
                out.push(b as u8);
            }
        }
    }
    Ok(out)
}

fn u32_at(d: &[u8], at: usize) -> Option<usize> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// Extrae los archivos de un GPX (BCFZ o BCFS); devuelve (nombre, contenido).
fn bcfs_files(data: &[u8]) -> Res<Vec<(String, Vec<u8>)>> {
    const BCFS: &[u8] = b"BCFS";
    const BCFZ: &[u8] = b"BCFZ";
    let decompressed;
    let bcfs: &[u8] = match data.get(..4) {
        Some(h) if h == BCFZ => {
            decompressed = bcfz_decompress(&data[4..])?;
            &decompressed
        }
        Some(h) if h == BCFS => data,
        _ => return invalid("no es un archivo GPX"),
    };
    if bcfs.get(..4) != Some(BCFS) {
        return invalid("el contenido descomprimido no es BCFS");
    }
    // TuxGuitar indexa los sectores sin contar la cabecera de 4 bytes.
    let files = read_sectors(&bcfs[4..]);
    if !files.is_empty() {
        return Ok(files);
    }
    let files = read_sectors(bcfs);
    if files.is_empty() {
        return invalid("el GPX no contiene archivos");
    }
    Ok(files)
}

fn read_sectors(d: &[u8]) -> Vec<(String, Vec<u8>)> {
    const SECTOR: usize = 0x1000;
    let mut files = Vec::new();
    let mut offset = 0usize;
    let mut guard = 0;
    loop {
        offset += SECTOR;
        guard += 1;
        if offset + 3 >= d.len() || guard > (1 << 20) {
            break;
        }
        if u32_at(d, offset) != Some(2) {
            continue;
        }
        let name_at = offset + 4;
        let size_at = offset + 0x8C;
        let blocks_at = offset + 0x94;

        let mut bytes = Vec::new();
        let mut n = 0;
        while let Some(block) = u32_at(d, blocks_at + 4 * n) {
            n += 1;
            if block == 0 || n > (1 << 16) {
                break;
            }
            // TuxGuitar reutiliza `offset` para el último bloque leído.
            offset = block.saturating_mul(SECTOR);
            for i in 0..SECTOR {
                bytes.push(d.get(offset + i).copied().unwrap_or(0));
            }
        }
        let size = u32_at(d, size_at).unwrap_or(0);
        if bytes.len() >= size {
            let name_bytes = d.get(name_at..(name_at + 127).min(d.len())).unwrap_or(&[]);
            let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
            let name: String = name_bytes[..end].iter().map(|&b| b as char).collect();
            bytes.truncate(size);
            files.push((name, bytes));
        }
    }
    files
}

// ================================================================== XML

fn elem<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// Elementos hijos `item` dentro del hijo `container`.
fn items<'a, 'i>(n: Node<'a, 'i>, container: &str, item: &'static str) -> Vec<Node<'a, 'i>> {
    match elem(n, container) {
        Some(c) => c.children().filter(|x| x.is_element() && x.tag_name().name() == item).collect(),
        None => Vec::new(),
    }
}

fn content(n: Node) -> String {
    n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect()
}

fn text(n: Node, name: &str) -> Option<String> {
    elem(n, name).map(content)
}

fn to_int(s: &str) -> Option<i32> {
    s.trim().parse::<f64>().ok().map(|f| f as i32)
}

fn int(n: Node, name: &str) -> Option<i32> {
    text(n, name).and_then(|s| to_int(&s))
}

fn ints(n: Node, name: &str, sep: Option<char>) -> Option<Vec<i32>> {
    let raw = text(n, name)?;
    let raw = raw.trim();
    let parts: Vec<&str> = match sep {
        Some(c) => raw.split(c).collect(),
        None => raw.split_whitespace().collect(),
    };
    Some(parts.iter().map(|p| to_int(p).unwrap_or(0)).collect())
}

fn attr_int(n: Node, name: &str) -> i32 {
    n.attribute(name).and_then(to_int).unwrap_or(0)
}

fn has_enable(p: Node) -> bool {
    elem(p, "Enable").is_some()
}

// ======================================================== estructuras GPIF

#[derive(Default, Clone)]
struct GNote {
    string: i32,
    fret: i32,
    tone: i32,
    octave: i32,
    element: i32,
    variation: i32,
    midi: i32,
    trill: i32,
    accent: i32,
    tie_destination: bool,
    ghost: bool,
    let_ring: bool,
    vibrato: bool,
    muted: bool,
    palm_muted: bool,
    slide: bool,
    slide_flags: i32,
    trill_duration: i32,
    tapped: bool,
    hammer: bool,
    bend_enabled: bool,
    bend: GCurve,
    harmonic: String,
}

/// Curva de bend o de barra de trémolo tal como la guarda GPIF.
#[derive(Default, Clone)]
struct GCurve {
    origin: Option<i32>,
    middle: Option<i32>,
    destination: Option<i32>,
    origin_offset: Option<i32>,
    middle_offset1: Option<i32>,
    middle_offset2: Option<i32>,
    destination_offset: Option<i32>,
}

impl GCurve {
    /// Asigna el valor de una propiedad `<prefix>OriginValue`, etc.
    fn set(&mut self, suffix: &str, v: Option<i32>) {
        match suffix {
            "OriginValue" => self.origin = v,
            "MiddleValue" => self.middle = v,
            "DestinationValue" => self.destination = v,
            "OriginOffset" => self.origin_offset = v,
            "MiddleOffset1" => self.middle_offset1 = v,
            "MiddleOffset2" => self.middle_offset2 = v,
            "DestinationOffset" => self.destination_offset = v,
            _ => {}
        }
    }
}

#[derive(Default)]
struct GBeat {
    rhythm: i32,
    notes: Vec<i32>,
    chord: Option<i32>,
    text: String,
    fade_in: bool,
    dynamic: i32,
    /// Tipo de nota de adorno ("OnBeat" / "BeforeBeat") si este pulso es de adorno.
    grace: Option<String>,
    /// Trémolo de púa: figura de repetición (8, 16, 32).
    tremolo: Option<u32>,
    whammy_enabled: bool,
    whammy: GCurve,
    slapped: bool,
    popped: bool,
}

struct GRhythm {
    value: u32,
    dots: i32,
    num: u32,
    den: u32,
}

struct GBar {
    voices: Vec<i32>,
    clef: String,
    simile: Option<String>,
}

struct GMasterBar {
    bars: Vec<i32>,
    time: Option<(u8, u8)>,
    triplet: TripletFeel,
    repeat_start: bool,
    repeat_close: i32,
    key: i8,
    alternate: Vec<i32>,
    marker: Option<String>,
}

struct Ctx {
    voices: HashMap<i32, Vec<i32>>,
    beats: HashMap<i32, GBeat>,
    notes: HashMap<i32, GNote>,
    rhythms: HashMap<i32, GRhythm>,
    chords: HashMap<i32, Chord>,
}

/// (nota MIDI, elemento, variación) de los kits de batería de GP6/7.
const DRUMKIT: [(i32, i32, i32); 21] = [
    (36, 0, 0),
    (36, 0, 0),
    (37, 1, 2),
    (38, 1, 0),
    (41, 5, 0),
    (42, 10, 0),
    (43, 6, 0),
    (44, 11, 0),
    (45, 7, 0),
    (46, 10, 2),
    (47, 8, 0),
    (48, 9, 0),
    (49, 12, 0),
    (50, 9, 0),
    (51, 15, 0),
    (52, 16, 0),
    (53, 15, 2),
    (55, 14, 0),
    (56, 3, 0),
    (57, 13, 0),
    (59, 15, 1),
];

struct GTrack {
    name: String,
    color: [u8; 3],
    program: i32,
    percussion: bool,
    tuning: Option<Vec<i32>>,
    capo: i32,
    clef: Clef,
}

// ============================================================= lectura

fn parse_gpif(xml: &str, format: &str) -> Res<Song> {
    let doc = Document::parse(xml).map_err(|e| GpError::Invalid(format!("XML inválido: {e}")))?;
    let root = doc.root_element();

    let mut song = Song { format: format.to_string(), ..Default::default() };
    if let Some(score) = elem(root, "Score") {
        let get = |n: &str| text(score, n).unwrap_or_default();
        song.title = get("Title");
        song.subtitle = get("SubTitle");
        song.artist = get("Artist");
        song.album = get("Album");
        song.words = get("Words");
        song.music = get("Music");
        if song.words.is_empty() && song.music.is_empty() {
            song.words = get("WordsAndMusic");
        }
        song.copyright = get("Copyright");
        song.tab_by = get("Tabber");
        song.instructions = get("Instructions");
        song.comments = get("Notices");
    }

    let mut ctx = Ctx {
        voices: HashMap::new(),
        beats: HashMap::new(),
        notes: HashMap::new(),
        rhythms: HashMap::new(),
        chords: HashMap::new(),
    };
    let tempos = read_tempos(root);
    let (tracks, chords) = read_tracks(root);
    ctx.chords = chords;
    let master_bars = read_master_bars(root);
    let bars = read_bars(root);
    read_voices(root, &mut ctx);
    read_beats(root, &mut ctx);
    read_notes(root, &mut ctx);
    read_rhythms(root, &mut ctx);

    if master_bars.is_empty() {
        return invalid("el archivo no tiene compases");
    }

    // Pistas del modelo.
    for (i, t) in tracks.iter().enumerate() {
        let strings = match &t.tuning {
            Some(p) if !p.is_empty() => p.iter().rev().copied().collect(),
            _ if t.percussion => vec![0; 6],
            _ => vec![64, 59, 55, 50, 45, 40],
        };
        song.tracks.push(Track {
            number: i + 1,
            name: t.name.clone(),
            strings,
            capo: t.capo,
            color: t.color,
            is_percussion: t.percussion,
            program: t.program,
            volume: 100,
            pan: 64,
            clef: t.clef,
            measures: Vec::new(),
        });
    }

    // Compases.
    let mut start = QUARTER_TIME;
    let mut tempo = 120;
    for (i, mb) in master_bars.iter().enumerate() {
        let (numerator, denominator) = mb.time.unwrap_or_else(|| {
            song.headers.last().map(|h| (h.numerator, h.denominator)).unwrap_or((4, 4))
        });
        let mut alt = 0u8;
        for &a in &mb.alternate {
            if (1..=8).contains(&a) {
                alt |= 1 << (a - 1);
            }
        }
        // Tempo al inicio del compás y cambios internos (posición = fracción del compás).
        let length_ticks = numerator as u64 * (QUARTER_TIME * 4 / denominator.max(1) as u64);
        let mut tempo_changes = Vec::new();
        if let Some(list) = tempos.get(&(i as i32)) {
            let mut list = list.clone();
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (position, bpm) in list {
                if position <= 0.0001 {
                    tempo = bpm;
                } else {
                    tempo_changes.push((start + (position * length_ticks as f64) as u64, bpm));
                }
            }
        }
        let start_tempo = tempo;
        if let Some(&(_, last)) = tempo_changes.last() {
            tempo = last;
        }
        let header = MeasureHeader {
            number: i + 1,
            start,
            numerator,
            denominator,
            repeat_open: mb.repeat_start,
            repeat_close: mb.repeat_close,
            repeat_alternative: alt,
            marker: mb.marker.clone().filter(|m| !m.is_empty()),
            key_signature: mb.key,
            tempo: start_tempo,
            tempo_changes,
            triplet_feel: mb.triplet,
        };
        let length = header.length();
        song.headers.push(header);

        for (t, track) in song.tracks.iter_mut().enumerate() {
            let mut bar = mb.bars.get(t).and_then(|id| bars.get(id));
            // Los compases "simile" repiten uno anterior.
            let mut index = i as i64;
            let mut guard = 0;
            while let Some(b) = bar {
                let Some(mark) = &b.simile else { break };
                let step = match mark.as_str() {
                    "Simple" => 1,
                    "FirstOfDouble" | "SecondOfDouble" => 2,
                    _ => 0,
                };
                guard += 1;
                if step == 0 || guard > 64 {
                    bar = None;
                    break;
                }
                index -= step;
                bar = usize::try_from(index)
                    .ok()
                    .and_then(|k| master_bars.get(k))
                    .and_then(|src| src.bars.get(t))
                    .and_then(|id| bars.get(id));
            }
            if i == 0 && bar.map(|b| b.clef == "F4").unwrap_or(false) {
                track.clef = Clef::Bass;
            }
            let mut measure = match bar {
                Some(b) => parse_bar(&ctx, b, start, track),
                None => Measure::default(),
            };
            if i == 0 {
                fix_first_measure(&mut measure, start, length);
            }
            track.measures.push(measure);
        }
        start += length;
    }
    song.tempo = song.headers.first().map(|h| h.tempo).unwrap_or(120);
    Ok(song)
}

/// Cambios de tempo por compás: (fracción del compás, BPM).
fn read_tempos(root: Node) -> HashMap<i32, Vec<(f64, i32)>> {
    let mut map: HashMap<i32, Vec<(f64, i32)>> = HashMap::new();
    let Some(master) = elem(root, "MasterTrack") else { return map };
    for a in items(master, "Automations", "Automation") {
        if text(a, "Type").as_deref() != Some("Tempo") {
            continue;
        }
        let bar = int(a, "Bar").unwrap_or(0);
        let position = text(a, "Position").and_then(|p| p.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let raw = text(a, "Value").unwrap_or_default();
        let mut parts = raw.split_whitespace();
        let (Some(bpm), unit) = (parts.next().and_then(|v| v.parse::<f64>().ok()), parts.next().and_then(|u| u.parse::<i32>().ok())) else {
            continue;
        };
        let bpm = match unit {
            Some(1) => bpm / 2.0,
            Some(3) => bpm * 1.5,
            Some(4) => bpm * 2.0,
            Some(5) => bpm * 3.0,
            _ => bpm,
        };
        map.entry(bar).or_default().push((position.clamp(0.0, 1.0), bpm.round() as i32));
    }
    map
}

fn read_tracks(root: Node) -> (Vec<GTrack>, HashMap<i32, Chord>) {
    let mut tracks = Vec::new();
    let mut chords = HashMap::new();
    for tn in items(root, "Tracks", "Track") {
        let mut t = GTrack {
            name: text(tn, "Name").unwrap_or_default().replace('\n', "").trim().to_string(),
            color: [0, 0, 0],
            program: 0,
            percussion: false,
            tuning: None,
            capo: 0,
            clef: Clef::Treble,
        };
        if let Some(c) = ints(tn, "Color", None).filter(|c| c.len() == 3) {
            t.color = [c[0].clamp(0, 255) as u8, c[1].clamp(0, 255) as u8, c[2].clamp(0, 255) as u8];
        }

        // Canal MIDI y programa (GP6: GeneralMidi; GP7: MidiConnection + Sounds).
        let mut channel = -1;
        if let Some(gm) = elem(tn, "GeneralMidi") {
            t.program = int(gm, "Program").unwrap_or(0);
            channel = int(gm, "PrimaryChannel").unwrap_or(-1);
        } else {
            let (mut primary, mut secondary) = (-1, -1);
            if let Some(mc) = elem(tn, "MidiConnection") {
                primary = int(mc, "PrimaryChannel").unwrap_or(-1);
                secondary = int(mc, "SecondaryChannel").unwrap_or(-1);
            }
            for s in items(tn, "Sounds", "Sound") {
                if let Some(midi) = elem(s, "MIDI") {
                    t.program = int(midi, "Program").unwrap_or(0);
                }
            }
            if primary >= 0 && secondary >= 0 {
                channel = primary;
            } else if let Some(set) = elem(tn, "InstrumentSet") {
                let drums = text(set, "Name").unwrap_or_default().to_lowercase().contains("drum")
                    || text(set, "Type").unwrap_or_default().to_lowercase().contains("drum");
                if drums && t.program == 0 {
                    channel = 9;
                }
            }
        }
        t.percussion = channel == 9;

        // Propiedades de la pista (GP6: Track/Properties; GP7: Track/Staves/Staff/Properties).
        let properties = match elem(tn, "Properties") {
            Some(p) => Some(p),
            None => elem(tn, "Staves").and_then(|s| elem(s, "Staff")).and_then(|s| elem(s, "Properties")),
        };
        if let Some(props) = properties {
            for p in props.children().filter(|c| c.is_element() && c.tag_name().name() == "Property") {
                match p.attribute("name").unwrap_or("") {
                    "Tuning" => t.tuning = ints(p, "Pitches", None),
                    "CapoFret" => t.capo = int(p, "Fret").unwrap_or(0),
                    "DiagramCollection" => read_chords(p, &mut chords),
                    _ => {}
                }
            }
        }
        tracks.push(t);
    }
    (tracks, chords)
}

fn read_chords(property: Node, chords: &mut HashMap<i32, Chord>) {
    for item in items(property, "Items", "Item") {
        let Some(diagram) = elem(item, "Diagram") else { continue };
        let string_count = attr_int(diagram, "stringCount").clamp(0, 12) as usize;
        let base_fret = attr_int(diagram, "baseFret");
        let mut frets = vec![-1; string_count];
        for f in diagram.children().filter(|c| c.is_element() && c.tag_name().name() == "Fret") {
            let s = attr_int(f, "string");
            if s >= 0 && (s as usize) < string_count {
                frets[string_count - s as usize - 1] = attr_int(f, "fret") + base_fret;
            }
        }
        chords.entry(attr_int(item, "id")).or_insert(Chord {
            name: item.attribute("name").unwrap_or("").to_string(),
            first_fret: base_fret,
            frets,
        });
    }
}

fn read_master_bars(root: Node) -> Vec<GMasterBar> {
    let mut out = Vec::new();
    for mb in items(root, "MasterBars", "MasterBar") {
        let time = ints(mb, "Time", Some('/')).filter(|t| t.len() == 2 && t[0] > 0 && t[1] > 0 && t[1] < 256 && t[0] < 256);
        let (mut repeat_start, mut repeat_close) = (false, 0);
        if let Some(r) = elem(mb, "Repeat") {
            repeat_start = r.attribute("start") == Some("true");
            if r.attribute("end") == Some("true") {
                repeat_close = attr_int(r, "count") - 1;
            }
        }
        out.push(GMasterBar {
            bars: ints(mb, "Bars", None).unwrap_or_default(),
            time: time.map(|t| (t[0] as u8, t[1] as u8)),
            triplet: match text(mb, "TripletFeel").as_deref() {
                Some("Triplet8th") => TripletFeel::Eighth,
                Some("Triplet16th") => TripletFeel::Sixteenth,
                _ => TripletFeel::None,
            },
            repeat_start,
            repeat_close,
            key: elem(mb, "Key").and_then(|k| int(k, "AccidentalCount")).unwrap_or(0).clamp(-7, 7) as i8,
            alternate: ints(mb, "AlternateEndings", None).unwrap_or_default(),
            marker: elem(mb, "Section").and_then(|s| text(s, "Text")).map(|t| t.replace('\n', "").trim().to_string()),
        });
    }
    out
}

fn read_bars(root: Node) -> HashMap<i32, GBar> {
    let mut map = HashMap::new();
    for b in items(root, "Bars", "Bar") {
        map.insert(
            attr_int(b, "id"),
            GBar {
                voices: ints(b, "Voices", None).unwrap_or_default(),
                clef: text(b, "Clef").unwrap_or_default(),
                simile: text(b, "SimileMark").filter(|s| !s.is_empty()),
            },
        );
    }
    map
}

fn read_voices(root: Node, ctx: &mut Ctx) {
    for v in items(root, "Voices", "Voice") {
        ctx.voices.insert(attr_int(v, "id"), ints(v, "Beats", None).unwrap_or_default());
    }
}

fn read_beats(root: Node, ctx: &mut Ctx) {
    for bn in items(root, "Beats", "Beat") {
        let mut b = GBeat {
            rhythm: elem(bn, "Rhythm").map(|r| attr_int(r, "ref")).unwrap_or(0),
            notes: ints(bn, "Notes", None).unwrap_or_default(),
            chord: int(bn, "Chord"),
            text: text(bn, "FreeText").unwrap_or_default(),
            fade_in: text(bn, "Fadding").as_deref() == Some("FadeIn"),
            dynamic: match text(bn, "Dynamic").as_deref() {
                Some("PPP") => 1,
                Some("PP") => 2,
                Some("P") => 3,
                Some("MP") => 4,
                Some("MF") => 5,
                Some("F") => 6,
                Some("FF") => 7,
                Some("FFF") => 8,
                _ => 0,
            },
            grace: elem(bn, "GraceNotes").map(content),
            tremolo: ints(bn, "Tremolo", Some('/')).filter(|t| t.len() == 2 && t[1] > 0).map(|t| (4 * t[1] as u32).clamp(8, 64)),
            ..Default::default()
        };
        for p in items(bn, "Properties", "Property") {
            match p.attribute("name").unwrap_or("") {
                "WhammyBar" => b.whammy_enabled = has_enable(p),
                name if name.starts_with("WhammyBar") && name != "WhammyBar" => {
                    b.whammy.set(&name["WhammyBar".len()..], int(p, "Float"));
                }
                "Slapped" => b.slapped = has_enable(p),
                "Popped" => b.popped = has_enable(p),
                _ => {}
            }
        }
        ctx.beats.insert(attr_int(bn, "id"), b);
    }
}

fn read_notes(root: Node, ctx: &mut Ctx) {
    for nn in items(root, "Notes", "Note") {
        let mut g = GNote { string: -1, fret: -1, tone: -1, octave: -1, element: -1, variation: -1, midi: -1, ..Default::default() };
        g.tie_destination = elem(nn, "Tie").map(|t| t.attribute("destination") == Some("true")).unwrap_or(false);
        g.ghost = text(nn, "AntiAccent").as_deref() == Some("Normal");
        g.accent = int(nn, "Accent").unwrap_or(0);
        g.trill = int(nn, "Trill").unwrap_or(0);
        g.let_ring = elem(nn, "LetRing").is_some();
        g.vibrato = elem(nn, "Vibrato").is_some();
        for p in items(nn, "Properties", "Property") {
            match p.attribute("name").unwrap_or("") {
                "String" => g.string = int(p, "String").unwrap_or(0),
                "Fret" => g.fret = int(p, "Fret").unwrap_or(0),
                "Midi" => g.midi = int(p, "Number").unwrap_or(0),
                "Tone" => g.tone = int(p, "Step").unwrap_or(0),
                "Octave" => g.octave = int(p, "Number").unwrap_or(0),
                "Element" => g.element = int(p, "Element").unwrap_or(0),
                "Variation" => g.variation = int(p, "Variation").unwrap_or(0),
                "Muted" => g.muted = has_enable(p),
                "PalmMuted" => g.palm_muted = has_enable(p),
                "Slide" => {
                    g.slide = true;
                    g.slide_flags = int(p, "Flags").unwrap_or(0);
                }
                "Tapped" => g.tapped = has_enable(p),
                "Bended" => g.bend_enabled = has_enable(p),
                name if name.starts_with("Bend") && name != "Bended" => {
                    g.bend.set(&name["Bend".len()..], int(p, "Float"));
                }
                "HopoOrigin" => g.hammer = true,
                "HarmonicType" => g.harmonic = text(p, "HType").unwrap_or_default(),
                _ => {}
            }
        }
        // Duración del trino (XProperty 688062467).
        for xp in items(nn, "XProperties", "XProperty") {
            if xp.attribute("id") == Some("688062467") {
                g.trill_duration = int(xp, "Int").unwrap_or(0);
            }
        }
        ctx.notes.insert(attr_int(nn, "id"), g);
    }
}

fn read_rhythms(root: Node, ctx: &mut Ctx) {
    for rn in items(root, "Rhythms", "Rhythm") {
        let tuplet = elem(rn, "PrimaryTuplet");
        let value = match text(rn, "NoteValue").as_deref() {
            Some("Whole") => 1,
            Some("Half") => 2,
            Some("Quarter") => 4,
            Some("Eighth") => 8,
            Some("16th") => 16,
            Some("32nd") => 32,
            Some("64th") => 64,
            _ => 4,
        };
        ctx.rhythms.insert(
            attr_int(rn, "id"),
            GRhythm {
                value,
                dots: elem(rn, "AugmentationDot").map(|d| attr_int(d, "count")).unwrap_or(0),
                num: tuplet.map(|t| attr_int(t, "num").max(1) as u32).unwrap_or(1),
                den: tuplet.map(|t| attr_int(t, "den").max(1) as u32).unwrap_or(1),
            },
        );
    }
}

// ===================================================== GPIF -> modelo

fn rhythm_to_duration(r: &GRhythm) -> Duration {
    Duration { value: r.value, dotted: r.dots == 1, double_dotted: r.dots == 2, enters: r.num, times: r.den }
}

fn parse_bar(ctx: &Ctx, bar: &GBar, measure_start: u64, track: &Track) -> Measure {
    let mut m = Measure::default();
    for v in 0..2usize {
        let Some(&voice_id) = bar.voices.get(v) else { continue };
        if voice_id < 0 {
            continue;
        }
        let Some(beat_ids) = ctx.voices.get(&voice_id) else { continue };

        let mut start = measure_start;
        // (notas de adorno, sobre el tiempo, duración en ticks) del pulso de adorno anterior.
        let mut prev_grace: Option<(Vec<&GNote>, bool, u64)> = None;
        for bid in beat_ids {
            let Some(beat) = ctx.beats.get(bid) else { continue };
            let Some(rhythm) = ctx.rhythms.get(&beat.rhythm) else { continue };

            let gnotes: Vec<&GNote> = beat.notes.iter().filter_map(|id| ctx.notes.get(id)).collect();

            // Las notas de adorno van en un pulso aparte: se aplican al siguiente.
            if let Some(kind) = &beat.grace {
                let ticks = rhythm_to_duration(rhythm).time().clamp(60, 480);
                prev_grace = Some((gnotes, kind == "OnBeat", ticks));
                continue;
            }

            let idx = match m.beats.iter().position(|b| b.start == start) {
                Some(i) => i,
                None => {
                    m.beats.push(Beat { start, voices: vec![Voice::default(), Voice::default()], ..Default::default() });
                    m.beats.len() - 1
                }
            };
            let duration = rhythm_to_duration(rhythm);
            {
                let b = &mut m.beats[idx];
                b.voices[v].empty = false;
                b.voices[v].duration = duration;
                if !beat.text.trim().is_empty() {
                    b.text = Some(beat.text.trim().to_string());
                }
                if let Some(c) = beat.chord.and_then(|id| ctx.chords.get(&id)) {
                    b.chord = Some(c.clone());
                }
            }

            for g in gnotes {
                if let Some(mut note) = build_note(g, beat, &m.beats[idx], track) {
                    if let Some((graces, on_beat, ticks)) = &prev_grace {
                        note.effect.grace = graces.iter().find(|n| n.string == g.string).map(|n| Grace {
                            fret: n.fret.clamp(0, 99) as u8,
                            ticks: *ticks,
                            on_beat: *on_beat,
                            dead: n.muted,
                        });
                    }
                    m.beats[idx].voices[v].notes.push(note);
                }
            }
            prev_grace = None;
            start += duration.time();
        }
    }
    m.beats.sort_by_key(|b| b.start);
    m
}

fn build_note(g: &GNote, beat: &GBeat, tg_beat: &Beat, track: &Track) -> Option<Note> {
    let string_count = track.strings.len() as i32;
    let (value, string) = if g.string >= 0 && g.fret >= 0 {
        (g.fret, string_count - g.string)
    } else {
        let mut gm = -1;
        if g.midi >= 0 {
            gm = g.midi;
        } else if g.tone >= 0 && g.octave >= 0 {
            gm = g.tone + 12 * g.octave - 12;
        } else if g.element >= 0 {
            for &(midi, element, variation) in &DRUMKIT {
                if element == g.element && variation == g.variation {
                    gm = midi;
                }
            }
        }
        let mut found = None;
        if gm >= 0 {
            for (i, &tuning) in track.strings.iter().enumerate() {
                if gm >= tuning {
                    let number = i as u8 + 1;
                    let used = tg_beat.voices.iter().any(|v| v.notes.iter().any(|n| n.string == number));
                    if !used {
                        found = Some((gm - tuning, number as i32));
                        break;
                    }
                }
            }
        }
        found.unwrap_or((-1, -1))
    };
    if value < 0 || string <= 0 || string > string_count {
        return None;
    }

    // Trino: semitonos entre la nota y la nota alternada (GPX guarda la nota MIDI destino).
    let pitch = track.strings.get(string as usize - 1).copied().unwrap_or(0) + value;
    let trill = (g.trill > 0).then(|| Trill {
        semitones: (g.trill - pitch).clamp(-12, 12),
        ticks: if g.trill_duration > 0 { (g.trill_duration as u64 * 2).clamp(60, 960) } else { 240 },
    });
    let slide = if g.slide {
        Some(match g.slide_flags {
            2 => SlideKind::Legato,
            4 => SlideKind::OutDown,
            8 => SlideKind::OutUp,
            16 => SlideKind::InBelow,
            32 => SlideKind::InAbove,
            _ => SlideKind::Shift,
        })
    } else {
        None
    };
    let harmonic = match g.harmonic.as_str() {
        "" => None,
        "Artificial" => Some(Harmonic::Artificial),
        "Pinch" => Some(Harmonic::Pinch),
        _ => Some(Harmonic::Natural),
    };
    Some(Note {
        string: string as u8,
        fret: value.min(99) as u8,
        velocity: beat.dynamic,
        tied: g.tie_destination,
        effect: Effects {
            dead: g.muted,
            ghost: g.ghost,
            accent: g.accent == 8,
            heavy_accent: g.accent == 4,
            hammer: g.hammer,
            slide,
            let_ring: g.let_ring,
            palm_mute: g.palm_muted,
            staccato: g.accent == 1,
            vibrato: g.vibrato,
            fade_in: beat.fade_in,
            bend: if g.bend_enabled { build_curve(&g.bend) } else { None },
            tremolo_bar: if beat.whammy_enabled { build_curve(&beat.whammy) } else { None },
            grace: None,
            trill,
            tremolo_picking: beat.tremolo,
            harmonic,
            tapping: g.tapped,
            slapping: beat.slapped,
            popping: beat.popped,
        },
    })
}

/// Curva de GPX (bend o barra de trémolo): posiciones 0..100 y alturas de 50 unidades por semitono.
fn build_curve(c: &GCurve) -> Option<Bend> {
    const POSITION: f32 = 100.0;
    let (origin, destination) = (c.origin?, c.destination?);
    let value = |v: i32| v as f32 / crate::gp::GP_BEND_UNITS_PER_SEMITONE;
    let pos = |p: i32| (p as f32 / POSITION).clamp(0.0, 1.0);

    let mut pts = vec![(0.0, value(origin))];
    if let Some(o) = c.origin_offset {
        pts.push((pos(o), value(origin)));
    }
    if let Some(mid) = c.middle {
        let default = (POSITION / 2.0).round() as i32;
        // Un offset de 12 es el valor "sin usar" que escribe Guitar Pro.
        if c.middle_offset1 != Some(12) {
            pts.push((pos(c.middle_offset1.unwrap_or(default)), value(mid)));
        }
        if c.middle_offset2 != Some(12) {
            pts.push((pos(c.middle_offset2.unwrap_or(default)), value(mid)));
        }
    }
    if let Some(d) = c.destination_offset.filter(|&d| (d as f32) < POSITION) {
        pts.push((pos(d), value(destination)));
    }
    pts.push((1.0, value(destination)));
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(Bend { points: pts })
}

/// Si el primer compás está incompleto (anacrusa), TuxGuitar lo alinea al final.
fn fix_first_measure(measure: &mut Measure, start: u64, length: u64) {
    let end = start + length;
    let max_end = measure
        .beats
        .iter()
        .flat_map(|b| b.voices.iter().filter(|v| !v.empty).map(move |v| b.start + v.duration.time()))
        .max()
        .unwrap_or(0);
    if max_end < end {
        let shift = end - max_end;
        for b in &mut measure.beats {
            b.start += shift;
        }
    }
}

//! Playlist de ensayo: una lista de tabs, cada una con su propia configuración (pistas que suenan, volúmenes,
//! velocidad, metrónomo, cuenta previa, bucle y transposición). Se guarda en un archivo de texto legible
//! que se puede copiar a otro equipo: las rutas dentro de la carpeta de la playlist se guardan relativas.

use std::path::{Path, PathBuf};

/// Configuración de una tab dentro de la playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    /// Pistas que suenan (bit i = pista i).
    pub mask: u64,
    /// Ganancia en % de las pistas que no están al 100 %: (pista, %).
    pub gains: Vec<(usize, u8)>,
    /// Velocidad en %.
    pub speed: u32,
    pub metronome: bool,
    pub count_in: bool,
    /// Compases (desde, hasta) del bucle, base 0.
    pub loop_measures: Option<(usize, usize)>,
    pub transpose: i32,
    /// Nota libre (p. ej. "tocar solo estrofas").
    pub note: String,
}

impl Entry {
    pub fn new(path: PathBuf) -> Entry {
        Entry { path, mask: u64::MAX, gains: Vec::new(), speed: 100, metronome: false, count_in: false, loop_measures: None, transpose: 0, note: String::new() }
    }

    /// Nombre para mostrar en la lista.
    pub fn title(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.display().to_string())
    }

    /// Cuántas pistas (de las `total`) suenan.
    pub fn active_tracks(&self, total: usize) -> usize {
        (0..total.min(64)).filter(|&i| self.mask & (1u64 << i) != 0).count()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Playlist {
    pub name: String,
    pub entries: Vec<Entry>,
}

fn parse_bool(v: &str) -> bool {
    matches!(v.trim(), "1" | "true" | "si" | "sí")
}

impl Playlist {
    /// Interpreta el texto del archivo; `base` es la carpeta de la playlist (para las rutas relativas).
    /// Lo desconocido o inválido se ignora.
    pub fn parse(text: &str, base: &Path) -> Playlist {
        let mut pl = Playlist::default();
        let mut cur: Option<Entry> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[song]" {
                if let Some(e) = cur.take() {
                    pl.entries.push(e);
                }
                cur = Some(Entry::new(PathBuf::new()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let Some(e) = cur.as_mut() else {
                if key == "name" {
                    pl.name = value.to_string();
                }
                continue;
            };
            match key {
                "file" => {
                    let p = PathBuf::from(value);
                    e.path = if p.is_absolute() { p } else { base.join(p) };
                }
                "mask" => e.mask = u64::from_str_radix(value, 16).unwrap_or(u64::MAX),
                "gain" => {
                    if let Some((t, g)) = value.split_once(':') {
                        if let (Ok(t), Ok(g)) = (t.trim().parse::<usize>(), g.trim().parse::<u8>()) {
                            if t < 64 && g <= 200 {
                                e.gains.push((t, g));
                            }
                        }
                    }
                }
                "speed" => e.speed = value.parse::<u32>().map(|v| v.clamp(30, 150)).unwrap_or(100),
                "metronome" => e.metronome = parse_bool(value),
                "count_in" => e.count_in = parse_bool(value),
                "loop" => {
                    e.loop_measures = value.split_once('-').and_then(|(a, b)| Some((a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?)));
                }
                "transpose" => e.transpose = value.parse::<i32>().map(|v| v.clamp(-12, 12)).unwrap_or(0),
                "note" => e.note = value.to_string(),
                _ => {}
            }
        }
        if let Some(e) = cur {
            pl.entries.push(e);
        }
        pl.entries.retain(|e| !e.path.as_os_str().is_empty());
        pl
    }

    /// Texto del archivo; las rutas bajo `base` se escriben relativas.
    pub fn serialize(&self, base: &Path) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let mut out = String::from("# NeoGuitar playlist\n");
        out.push_str(&format!("name={}\n", self.name));
        for e in &self.entries {
            out.push_str("\n[song]\n");
            let shown = e.path.strip_prefix(base).unwrap_or(&e.path);
            out.push_str(&format!("file={}\n", shown.display()));
            out.push_str(&format!("mask={:x}\n", e.mask));
            for (t, g) in &e.gains {
                out.push_str(&format!("gain={t}:{g}\n"));
            }
            out.push_str(&format!("speed={}\nmetronome={}\ncount_in={}\ntranspose={}\n", e.speed, b(e.metronome), b(e.count_in), e.transpose));
            if let Some((a, z)) = e.loop_measures {
                out.push_str(&format!("loop={a}-{z}\n"));
            }
            if !e.note.is_empty() {
                out.push_str(&format!("note={}\n", e.note.replace(['\n', '\r'], " ")));
            }
        }
        out
    }

    pub fn load(path: &Path) -> Result<Playlist, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let mut pl = Playlist::parse(&text, base);
        if pl.name.is_empty() {
            pl.name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
        Ok(pl)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::write(path, self.serialize(base)).map_err(|e| format!("{}: {e}", path.display()))
    }
}

//! Ajustes persistentes: un archivo de texto `clave=valor` en la carpeta de configuración.

use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub soundfont: Option<PathBuf>,
    pub zoom: f32,
    pub show_staff: bool,
    pub show_side: bool,
    pub follow: bool,
    /// Vista de varias pistas a la vez.
    pub multi_view: bool,
    /// Velocidad de reproducción en porcentaje.
    pub speed: f32,
    pub volume: f32,
    pub metronome: bool,
    pub count_in: bool,
    /// Entrenador de velocidad: sube `trainer_step` % cada vuelta del bucle hasta `trainer_max` %.
    pub trainer_on: bool,
    pub trainer_step: u32,
    pub trainer_max: u32,
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            soundfont: None,
            zoom: 1.0,
            show_staff: true,
            show_side: true,
            follow: true,
            multi_view: false,
            speed: 100.0,
            volume: 1.0,
            metronome: false,
            count_in: false,
            trainer_on: false,
            trainer_step: 5,
            trainer_max: 100,
            recent: Vec::new(),
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "1" | "true" | "si" | "sí" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

impl Settings {
    /// Interpreta el texto del archivo; lo desconocido o inválido se ignora (queda el valor por defecto).
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "soundfont" if !value.is_empty() => s.soundfont = Some(PathBuf::from(value)),
                "zoom" => s.zoom = value.parse::<f32>().map(|v| v.clamp(0.6, 2.0)).unwrap_or(s.zoom),
                "show_staff" => s.show_staff = parse_bool(value).unwrap_or(s.show_staff),
                "show_side" => s.show_side = parse_bool(value).unwrap_or(s.show_side),
                "follow" => s.follow = parse_bool(value).unwrap_or(s.follow),
                "multi_view" => s.multi_view = parse_bool(value).unwrap_or(s.multi_view),
                "speed" => s.speed = value.parse::<f32>().map(|v| v.clamp(30.0, 150.0)).unwrap_or(s.speed),
                "volume" => s.volume = value.parse::<f32>().map(|v| v.clamp(0.0, 1.5)).unwrap_or(s.volume),
                "metronome" => s.metronome = parse_bool(value).unwrap_or(s.metronome),
                "count_in" => s.count_in = parse_bool(value).unwrap_or(s.count_in),
                "trainer_on" => s.trainer_on = parse_bool(value).unwrap_or(s.trainer_on),
                "trainer_step" => s.trainer_step = value.parse::<u32>().map(|v| v.clamp(1, 25)).unwrap_or(s.trainer_step),
                "trainer_max" => s.trainer_max = value.parse::<u32>().map(|v| v.clamp(30, 150)).unwrap_or(s.trainer_max),
                "recent" if !value.is_empty() => {
                    let p = PathBuf::from(value);
                    if !s.recent.contains(&p) && s.recent.len() < MAX_RECENT {
                        s.recent.push(p);
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let mut out = String::from("# Ajustes de NeoGuitar\n");
        if let Some(p) = &self.soundfont {
            out.push_str(&format!("soundfont={}\n", p.display()));
        }
        out.push_str(&format!("zoom={:.2}\n", self.zoom));
        out.push_str(&format!("show_staff={}\nshow_side={}\nfollow={}\nmulti_view={}\n", b(self.show_staff), b(self.show_side), b(self.follow), b(self.multi_view)));
        out.push_str(&format!("speed={:.0}\nvolume={:.2}\n", self.speed, self.volume));
        out.push_str(&format!("metronome={}\ncount_in={}\n", b(self.metronome), b(self.count_in)));
        out.push_str(&format!("trainer_on={}\ntrainer_step={}\ntrainer_max={}\n", b(self.trainer_on), self.trainer_step, self.trainer_max));
        for p in &self.recent {
            out.push_str(&format!("recent={}\n", p.display()));
        }
        out
    }

    /// Archivo de ajustes: `NEOGUITAR_CONFIG_DIR`, o `%APPDATA%\NeoGuitar`, o `~/.config/neoguitar`.
    pub fn path() -> Option<PathBuf> {
        let dir = if let Some(d) = std::env::var_os("NEOGUITAR_CONFIG_DIR") {
            PathBuf::from(d)
        } else if let Some(d) = std::env::var_os("APPDATA") {
            PathBuf::from(d).join("NeoGuitar")
        } else {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            PathBuf::from(home).join(".config").join("neoguitar")
        };
        Some(dir.join("settings.txt"))
    }

    pub fn load() -> Settings {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.serialize())
    }

    /// Pone `path` al frente de la lista de recientes.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}

//! Small settings file in the app's private storage: chosen SoundFont, recent songs and view options.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub soundfont: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub count_in: bool,
    pub count_in_measures: u32,
    pub count_in_delay: f32,
    pub show_staff: bool,
    pub zoom: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { soundfont: None, recent: Vec::new(), count_in: false, count_in_measures: 1, count_in_delay: 0.0, show_staff: false, zoom: 1.0 }
    }
}

const MAX_RECENT: usize = 8;

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "soundfont" if !v.trim().is_empty() => s.soundfont = Some(v.trim().into()),
                "recent" if !v.trim().is_empty() => s.recent.push(v.trim().into()),
                "count_in" => s.count_in = v.trim() == "1",
                "count_in_measures" => s.count_in_measures = v.trim().parse::<u32>().map(|n| n.clamp(1, 2)).unwrap_or(1),
                "count_in_delay" => s.count_in_delay = v.trim().parse::<f32>().map(|d| d.clamp(0.0, 10.0)).unwrap_or(0.0),
                "show_staff" => s.show_staff = v.trim() == "1",
                "zoom" => s.zoom = v.trim().parse::<f32>().map(|z| z.clamp(0.6, 2.0)).unwrap_or(1.0),
                _ => {}
            }
        }
        s.recent.truncate(MAX_RECENT);
        s
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        if let Some(sf) = &self.soundfont {
            out += &format!("soundfont={}\n", sf.display());
        }
        for r in &self.recent {
            out += &format!("recent={}\n", r.display());
        }
        out += &format!("count_in={}\ncount_in_measures={}\ncount_in_delay={}\nshow_staff={}\nzoom={}\n", self.count_in as u8, self.count_in_measures, self.count_in_delay, self.show_staff as u8, self.zoom);
        out
    }

    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join("settings.txt")).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::write(dir.join("settings.txt"), self.serialize());
    }

    /// Puts a song at the front of the recent list without duplicating it.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}
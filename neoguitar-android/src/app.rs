//! First slice of the practice screen: play/pause, tempo, metronome and which tracks are heard.

use android_activity::AndroidApp;
use eframe::egui;
use neoguitar_core::audio::{Player, Status};
use neoguitar_core::model::Song;
use std::ffi::CString;
use std::io::Read;
use std::path::PathBuf;

const SOUNDFONT_ASSET: &str = "GeneralUser-GS.sf2";
const DEMO_ASSET: &str = "demo.gp5";

pub struct App {
    player: Player,
    song: Option<Song>,
    error: Option<String>,
    speed: f32,
}

fn read_asset(android: &AndroidApp, name: &str) -> Result<Vec<u8>, String> {
    let c = CString::new(name).map_err(|e| e.to_string())?;
    let mut asset = android.asset_manager().open(&c).ok_or_else(|| format!("missing asset {name}"))?;
    let mut bytes = Vec::new();
    asset.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The SoundFont lives inside the APK; it is copied once to private storage because the synthesizer reads files.
fn soundfont_path(android: &AndroidApp) -> Result<PathBuf, String> {
    let dir = android.internal_data_path().ok_or("no private storage")?;
    let path = dir.join(SOUNDFONT_ASSET);
    let expected = android.asset_manager().open(&CString::new(SOUNDFONT_ASSET).unwrap()).map(|a| a.length() as u64);
    let current = std::fs::metadata(&path).map(|m| m.len()).ok();
    if current.is_none() || current != expected.map(|n| n) {
        let bytes = read_asset(android, SOUNDFONT_ASSET)?;
        std::fs::write(&path, bytes).map_err(|e| format!("could not copy the SoundFont: {e}"))?;
    }
    Ok(path)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, android: AndroidApp) -> Self {
        cc.egui_ctx.set_pixels_per_point(2.6);
        let mut error = None;
        let font = soundfont_path(&android).map_err(|e| error = Some(e)).ok();
        let player = Player::new(font);
        let song = read_asset(&android, DEMO_ASSET).and_then(|b| neoguitar_core::parse(&b).map_err(|e| e.to_string()));
        let song = match song {
            Ok(s) => Some(s),
            Err(e) => {
                error = Some(e);
                None
            }
        };
        let mut app = App { player, song, error, speed: 100.0 };
        if let Some(s) = &app.song {
            app.player.set_song(s);
        }
        app
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.player.poll();
        if self.player.is_playing() || matches!(self.player.status, Status::Loading(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(24.0);
            ui.heading("NeoGuitar");
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
            }
            match &self.player.status {
                Status::Loading(_) => {
                    ui.label("Loading sounds…");
                }
                Status::Error(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                Status::NoSoundFont => {
                    ui.label("No SoundFont");
                }
                Status::Ready(_) => {}
            }
            let Some(song) = &self.song else { return };
            ui.label(format!("{} - {}", song.artist, song.title));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let playing = self.player.is_playing();
                if ui.add_enabled(self.player.ready(), egui::Button::new(if playing { "Pause" } else { "Play" }).min_size(egui::vec2(120.0, 44.0))).clicked() {
                    if playing {
                        self.player.pause();
                    } else {
                        self.player.play();
                    }
                }
                if ui.add(egui::Button::new("Stop").min_size(egui::vec2(100.0, 44.0))).clicked() {
                    self.player.stop();
                }
                let mut metro = self.player.metronome();
                if ui.checkbox(&mut metro, "Metronome").changed() {
                    self.player.set_metronome(metro);
                }
            });
            ui.add_space(8.0);
            if ui.add(egui::Slider::new(&mut self.speed, 40.0..=150.0).suffix("%").text("Tempo")).changed() {
                self.player.set_speed(self.speed / 100.0);
            }
            ui.add_space(8.0);
            ui.label("Instruments");
            for (i, t) in song.tracks.iter().enumerate() {
                let mut on = self.player.track_enabled(i);
                if ui.checkbox(&mut on, &t.name).changed() {
                    self.player.set_track_enabled(i, on);
                }
            }
        });
    }
}

//! Practice screen: the tablature on top, transport (play, tempo, metronome, count-in, loop) at the bottom
//! and a sheet with the instruments (mute, solo, volume, sound) on the right.

use android_activity::AndroidApp;
use egui::{Align, Layout, Margin, RichText};
use neoguitar_core::audio::{Player, Status};
use neoguitar_core::model::{Song, Track};
use neoguitar_ui::tab_view::{self, ViewAction};
use neoguitar_ui::theme::{self, Icon, IconStyle};
use std::ffi::CString;
use std::io::Read;
use std::path::PathBuf;

const SOUNDFONT_ASSET: &str = "GeneralUser-GS.sf2";
const DEMO_ASSET: &str = "demo.gp5";

pub struct App {
    android: AndroidApp,
    player: Player,
    song: Option<Song>,
    track: usize,
    error: Option<String>,
    zoom: f32,
    show_staff: bool,
    count_in: bool,
    follow: bool,
    last_followed: Option<usize>,
    jump_to: Option<usize>,
    sheet: bool,
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
    if current.is_none() || current != expected {
        let bytes = read_asset(android, SOUNDFONT_ASSET)?;
        std::fs::write(&path, bytes).map_err(|e| format!("could not copy the SoundFont: {e}"))?;
    }
    Ok(path)
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Pixels of the screen covered by the system bars or a camera cut-out, in egui points.
struct Insets {
    left: i8,
    top: i8,
    right: i8,
    bottom: i8,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, android: AndroidApp) -> Self {
        theme::apply(&cc.egui_ctx);
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
        let mut app = App { android, player, song, track: 0, error, zoom: 1.0, show_staff: false, count_in: false, follow: true, last_followed: None, jump_to: None, sheet: false };
        if let Some(s) = &app.song {
            app.player.set_song(s);
            app.track = s.tracks.iter().position(|t| !t.is_percussion).unwrap_or(0);
        }
        app
    }

    fn insets(&self, ctx: &egui::Context) -> Insets {
        let ppp = ctx.pixels_per_point();
        let screen = ctx.viewport_rect();
        let r = self.android.content_rect();
        let to_pt = |px: i32| (px as f32 / ppp).max(0.0);
        let clamp = |v: f32| v.round().clamp(0.0, 120.0) as i8;
        // `content_rect` is the window area free of system bars; what is left over on each side is the inset.
        Insets {
            left: clamp(to_pt(r.left)),
            top: clamp(to_pt(r.top)),
            right: clamp(screen.right() - to_pt(r.right)),
            bottom: clamp(screen.bottom() - to_pt(r.bottom)),
        }
    }

    fn toggle_play(&mut self) {
        if self.player.is_playing() {
            self.player.pause();
        } else if self.player.ready() {
            if self.count_in {
                self.player.play_with_count_in();
            } else {
                self.player.play();
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(song) = &self.song {
                let label = |t: &Track| t.name.clone();
                let current = song.tracks.get(self.track).map(label).unwrap_or_default();
                egui::ComboBox::from_id_salt("track").width(170.0).selected_text(current).show_ui(ui, |ui| {
                    for (i, t) in song.tracks.iter().enumerate() {
                        ui.selectable_value(&mut self.track, i, label(t));
                    }
                });
                let title = if song.artist.is_empty() { song.title.clone() } else { format!("{} – {}", song.artist, song.title) };
                let room = (ui.available_width() - 330.0).max(0.0);
                if room > 60.0 {
                    ui.add_sized([room, 20.0], egui::Label::new(RichText::new(title).color(theme::MUTED)).truncate());
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::chip(ui, Icon::Sliders, self.sheet, "Instruments").clicked() {
                    self.sheet = !self.sheet;
                }
                if theme::icon_button(ui, Icon::Staff, 32.0, if self.show_staff { IconStyle::FlatOn } else { IconStyle::Flat }, true).clicked() {
                    self.show_staff = !self.show_staff;
                }
                if theme::icon_button(ui, Icon::Plus, 32.0, IconStyle::Flat, true).clicked() {
                    self.zoom = (self.zoom + 0.1).min(2.0);
                }
                if theme::icon_button(ui, Icon::Minus, 32.0, IconStyle::Flat, true).clicked() {
                    self.zoom = (self.zoom - 0.1).max(0.6);
                }
            });
        });
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        let total = self.player.timeline.total;
        let can_play = self.player.ready() && total > 0.0;
        let playing = self.player.is_playing();
        let pos = self.player.position().min(total);

        let loop_frac = self.player.loop_measures.and_then(|(a, b)| {
            let t0 = self.player.timeline.time_of_measure(a)?;
            let t1 = self.player.timeline.first_end_of_measure(b)?;
            (total > 0.0).then(|| ((t0 / total) as f32, (t1 / total) as f32))
        });
        ui.horizontal(|ui| {
            ui.add_sized([40.0, 24.0], egui::Label::new(RichText::new(clock(pos)).monospace().color(theme::TEXT)));
            let w = (ui.available_width() - 48.0).max(50.0);
            let mut new_frac = None;
            ui.allocate_ui_with_layout(egui::vec2(w, 26.0), Layout::left_to_right(Align::Center), |ui| {
                new_frac = theme::seek_bar(ui, if total > 0.0 { (pos / total) as f32 } else { 0.0 }, loop_frac, can_play);
            });
            ui.add_sized([40.0, 24.0], egui::Label::new(RichText::new(clock(total)).monospace().color(theme::MUTED)));
            if let Some(f) = new_frac {
                self.player.seek(f as f64 * total);
                self.last_followed = None;
            }
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if theme::icon_button(ui, Icon::Stop, 34.0, IconStyle::Flat, can_play).clicked() {
                self.player.stop();
                self.last_followed = None;
            }
            if theme::icon_button(ui, if playing { Icon::Pause } else { Icon::Play }, 46.0, IconStyle::Round, can_play).clicked() {
                self.toggle_play();
            }
            ui.add_space(6.0);

            // Tempo: ±5 % buttons and a slider.
            let mut speed = (self.player.speed * 100.0).round();
            if theme::icon_button(ui, Icon::Minus, 34.0, IconStyle::Flat, true).clicked() {
                speed = (speed - 5.0).max(30.0);
                self.player.set_speed(speed / 100.0);
            }
            ui.add_sized([58.0, 24.0], egui::Label::new(RichText::new(format!("{speed:.0}%")).size(15.0).strong().color(theme::TEXT)));
            if theme::icon_button(ui, Icon::Plus, 34.0, IconStyle::Flat, true).clicked() {
                speed = (speed + 5.0).min(150.0);
                self.player.set_speed(speed / 100.0);
            }
            ui.spacing_mut().slider_width = (ui.available_width() - 520.0).clamp(60.0, 260.0);
            if ui.add(egui::Slider::new(&mut speed, 30.0..=150.0).show_value(false)).changed() {
                self.player.set_speed(speed / 100.0);
            }
            ui.add_space(6.0);

            let metronome = self.player.metronome();
            if theme::chip(ui, Icon::Metronome, metronome, "Metronome").clicked() {
                self.player.set_metronome(!metronome);
            }
            if theme::chip(ui, Icon::Timer, self.count_in, "Count-in").clicked() {
                self.count_in = !self.count_in;
            }
            if theme::chip(ui, Icon::Follow, self.follow, "Follow").clicked() {
                self.follow = !self.follow;
            }
            if self.player.loop_measures.is_some() && theme::chip(ui, Icon::Loop, true, "Clear loop").clicked() {
                self.player.clear_loop();
            }
        });
    }

    /// Sheet with one row per track: mute, solo, volume and instrument.
    fn instruments(&mut self, ui: &mut egui::Ui) {
        let Some(song) = &self.song else { return };
        ui.label(RichText::new("What you hear").strong().size(16.0));
        ui.label(RichText::new("Mute the part you are playing yourself.").size(12.0).color(theme::MUTED));
        ui.add_space(6.0);
        let total = song.tracks.len();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, t) in song.tracks.iter().enumerate() {
                let enabled = self.player.track_enabled(i);
                theme::card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let [r, g, b] = t.color;
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 5.0, theme::track_color([r, g, b]));
                        ui.label(RichText::new(&t.name).strong().color(if enabled { theme::TEXT } else { theme::MUTED }));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let solo = (0..total).filter(|&k| self.player.track_enabled(k)).count() == 1 && enabled;
                            if theme::icon_button(ui, Icon::Headphones, 38.0, if solo { IconStyle::FlatOn } else { IconStyle::Flat }, true).clicked() {
                                self.player.toggle_solo(i, total);
                            }
                            let (icon, style) = if enabled { (Icon::Speaker, IconStyle::Flat) } else { (Icon::SpeakerOff, IconStyle::Danger) };
                            if theme::icon_button(ui, icon, 38.0, style, true).clicked() {
                                self.player.set_track_enabled(i, !enabled);
                            }
                        });
                    });
                    let mut gain = self.player.track_gain(i) as f32;
                    ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(80.0);
                    if ui.add(egui::Slider::new(&mut gain, 0.0..=150.0).suffix("%").integer()).changed() {
                        self.player.set_track_gain(i, gain as u8);
                    }
                    if !t.is_percussion {
                        let program = self.player.mix_program(i).unwrap_or(t.program.clamp(0, 127) as u8) as usize;
                        egui::ComboBox::from_id_salt(("program", i)).width(ui.available_width() - 8.0).selected_text(neoguitar_core::gm::NAMES[program.min(127)]).show_ui(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                for (k, name) in neoguitar_core::gm::NAMES.iter().enumerate() {
                                    if ui.selectable_label(program == k, format!("{:>3}  {name}", k + 1)).clicked() {
                                        let (pan, rev) = (self.player.mix_pan(i), self.player.mix_reverb(i));
                                        self.player.set_mix(i, pan, Some(k as u8), rev);
                                    }
                                }
                            });
                        });
                    }
                });
                ui.add_space(4.0);
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.player.poll();
        if self.player.is_playing() || matches!(self.player.status, Status::Loading(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        let ins = self.insets(&ctx);
        let margin = |l: i8, t: i8, r: i8, b: i8| Margin { left: l.max(ins.left), top: t.max(ins.top), right: r.max(ins.right), bottom: b.max(ins.bottom) };

        egui::Panel::top("top").frame(theme::panel_frame(margin(12, 6, 12, 6))).show_separator_line(true).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("transport").frame(theme::panel_frame(margin(14, 6, 14, 8))).show_separator_line(true).show(ui, |ui| self.transport(ui));
        if self.sheet {
            egui::Panel::right("sheet")
                .default_size(340.0)
                .size_range(260.0..=520.0)
                .frame(theme::panel_frame(Margin { left: 12, top: 10, right: 12.max(ins.right), bottom: 8 }))
                .show(ui, |ui| self.instruments(ui));
        }

        // Where the playback is in the score, so the view follows it.
        let pos = self.player.position();
        let playhead = if self.song.is_some() && (self.player.is_playing() || pos > 0.0) { self.player.timeline.position(pos) } else { None };
        let mut scroll_to = self.jump_to.take();
        if self.follow && self.player.is_playing() {
            if let Some((m, _)) = playhead {
                if self.last_followed != Some(m) {
                    self.last_followed = Some(m);
                    scroll_to = Some(m + 1);
                }
            }
        }

        let mut action = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::SCORE_BG).inner_margin(Margin { left: 10.max(ins.left), top: 6, right: if self.sheet { 10 } else { 10.max(ins.right) }, bottom: 4 })).show(ui, |ui| {
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
            }
            match &self.player.status {
                Status::Loading(_) => {
                    ui.label(RichText::new("Loading sounds…").color(theme::MUTED));
                }
                Status::Error(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                _ => {}
            }
            if let Some(song) = &self.song {
                if let Some(track) = song.tracks.get(self.track) {
                    action = tab_view::show(ui, song, &[track], self.zoom, scroll_to, playhead, self.player.loop_measures, self.show_staff, self.player.transpose);
                }
            }
        });

        match action {
            Some(ViewAction::Seek(m)) => {
                if let Some(t) = self.player.timeline.time_of_measure(m) {
                    self.player.seek(t);
                    self.last_followed = None;
                }
            }
            Some(ViewAction::LoopFrom(m)) => {
                let end = self.player.loop_measures.map(|(_, b)| b.max(m)).unwrap_or(m);
                self.player.set_loop(m, end);
            }
            Some(ViewAction::LoopTo(m)) => {
                let start = self.player.loop_measures.map(|(a, _)| a.min(m)).unwrap_or(m);
                self.player.set_loop(start, m);
            }
            Some(ViewAction::ClearLoop) => self.player.clear_loop(),
            None => {}
        }
    }
}

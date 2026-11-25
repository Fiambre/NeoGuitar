//! Practice screen: the tablature on top, transport (play, tempo, metronome, count-in, loop) at the bottom
//! and a side sheet with the mix (what you hear), the lyrics and the settings.

use crate::platform::{self, Picked};
use crate::settings::Settings;
use android_activity::AndroidApp;
use egui::{Align, Layout, Margin, RichText};
use neoguitar_core::audio::{Player, Status};
use neoguitar_core::lyrics::{self, Lyrics};
use neoguitar_core::model::Song;
use neoguitar_ui::tab_view::{self, ViewAction};
use neoguitar_ui::theme::{self, Icon, IconStyle};
use std::ffi::CString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

const SOUNDFONT_ASSET: &str = "GeneralUser-GS.sf2";
const DEMO_ASSET: &str = "demo.gp5";

#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    Mix,
    Lyrics,
    Settings,
}

/// Loaded lyrics and the sync points the user pinned: (lyric time, tablature time).
struct LyricsState {
    data: Lyrics,
    anchors: Vec<(f64, f64)>,
}

pub struct App {
    android: AndroidApp,
    data_dir: PathBuf,
    settings: Settings,
    player: Player,
    song: Option<Song>,
    track: usize,
    notice: Option<String>,
    /// Waiting for the file picker or a copy to finish.
    busy: Option<&'static str>,
    last_poll: f64,
    follow: bool,
    last_followed: Option<usize>,
    sheet: Option<Sheet>,
    expanded: Option<usize>,
    lyrics: Option<LyricsState>,
    lyr_artist: String,
    lyr_title: String,
    lyr_job: Option<Receiver<Result<Option<Lyrics>, String>>>,
    lyr_msg: Option<String>,
    /// Is the optional "NeoGuitar Tabs" companion app installed? (checked when the open menu is tapped)
    tabs_companion: bool,
    lyr_last: Option<usize>,
}

fn read_asset(android: &AndroidApp, name: &str) -> Result<Vec<u8>, String> {
    let c = CString::new(name).map_err(|e| e.to_string())?;
    let mut asset = android.asset_manager().open(&c).ok_or_else(|| format!("missing asset {name}"))?;
    let mut bytes = Vec::new();
    asset.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The bundled SoundFont lives inside the APK; it is copied once to private storage because the synthesizer reads files.
fn bundled_soundfont(android: &AndroidApp, dir: &Path) -> Result<PathBuf, String> {
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

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
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
        let data_dir = android.internal_data_path().unwrap_or_else(|| std::env::temp_dir());
        let settings = Settings::load(&data_dir);
        let mut notice = None;

        let bundled = bundled_soundfont(&android, &data_dir).map_err(|e| notice = Some(e)).ok();
        let font = settings.soundfont.clone().filter(|p| p.is_file()).or(bundled);
        let mut player = Player::new(font);
        player.count_in_measures = settings.count_in_measures;
        player.count_in_delay = settings.count_in_delay;

        let mut app = App {
            android,
            data_dir,
            settings,
            player,
            song: None,
            track: 0,
            notice,
            busy: None,
            last_poll: 0.0,
            follow: true,
            last_followed: None,
            sheet: None,
            expanded: None,
            lyrics: None,
            lyr_artist: String::new(),
            lyr_title: String::new(),
            lyr_job: None,
            lyr_msg: None,
            tabs_companion: false,
            lyr_last: None,
        };
        // A file sent to the app wins over the last song; the demo is the fallback.
        let start = app.settings.recent.first().cloned().filter(|p| p.is_file());
        let opened = start.map(|p| app.open_song(&p)).unwrap_or(false);
        if !opened {
            app.open_demo();
        }
        app
    }

    fn set_song(&mut self, song: Song) {
        self.player.set_song(&song);
        self.track = song.tracks.iter().position(|t| !t.is_percussion).unwrap_or(0);
        self.lyr_artist = song.artist.clone();
        self.lyr_title = song.title.clone();
        self.lyrics = None;
        self.lyr_msg = None;
        self.lyr_last = None;
        self.last_followed = None;
        self.expanded = None;
        // Lyrics found before are reused without asking the network again.
        if !song.title.trim().is_empty() {
            if let Some(l) = lyrics::cache_load(&self.lyrics_dir(), &song.artist, &song.title) {
                self.lyrics = Some(LyricsState { data: l, anchors: Vec::new() });
            }
        }
        self.song = Some(song);
    }

    fn open_demo(&mut self) {
        match read_asset(&self.android, DEMO_ASSET).and_then(|b| neoguitar_core::parse(&b).map_err(|e| e.to_string())) {
            Ok(song) => self.set_song(song),
            Err(e) => self.notice = Some(e),
        }
    }

    /// Opens a Guitar Pro file; returns whether it worked.
    fn open_song(&mut self, path: &Path) -> bool {
        match neoguitar_core::load(path) {
            Ok(song) => {
                self.set_song(song);
                self.settings.add_recent(path);
                self.settings.save(&self.data_dir);
                self.notice = None;
                true
            }
            Err(e) => {
                self.notice = Some(format!("Could not open {}: {e}", file_name(path)));
                false
            }
        }
    }

    fn lyrics_dir(&self) -> PathBuf {
        self.data_dir.join("lyrics")
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
            if self.settings.count_in {
                self.player.play_with_count_in();
            } else {
                self.player.play();
            }
        }
    }

    fn pick(&mut self, kind: i32, what: &'static str) {
        self.busy = Some(what);
        platform::pick_file(&self.android, kind);
    }

    /// Collects what the Java side produced: a picked file, or one sent with "Open with" / "Share".
    fn poll_picked(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.busy.is_none() && now - self.last_poll < 0.3 {
            return;
        }
        self.last_poll = now;
        let Some(picked) = platform::take_picked(&self.android) else { return };
        self.busy = None;
        match picked {
            Picked::Song(path) => {
                self.open_song(&path);
            }
            Picked::SoundFont(path) => {
                self.player.load_soundfont(&path);
                self.settings.soundfont = Some(path);
                self.settings.save(&self.data_dir);
            }
            Picked::Lyrics(path) => match std::fs::read_to_string(&path) {
                Ok(text) => {
                    self.lyrics = Some(LyricsState { data: lyrics::from_text(&text, file_name(&path)), anchors: Vec::new() });
                    self.lyr_msg = None;
                    self.lyr_last = None;
                }
                Err(e) => self.lyr_msg = Some(format!("Could not read the file: {e}")),
            },
            Picked::Text(0, text) => self.lyr_artist = text,
            Picked::Text(_, text) => self.lyr_title = text,
            Picked::Cancelled => {}
            Picked::Failed(e) => self.notice = Some(e),
        }
    }

    fn use_bundled_soundfont(&mut self) {
        if let Ok(path) = bundled_soundfont(&self.android, &self.data_dir) {
            self.player.load_soundfont(&path);
        }
        self.settings.soundfont = None;
        self.settings.save(&self.data_dir);
    }

    // ---------------------------------------------------------------- lyrics

    fn poll_lyrics(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.lyr_job else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.lyr_job = None;
                match result {
                    Ok(Some(l)) => {
                        lyrics::cache_store(&self.lyrics_dir(), &self.lyr_artist, &self.lyr_title, &l);
                        self.lyr_msg = None;
                        self.lyr_last = None;
                        self.lyrics = Some(LyricsState { data: l, anchors: Vec::new() });
                    }
                    Ok(None) => self.lyr_msg = Some("No lyrics found. Check the title and artist.".into()),
                    Err(e) => self.lyr_msg = Some(e),
                }
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(200)),
            Err(TryRecvError::Disconnected) => self.lyr_job = None,
        }
    }

    fn search_lyrics(&mut self, ctx: &egui::Context) {
        if self.lyr_job.is_some() {
            return;
        }
        let (artist, title, secs) = (self.lyr_artist.clone(), self.lyr_title.clone(), self.player.timeline.total);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(lyrics::fetch(&artist, &title, secs));
            ctx.request_repaint();
        });
        self.lyr_job = Some(rx);
        self.lyr_msg = Some("Searching…".into());
    }

    /// Tablature time of every lyric line and the line that is sounding now.
    fn lyric_position(&self) -> Option<(Vec<f64>, Option<usize>)> {
        let state = self.lyrics.as_ref()?;
        let times = lyrics::tab_times(&state.data, &state.anchors, self.player.timeline.total);
        let pos = self.player.position();
        let cur = if self.player.is_playing() || pos > 0.0 { lyrics::current(&state.data, &times, pos) } else { None };
        Some((times, cur))
    }

    fn pin_line(&mut self, i: usize) {
        let pos = self.player.position();
        if let Some(state) = &mut self.lyrics {
            let t = state.data.lines[i].time;
            state.anchors.retain(|a| (a.0 - t).abs() > 1e-6);
            state.anchors.push((t, pos));
        }
    }

    // ---------------------------------------------------------------- screens

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Open: browse the phone or pick a recent song.
            let open = theme::icon_button(ui, Icon::Folder, 34.0, IconStyle::Flat, true);
            let recent = self.settings.recent.clone();
            let mut to_open = None;
            let mut browse = false;
            let mut find_tabs = false;
            if open.clicked() {
                self.tabs_companion = platform::tabs_companion_installed(&self.android);
            }
            let companion = self.tabs_companion;
            egui::Popup::menu(&open).show(|ui| {
                ui.set_min_width(260.0);
                if ui.button("Open a file…").clicked() {
                    browse = true;
                    ui.close();
                }
                // Only offered when the separate companion app is already installed.
                if companion && ui.button("Find tabs online…").clicked() {
                    find_tabs = true;
                    ui.close();
                }
                if !recent.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Recent").size(12.0).color(theme::MUTED));
                    for p in recent.iter().filter(|p| p.is_file()) {
                        if ui.button(file_name(p)).clicked() {
                            to_open = Some(p.clone());
                            ui.close();
                        }
                    }
                }
                ui.separator();
                if ui.button("Demo song").clicked() {
                    to_open = Some(PathBuf::new());
                    ui.close();
                }
            });
            if browse {
                self.pick(0, "Opening…");
            }
            if find_tabs {
                platform::open_tabs_companion(&self.android);
            }
            if let Some(p) = to_open {
                if p.as_os_str().is_empty() {
                    self.open_demo();
                } else {
                    self.open_song(&p);
                }
            }

            if let Some(song) = &self.song {
                let names: Vec<String> = song.tracks.iter().map(|t| t.name.clone()).collect();
                let current = names.get(self.track).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt("track").width(160.0).selected_text(current).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.track, i, n);
                    }
                });
                let title = if song.artist.is_empty() { song.title.clone() } else { format!("{} – {}", song.artist, song.title) };
                let room = (ui.available_width() - 400.0).max(0.0);
                if room > 60.0 {
                    ui.add_sized([room, 20.0], egui::Label::new(RichText::new(title).color(theme::MUTED)).truncate());
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let toggle = |ui: &mut egui::Ui, which: Sheet, icon: Icon, text: &str, this: &mut App| {
                    let on = this.sheet == Some(which);
                    let r = if text.is_empty() { theme::icon_button(ui, icon, 34.0, if on { IconStyle::FlatOn } else { IconStyle::Flat }, true) } else { theme::chip(ui, icon, on, text) };
                    if r.clicked() {
                        this.sheet = if on { None } else { Some(which) };
                    }
                };
                toggle(ui, Sheet::Settings, Icon::Dots, "", self);
                toggle(ui, Sheet::Lyrics, Icon::Lyrics, "", self);
                toggle(ui, Sheet::Mix, Icon::Sliders, "Instruments", self);
                if theme::icon_button(ui, Icon::Staff, 32.0, if self.settings.show_staff { IconStyle::FlatOn } else { IconStyle::Flat }, true).clicked() {
                    self.settings.show_staff = !self.settings.show_staff;
                    self.settings.save(&self.data_dir);
                }
                if theme::icon_button(ui, Icon::Plus, 32.0, IconStyle::Flat, true).clicked() {
                    self.settings.zoom = (self.settings.zoom + 0.1).min(2.0);
                    self.settings.save(&self.data_dir);
                }
                if theme::icon_button(ui, Icon::Minus, 32.0, IconStyle::Flat, true).clicked() {
                    self.settings.zoom = (self.settings.zoom - 0.1).max(0.6);
                    self.settings.save(&self.data_dir);
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
            let count_chip = theme::chip(ui, Icon::Timer, self.settings.count_in, "Count-in");
            let mut count_changed = false;
            egui::Popup::menu(&count_chip).show(|ui| {
                let s = &mut self.settings;
                count_changed = theme::count_in_controls(ui, &mut s.count_in, &mut s.count_in_measures, &mut s.count_in_delay);
            });
            if count_changed {
                self.player.count_in_measures = self.settings.count_in_measures;
                self.player.count_in_delay = self.settings.count_in_delay;
                self.settings.save(&self.data_dir);
            }
            if theme::chip(ui, Icon::Follow, self.follow, "Follow").clicked() {
                self.follow = !self.follow;
            }
            if self.player.loop_measures.is_some() && theme::chip(ui, Icon::Loop, true, "Clear loop").clicked() {
                self.player.clear_loop();
            }
        });
    }

    /// One compact row per track: tap the name to show it, then volume, solo and mute; the sound is behind the slider icon.
    fn mix_sheet(&mut self, ui: &mut egui::Ui) {
        let Some(song) = &self.song else { return };
        ui.label(RichText::new("What you hear").strong().size(15.0));
        ui.label(RichText::new("Mute the part you play yourself. Tap a name to show its tab.").size(11.5).color(theme::MUTED));
        ui.add_space(4.0);
        let total = song.tracks.len();
        let mut expanded = self.expanded;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for (i, t) in song.tracks.iter().enumerate() {
                let enabled = self.player.track_enabled(i);
                let shown = self.track == i;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let [r, g, b] = t.color;
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 5.0, theme::track_color([r, g, b]));
                    let name_color = if shown { theme::ACCENT } else if enabled { theme::TEXT } else { theme::MUTED };
                    let name = ui.add_sized([92.0, 28.0], egui::Button::new(RichText::new(&t.name).color(name_color)).frame(false).truncate());
                    if name.clicked() {
                        self.track = i;
                    }
                    let mut gain = self.player.track_gain(i) as f32;
                    ui.spacing_mut().slider_width = 66.0;
                    if ui.add(egui::Slider::new(&mut gain, 0.0..=150.0).show_value(false)).changed() {
                        self.player.set_track_gain(i, gain as u8);
                    }
                    if !t.is_percussion && theme::icon_button(ui, Icon::Sliders, 28.0, if expanded == Some(i) { IconStyle::FlatOn } else { IconStyle::Flat }, true).clicked() {
                        expanded = if expanded == Some(i) { None } else { Some(i) };
                    }
                    let solo = (0..total).filter(|&k| self.player.track_enabled(k)).count() == 1 && enabled;
                    if theme::icon_button(ui, Icon::Headphones, 28.0, if solo { IconStyle::FlatOn } else { IconStyle::Flat }, true).clicked() {
                        self.player.toggle_solo(i, total);
                    }
                    let (icon, style) = if enabled { (Icon::Speaker, IconStyle::Flat) } else { (Icon::SpeakerOff, IconStyle::Danger) };
                    if theme::icon_button(ui, icon, 28.0, style, true).clicked() {
                        self.player.set_track_enabled(i, !enabled);
                    }
                });
                if expanded == Some(i) {
                    let program = self.player.mix_program(i).unwrap_or(t.program.clamp(0, 127) as u8) as usize;
                    ui.label(RichText::new("Sound").size(11.5).color(theme::MUTED));
                    egui::ComboBox::from_id_salt(("program", i)).width(ui.available_width() - 8.0).selected_text(neoguitar_core::gm::NAMES[program.min(127)]).show_ui(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for (k, name) in neoguitar_core::gm::NAMES.iter().enumerate() {
                                if ui.selectable_label(program == k, format!("{:>3}  {name}", k + 1)).clicked() {
                                    let (pan, rev) = (self.player.mix_pan(i), self.player.mix_reverb(i));
                                    self.player.set_mix(i, pan, Some(k as u8), rev);
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
        self.expanded = expanded;
    }

    fn lyrics_sheet(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Lyrics").strong().size(15.0));
        let ctx = ui.ctx().clone();
        if self.lyrics.is_none() {
            ui.label(RichText::new("Artist").size(11.5).color(theme::MUTED));
            self.text_field(ui, 0);
            ui.label(RichText::new("Title").size(11.5).color(theme::MUTED));
            self.text_field(ui, 1);
            ui.horizontal(|ui| {
                let can = self.lyr_job.is_none() && !self.lyr_title.trim().is_empty();
                if ui.add_enabled(can, egui::Button::new("Search LRCLIB")).clicked() {
                    self.search_lyrics(&ctx);
                }
                if ui.button("Load .lrc / .txt").clicked() {
                    self.pick(2, "Loading…");
                }
            });
            if let Some(m) = &self.lyr_msg {
                ui.label(RichText::new(m).size(12.0).color(theme::MUTED));
            }
            ui.label(RichText::new("The title and artist are sent to lrclib.net.").size(11.0).color(theme::MUTED));
            return;
        }
        let Some((times, cur)) = self.lyric_position() else { return };
        let changed = cur != self.lyr_last;
        let (mut seek, mut remove, mut reset, mut pin) = (None, false, false, None);
        if let Some(state) = &self.lyrics {
            ui.label(RichText::new(&state.data.source).size(11.0).color(theme::MUTED));
            ui.horizontal(|ui| {
                let next = cur.and_then(|c| lyrics::next_line(&state.data, c));
                if ui.add_enabled(next.is_some(), egui::Button::new("Next line starts now")).clicked() {
                    pin = next;
                }
                if ui.add_enabled(!state.anchors.is_empty(), egui::Button::new("Reset")).clicked() {
                    reset = true;
                }
                if ui.button("Remove").clicked() {
                    remove = true;
                }
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (i, line) in state.data.lines.iter().enumerate() {
                    if line.text.is_empty() {
                        ui.add_space(8.0);
                        continue;
                    }
                    let is_cur = cur == Some(i);
                    let pinned = state.anchors.iter().any(|a| (a.0 - line.time).abs() < 1e-6);
                    let label = format!("{}{}", if pinned { "● " } else { "" }, line.text);
                    let text = if is_cur { RichText::new(label).size(16.0).strong().color(theme::ACCENT) } else { RichText::new(label).size(13.5).color(theme::MUTED) };
                    let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                    if is_cur && changed {
                        ui.scroll_to_rect(resp.rect, Some(Align::Center));
                    }
                    if resp.clicked() {
                        seek = Some(times[i]);
                    }
                }
            });
        }
        self.lyr_last = cur;
        if let Some(t) = seek {
            self.player.seek(t.max(0.0));
            self.last_followed = None;
        }
        if let Some(i) = pin {
            self.pin_line(i);
        }
        if reset {
            if let Some(s) = &mut self.lyrics {
                s.anchors.clear();
            }
        }
        if remove {
            self.lyrics = None;
            self.lyr_last = None;
        }
    }

    /// A text "box" that opens the system's text prompt when tapped. `which`: 0 = artist, 1 = title.
    fn text_field(&mut self, ui: &mut egui::Ui, which: i32) {
        let (value, title) = if which == 0 { (self.lyr_artist.clone(), "Artist") } else { (self.lyr_title.clone(), "Title") };
        let shown = if value.is_empty() { RichText::new("Tap to write").color(theme::MUTED) } else { RichText::new(value.clone()) };
        let width = ui.available_width();
        if ui.add_sized([width, 34.0], egui::Button::new(shown).truncate()).clicked() {
            platform::prompt_text(&self.android, which, title, &value);
        }
    }

    fn settings_sheet(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Settings").strong().size(15.0));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            theme::section(ui, "Sound");
            let (name, loading) = match &self.player.status {
                Status::Ready(p) => (file_name(p), false),
                Status::Loading(p) => (file_name(p), true),
                Status::Error(e) => (e.clone(), false),
                Status::NoSoundFont => ("none".to_string(), false),
            };
            ui.label(RichText::new(format!("SoundFont: {name}")).size(13.0));
            if loading {
                ui.label(RichText::new("Loading…").size(12.0).color(theme::MUTED));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!loading, egui::Button::new("Choose SoundFont…")).clicked() {
                    self.pick(1, "Copying the SoundFont…");
                }
                if self.settings.soundfont.is_some() && ui.add_enabled(!loading, egui::Button::new("Use the bundled one")).clicked() {
                    self.use_bundled_soundfont();
                }
            });
            ui.label(RichText::new("Pick a .sf2 file. Large ones take a while to copy and load; the choice is remembered.").size(11.5).color(theme::MUTED));

            theme::section(ui, "Practice");
            let mut changed = false;
            changed |= ui.checkbox(&mut self.settings.count_in, "Count-in before playing").changed();
            changed |= ui.checkbox(&mut self.settings.show_staff, "Show the staff above the tab").changed();
            ui.checkbox(&mut self.follow, "Follow the playback");
            if changed {
                self.settings.save(&self.data_dir);
            }

            theme::section(ui, "About");
            ui.label(RichText::new(format!("NeoGuitar {}", env!("CARGO_PKG_VERSION"))).size(13.0));
            ui.label(RichText::new("Lyrics from lrclib.net. Sounds: GeneralUser GS by S. Christian Collins, unless you choose another SoundFont. Licensed LGPL-2.1-or-later.").size(11.5).color(theme::MUTED));
        });
    }

    /// Strip under the tab: the line that sounds and the next one.
    fn lyrics_strip(&mut self, ui: &mut egui::Ui) {
        let Some((_, cur)) = self.lyric_position() else { return };
        let Some(state) = &self.lyrics else { return };
        let next = cur.and_then(|c| lyrics::next_line(&state.data, c));
        let (cur_text, next_text) = (cur.map(|c| state.data.lines[c].text.clone()).unwrap_or_default(), next.map(|n| state.data.lines[n].text.clone()).unwrap_or_default());
        let mut pin = None;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width((ui.available_width() - 190.0).max(100.0));
                ui.label(RichText::new(if cur_text.is_empty() { "…" } else { &cur_text }).size(18.0).strong().color(theme::ACCENT));
                ui.label(RichText::new(&next_text).size(14.0).color(theme::MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(next.is_some(), egui::Button::new("Next starts now")).clicked() {
                    pin = next;
                }
            });
        });
        if let Some(i) = pin {
            self.pin_line(i);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.player.poll();
        self.poll_picked(&ctx);
        self.poll_lyrics(&ctx);
        if self.player.is_playing() || matches!(self.player.status, Status::Loading(_)) {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            // Keeps checking for files sent to the app while idle.
            ctx.request_repaint_after(Duration::from_millis(400));
        }
        let ins = self.insets(&ctx);
        let margin = |l: i8, t: i8, r: i8, b: i8| Margin { left: l.max(ins.left), top: t.max(ins.top), right: r.max(ins.right), bottom: b.max(ins.bottom) };

        egui::Panel::top("top").frame(theme::panel_frame(margin(12, 6, 12, 6))).show_separator_line(true).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("transport").frame(theme::panel_frame(margin(14, 6, 14, 8))).show_separator_line(true).show(ui, |ui| self.transport(ui));
        if let Some(sheet) = self.sheet {
            egui::Panel::right("sheet")
                .default_size(380.0)
                .size_range(300.0..=560.0)
                .frame(theme::panel_frame(Margin { left: 12, top: 8, right: 12.max(ins.right), bottom: 6 }))
                .show(ui, |ui| match sheet {
                    Sheet::Mix => self.mix_sheet(ui),
                    Sheet::Lyrics => self.lyrics_sheet(ui),
                    Sheet::Settings => self.settings_sheet(ui),
                });
        }
        if self.lyrics.is_some() && self.sheet != Some(Sheet::Lyrics) {
            egui::Panel::bottom("lyrics_strip").frame(theme::panel_frame(margin(14, 4, 14, 4))).show_separator_line(true).show(ui, |ui| self.lyrics_strip(ui));
        }

        // Where the playback is in the score, so the view follows it.
        let pos = self.player.position();
        let playhead = if self.song.is_some() && (self.player.is_playing() || pos > 0.0) { self.player.timeline.position(pos) } else { None };
        let mut scroll_to = None;
        if self.follow && self.player.is_playing() {
            if let Some((m, _)) = playhead {
                if self.last_followed != Some(m) {
                    self.last_followed = Some(m);
                    scroll_to = Some(m + 1);
                }
            }
        }

        let mut action = None;
        let right = if self.sheet.is_some() { 10 } else { 10.max(ins.right) };
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::SCORE_BG).inner_margin(Margin { left: 10.max(ins.left), top: 6, right, bottom: 4 })).show(ui, |ui| {
            if let Some(what) = self.busy {
                ui.label(RichText::new(what).color(theme::MUTED));
            }
            if let Some(e) = &self.notice {
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
                    action = tab_view::show(ui, song, &[track], self.settings.zoom, scroll_to, playhead, self.player.loop_measures, self.settings.show_staff, self.player.transpose);
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

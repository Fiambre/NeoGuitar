use crate::theme::{self, Icon, IconStyle};
use crate::tab_view::{self, ViewAction};
use eframe::egui::{self, Align, Layout, RichText};
use neoguitar::audio::{Player, Status};
use neoguitar::lyrics::{self, Lyrics};
use neoguitar::model::{midi_note_name, Song};
use neoguitar::settings::Settings;
use neoguitar::update::{self, Release};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct App {
    song: Option<Song>,
    path: Option<PathBuf>,
    error: Option<String>,
    track: usize,
    zoom: f32,
    show_side: bool,
    player: Player,
    /// Desplazar la vista para seguir el compás que suena.
    follow: bool,
    show_staff: bool,
    /// Mostrar varias pistas a la vez (las marcadas en `visible`).
    multi: bool,
    /// Mástil virtual bajo la partitura.
    show_fretboard: bool,
    visible: Vec<bool>,
    last_followed: Option<usize>,
    /// Si está definido (NEOGUITAR_SCREENSHOT), guarda un PNG del frame y cierra.
    screenshot: Option<ScreenshotJob>,
    focus_measure: Option<usize>,
    autoplay_at: Option<f64>,
    /// Cuenta regresiva de un compás antes de reproducir.
    count_in: bool,
    settings: Settings,
    last_saved: Settings,
    /// No se guardan ajustes en las corridas de captura (NEOGUITAR_SCREENSHOT).
    persist: bool,
    /// Título de ventana ya enviado al sistema.
    title: String,
    /// Resultado de la última asociación de archivos (solo Windows).
    #[allow(dead_code)]
    assoc_msg: Option<String>,
    /// Exportación de audio en curso (hilo aparte) y aviso para mostrar en la barra inferior.
    export_job: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    notice: Option<String>,
    /// Compás (base 1) al que desplazar la vista en el próximo frame.
    jump_to: Option<usize>,
    /// Pista con el panel de mezcla desplegado.
    mixer_open: Option<usize>,
    /// Vista dividida: dos pistas lado a lado (la izquierda es `track`, la derecha `split_track`).
    split: bool,
    split_track: usize,
    /// Pantalla completa (sin barra superior ni panel lateral).
    fullscreen: bool,
    /// Letra externa (LRCLIB o archivo) con la que se sigue la reproducción.
    show_lyrics: bool,
    lyrics: Option<LyricsState>,
    lyr_artist: String,
    lyr_title: String,
    lyr_job: Option<std::sync::mpsc::Receiver<Result<Option<Lyrics>, String>>>,
    lyr_msg: Option<String>,
    /// Última línea resaltada (para desplazar la lista solo cuando cambia).
    lyr_last: Option<usize>,
    /// Versión nueva disponible, consulta en curso, descarga en curso y último aviso.
    update: Option<Release>,
    update_check: Option<std::sync::mpsc::Receiver<Result<Option<Release>, String>>>,
    update_manual: bool,
    update_dl: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    update_msg: Option<String>,
}

/// Letra cargada y sus puntos de sincronización: (tiempo de la letra, tiempo de la tablatura).
struct LyricsState {
    data: Lyrics,
    anchors: Vec<(f64, f64)>,
}

struct ScreenshotJob {
    path: PathBuf,
    frames: u32,
    requested: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<String>) -> Self {
        theme::apply(&cc.egui_ctx);
        let persist = std::env::var_os("NEOGUITAR_SCREENSHOT").is_none() || std::env::var_os("NEOGUITAR_PERSIST").is_some();
        let settings = if persist { Settings::load() } else { Settings::default() };
        let mut app = App {
            song: None,
            path: None,
            error: None,
            track: 0,
            zoom: settings.zoom,
            show_side: settings.show_side,
            player: Player::new(settings.soundfont.clone()),
            follow: settings.follow,
            multi: settings.multi_view,
            show_fretboard: settings.show_fretboard,
            visible: Vec::new(),
            show_staff: std::env::var("NEOGUITAR_STAFF").map(|v| v != "0").unwrap_or(settings.show_staff),
            last_followed: None,
            screenshot: None,
            focus_measure: None,
            autoplay_at: None,
            count_in: settings.count_in,
            last_saved: settings.clone(),
            settings,
            persist,
            title: String::new(),
            assoc_msg: None,
            export_job: None,
            notice: None,
            jump_to: None,
            mixer_open: None,
            split: false,
            split_track: 1,
            fullscreen: false,
            show_lyrics: false,
            lyrics: None,
            lyr_artist: String::new(),
            lyr_title: String::new(),
            lyr_job: None,
            lyr_msg: None,
            lyr_last: None,
            update: None,
            update_check: None,
            update_manual: false,
            update_dl: None,
            update_msg: None,
        };
        // Consulta de versión nueva: una vez al día como máximo y nunca en las corridas de captura.
        if (persist || std::env::var_os("NEOGUITAR_CURRENT_VERSION").is_some()) && app.settings.check_updates && unix_now().saturating_sub(app.settings.last_update_check) >= 86_400 {
            app.start_update_check(&cc.egui_ctx, false);
        }
        // Se restauran los ajustes del reproductor.
        app.player.set_speed(app.settings.speed / 100.0);
        app.player.set_volume(app.settings.volume);
        app.player.set_metronome(app.settings.metronome);
        app.player.set_trainer(app.settings.trainer_on, app.settings.trainer_step, app.settings.trainer_max);
        if let Some(p) = initial {
            app.open(Path::new(&p));
        }
        // Opciones de depuración por entorno.
        if let Some(t) = std::env::var("NEOGUITAR_TRACK").ok().and_then(|v| v.parse::<usize>().ok()) {
            app.track = t.saturating_sub(1);
        }
        if let Some(z) = std::env::var("NEOGUITAR_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.zoom = z.clamp(0.6, 2.0);
        }
        if let Some(t) = std::env::var("NEOGUITAR_MIXER").ok().and_then(|v| v.parse::<usize>().ok()) {
            app.mixer_open = Some(t.saturating_sub(1));
        }
        if std::env::var_os("NEOGUITAR_FRETBOARD").is_some() {
            app.show_fretboard = true;
        }
        if std::env::var_os("NEOGUITAR_MULTI").is_some() {
            app.multi = true;
        }
        if let Some(p) = std::env::var_os("NEOGUITAR_LYRICS") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                app.lyrics = Some(LyricsState { data: lyrics::from_text(&text, "test file".into()), anchors: Vec::new() });
                app.show_lyrics = true;
            }
        }
        if let Some(t) = std::env::var("NEOGUITAR_SPLIT").ok().and_then(|v| v.parse::<usize>().ok()) {
            app.split = true;
            app.split_track = t.saturating_sub(1);
        }
        app.focus_measure = std::env::var("NEOGUITAR_MEASURE").ok().and_then(|v| v.parse().ok());
        // NEOGUITAR_PLAY_AT=segundos: arranca la reproducción en ese instante (para probar el cursor).
        app.autoplay_at = std::env::var("NEOGUITAR_PLAY_AT").ok().and_then(|v| v.parse().ok());
        if let Some(path) = std::env::var_os("NEOGUITAR_SCREENSHOT") {
            app.screenshot = Some(ScreenshotJob { path: PathBuf::from(path), frames: 0, requested: false });
        }
        app
    }

    fn open(&mut self, path: &Path) {
        match neoguitar::load(path) {
            Ok(song) => {
                self.player.stop();
                self.player.set_song(&song);
                // Por defecto, en la vista multipista se muestran las primeras pistas.
                self.visible = (0..song.tracks.len()).map(|i| i < 3).collect();
                self.lyr_artist = song.artist.trim().to_string();
                self.lyr_title = song.title.trim().to_string();
                self.lyrics = None;
                self.lyr_msg = None;
                self.lyr_last = None;
                if let (Some(dir), false) = (lyrics_dir(), self.lyr_title.is_empty()) {
                    if let Some(l) = lyrics::cache_load(&dir, &self.lyr_artist, &self.lyr_title) {
                        self.lyrics = Some(LyricsState { data: l, anchors: Vec::new() });
                    }
                }
                self.song = Some(song);
                self.path = Some(path.to_path_buf());
                self.settings.add_recent(path);
                self.error = None;
                self.track = 0;
                self.last_followed = None;
            }
            Err(e) => {
                self.error = Some(format!("Could not open {}: {e}", path.display()));
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.screenshot else { return };
        job.frames += 1;
        ctx.request_repaint();
        // Se espera a que cargue el SoundFont (si hay reproducción de prueba) antes de capturar.
        let waiting_audio = self.autoplay_at.is_some() && !self.player.ready() && job.frames < 600;
        let waiting_play = self.autoplay_at.is_some() && self.player.ready() && self.player.position() < self.autoplay_at.unwrap_or(0.0) + 0.5;
        if !job.requested && job.frames >= 8 && !waiting_audio && !waiting_play {
            job.requested = true;
            let path = job.path.clone();
            let close_ctx = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                if let Err(e) = save_png(&path, &img) {
                    eprintln!("could not save the screenshot: {e}");
                }
                close_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            })));
        }
    }

    /// Ajustes actuales, listos para guardar.
    fn collect_settings(&self) -> Settings {
        let mut s = self.settings.clone();
        s.zoom = self.zoom;
        s.show_staff = self.show_staff;
        s.show_side = self.show_side;
        s.follow = self.follow;
        s.multi_view = self.multi;
        s.show_fretboard = self.show_fretboard;
        let (trainer_on, step, max) = self.player.trainer();
        s.trainer_on = trainer_on;
        s.trainer_step = step;
        s.trainer_max = max;
        // Con el entrenador activo la velocidad cambia sola: se conserva la elegida por el usuario.
        if !trainer_on {
            s.speed = (self.player.speed * 100.0).round();
        }
        s.volume = self.player.volume;
        s.metronome = self.player.metronome();
        s.count_in = self.count_in;
        if let Status::Ready(p) = &self.player.status {
            s.soundfont = Some(p.clone());
        }
        s
    }

    fn persist_settings(&mut self) {
        if !self.persist {
            return;
        }
        let s = self.collect_settings();
        if s != self.last_saved {
            let _ = s.save();
            self.last_saved = s.clone();
        }
        self.settings = s;
    }

    fn pick_file(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("Guitar Pro", &["gp3", "gp4", "gp5", "gpx", "gp", "gtp"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(p) = picked {
            self.open(&p);
        }
    }

    fn pick_soundfont(&mut self) {
        let picked = rfd::FileDialog::new().add_filter("SoundFont", &["sf2"]).pick_file();
        if let Some(p) = picked {
            self.player.load_soundfont(&p);
        }
    }

    fn export_midi(&mut self) {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| format!("{}.mid", s.to_string_lossy()))
            .unwrap_or_else(|| "song.mid".to_string());
        let target = rfd::FileDialog::new().add_filter("MIDI", &["mid"]).set_file_name(name).save_file();
        if let Some(p) = target {
            self.error = self.player.export_midi(&p).err().map(|e| format!("Export failed: {e}"));
        }
    }

    fn export_wav(&mut self) {
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "song".into());
        let Some(p) = rfd::FileDialog::new().add_filter("Audio WAV", &["wav"]).set_file_name(format!("{stem}.wav")).save_file() else { return };
        match self.player.export_wav(&p) {
            Ok(rx) => {
                self.export_job = Some(rx);
                self.error = None;
                self.notice = Some("Exporting audio…".into());
            }
            Err(e) => self.error = Some(format!("Export failed: {e}")),
        }
    }

    fn export_ascii(&mut self) {
        let Some(song) = &self.song else { return };
        let Some(track) = song.tracks.get(self.track) else { return };
        let text = neoguitar::ascii::ascii_tab(song, track, 100);
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "tab".into());
        let name = format!("{stem} - {}.txt", track.name.trim().replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_"));
        if let Some(p) = rfd::FileDialog::new().add_filter("Text", &["txt"]).set_file_name(name).save_file() {
            self.error = std::fs::write(&p, text).err().map(|e| format!("Export failed: {e}"));
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

    /// Barra superior: abrir, pista y opciones de vista.
    fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update_check.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
            ctx.request_repaint();
        });
        self.update_check = Some(rx);
        self.update_manual = manual;
        if manual {
            self.notice = Some("Checking for updates…".into());
        }
    }

    fn start_update_download(&mut self, ctx: &egui::Context, release: &Release) {
        let Some(installer) = release.installer.clone() else { return };
        if self.update_dl.is_some() {
            return;
        }
        let dest = update::installer_path(&release.version);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::download(&installer, &dest).map(|_| dest));
            ctx.request_repaint();
        });
        self.update_dl = Some(rx);
        self.update_msg = Some(format!("Downloading {} ({} MB)…", release.version, (release.installer.as_ref().map(|i| i.size).unwrap_or(0) / 1_000_000).max(1)));
    }

    /// Recoge los resultados de la consulta y de la descarga de actualizaciones.
    fn poll_updates(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update_check {
            match rx.try_recv() {
                Ok(result) => {
                    self.update_check = None;
                    match result {
                        Ok(found) => {
                            self.settings.last_update_check = unix_now();
                            if found.is_none() && self.update_manual {
                                self.notice = Some(format!("You have the latest version ({})", env!("CARGO_PKG_VERSION")));
                            } else if self.update_manual {
                                self.notice = None;
                            }
                            self.update = found;
                        }
                        Err(e) => {
                            if self.update_manual {
                                self.notice = None;
                                self.error = Some(e);
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.update_check = None,
            }
        }
        if let Some(rx) = &self.update_dl {
            match rx.try_recv() {
                Ok(result) => {
                    self.update_dl = None;
                    match result.and_then(|path| update::run_installer(&path)) {
                        Ok(()) => {
                            // El instalador cierra y vuelve a abrir la aplicación; se sale para que pueda reemplazar el ejecutable.
                            self.update_msg = Some("Installing… NeoGuitar will restart.".into());
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => self.update_msg = Some(e),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.update_dl = None,
            }
        }
    }

    /// Botón de "actualización disponible" con su menú.
    fn update_button(&mut self, ui: &mut egui::Ui) {
        let Some(release) = self.update.clone() else { return };
        let btn = theme::icon_label(ui, Some(Icon::Trend), &format!("Update to {}", release.version), true, true, false);
        let ctx = ui.ctx().clone();
        egui::Popup::menu(&btn).show(|ui| {
            ui.set_min_width(260.0);
            ui.label(RichText::new(format!("NeoGuitar {} is available", release.version)).strong());
            ui.label(RichText::new(format!("You have {}", env!("CARGO_PKG_VERSION"))).size(12.0).color(theme::MUTED));
            ui.add_space(6.0);
            let can_install = release.installer.is_some() && update::is_installed_copy();
            if can_install {
                let busy = self.update_dl.is_some();
                if ui.add_enabled(!busy, egui::Button::new("Download and install now")).on_hover_text("Downloads the installer from GitHub, verifies its checksum and restarts NeoGuitar").clicked() {
                    self.start_update_download(&ctx, &release);
                }
            } else if release.installer.is_some() {
                ui.label(RichText::new("This copy was not installed with the installer (portable), so download the new version from the release page.").size(12.0).color(theme::MUTED));
            }
            if ui.button("Open release page").clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                ui.close();
            }
            if let Some(m) = &self.update_msg {
                ui.add_space(4.0);
                ui.label(RichText::new(m).size(12.0).color(theme::MUTED));
            }
        });
    }

    /// Recoge el resultado de la búsqueda de letra en curso.
    fn poll_lyrics(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.lyr_job else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.lyr_job = None;
                match result {
                    Ok(Some(l)) => {
                        if let Some(dir) = lyrics_dir() {
                            lyrics::cache_store(&dir, &self.lyr_artist, &self.lyr_title, &l);
                        }
                        self.lyr_msg = None;
                        self.lyr_last = None;
                        self.lyrics = Some(LyricsState { data: l, anchors: Vec::new() });
                    }
                    Ok(None) => self.lyr_msg = Some("No lyrics found. Check the title and artist.".into()),
                    Err(e) => self.lyr_msg = Some(e),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(200)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.lyr_job = None,
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

    fn load_lyrics_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Lyrics", &["lrc", "txt"]).pick_file() else { return };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.lyrics = Some(LyricsState { data: lyrics::from_text(&text, name), anchors: Vec::new() });
                self.lyr_msg = None;
                self.lyr_last = None;
            }
            Err(e) => self.lyr_msg = Some(format!("Could not read the file: {e}")),
        }
    }

    /// Búsqueda y carga de letra, o su estado y los botones de sincronización si ya hay una.
    fn lyrics_controls(&mut self, ui: &mut egui::Ui, compact: bool) {
        if self.lyrics.is_none() {
            let ctx = ui.ctx().clone();
            let row = |ui: &mut egui::Ui, this: &mut App| {
                ui.label(RichText::new("Artist").color(theme::MUTED));
                ui.add(egui::TextEdit::singleline(&mut this.lyr_artist).desired_width(if compact { 150.0 } else { ui.available_width() }));
                ui.label(RichText::new("Title").color(theme::MUTED));
                ui.add(egui::TextEdit::singleline(&mut this.lyr_title).desired_width(if compact { 180.0 } else { ui.available_width() }));
            };
            if compact {
                ui.horizontal_wrapped(|ui| row(ui, self));
            } else {
                ui.vertical(|ui| row(ui, self));
            }
            ui.horizontal(|ui| {
                let can = self.lyr_job.is_none() && !self.lyr_title.trim().is_empty();
                if ui.add_enabled(can, egui::Button::new("Search LRCLIB")).on_hover_text("Sends the title and artist to lrclib.net").clicked() {
                    self.search_lyrics(&ctx);
                }
                if ui.button("Load file…").on_hover_text(".lrc (with times) or .txt").clicked() {
                    self.load_lyrics_file();
                }
            });
        } else if let Some(state) = &mut self.lyrics {
            ui.label(RichText::new(&state.data.source).size(11.0).color(theme::MUTED));
            let mut remove = false;
            ui.horizontal(|ui| {
                if ui.add_enabled(!state.anchors.is_empty(), egui::Button::new("Reset sync")).on_hover_text("Forget the sync points").clicked() {
                    state.anchors.clear();
                }
                if ui.button("Remove").on_hover_text("Search or load another lyric").clicked() {
                    remove = true;
                }
            });
            if remove {
                self.lyrics = None;
                self.lyr_last = None;
            }
        }
        if let Some(m) = &self.lyr_msg {
            ui.label(RichText::new(m).size(12.0).color(theme::MUTED));
        }
    }

    /// Panel lateral con la letra completa: la línea que suena se resalta y la lista se desplaza sola.
    fn lyrics_panel(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Lyrics");
        self.lyrics_controls(ui, false);
        let Some(state) = &self.lyrics else { return };
        ui.add_space(6.0);
        let total = self.player.timeline.total;
        let times = lyrics::tab_times(&state.data, &state.anchors, total);
        let pos = self.player.position();
        let cur = if self.player.is_playing() || pos > 0.0 { lyrics::current(&state.data, &times, pos) } else { None };
        let changed = cur != self.lyr_last;
        let (mut seek, mut pin, mut unpin) = (None, None, None);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, line) in state.data.lines.iter().enumerate() {
                if line.text.is_empty() {
                    ui.add_space(8.0);
                    continue;
                }
                let is_cur = cur == Some(i);
                let pinned = state.anchors.iter().any(|a| (a.0 - line.time).abs() < 1e-6);
                let label = format!("{}{}", if pinned { "● " } else { "" }, line.text);
                let text = if is_cur { RichText::new(label).size(17.0).strong().color(theme::ACCENT) } else { RichText::new(label).size(14.0).color(theme::MUTED) };
                let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                if is_cur && changed {
                    ui.scroll_to_rect(resp.rect, Some(Align::Center));
                }
                if resp.clicked() {
                    seek = Some(times[i]);
                }
                resp.context_menu(|ui| {
                    if ui.button("Starts at the current position").on_hover_text("Pins this line to the playback position").clicked() {
                        pin = Some(i);
                        ui.close();
                    }
                    if pinned && ui.button("Remove sync point").clicked() {
                        unpin = Some(i);
                        ui.close();
                    }
                });
            }
        });
        self.lyr_last = cur;
        self.apply_lyric_actions(seek, pin, unpin);
    }

    /// Franja bajo la partitura (vista dividida): línea actual y siguiente, y ajuste rápido.
    fn lyrics_strip(&mut self, ui: &mut egui::Ui) {
        if self.lyrics.is_none() {
            self.lyrics_controls(ui, true);
            return;
        }
        let Some(state) = &self.lyrics else { return };
        let total = self.player.timeline.total;
        let times = lyrics::tab_times(&state.data, &state.anchors, total);
        let pos = self.player.position();
        let cur = if self.player.is_playing() || pos > 0.0 { lyrics::current(&state.data, &times, pos) } else { None };
        let next = cur.and_then(|c| lyrics::next_line(&state.data, c));
        // Ventana de líneas: las dos anteriores (atenuadas), la actual y las tres siguientes.
        let spoken: Vec<usize> = (0..state.data.lines.len()).filter(|&k| !state.data.lines[k].text.is_empty()).collect();
        let at = match cur {
            Some(c) => spoken.iter().position(|&k| k == c).unwrap_or(0) as isize,
            None => -1,
        };
        let window: Vec<(isize, String)> = (at - 2..=at + 3)
            .filter(|&p| p >= 0 && (p as usize) < spoken.len())
            .map(|p| (p - at, state.data.lines[spoken[p as usize]].text.clone()))
            .collect();
        let mut pin = None;
        let mut reset = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width((ui.available_width() - 220.0).max(100.0));
                ui.set_min_height(150.0);
                for (rel, text) in &window {
                    let rt = match rel {
                        0 => RichText::new(text).size(22.0).strong().color(theme::ACCENT),
                        r if *r < 0 => RichText::new(text).size(14.0).color(theme::MUTED.gamma_multiply(0.55)),
                        1 => RichText::new(text).size(17.0).color(theme::TEXT),
                        _ => RichText::new(text).size(15.0).color(theme::MUTED),
                    };
                    ui.label(rt);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Remove").clicked() {
                    remove = true;
                }
                if ui.add_enabled(!state.anchors.is_empty(), egui::Button::new("Reset sync")).clicked() {
                    reset = true;
                }
                if ui.add_enabled(next.is_some(), egui::Button::new("Next line starts now")).on_hover_text("Pins the next line to the playback position").clicked() {
                    pin = next;
                }
            });
        });
        self.apply_lyric_actions(None, pin, None);
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

    fn apply_lyric_actions(&mut self, seek: Option<f64>, pin: Option<usize>, unpin: Option<usize>) {
        if let Some(t) = seek {
            self.player.seek(t.max(0.0));
            self.last_followed = None;
        }
        let pos = self.player.position();
        if let Some(state) = &mut self.lyrics {
            if let Some(i) = pin {
                let t = state.data.lines[i].time;
                state.anchors.retain(|a| (a.0 - t).abs() > 1e-6);
                state.anchors.push((t, pos));
            }
            if let Some(i) = unpin {
                let t = state.data.lines[i].time;
                state.anchors.retain(|a| (a.0 - t).abs() > 1e-6);
            }
        }
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn toggle_split(&mut self) {
        self.split = !self.split;
        if self.split {
            // La pista de la derecha arranca distinta de la de la izquierda.
            let n = self.song.as_ref().map(|s| s.tracks.len()).unwrap_or(0);
            if n > 1 && (self.split_track == self.track || self.split_track >= n) {
                self.split_track = (self.track + 1) % n;
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let labels: Vec<String> = self.song.as_ref().map(|s| s.tracks.iter().map(track_label).collect()).unwrap_or_default();
        // Con poco ancho los botones de vista quedan solo con el icono.
        let narrow = ui.available_width() < 1260.0 + if self.update.is_some() { 160.0 } else { 0.0 };
        let label = move |s: &'static str| if narrow { "" } else { s };
        ui.horizontal(|ui| {
            if theme::primary_button(ui, "Open").on_hover_text("Ctrl+O").clicked() {
                self.pick_file();
            }
            let mut reopen: Option<PathBuf> = None;
            let recent_btn = theme::icon_label(ui, Some(Icon::Clock), "Recent", false, true, false);
            egui::Popup::menu(&recent_btn).show(|ui| {
                ui.set_min_width(220.0);
                if self.settings.recent.is_empty() {
                    ui.weak("(no recent files)");
                }
                for p in &self.settings.recent {
                    let label = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
                    if ui.button(label).on_hover_text(p.display().to_string()).clicked() {
                        reopen = Some(p.clone());
                        ui.close();
                    }
                }
            });
            if let Some(p) = reopen {
                self.open(&p);
            }
            self.update_button(ui);
            if labels.is_empty() {
                return;
            }
            ui.add_space(4.0);
            let current = labels.get(self.track).cloned().unwrap_or_default();
            egui::ComboBox::from_id_salt("track").width(220.0).selected_text(current).show_ui(ui, |ui| {
                for (i, l) in labels.iter().enumerate() {
                    ui.selectable_value(&mut self.track, i, l);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let more_btn = theme::icon_label(ui, Some(Icon::Dots), "More", false, true, false);
                egui::Popup::menu(&more_btn).show(|ui| {
                    if ui.button("Export MIDI…").on_hover_text("Active tracks").clicked() {
                        self.export_midi();
                        ui.close();
                    }
                    let wav = ui.add_enabled(self.player.ready() && self.export_job.is_none(), egui::Button::new("Export audio (WAV)…"));
                    if wav.on_hover_text("Active tracks, at normal speed").on_disabled_hover_text("Needs a loaded SoundFont").clicked() {
                        self.export_wav();
                        ui.close();
                    }
                    if ui.button("Export tab as text (txt)…").on_hover_text("Current track").clicked() {
                        self.export_ascii();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Check for updates now").clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_update_check(&ctx, true);
                        ui.close();
                    }
                    ui.checkbox(&mut self.settings.check_updates, "Check for updates on startup").on_hover_text("Asks GitHub for the latest release, at most once a day. Nothing about you is sent.");
                    ui.separator();
                    if ui.button("Choose SoundFont…").clicked() {
                        self.pick_soundfont();
                        ui.close();
                    }
                    #[cfg(windows)]
                    {
                        let tip = "Open .gp3, .gp4, .gp5, .gpx and .gp files with NeoGuitar when double-clicked";
                        if ui.button("Associate Guitar Pro files").on_hover_text(tip).clicked() {
                            self.assoc_msg = Some(match crate::assoc::associate() {
                                Ok(()) => "Files associated. If Windows still opens another program, choose NeoGuitar under “Open with”.".to_string(),
                                Err(e) => format!("Could not associate: {e}"),
                            });
                        }
                        if let Some(m) = &self.assoc_msg {
                            ui.weak(m);
                        }
                    }
                    match &self.player.status {
                        Status::NoSoundFont => ui.colored_label(theme::DANGER, "No SoundFont (choose a .sf2)"),
                        Status::Loading(_) => ui.weak("Loading SoundFont…"),
                        Status::Ready(p) => ui.weak(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                        Status::Error(e) => ui.colored_label(theme::DANGER, e),
                    };
                });
                ui.add_space(6.0);
                if theme::icon_button(ui, Icon::Plus, 28.0, IconStyle::Flat, true).on_hover_text("Zoom in").clicked() {
                    self.zoom = (self.zoom + 0.1).min(2.0);
                }
                ui.spacing_mut().slider_width = 90.0;
                ui.add(egui::Slider::new(&mut self.zoom, 0.6..=2.0).show_value(false));
                if theme::icon_button(ui, Icon::Minus, 28.0, IconStyle::Flat, true).on_hover_text("Zoom out").clicked() {
                    self.zoom = (self.zoom - 0.1).max(0.6);
                }
                ui.add_space(6.0);
                theme::chip(ui, Icon::Sidebar, self.show_side, label("Panel")).on_hover_text("Side panel").clicked().then(|| self.show_side = !self.show_side);
                theme::chip(ui, Icon::Layers, self.multi, label("Multi-track")).on_hover_text("Show several tracks at once (pick which with the eye icon in the panel)").clicked().then(|| self.multi = !self.multi);
                theme::chip(ui, Icon::Split, self.split, label("Split")).on_hover_text("Two tracks side by side").clicked().then(|| self.toggle_split());
                theme::chip(ui, Icon::Lyrics, self.show_lyrics, label("Lyrics")).on_hover_text("Lyrics next to the score (search LRCLIB or load a file)").clicked().then(|| self.show_lyrics = !self.show_lyrics);
                theme::chip(ui, Icon::Fretboard, self.show_fretboard, label("Fretboard")).on_hover_text("Show the sounding notes on the fretboard").clicked().then(|| self.show_fretboard = !self.show_fretboard);
                theme::chip(ui, Icon::Staff, self.show_staff, label("Staff")).on_hover_text("Standard notation").clicked().then(|| self.show_staff = !self.show_staff);
            });
        });
        if let Some(err) = &self.error {
            ui.colored_label(theme::DANGER, err);
        }
    }

    /// Barra inferior: reproducción, progreso y ajustes de práctica.
    fn transport(&mut self, ui: &mut egui::Ui) {
        let total = self.player.timeline.total;
        let playing = self.player.is_playing();
        let can_play = self.player.ready() && total > 0.0;
        let pos = self.player.position().min(total);

        // Progreso.
        let loop_frac = self.player.loop_measures.and_then(|(a, b)| {
            let t0 = self.player.timeline.time_of_measure(a)?;
            let t1 = self.player.timeline.first_end_of_measure(b)?;
            (total > 0.0).then(|| ((t0 / total) as f32, (t1 / total) as f32))
        });
        ui.horizontal(|ui| {
            ui.add_sized([44.0, 20.0], egui::Label::new(RichText::new(clock(pos)).monospace().color(theme::TEXT)));
            let w = (ui.available_width() - 52.0).max(50.0);
            let mut new_frac = None;
            ui.allocate_ui_with_layout(egui::vec2(w, 22.0), Layout::left_to_right(Align::Center), |ui| {
                new_frac = theme::seek_bar(ui, if total > 0.0 { (pos / total) as f32 } else { 0.0 }, loop_frac, can_play);
            });
            ui.add_sized([44.0, 20.0], egui::Label::new(RichText::new(clock(total)).monospace().color(theme::MUTED)));
            if let Some(f) = new_frac {
                self.player.seek(f as f64 * total);
                self.last_followed = None;
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if theme::icon_button(ui, Icon::Stop, 32.0, IconStyle::Flat, can_play).on_hover_text("Stop (Home)").clicked() {
                self.player.stop();
                self.last_followed = None;
            }
            let icon = if playing { Icon::Pause } else { Icon::Play };
            if theme::icon_button(ui, icon, 42.0, IconStyle::Round, can_play).on_hover_text("Play / pause (Space)").clicked() {
                self.toggle_play();
            }
            ui.add_space(8.0);
            let fs_style = if self.fullscreen { IconStyle::FlatOn } else { IconStyle::Flat };
            if theme::icon_button(ui, Icon::Fullscreen, 30.0, fs_style, true).on_hover_text("Full screen (F11, Esc to exit)").clicked() {
                self.set_fullscreen(&ui.ctx().clone(), !self.fullscreen);
            }
            ui.add_space(4.0);

            let mut speed = self.player.speed * 100.0;
            ui.label(RichText::new("Speed").color(theme::MUTED));
            ui.spacing_mut().slider_width = 100.0;
            if ui.add(egui::Slider::new(&mut speed, 30.0..=150.0).suffix("%").integer()).changed() {
                self.player.set_speed(speed / 100.0);
            }
            ui.add_space(4.0);
            let mut volume = self.player.volume;
            let muted = volume <= 0.001;
            if theme::icon_button(ui, if muted { Icon::SpeakerOff } else { Icon::Speaker }, 30.0, IconStyle::Flat, true).on_hover_text("Volume").clicked() {
                self.player.set_volume(if muted { 1.0 } else { 0.0 });
            }
            ui.spacing_mut().slider_width = 80.0;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false)).changed() {
                self.player.set_volume(volume);
            }
            ui.add_space(8.0);

            let metronome = self.player.metronome();
            if theme::chip(ui, Icon::Metronome, metronome, "Metronome").on_hover_text("M").clicked() {
                self.player.set_metronome(!metronome);
            }
            let has_loop = self.player.loop_measures.is_some();
            let looping = self.player.loop_enabled();
            let loop_label = match self.player.loop_measures {
                Some((a, b)) => format!("Loop {}–{}", a + 1, b + 1),
                None => "Loop".to_string(),
            };
            let hint = "Right-click a measure to set the loop";
            if theme::chip_enabled(ui, Icon::Loop, looping, &loop_label, has_loop).on_hover_text(hint).clicked() {
                self.player.set_loop_enabled(!looping);
            }
            if theme::chip(ui, Icon::Timer, self.count_in, "Count-in").on_hover_text("One measure of clicks before starting").clicked() {
                self.count_in = !self.count_in;
            }
            let tone_label = match self.player.transpose {
                0 => "Transpose".to_string(),
                n => format!("Transpose {n:+}"),
            };
            let tone_btn = theme::icon_label(ui, Some(Icon::Transpose), &tone_label, self.player.transpose != 0, true, false);
            egui::Popup::menu(&tone_btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_min_width(260.0);
                ui.label("Transpose the sound");
                ui.label(RichText::new("The tab stays the same; the staff follows the new pitch.").size(12.0).color(theme::MUTED));
                let mut n = self.player.transpose;
                ui.spacing_mut().slider_width = 200.0;
                let changed = ui.add(egui::Slider::new(&mut n, -12..=12).suffix(" semitones")).changed();
                let meaning = match self.player.transpose {
                    0 => "No transposition".to_string(),
                    t if t < 0 => format!("Like tuning the guitar {} semitone(s) lower", -t),
                    t => format!("Like a capo on fret {t}"),
                };
                ui.label(RichText::new(meaning).size(12.0).color(theme::ACCENT));
                ui.horizontal(|ui| {
                    let mut set = changed;
                    if ui.button("−1").clicked() {
                        n -= 1;
                        set = true;
                    }
                    if ui.button("+1").clicked() {
                        n += 1;
                        set = true;
                    }
                    if ui.add_enabled(self.player.transpose != 0, egui::Button::new("Reset")).clicked() {
                        n = 0;
                        set = true;
                    }
                    if set {
                        self.player.set_transpose(n);
                    }
                });
            });
            let (trainer_on, ..) = self.player.trainer();
            let trainer_btn = theme::icon_label(ui, Some(Icon::Trend), "Trainer", trainer_on, true, false);
            egui::Popup::menu(&trainer_btn).show(|ui| {
                let (mut on, mut step, mut max) = self.player.trainer();
                ui.label("With an active loop, the speed rises on every pass.");
                let mut changed = ui.checkbox(&mut on, "Enable").changed();
                changed |= ui.add(egui::Slider::new(&mut step, 1..=25).suffix("%").text("Step")).changed();
                changed |= ui.add(egui::Slider::new(&mut max, 30..=150).suffix("%").text("Max")).changed();
                if changed {
                    self.player.set_trainer(on, step, max);
                }
            });
            if theme::chip(ui, Icon::Follow, self.follow, "Follow").on_hover_text("Scroll the score along with playback").clicked() {
                self.follow = !self.follow;
            }
            match &self.player.status {
                Status::NoSoundFont => {
                    ui.colored_label(theme::DANGER, "No SoundFont: choose a .sf2 under “More”");
                }
                Status::Loading(_) => {
                    ui.weak("Loading SoundFont…");
                }
                Status::Error(e) => {
                    ui.colored_label(theme::DANGER, e);
                }
                Status::Ready(_) => {}
            }
            if let Some(n) = &self.notice {
                ui.colored_label(theme::ACCENT, n);
            }
        });
    }
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.player.poll();
        if let Some(rx) = &self.export_job {
            match rx.try_recv() {
                Ok(result) => {
                    self.export_job = None;
                    match result {
                        Ok(()) => self.notice = Some("Audio exported".into()),
                        Err(e) => {
                            self.notice = None;
                            self.error = Some(format!("Export failed: {e}"));
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(200)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.export_job = None;
                    self.notice = None;
                }
            }
        }
        self.poll_lyrics(&ctx);
        self.poll_updates(&ctx);
        self.player.sync_speed();
        self.persist_settings();
        if matches!(self.player.status, Status::Loading(_)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Arranque automático de prueba (NEOGUITAR_PLAY_AT).
        if let Some(t) = self.autoplay_at {
            if self.player.ready() && !self.player.is_playing() && self.player.position() < t + 0.5 {
                self.player.seek(t);
                self.player.play();
            }
        }
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.handle_screenshot(&ctx);

        // Arrastrar y soltar un archivo sobre la ventana.
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).next());
        if let Some(p) = dropped {
            self.open(&p);
        }
        let (f11, esc) = ctx.input(|i| (i.key_pressed(egui::Key::F11), i.key_pressed(egui::Key::Escape)));
        if f11 {
            self.set_fullscreen(&ctx, !self.fullscreen);
        } else if esc && self.fullscreen {
            self.set_fullscreen(&ctx, false);
        }
        let (space, ctrl_o, metro, home) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.modifiers.command && i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::M) && !i.modifiers.command,
                i.key_pressed(egui::Key::Home),
            )
        });
        let typing = ctx.egui_wants_keyboard_input();
        if space && !typing {
            self.toggle_play();
        }
        if ctrl_o {
            self.pick_file();
        }
        if metro && !typing {
            let on = !self.player.metronome();
            self.player.set_metronome(on);
        }
        if home && !typing {
            self.player.stop();
            self.last_followed = None;
        }

        // Título de la ventana según la canción abierta.
        let title = match &self.song {
            Some(s) if !s.title.trim().is_empty() => format!("{} — NeoGuitar", s.title.trim()),
            _ => "NeoGuitar".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        if !self.fullscreen {
            egui::Panel::top("toolbar").frame(theme::panel_frame(egui::Margin::symmetric(12, 8))).show_separator_line(true).show(ui, |ui| {
                self.top_bar(ui);
            });
        }
        if self.song.is_some() {
            egui::Panel::bottom("transport").frame(theme::panel_frame(egui::Margin::symmetric(14, 8))).show_separator_line(true).show(ui, |ui| {
                self.transport(ui);
            });
        }
        if self.show_fretboard {
            if let Some(track) = self.song.as_ref().and_then(|s| s.tracks.get(self.track)) {
                let pos = self.player.position();
                let active = if self.player.is_playing() || pos > 0.0 {
                    self.player.timeline.position(pos).map(|(m, tick)| track.sounding_notes(m, tick)).unwrap_or_default()
                } else {
                    Vec::new()
                };
                egui::Panel::bottom("fretboard").frame(theme::panel_frame(egui::Margin::symmetric(14, 8))).show_separator_line(true).show(ui, |ui| {
                    crate::fretboard::show(ui, track, &active);
                });
            }
        }
        if let Some(song) = &self.song {
            if self.show_side && !self.fullscreen {
                egui::Panel::left("info").default_size(300.0).size_range(240.0..=460.0).frame(theme::panel_frame(egui::Margin::same(12))).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        side_info(ui, song, self.path.as_deref(), &mut self.track, &self.player, &mut self.visible, self.multi, &mut self.jump_to, &mut self.mixer_open)
                    });
                });
            }
        }

        if self.show_lyrics && self.song.is_some() {
            if self.split {
                egui::Panel::bottom("lyrics_strip").frame(theme::panel_frame(egui::Margin::symmetric(14, 8))).show_separator_line(true).show(ui, |ui| {
                    self.lyrics_strip(ui);
                });
            } else {
                egui::Panel::right("lyrics").default_size(300.0).size_range(220.0..=480.0).frame(theme::panel_frame(egui::Margin::same(12))).show(ui, |ui| {
                    self.lyrics_panel(ui);
                });
            }
        }

        // Posición de reproducción en la partitura (compás y tick).
        let pos = self.player.position();
        let playhead = if self.song.is_some() && (self.player.is_playing() || pos > 0.0) {
            self.player.timeline.position(pos)
        } else {
            None
        };
        let mut scroll_to = self.jump_to.take().or(self.focus_measure);
        if self.follow && self.player.is_playing() {
            if let Some((m, _)) = playhead {
                if self.last_followed != Some(m) {
                    self.last_followed = Some(m);
                    scroll_to = Some(m + 1);
                }
            }
        }

        let mut action = None;
        let mut open_request: Option<PathBuf> = None;
        let mut pick_request = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::SCORE_BG).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| match &self.song {
            Some(song) => match song.tracks.get(self.track) {
                Some(_) if self.split && song.tracks.len() >= 2 => {
                    // Vista dividida: dos pistas lado a lado, cada una con su selector y su desplazamiento.
                    let labels: Vec<String> = song.tracks.iter().map(track_label).collect();
                    let last = song.tracks.len() - 1;
                    let mut sel = [self.track.min(last), self.split_track.min(last)];
                    ui.columns(2, |cols| {
                        for (k, col) in cols.iter_mut().enumerate() {
                            egui::ComboBox::from_id_salt(("split_track", k)).width(col.available_width().min(280.0)).selected_text(labels[sel[k]].clone()).show_ui(col, |ui| {
                                for (i, l) in labels.iter().enumerate() {
                                    ui.selectable_value(&mut sel[k], i, l);
                                }
                            });
                            col.add_space(2.0);
                            let track = &song.tracks[sel[k]];
                            let a = col.push_id(("split_view", k), |ui| {
                                tab_view::show(ui, song, &[track], self.zoom, scroll_to, playhead, self.player.loop_measures, self.show_staff, self.player.transpose)
                            });
                            if a.inner.is_some() {
                                action = a.inner;
                            }
                        }
                    });
                    self.track = sel[0];
                    self.split_track = sel[1];
                }
                Some(selected) => {
                    // Vista multipista: las pistas marcadas con el ojo; si no hay ninguna, la seleccionada.
                    let mut list: Vec<&neoguitar::model::Track> = Vec::new();
                    if self.multi {
                        for (i, t) in song.tracks.iter().enumerate() {
                            if self.visible.get(i).copied().unwrap_or(false) {
                                list.push(t);
                            }
                        }
                    }
                    if list.is_empty() {
                        list.push(selected);
                    }
                    action = tab_view::show(ui, song, &list, self.zoom, scroll_to, playhead, self.player.loop_measures, self.show_staff, self.player.transpose);
                }
                None => {
                    ui.label("The file has no tracks.");
                }
            },
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.28).max(20.0));
                    ui.label(RichText::new("NeoGuitar").size(38.0).strong().color(theme::TEXT));
                    ui.label(RichText::new("Guitar Pro tab reader and player").size(15.0).color(theme::MUTED));
                    ui.add_space(18.0);
                    if theme::primary_button(ui, "Open a file…").clicked() {
                        pick_request = true;
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("or drag a .gp3 · .gp4 · .gp5 · .gpx · .gp file here").size(12.5).color(theme::MUTED));
                    if !self.settings.recent.is_empty() {
                        ui.add_space(22.0);
                        theme::section(ui, "Recent");
                        for p in self.settings.recent.iter().take(6) {
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
                            if ui.add(egui::Button::new(name).min_size(egui::vec2(280.0, 30.0))).on_hover_text(p.display().to_string()).clicked() {
                                open_request = Some(p.clone());
                            }
                        }
                    }
                });
            }
        });
        if pick_request {
            self.pick_file();
        }
        if let Some(p) = open_request {
            self.open(&p);
        }
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

/// Carpeta donde se guardan las letras ya descargadas.
fn lyrics_dir() -> Option<PathBuf> {
    Settings::path().and_then(|p| p.parent().map(|d| d.join("lyrics")))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn track_label(t: &neoguitar::model::Track) -> String {
    let name = if t.name.trim().is_empty() { "(unnamed)" } else { t.name.trim() };
    format!("{}. {}", t.number, name)
}

fn side_info(ui: &mut egui::Ui, song: &Song, path: Option<&Path>, selected: &mut usize, player: &Player, visible: &mut [bool], multi: bool, jump: &mut Option<usize>, mixer_open: &mut Option<usize>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(p) = path.and_then(|p| p.file_name()) {
            ui.label(RichText::new(p.to_string_lossy()).size(11.0).color(theme::MUTED));
        }
        let title = if song.title.trim().is_empty() { "(untitled)" } else { song.title.trim() };
        ui.label(RichText::new(title).size(20.0).strong());
        let by: Vec<&str> = [song.artist.trim(), song.album.trim()].into_iter().filter(|s| !s.is_empty()).collect();
        if !by.is_empty() {
            ui.label(RichText::new(by.join("  ·  ")).color(theme::MUTED));
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            theme::pill(ui, &song.format.to_string());
            theme::pill(ui, &format!("{} BPM", song.tempo));
            theme::pill(ui, &format!("{} measures", song.headers.len()));
        });
        let extra: Vec<(&str, &str)> = [
            ("Subtitle", song.subtitle.trim()),
            ("Words", song.words.trim()),
            ("Music", song.music.trim()),
            ("Tabbed by", song.tab_by.trim()),
            ("Copyright", song.copyright.trim()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .collect();
        if !extra.is_empty() || !song.comments.trim().is_empty() {
            ui.add_space(2.0);
            ui.collapsing("Details", |ui| {
                for (k, v) in &extra {
                    ui.label(RichText::new(format!("{k}: {v}")).size(12.5).color(theme::MUTED));
                }
                if !song.comments.trim().is_empty() {
                    ui.label(RichText::new(song.comments.trim()).size(12.5).color(theme::MUTED));
                }
            });
        }
    });

    theme::section(ui, "Tracks");
    for (i, t) in song.tracks.iter().enumerate() {
        let tuning = if t.is_percussion {
            "percussion".to_string()
        } else {
            let notes = t.strings.iter().map(|&m| midi_note_name(m)).collect::<Vec<_>>().join(" ");
            if t.capo > 0 { format!("{notes} · capo {}", t.capo) } else { notes }
        };
        let [r, g, b] = t.color;
        let is_sel = *selected == i;
        let frame = egui::Frame::new()
            .fill(if is_sel { egui::Color32::from_rgb(30, 44, 70) } else { theme::CARD })
            .stroke(egui::Stroke::new(1.0, if is_sel { theme::ACCENT } else { theme::BORDER }))
            .corner_radius(egui::CornerRadius::same(9))
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::same(2), theme::track_color([r, g, b]));
                let reserve = if multi { 144.0 } else { 112.0 };
                let w = (ui.available_width() - reserve).max(40.0);
                ui.allocate_ui_with_layout(egui::vec2(w, 22.0), Layout::left_to_right(Align::Center), |ui| {
                    let mut name = RichText::new(track_label(t)).size(14.0);
                    if is_sel {
                        name = name.strong();
                    }
                    let resp = ui.add(egui::Label::new(name).truncate().sense(egui::Sense::click()));
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *selected = i;
                        if let Some(v) = visible.get_mut(i) {
                            *v = true;
                        }
                    }
                });
                let on = player.track_enabled(i);
                let (mute_icon, mute_style) = if on { (Icon::Speaker, IconStyle::Flat) } else { (Icon::SpeakerOff, IconStyle::Danger) };
                let mute_tip = if on { "Mute track" } else { "Track muted: click to unmute" };
                if theme::icon_button(ui, mute_icon, 24.0, mute_style, true).on_hover_text(mute_tip).clicked() {
                    player.set_track_enabled(i, !on);
                }
                let solo = on && song.tracks.len() > 1 && (0..song.tracks.len()).all(|k| k == i || !player.track_enabled(k));
                let solo_style = if solo { IconStyle::FlatOn } else { IconStyle::Flat };
                if theme::icon_button(ui, Icon::Headphones, 24.0, solo_style, true).on_hover_text("Solo: hear only this track").clicked() {
                    player.toggle_solo(i, song.tracks.len());
                }
                let mix_on = *mixer_open == Some(i);
                let mix_style = if mix_on { IconStyle::FlatOn } else { IconStyle::Flat };
                if theme::icon_button(ui, Icon::Sliders, 24.0, mix_style, true).on_hover_text("Track mix: instrument, pan and reverb").clicked() {
                    *mixer_open = if mix_on { None } else { Some(i) };
                }
                if multi {
                    if let Some(v) = visible.get_mut(i) {
                        let style = if *v { IconStyle::FlatOn } else { IconStyle::Flat };
                        if theme::icon_button(ui, Icon::Eye, 24.0, style, true).on_hover_text("Show in the multi-track view").clicked() {
                            *v = !*v;
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(&tuning).size(11.5).color(theme::MUTED));
                let mut gain = player.track_gain(i) as u32;
                ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(40.0);
                if ui.add(egui::Slider::new(&mut gain, 0..=200).show_value(false)).on_hover_text(format!("Track volume: {gain}%")).changed() {
                    player.set_track_gain(i, gain as u8);
                }
            });
            if *mixer_open == Some(i) {
                mixer_controls(ui, i, t, player);
            }
        });
    }
    sections_panel(ui, song, player, jump);
    chords_panel(ui, song);
}

/// Mezcla de una pista: instrumento, paneo y envío de reverb. Los cambios suenan al instante.
fn mixer_controls(ui: &mut egui::Ui, i: usize, t: &neoguitar::model::Track, player: &Player) {
    ui.add_space(2.0);
    ui.separator();
    let (mut pan_o, mut prog_o, mut rev_o) = (player.mix_pan(i), player.mix_program(i), player.mix_reverb(i));
    let mut changed = false;
    let label = |ui: &mut egui::Ui, text: &str| ui.label(RichText::new(text).size(11.5).color(theme::MUTED));

    if !t.is_percussion {
        let program = prog_o.unwrap_or(t.program.clamp(0, 127) as u8) as usize;
        label(ui, "Instrument");
        egui::ComboBox::from_id_salt(("instrumento", i)).width(ui.available_width() - 8.0).selected_text(neoguitar::gm::NAMES[program.min(127)]).show_ui(ui, |ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (k, name) in neoguitar::gm::NAMES.iter().enumerate() {
                    if ui.selectable_label(program == k, format!("{:>3}  {name}", k + 1)).clicked() {
                        prog_o = Some(k as u8);
                        changed = true;
                    }
                }
            });
        });
    }

    let mut pan = pan_o.unwrap_or(t.pan.min(127));
    label(ui, "Pan");
    ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
    let pan_fmt = |v: f64, _: std::ops::RangeInclusive<usize>| {
        let d = (v - 64.0).round() as i32;
        match d {
            -1..=1 => "Center".to_string(),
            d if d < 0 => format!("L {}", -d * 100 / 64),
            d => format!("R {}", d * 100 / 63),
        }
    };
    if ui.add(egui::Slider::new(&mut pan, 0..=127).custom_formatter(pan_fmt)).changed() {
        pan_o = Some(pan);
        changed = true;
    }

    let mut reverb = rev_o.unwrap_or(neoguitar::playback::DEFAULT_REVERB);
    label(ui, "Reverb");
    if ui.add(egui::Slider::new(&mut reverb, 0..=127).custom_formatter(|v, _| format!("{:.0} %", v * 100.0 / 127.0))).changed() {
        rev_o = Some(reverb);
        changed = true;
    }

    let customized = pan_o.is_some() || prog_o.is_some() || rev_o.is_some();
    if ui.add_enabled(customized, egui::Button::new("Reset")).on_hover_text("Back to the file’s values").clicked() {
        (pan_o, prog_o, rev_o) = (None, None, None);
        changed = true;
    }
    if changed {
        player.set_mix(i, pan_o, prog_o, rev_o);
    }
}

/// Secciones de la canción (marcadores de compás): un clic lleva la vista y la reproducción a ese compás.
fn sections_panel(ui: &mut egui::Ui, song: &Song, player: &Player, jump: &mut Option<usize>) {
    let sections: Vec<(usize, &str)> = song
        .headers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| h.marker.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(|m| (i, m)))
        .collect();
    if sections.is_empty() {
        return;
    }
    theme::section(ui, &format!("Sections ({})", sections.len()));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (i, name) in sections {
            let label = format!("{name} · {}", i + 1);
            if theme::icon_label(ui, None, &label, false, true, false).on_hover_text(format!("Go to measure {}", i + 1)).clicked() {
                if let Some(t) = player.timeline.time_of_measure(i) {
                    player.seek(t);
                }
                *jump = Some(i + 1);
            }
        }
    });
}

fn save_png(path: &Path, img: &egui::ColorImage) -> Result<(), String> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in &img.pixels {
        rgba.extend_from_slice(&px.to_array());
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}

/// Acordes distintos de la canción, con su diagrama.
fn chords_panel(ui: &mut egui::Ui, song: &Song) {
    let mut chords: Vec<&neoguitar::model::Chord> = Vec::new();
    for beat in song.tracks.iter().flat_map(|t| &t.measures).flat_map(|m| &m.beats) {
        if let Some(c) = &beat.chord {
            if !chords.iter().any(|k| k.name == c.name && k.frets == c.frets) {
                chords.push(c);
            }
        }
    }
    if chords.is_empty() {
        return;
    }
    ui.separator();
    egui::CollapsingHeader::new(format!("Chords ({})", chords.len())).default_open(std::env::var_os("NEOGUITAR_OPEN_CHORDS").is_some()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for c in chords.iter().take(60) {
                chord_diagram(ui, c);
            }
        });
    });
}

/// Diagrama de acorde: cuerdas verticales, 5 trastes, puntos y marcas de cuerda al aire o muda.
fn chord_diagram(ui: &mut egui::Ui, chord: &neoguitar::model::Chord) {
    let size = egui::vec2(58.0, 76.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let fg = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let strings = chord.frets.len().clamp(2, 8);
    let (left, right) = (rect.left() + 10.0, rect.right() - 10.0);
    let top = rect.top() + 26.0;
    let fret_h = 9.5;
    let gap = (right - left) / (strings as f32 - 1.0);

    p.text(egui::pos2(rect.center().x, rect.top() + 2.0), egui::Align2::CENTER_TOP, &chord.name, egui::FontId::proportional(10.5), fg);

    // Ventana de 5 trastes: desde el 1 si el acorde cabe, si no desde el traste más bajo usado.
    let used: Vec<i32> = chord.frets.iter().copied().filter(|&f| f > 0).collect();
    let max = used.iter().copied().max().unwrap_or(0);
    let min = used.iter().copied().min().unwrap_or(1);
    let base = if max <= 5 { 1 } else { min };
    for s in 0..strings {
        let x = left + s as f32 * gap;
        p.line_segment([egui::pos2(x, top), egui::pos2(x, top + fret_h * 5.0)], egui::Stroke::new(1.0, weak));
    }
    for f in 0..=5 {
        let y = top + f as f32 * fret_h;
        let w = if f == 0 && base == 1 { 2.5 } else { 1.0 };
        p.line_segment([egui::pos2(left, y), egui::pos2(right, y)], egui::Stroke::new(w, weak));
    }
    if base > 1 {
        p.text(egui::pos2(rect.left() + 1.0, top + fret_h * 0.5), egui::Align2::LEFT_CENTER, base.to_string(), egui::FontId::proportional(8.5), weak);
    }
    for (s, &f) in chord.frets.iter().enumerate().take(strings) {
        let x = left + s as f32 * gap;
        if f < 0 {
            p.text(egui::pos2(x, top - 4.0), egui::Align2::CENTER_BOTTOM, "x", egui::FontId::proportional(9.0), weak);
        } else if f == 0 {
            p.circle_stroke(egui::pos2(x, top - 7.0), 2.5, egui::Stroke::new(1.0, weak));
        } else {
            let row = (f - base).clamp(0, 4) as f32;
            p.circle_filled(egui::pos2(x, top + fret_h * (row + 0.5)), 3.2, fg);
        }
    }
}

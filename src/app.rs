use crate::theme::{self, Icon, IconStyle};
use crate::tab_view::{self, ViewAction};
use eframe::egui::{self, Align, Layout, RichText};
use neoguitar::audio::{Player, Status};
use neoguitar::model::{midi_note_name, Song};
use neoguitar::settings::Settings;
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
        };
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
        if std::env::var_os("NEOGUITAR_MULTI").is_some() {
            app.multi = true;
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
                self.song = Some(song);
                self.path = Some(path.to_path_buf());
                self.settings.add_recent(path);
                self.error = None;
                self.track = 0;
                self.last_followed = None;
            }
            Err(e) => {
                self.error = Some(format!("No se pudo abrir {}: {e}", path.display()));
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
                    eprintln!("no se pudo guardar la captura: {e}");
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
            .add_filter("Todos los archivos", &["*"])
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
            .unwrap_or_else(|| "cancion.mid".to_string());
        let target = rfd::FileDialog::new().add_filter("MIDI", &["mid"]).set_file_name(name).save_file();
        if let Some(p) = target {
            self.error = self.player.export_midi(&p).err().map(|e| format!("No se pudo exportar: {e}"));
        }
    }

    fn export_ascii(&mut self) {
        let Some(song) = &self.song else { return };
        let Some(track) = song.tracks.get(self.track) else { return };
        let text = neoguitar::ascii::ascii_tab(song, track, 100);
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "tab".into());
        let name = format!("{stem} - {}.txt", track.name.trim().replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_"));
        if let Some(p) = rfd::FileDialog::new().add_filter("Texto", &["txt"]).set_file_name(name).save_file() {
            self.error = std::fs::write(&p, text).err().map(|e| format!("No se pudo exportar: {e}"));
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
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let labels: Vec<String> = self.song.as_ref().map(|s| s.tracks.iter().map(track_label).collect()).unwrap_or_default();
        ui.horizontal(|ui| {
            if theme::primary_button(ui, "Abrir").on_hover_text("Ctrl+O").clicked() {
                self.pick_file();
            }
            let mut reopen: Option<PathBuf> = None;
            let recent_btn = theme::icon_label(ui, Some(Icon::Clock), "Recientes", false, true, false);
            egui::Popup::menu(&recent_btn).show(|ui| {
                ui.set_min_width(220.0);
                if self.settings.recent.is_empty() {
                    ui.weak("(sin archivos recientes)");
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
                let more_btn = theme::icon_label(ui, Some(Icon::Dots), "Más", false, true, false);
                egui::Popup::menu(&more_btn).show(|ui| {
                    if ui.button("Exportar MIDI…").on_hover_text("Pistas activas").clicked() {
                        self.export_midi();
                        ui.close();
                    }
                    if ui.button("Exportar tablatura (txt)…").on_hover_text("Pista actual").clicked() {
                        self.export_ascii();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Elegir SoundFont…").clicked() {
                        self.pick_soundfont();
                        ui.close();
                    }
                    #[cfg(windows)]
                    {
                        let tip = "Abrir los archivos .gp3, .gp4, .gp5, .gpx y .gp con NeoGuitar al hacer doble clic";
                        if ui.button("Asociar archivos Guitar Pro").on_hover_text(tip).clicked() {
                            self.assoc_msg = Some(match crate::assoc::associate() {
                                Ok(()) => "Archivos asociados. Si Windows sigue abriendo otro programa, elígelo en «Abrir con».".to_string(),
                                Err(e) => format!("No se pudo asociar: {e}"),
                            });
                        }
                        if let Some(m) = &self.assoc_msg {
                            ui.weak(m);
                        }
                    }
                    match &self.player.status {
                        Status::NoSoundFont => ui.colored_label(theme::DANGER, "Sin SoundFont (elegí un .sf2)"),
                        Status::Loading(_) => ui.weak("Cargando SoundFont…"),
                        Status::Ready(p) => ui.weak(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                        Status::Error(e) => ui.colored_label(theme::DANGER, e),
                    };
                });
                ui.add_space(6.0);
                if theme::icon_button(ui, Icon::Plus, 28.0, IconStyle::Flat, true).on_hover_text("Acercar").clicked() {
                    self.zoom = (self.zoom + 0.1).min(2.0);
                }
                ui.spacing_mut().slider_width = 90.0;
                ui.add(egui::Slider::new(&mut self.zoom, 0.6..=2.0).show_value(false));
                if theme::icon_button(ui, Icon::Minus, 28.0, IconStyle::Flat, true).on_hover_text("Alejar").clicked() {
                    self.zoom = (self.zoom - 0.1).max(0.6);
                }
                ui.add_space(6.0);
                theme::chip(ui, Icon::Sidebar, self.show_side, "Panel").clicked().then(|| self.show_side = !self.show_side);
                theme::chip(ui, Icon::Layers, self.multi, "Multipista").on_hover_text("Ver varias pistas a la vez (elegí cuáles con el ojo del panel)").clicked().then(|| self.multi = !self.multi);
                theme::chip(ui, Icon::Staff, self.show_staff, "Pentagrama").clicked().then(|| self.show_staff = !self.show_staff);
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
            if theme::icon_button(ui, Icon::Stop, 32.0, IconStyle::Flat, can_play).on_hover_text("Detener (Inicio)").clicked() {
                self.player.stop();
                self.last_followed = None;
            }
            let icon = if playing { Icon::Pause } else { Icon::Play };
            if theme::icon_button(ui, icon, 42.0, IconStyle::Round, can_play).on_hover_text("Reproducir / pausar (Espacio)").clicked() {
                self.toggle_play();
            }
            ui.add_space(8.0);

            let mut speed = self.player.speed * 100.0;
            ui.label(RichText::new("Velocidad").color(theme::MUTED));
            ui.spacing_mut().slider_width = 100.0;
            if ui.add(egui::Slider::new(&mut speed, 30.0..=150.0).suffix("%").integer()).changed() {
                self.player.set_speed(speed / 100.0);
            }
            ui.add_space(4.0);
            let mut volume = self.player.volume;
            let muted = volume <= 0.001;
            if theme::icon_button(ui, if muted { Icon::SpeakerOff } else { Icon::Speaker }, 30.0, IconStyle::Flat, true).on_hover_text("Volumen").clicked() {
                self.player.set_volume(if muted { 1.0 } else { 0.0 });
            }
            ui.spacing_mut().slider_width = 80.0;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false)).changed() {
                self.player.set_volume(volume);
            }
            ui.add_space(8.0);

            let metronome = self.player.metronome();
            if theme::chip(ui, Icon::Metronome, metronome, "Metrónomo").on_hover_text("M").clicked() {
                self.player.set_metronome(!metronome);
            }
            let has_loop = self.player.loop_measures.is_some();
            let looping = self.player.loop_enabled();
            let loop_label = match self.player.loop_measures {
                Some((a, b)) => format!("Bucle {}–{}", a + 1, b + 1),
                None => "Bucle".to_string(),
            };
            let hint = "Clic derecho sobre un compás para definir el bucle";
            if theme::chip_enabled(ui, Icon::Loop, looping, &loop_label, has_loop).on_hover_text(hint).clicked() {
                self.player.set_loop_enabled(!looping);
            }
            if theme::chip(ui, Icon::Timer, self.count_in, "Cuenta").on_hover_text("Un compás de clics antes de empezar").clicked() {
                self.count_in = !self.count_in;
            }
            let (trainer_on, ..) = self.player.trainer();
            let trainer_btn = theme::icon_label(ui, Some(Icon::Trend), "Entrenador", trainer_on, true, false);
            egui::Popup::menu(&trainer_btn).show(|ui| {
                let (mut on, mut step, mut max) = self.player.trainer();
                ui.label("Con un bucle activo, la velocidad sube en cada vuelta.");
                let mut changed = ui.checkbox(&mut on, "Activar").changed();
                changed |= ui.add(egui::Slider::new(&mut step, 1..=25).suffix("%").text("Paso")).changed();
                changed |= ui.add(egui::Slider::new(&mut max, 30..=150).suffix("%").text("Máximo")).changed();
                if changed {
                    self.player.set_trainer(on, step, max);
                }
            });
            if theme::chip(ui, Icon::Follow, self.follow, "Seguir").on_hover_text("Desplazar la partitura con la reproducción").clicked() {
                self.follow = !self.follow;
            }
            match &self.player.status {
                Status::NoSoundFont => {
                    ui.colored_label(theme::DANGER, "Sin SoundFont: elegí un .sf2 en «Más»");
                }
                Status::Loading(_) => {
                    ui.weak("Cargando SoundFont…");
                }
                Status::Error(e) => {
                    ui.colored_label(theme::DANGER, e);
                }
                Status::Ready(_) => {}
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
        let (space, ctrl_o, metro, home) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.modifiers.command && i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::M) && !i.modifiers.command,
                i.key_pressed(egui::Key::Home),
            )
        });
        if space {
            self.toggle_play();
        }
        if ctrl_o {
            self.pick_file();
        }
        if metro {
            let on = !self.player.metronome();
            self.player.set_metronome(on);
        }
        if home {
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

        egui::Panel::top("toolbar").frame(theme::panel_frame(egui::Margin::symmetric(12, 8))).show_separator_line(true).show(ui, |ui| {
            self.top_bar(ui);
        });
        if self.song.is_some() {
            egui::Panel::bottom("transport").frame(theme::panel_frame(egui::Margin::symmetric(14, 8))).show_separator_line(true).show(ui, |ui| {
                self.transport(ui);
            });
        }
        if let Some(song) = &self.song {
            if self.show_side {
                egui::Panel::left("info").default_size(300.0).size_range(240.0..=460.0).frame(theme::panel_frame(egui::Margin::same(12))).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        side_info(ui, song, self.path.as_deref(), &mut self.track, &self.player, &mut self.visible, self.multi)
                    });
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
        let mut scroll_to = self.focus_measure;
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
                    action = tab_view::show(ui, song, &list, self.zoom, scroll_to, playhead, self.player.loop_measures, self.show_staff);
                }
                None => {
                    ui.label("El archivo no tiene pistas.");
                }
            },
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.28).max(20.0));
                    ui.label(RichText::new("NeoGuitar").size(38.0).strong().color(theme::TEXT));
                    ui.label(RichText::new("Lector y reproductor de tablaturas Guitar Pro").size(15.0).color(theme::MUTED));
                    ui.add_space(18.0);
                    if theme::primary_button(ui, "Abrir un archivo…").clicked() {
                        pick_request = true;
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("o arrastrá aquí un .gp3 · .gp4 · .gp5 · .gpx · .gp").size(12.5).color(theme::MUTED));
                    if !self.settings.recent.is_empty() {
                        ui.add_space(22.0);
                        theme::section(ui, "Recientes");
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

fn track_label(t: &neoguitar::model::Track) -> String {
    let name = if t.name.trim().is_empty() { "(sin nombre)" } else { t.name.trim() };
    format!("{}. {}", t.number, name)
}

fn side_info(ui: &mut egui::Ui, song: &Song, path: Option<&Path>, selected: &mut usize, player: &Player, visible: &mut [bool], multi: bool) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(p) = path.and_then(|p| p.file_name()) {
            ui.label(RichText::new(p.to_string_lossy()).size(11.0).color(theme::MUTED));
        }
        let title = if song.title.trim().is_empty() { "(sin título)" } else { song.title.trim() };
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
            theme::pill(ui, &format!("{} compases", song.headers.len()));
        });
        let extra: Vec<(&str, &str)> = [
            ("Subtítulo", song.subtitle.trim()),
            ("Letra", song.words.trim()),
            ("Música", song.music.trim()),
            ("Tab por", song.tab_by.trim()),
            ("Copyright", song.copyright.trim()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .collect();
        if !extra.is_empty() || !song.comments.trim().is_empty() {
            ui.add_space(2.0);
            ui.collapsing("Detalles", |ui| {
                for (k, v) in &extra {
                    ui.label(RichText::new(format!("{k}: {v}")).size(12.5).color(theme::MUTED));
                }
                if !song.comments.trim().is_empty() {
                    ui.label(RichText::new(song.comments.trim()).size(12.5).color(theme::MUTED));
                }
            });
        }
    });

    theme::section(ui, "Pistas");
    for (i, t) in song.tracks.iter().enumerate() {
        let tuning = if t.is_percussion {
            "percusión".to_string()
        } else {
            t.strings.iter().map(|&m| midi_note_name(m)).collect::<Vec<_>>().join(" ")
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
                let reserve = if multi { 112.0 } else { 80.0 };
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
                let mute_tip = if on { "Silenciar la pista" } else { "Pista silenciada: clic para activarla" };
                if theme::icon_button(ui, mute_icon, 24.0, mute_style, true).on_hover_text(mute_tip).clicked() {
                    player.set_track_enabled(i, !on);
                }
                let solo = on && song.tracks.len() > 1 && (0..song.tracks.len()).all(|k| k == i || !player.track_enabled(k));
                let solo_style = if solo { IconStyle::FlatOn } else { IconStyle::Flat };
                if theme::icon_button(ui, Icon::Headphones, 24.0, solo_style, true).on_hover_text("Solo: escuchar solo esta pista").clicked() {
                    player.toggle_solo(i, song.tracks.len());
                }
                if multi {
                    if let Some(v) = visible.get_mut(i) {
                        let style = if *v { IconStyle::FlatOn } else { IconStyle::Flat };
                        if theme::icon_button(ui, Icon::Eye, 24.0, style, true).on_hover_text("Mostrar en la vista multipista").clicked() {
                            *v = !*v;
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(&tuning).size(11.5).color(theme::MUTED));
                let mut gain = player.track_gain(i) as u32;
                ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(40.0);
                if ui.add(egui::Slider::new(&mut gain, 0..=200).show_value(false)).on_hover_text(format!("Volumen de la pista: {gain}%")).changed() {
                    player.set_track_gain(i, gain as u8);
                }
            });
        });
    }
    chords_panel(ui, song);
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
    egui::CollapsingHeader::new(format!("Acordes ({})", chords.len())).default_open(std::env::var_os("NEOGUITAR_OPEN_CHORDS").is_some()).show(ui, |ui| {
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

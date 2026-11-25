//! Ventana "Find tabs online": usa el complemento opcional (`neoguitar_win::plugin`). Si no está instalado,
//! ofrece instalarlo; todo el trabajo de red va en hilos para no bloquear la interfaz.

use eframe::egui;
use neoguitar_win::plugin::{self, Hit, Page};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

enum Msg {
    Search(u32, Result<Page, String>),
    Download(Result<PathBuf, String>),
    Install(Result<String, String>),
    Version(Result<String, String>),
}

#[derive(Default)]
pub struct TabsSearch {
    pub open: bool,
    query: String,
    page: u32,
    has_next: bool,
    hits: Vec<Hit>,
    searched: bool,
    rx: Option<Receiver<Msg>>,
    busy: Option<&'static str>,
    msg: Option<String>,
    version: Option<String>,
    agreed: bool,
}

impl TabsSearch {
    pub fn show_window(&mut self) {
        self.open = true;
    }

    fn spawn(&mut self, ctx: &egui::Context, what: &'static str, job: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        self.rx = Some(rx);
        self.busy = Some(what);
        self.msg = None;
    }

    fn start_search(&mut self, ctx: &egui::Context, page: u32) {
        let q = self.query.clone();
        self.spawn(ctx, "Searching…", move || Msg::Search(page, plugin::search(&q, page)));
    }

    fn poll(&mut self) -> Option<PathBuf> {
        let rx = self.rx.as_ref()?;
        let msg = match rx.try_recv() {
            Ok(m) => m,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                self.busy = None;
                return None;
            }
        };
        self.rx = None;
        self.busy = None;
        match msg {
            Msg::Search(page, Ok(p)) => {
                self.page = page;
                self.has_next = p.has_next;
                self.hits = p.hits;
                self.searched = true;
            }
            Msg::Search(_, Err(e)) | Msg::Install(Err(e)) => self.msg = Some(e),
            Msg::Download(Err(e)) => self.msg = Some(format!("Could not download: {e}")),
            Msg::Download(Ok(path)) => return Some(path),
            Msg::Install(Ok(v)) => {
                self.version = Some(v);
                self.msg = Some("Plugin installed.".into());
            }
            Msg::Version(Ok(v)) => self.version = Some(v),
            Msg::Version(Err(e)) => self.msg = Some(e),
        }
        None
    }

    /// Dibuja la ventana. Devuelve la ruta de un tab recién descargado para que la aplicación lo abra.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let opened = self.poll();
        if !self.open {
            return opened;
        }
        let installed = plugin::is_installed();
        if installed && self.version.is_none() && self.rx.is_none() {
            self.spawn(ctx, "Checking the plugin…", || Msg::Version(plugin::version()));
        }
        let mut open = self.open;
        let mut pick: Option<Hit> = None;
        let mut go: Option<u32> = None;
        let mut install = false;
        let mut remove = false;
        egui::Window::new("Find tabs online").open(&mut open).default_width(520.0).collapsible(false).show(ctx, |ui| {
            if !installed {
                ui.label("Searching and downloading tabs is an optional plugin, not part of NeoGuitar. It is a separate program (gprotab, from github.com/Fiambre/GTabs-Downloader) that fetches Guitar Pro files from gprotab.net.");
                ui.add_space(4.0);
                ui.weak("Tabs are the work of their authors. Use them for your own practice and respect their copyright. NeoGuitar only installs the plugin after checking its SHA-256 from the project's releases.");
                ui.add_space(6.0);
                ui.checkbox(&mut self.agreed, "I understand and want to install the plugin");
                ui.add_space(4.0);
                if ui.add_enabled(self.agreed && self.busy.is_none(), egui::Button::new("Install plugin")).clicked() {
                    install = true;
                }
            } else {
                ui.horizontal(|ui| {
                    let edit = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Song title…").desired_width(340.0));
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let can = self.busy.is_none() && !self.query.trim().is_empty();
                    if (ui.add_enabled(can, egui::Button::new("Search")).clicked() || (enter && can)) && can {
                        go = Some(1);
                    }
                });
                ui.add_space(4.0);
                if self.searched && self.hits.is_empty() {
                    ui.weak("No results.");
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for h in &self.hits {
                        ui.horizontal(|ui| {
                            if ui.add_enabled(self.busy.is_none(), egui::Button::new("Open")).on_hover_text("Download and open").clicked() {
                                pick = Some(h.clone());
                            }
                            ui.label(format!("{} — {}", h.artist, h.title));
                        });
                    }
                });
                if self.searched {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(self.page > 1 && self.busy.is_none(), egui::Button::new("◀ Previous")).clicked() {
                            go = Some(self.page - 1);
                        }
                        ui.label(format!("Page {}", self.page));
                        if ui.add_enabled(self.has_next && self.busy.is_none(), egui::Button::new("Next ▶")).clicked() {
                            go = Some(self.page + 1);
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.weak(format!("Plugin {}", self.version.as_deref().unwrap_or("…")));
                    if ui.small_button("Remove plugin").clicked() {
                        remove = true;
                    }
                });
            }
            if let Some(b) = self.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(b);
                });
            }
            if let Some(m) = &self.msg {
                ui.colored_label(ui.visuals().warn_fg_color, m);
            }
        });
        self.open = open;

        if install {
            self.spawn(ctx, "Installing…", || Msg::Install(plugin::install()));
        }
        if remove {
            self.msg = plugin::uninstall().err();
            self.version = None;
            self.hits.clear();
            self.searched = false;
        }
        if let Some(p) = go {
            self.start_search(ctx, p);
        }
        if let Some(h) = pick {
            let dir = plugin::downloads_dir();
            let id = h.id.clone();
            self.spawn(ctx, "Downloading…", move || Msg::Download(plugin::download(&id, &dir)));
        }
        opened
    }
}

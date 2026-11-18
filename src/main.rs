#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
#[cfg(windows)]
mod assoc;
mod staff;
mod tab_view;
mod theme;

fn main() -> eframe::Result {
    let arg = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        viewport: egui_viewport(),
        ..Default::default()
    };
    eframe::run_native(
        "NeoGuitar",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, arg.clone())))),
    )
}

fn egui_viewport() -> eframe::egui::ViewportBuilder {
    eframe::egui::ViewportBuilder::default()
        .with_icon(window_icon())
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([640.0, 400.0])
        .with_drag_and_drop(true)
}

/// Ícono de la ventana (el mismo que lleva el ejecutable).
fn window_icon() -> eframe::egui::IconData {
    let bytes = include_bytes!("../assets/icon/icon-256.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes[..])).read_info().expect("icono PNG");
    let mut buf = vec![0; reader.output_buffer_size().expect("tamaño del icono")];
    let info = reader.next_frame(&mut buf).expect("icono PNG");
    buf.truncate(info.buffer_size());
    eframe::egui::IconData { rgba: buf, width: info.width, height: info.height }
}
//! NeoGuitar for Android. The whole app is compiled only for Android; elsewhere this crate is empty.
#![cfg(target_os = "android")]

mod app;
mod platform;
mod settings;

use android_activity::AndroidApp;

#[unsafe(no_mangle)]
pub fn android_main(android: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("NeoGuitar"));
    // The screen stays on while practising (hands are busy with the guitar).
    android.set_window_flags(android_activity::WindowManagerFlags::KEEP_SCREEN_ON, android_activity::WindowManagerFlags::empty());
    let options = eframe::NativeOptions { android_app: Some(android.clone()), ..Default::default() };
    let result = eframe::run_native("NeoGuitar", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, android.clone())))));
    if let Err(e) = result {
        log::error!("NeoGuitar stopped: {e}");
    }
}

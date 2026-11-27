#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bt;
mod config;
mod device;
mod models;
mod protocol;
mod settings;
mod theme;

fn main() -> eframe::Result {
    config::preload();
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok().map(|i| {
        let i = i.into_rgba8();
        let (w, h) = i.dimensions();
        eframe::egui::IconData { rgba: i.into_raw(), width: w, height: h }
    });
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("ear")
        .with_inner_size([460.0, 860.0])
        .with_min_inner_size([380.0, 560.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("ear-desktop", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

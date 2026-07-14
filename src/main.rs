mod app;
mod audio;
mod engine;
mod models;
mod network;
mod ui;
mod updater;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([650.0, 450.0])
            .with_min_inner_size([650.0, 450.0])
            .with_title("P2P Voice"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "P2P Voice",
        options,
        Box::new(|_cc| Box::new(app::P2PApp::default())),
    )
}

mod app;
mod audio;
mod engine;
mod models;
mod network;
mod ui;
mod updater;

use crate::app::P2PApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon = crate::app::load_icon_data();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([650.0, 520.0])
            .with_min_inner_size([650.0, 520.0])
            .with_title("VVcall")
            .with_icon(icon),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native("VVcall", options, Box::new(|cc| Box::new(P2PApp::new(cc))))
}

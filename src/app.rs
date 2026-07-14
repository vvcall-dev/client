use crate::audio;
use crate::models::PeerState;
use crate::updater::{UpdateInfo, check_for_updates};
use eframe::egui;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

pub struct P2PApp {
    pub username: String,
    pub room_name: String,
    pub is_connected: bool,
    pub volume_level: Arc<Mutex<f32>>,
    pub status_text: Arc<Mutex<String>>,
    pub kill_signal: Arc<AtomicBool>,
    pub is_muted: Arc<AtomicBool>,
    pub is_deafened: Arc<AtomicBool>,
    pub active_peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
    pub available_inputs: Vec<String>,
    pub available_outputs: Vec<String>,
    pub selected_input: String,
    pub selected_output: String,
    pub update_info: Arc<Mutex<UpdateInfo>>,
    pub show_update_dialog: bool,
    pub is_updating: bool,
    pub show_overlay: bool,
}

impl Default for P2PApp {
    fn default() -> Self {
        let update_info = Arc::new(Mutex::new(UpdateInfo::default()));
        check_for_updates(update_info.clone());

        let (inputs, outputs) = audio::get_audio_devices();
        let default_in = inputs
            .first()
            .cloned()
            .unwrap_or_else(|| "Нет устройств".into());
        let default_out = outputs
            .first()
            .cloned()
            .unwrap_or_else(|| "Нет устройств".into());

        Self {
            username: "defaultuser67".to_owned(),
            room_name: "123".to_owned(),
            is_connected: false,
            volume_level: Arc::new(Mutex::new(0.0)),
            status_text: Arc::new(Mutex::new("Ожидание...".to_string())),
            kill_signal: Arc::new(AtomicBool::new(false)),
            is_muted: Arc::new(AtomicBool::new(false)),
            is_deafened: Arc::new(AtomicBool::new(false)),
            active_peers: Arc::new(Mutex::new(HashMap::new())),
            available_inputs: inputs,
            available_outputs: outputs,
            selected_input: default_in,
            selected_output: default_out,
            update_info,
            show_update_dialog: false,
            is_updating: false,
            show_overlay: false,
        }
    }
}

impl eframe::App for P2PApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.show_update_dialog {
            let info = self.update_info.lock().unwrap();
            if let Some(latest) = &info.latest_version {
                if latest != &info.current_version {
                    self.show_update_dialog = true;
                }
            }
        }

        crate::ui::render(ctx, self);
        if self.is_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(66));
        }
    }
}

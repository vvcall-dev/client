use crate::audio;
use crate::models::PeerState;
use crate::updater::{UpdateInfo, check_for_updates};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc::Receiver};

#[derive(PartialEq)]
pub enum AppScreen {
    Login,
    Register,
    Main,
}

#[derive(Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub message: String,
    pub token: Option<String>,
    pub config_json: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub server_url: String,
    pub username: String,
    pub auth_token: String,
    pub selected_input: String,
    pub selected_output: String,
    pub show_overlay: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_url: "p2p.tallfly.me".to_string(),
            username: "".to_string(),
            auth_token: "".to_string(),
            selected_input: "".to_string(),
            selected_output: "".to_string(),
            show_overlay: false,
        }
    }
}

pub struct P2PApp {
    pub config: AppConfig,

    pub current_screen: AppScreen,
    pub password_input: String,
    pub auth_message: String,
    pub is_authenticating: bool,
    pub auth_rx: Option<Receiver<AuthResponse>>,

    pub room_name: String,
    pub room_password: String,
    pub is_connected: bool,
    pub volume_level: Arc<Mutex<f32>>,
    pub status_text: Arc<Mutex<String>>,
    pub kill_signal: Arc<AtomicBool>,
    pub is_muted: Arc<AtomicBool>,
    pub is_deafened: Arc<AtomicBool>,
    pub active_peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
    pub available_inputs: Vec<String>,
    pub available_outputs: Vec<String>,
    pub update_info: Arc<Mutex<UpdateInfo>>,
    pub show_update_dialog: bool,
    pub is_updating: bool,
}

impl P2PApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = AppConfig::default();
        if let Some(storage) = cc.storage {
            if let Some(saved_config) = eframe::get_value(storage, eframe::APP_KEY) {
                config = saved_config;
            }
        }

        let initial_screen = if config.auth_token.is_empty() {
            AppScreen::Login
        } else {
            AppScreen::Main
        };

        let update_info = Arc::new(Mutex::new(UpdateInfo::default()));
        check_for_updates(update_info.clone());

        let (inputs, outputs) = audio::get_audio_devices();

        if config.selected_input.is_empty() || !inputs.contains(&config.selected_input) {
            config.selected_input = inputs
                .first()
                .cloned()
                .unwrap_or_else(|| "Нет устройств".into());
        }
        if config.selected_output.is_empty() || !outputs.contains(&config.selected_output) {
            config.selected_output = outputs
                .first()
                .cloned()
                .unwrap_or_else(|| "Нет устройств".into());
        }

        Self {
            config,
            current_screen: initial_screen,
            password_input: String::new(),
            auth_message: String::new(),
            is_authenticating: false,
            auth_rx: None,

            room_name: "default".to_owned(),
            room_password: "".to_owned(),
            is_connected: false,
            volume_level: Arc::new(Mutex::new(0.0)),
            status_text: Arc::new(Mutex::new("Ожидание...".to_string())),
            kill_signal: Arc::new(AtomicBool::new(false)),
            is_muted: Arc::new(AtomicBool::new(false)),
            is_deafened: Arc::new(AtomicBool::new(false)),
            active_peers: Arc::new(Mutex::new(HashMap::new())),
            available_inputs: inputs,
            available_outputs: outputs,
            update_info,
            show_update_dialog: false,
            is_updating: false,
        }
    }

    fn sync_config_to_cloud(&self) {
        if self.config.auth_token.is_empty() {
            return;
        }

        let server_url = self.config.server_url.clone();
        let token = self.config.auth_token.clone();

        let config_json = serde_json::to_string(&self.config).unwrap_or_else(|_| "{}".to_string());

        std::thread::spawn(move || {
            let scheme = if server_url.contains("localhost") || server_url.contains("127.0.0.1") {
                "http"
            } else {
                "https"
            };
            let url = format!("{}://{}/api/config", scheme, server_url);

            let body = serde_json::json!({
                "token": token,
                "config_json": config_json
            });

            let _ = ureq::post(&url).send_json(body);
        });
    }
}

impl eframe::App for P2PApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.config);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let old_config = self.config.clone();

        if let Some(rx) = &self.auth_rx {
            if let Ok(response) = rx.try_recv() {
                self.is_authenticating = false;
                if response.success {
                    if let Some(token) = response.token {
                        self.config.auth_token = token;
                        self.current_screen = AppScreen::Main;
                        self.password_input.clear();
                        self.auth_message.clear();

                        if let Some(config_str) = response.config_json {
                            if !config_str.is_empty() && config_str != "{}" {
                                if let Ok(cloud_config) =
                                    serde_json::from_str::<AppConfig>(&config_str)
                                {
                                    self.config.selected_input = cloud_config.selected_input;
                                    self.config.selected_output = cloud_config.selected_output;
                                    self.config.show_overlay = cloud_config.show_overlay;
                                    println!("Конфиг успешно загружен из облака!");
                                }
                            }
                        }
                    } else {
                        self.auth_message = "Регистрация успешна! Теперь войдите.".into();
                        self.current_screen = AppScreen::Login;
                    }
                } else {
                    self.auth_message = response.message;
                }
            }
        }

        if !self.show_update_dialog {
            let info = self.update_info.lock().unwrap();
            if let Some(latest) = &info.latest_version {
                if latest != &info.current_version {
                    self.show_update_dialog = true;
                }
            }
        }

        crate::ui::render(ctx, self);

        if self.config != old_config {
            self.sync_config_to_cloud();
        }

        if self.is_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(66));
        }
    }
}

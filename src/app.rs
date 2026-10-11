use crate::audio;
use crate::models::PeerState;
use crate::updater::{UpdateInfo, check_for_updates};
use eframe::egui;
use resvg::usvg;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc::Receiver};
use std::time::{Duration, Instant};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq)]
pub enum Language {
    Russian,
    English,
    Japanese,
}

impl Default for Language {
    fn default() -> Self {
        Language::Russian
    }
}

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
    pub enable_denoise: bool,
    pub language: Language,
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
            enable_denoise: true,
            language: Language::Russian,
        }
    }
}

pub struct P2PApp {
    pub config: AppConfig,
    pub show_settings: bool,

    pub last_config_change: Option<Instant>,

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
    pub enable_denoise: Arc<AtomicBool>,
    pub active_peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
    pub available_inputs: Vec<String>,
    pub available_outputs: Vec<String>,
    pub update_info: Arc<Mutex<UpdateInfo>>,
    pub show_update_dialog: bool,
    pub update_declined: bool,
    pub is_updating: bool,
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "mplus".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/mplus.ttf")),
    );

    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "mplus".to_owned());

    ctx.set_fonts(fonts);
}

pub fn load_icon_data() -> egui::IconData {
    let svg_data = include_str!("../assets/icon.svg");

    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    let rtree = usvg::Tree::from_str(svg_data, &usvg::Options::default(), &fontdb).unwrap();

    let size = 32;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();
    let transform = tiny_skia::Transform::from_scale(
        size as f32 / rtree.size().width(),
        size as f32 / rtree.size().height(),
    );

    resvg::render(&rtree, transform, &mut pixmap.as_mut());

    egui::IconData {
        rgba: pixmap.data().to_vec(),
        width: size,
        height: size,
    }
}

impl P2PApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

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

        let enable_denoise_flag = Arc::new(AtomicBool::new(config.enable_denoise));

        Self {
            config,
            show_settings: false,
            current_screen: initial_screen,
            last_config_change: None,
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
            enable_denoise: enable_denoise_flag,
            active_peers: Arc::new(Mutex::new(HashMap::new())),
            available_inputs: inputs,
            available_outputs: outputs,
            update_info,
            show_update_dialog: false,
            update_declined: false,
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

            let _ = ureq::post(&url)
                .timeout(Duration::from_secs(5))
                .send_json(body);
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

        if !self.show_update_dialog && !self.update_declined {
            if let Ok(info) = self.update_info.try_lock() {
                if let Some(latest_str) = &info.latest_version {
                    let current_res = Version::parse(&info.current_version.trim_start_matches('v'));
                    let latest_res = Version::parse(latest_str.trim_start_matches('v'));

                    if let (Ok(current), Ok(latest)) = (current_res, latest_res) {
                        if latest > current {
                            self.show_update_dialog = true;
                        }
                    }
                }
            }
        }

        crate::ui::render(ctx, self);

        if self.config != old_config {
            self.last_config_change = Some(Instant::now());
        }

        if let Some(last_change) = self.last_config_change {
            if last_change.elapsed() > Duration::from_millis(1500) {
                self.sync_config_to_cloud();
                self.last_config_change = None;
            }
        }

        if self.is_connected {
            ctx.request_repaint_after(Duration::from_millis(66));
        }

        if self.last_config_change.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

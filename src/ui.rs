use crate::app::{AppScreen, AuthResponse, Language, P2PApp};
use crate::audio;
use crate::engine;
use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub fn tr(key: &str, lang: Language) -> &str {
    match lang {
        Language::Russian => match key {
            "settings" => "Настройки",
            "connect" => "Подключиться",
            "disconnect" => "Отключиться",
            "mic" => "Микрофон",
            "output" => "Динамики",
            "overlay" => "Игровой оверлей",
            "room" => "Комната",
            "room_pass" => "Пароль комнаты",
            "nick" => "Никнейм",
            "server" => "Сервер",
            "lang" => "Язык",
            "update_list" => "Обновить устройства",
            "waiting" => "Ожидание собеседников...",
            "logout" => "Выйти из аккаунта",
            "login_title" => "Вход в аккаунт",
            "register_title" => "Регистрация",
            "login" => "Логин",
            "password" => "Пароль",
            "btn_login" => "Войти",
            "btn_register" => "Зарегистрироваться",
            "no_account" => "Нет аккаунта? Зарегистрироваться",
            "have_account" => "Уже есть аккаунт? Войти",
            _ => key,
        },
        Language::English => match key {
            "settings" => "Settings",
            "connect" => "Connect",
            "disconnect" => "Disconnect",
            "mic" => "Microphone",
            "output" => "Speakers",
            "overlay" => "Game Overlay",
            "room" => "Room",
            "room_pass" => "Room Password",
            "nick" => "Nickname",
            "server" => "Server URL",
            "lang" => "Language",
            "update_list" => "Refresh Devices",
            "waiting" => "Waiting for peers...",
            "logout" => "Log Out",
            "login_title" => "Account Login",
            "register_title" => "Registration",
            "login" => "Username",
            "password" => "Password",
            "btn_login" => "Log In",
            "btn_register" => "Register",
            "no_account" => "No account? Register here",
            "have_account" => "Already have an account? Log in",
            _ => key,
        },
        Language::Japanese => match key {
            "settings" => "設定",
            "connect" => "接続する",
            "disconnect" => "切断する",
            "mic" => "マイク",
            "output" => "スピーカー",
            "overlay" => "オーバーレイ",
            "room" => "ルーム名",
            "room_pass" => "ルームのパスワード",
            "nick" => "ニックネーム",
            "server" => "サーバー",
            "lang" => "言語",
            "update_list" => "デバイスを更新",
            "waiting" => "待機中...",
            "logout" => "ログアウト",
            "login_title" => "ログイン",
            "register_title" => "新規登録",
            "login" => "ユーザー名",
            "password" => "パスワード",
            "btn_login" => "ログイン",
            "btn_register" => "登録する",
            "no_account" => "アカウントがありませんか？登録",
            "have_account" => "すでにアカウントをお持ちですか？ログイン",
            _ => key,
        },
    }
}

pub fn render(ctx: &egui::Context, app: &mut P2PApp) {
    match app.current_screen {
        AppScreen::Login => draw_auth_screen(ctx, app, true),
        AppScreen::Register => draw_auth_screen(ctx, app, false),
        AppScreen::Main => draw_main_screen(ctx, app),
    }

    if app.show_update_dialog {
        let screen_rect = ctx.screen_rect();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::PanelResizeLine,
            egui::Id::new("dim"),
        ))
        .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(150));

        egui::Window::new("🚀 Доступно обновление!")
            .collapsible(false)
            .resizable(false)
            .title_bar(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let info = app.update_info.lock().unwrap();

                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(format!("Текущая версия: v{}", info.current_version));
                    ui.label(
                        egui::RichText::new(format!(
                            "Новая версия: {}",
                            info.latest_version.as_deref().unwrap_or("?")
                        ))
                        .strong()
                        .color(egui::Color32::GREEN),
                    );
                    ui.add_space(10.0);
                });

                if let Some(notes) = &info.release_notes {
                    ui.separator();
                    ui.label(egui::RichText::new("Что нового:").strong());
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.label(notes);
                        });
                }

                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let btn_width = (ui.available_width() - 10.0) / 2.0;

                    if ui
                        .add_sized([btn_width, 30.0], egui::Button::new("Скачать"))
                        .clicked()
                        && !app.is_updating
                    {
                        app.is_updating = true;

                        std::thread::spawn(move || match crate::updater::perform_update() {
                            Ok(_) => std::process::exit(42),
                            Err(e) => eprintln!("Ошибка обновления: {}", e),
                        });
                    }

                    if ui
                        .add_sized([btn_width, 30.0], egui::Button::new("Позже"))
                        .clicked()
                    {
                        app.show_update_dialog = false;
                    }
                });

                if app.is_updating {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new("Скачивание и установка...").italics());
                    });
                }
            });

        if app.config.show_overlay && app.is_connected {
            let overlay_builder = egui::ViewportBuilder::default()
                .with_title("VVcall Overlay")
                .with_transparent(true)
                .with_decorations(false)
                .with_always_on_top()
                .with_mouse_passthrough(true)
                .with_inner_size([200.0, 300.0])
                .with_position([20.0, 20.0]);

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("overlay_viewport"),
                overlay_builder,
                |overlay_ctx, _class| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                        .show(overlay_ctx, |ui| {
                            let peers = app.active_peers.lock().unwrap();
                            let now = Instant::now();

                            for (_addr, state) in peers.iter() {
                                let is_speaking =
                                    now.duration_since(state.last_spoken).as_millis() < 300;

                                if is_speaking {
                                    egui::Frame::none()
                                        .fill(egui::Color32::from_black_alpha(180))
                                        .rounding(6.0)
                                        .inner_margin(6.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("🔊 {}", state.name))
                                                    .color(egui::Color32::GREEN)
                                                    .strong()
                                                    .size(16.0),
                                            );
                                        });
                                    ui.add_space(4.0);
                                }
                            }
                        });
                },
            );
        }
    }
}

fn draw_main_screen(ctx: &egui::Context, app: &mut P2PApp) {
    let lang = app.config.language;

    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("VVcall").strong());
            let version = env!("CARGO_PKG_VERSION");
            let version_color = if version.contains("beta") || version.contains("alpha") {
                egui::Color32::YELLOW
            } else {
                egui::Color32::GRAY
            };
            ui.label(
                egui::RichText::new(format!("v{}", version))
                    .small()
                    .color(version_color),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(&app.config.username).strong());
            });
        });
        ui.add_space(4.0);
    });

    egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let muted = app.is_muted.load(Ordering::Relaxed);
            let mic_color = if muted {
                egui::Color32::RED
            } else {
                ui.visuals().text_color()
            };
            if ui
                .button(egui::RichText::new("🎤").size(18.0).color(mic_color))
                .on_hover_text(tr("mic", lang))
                .clicked()
            {
                app.is_muted.store(!muted, Ordering::Relaxed);
            }

            let deafened = app.is_deafened.load(Ordering::Relaxed);
            let out_color = if deafened {
                egui::Color32::RED
            } else {
                ui.visuals().text_color()
            };
            if ui
                .button(egui::RichText::new("🎧").size(18.0).color(out_color))
                .on_hover_text(tr("output", lang))
                .clicked()
            {
                app.is_deafened.store(!deafened, Ordering::Relaxed);
            }

            ui.separator();

            let status = app.status_text.lock().unwrap().clone();
            ui.label(egui::RichText::new(status).small());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if app.is_connected {
                    if ui
                        .button(
                            egui::RichText::new(tr("disconnect", lang))
                                .color(egui::Color32::LIGHT_RED),
                        )
                        .clicked()
                    {
                        app.kill_signal.store(true, Ordering::Relaxed);
                        app.is_connected = false;
                        *app.status_text.lock().unwrap() = "Отключено".to_string();
                        app.active_peers.lock().unwrap().clear();
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                } else {
                    if ui
                        .button(
                            egui::RichText::new(tr("connect", lang)).color(egui::Color32::GREEN),
                        )
                        .clicked()
                    {
                        app.is_connected = true;
                        app.kill_signal.store(false, Ordering::Relaxed);
                        *app.status_text.lock().unwrap() = "Инициализация...".to_string();
                        engine::start_voice_engine(crate::engine::EngineArgs {
                            server_url: app.config.server_url.clone(),
                            username: app.config.username.clone(),
                            room: app.room_name.clone(),
                            room_password: app.room_password.clone(),
                            selected_input: app.config.selected_input.clone(),
                            selected_output: app.config.selected_output.clone(),
                            volume_level: app.volume_level.clone(),
                            status: app.status_text.clone(),
                            kill_signal: app.kill_signal.clone(),
                            is_muted: app.is_muted.clone(),
                            is_deafened: app.is_deafened.clone(),
                            active_peers: app.active_peers.clone(),
                        });
                    }
                }

                if ui
                    .button(egui::RichText::new(tr("settings", lang)))
                    .clicked()
                {
                    app.show_settings = !app.show_settings;
                }
            });
        });
        ui.add_space(6.0);
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut peers = app.active_peers.lock().unwrap();
            if peers.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 2.0 - 20.0);
                    ui.label(egui::RichText::new(tr("waiting", lang)).color(egui::Color32::GRAY));
                });
            } else {
                let now = Instant::now();
                ui.add_space(5.0);
                for (_addr, state) in peers.iter_mut() {
                    egui::Frame::group(ui.style())
                        .rounding(egui::Rounding::same(8.0))
                        .fill(ui.visuals().faint_bg_color)
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            let is_speaking =
                                now.duration_since(state.last_spoken).as_millis() < 300;

                            ui.horizontal(|ui| {
                                let (icon, color) = if is_speaking {
                                    ("🔊", egui::Color32::GREEN)
                                } else {
                                    ("🔈", ui.visuals().text_color())
                                };
                                ui.label(egui::RichText::new(icon).color(color).size(20.0));

                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&state.name).strong().size(15.0));
                                    ui.label(
                                        egui::RichText::new(format!("{} ms", state.ping_ms))
                                            .small()
                                            .color(egui::Color32::GRAY),
                                    );
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(5.0);

                                        ui.add(
                                            egui::Slider::new(&mut state.volume, 0.0..=2.0)
                                                .show_value(true)
                                                .smart_aim(false),
                                        );
                                    },
                                );
                            });
                        });
                    ui.add_space(8.0);
                }
            }
        });
    });

    if app.show_settings {
        let mut is_open = app.show_settings;

        egui::Window::new(tr("settings", lang))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(320.0)
            .show(ctx, |ui| {
                draw_settings_content(ui, app, lang);
            });

        app.show_settings = is_open;
    }
}

fn draw_settings_content(ui: &mut egui::Ui, app: &mut P2PApp, lang: Language) {
    egui::ScrollArea::vertical()
        .max_height(350.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(tr("lang", lang)).strong());
            ui.horizontal(|ui| {
                let total_width = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                let btn_width = (total_width - (spacing * 2.0)) / 3.0;
                let btn_height = 24.0;

                if ui
                    .add_sized(
                        [btn_width, btn_height],
                        egui::SelectableLabel::new(app.config.language == Language::Russian, "RU"),
                    )
                    .clicked()
                {
                    app.config.language = Language::Russian;
                }
                if ui
                    .add_sized(
                        [btn_width, btn_height],
                        egui::SelectableLabel::new(app.config.language == Language::English, "EN"),
                    )
                    .clicked()
                {
                    app.config.language = Language::English;
                }
                if ui
                    .add_sized(
                        [btn_width, btn_height],
                        egui::SelectableLabel::new(
                            app.config.language == Language::Japanese,
                            "日本語",
                        ),
                    )
                    .clicked()
                {
                    app.config.language = Language::Japanese;
                }
            });
            ui.separator();

            ui.label(egui::RichText::new("Network").strong());
            ui.label(tr("server", lang));
            ui.add(
                egui::TextEdit::singleline(&mut app.config.server_url).desired_width(f32::INFINITY),
            );
            ui.label(tr("nick", lang));
            ui.add(
                egui::TextEdit::singleline(&mut app.config.username).desired_width(f32::INFINITY),
            );
            ui.label(tr("room", lang));
            ui.add(egui::TextEdit::singleline(&mut app.room_name).desired_width(f32::INFINITY));
            ui.label(tr("room_pass", lang));
            ui.add(
                egui::TextEdit::singleline(&mut app.room_password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            ui.separator();

            ui.label(egui::RichText::new("Audio").strong());
            ui.label(tr("mic", lang));
            egui::ComboBox::from_id_source("mic")
                .selected_text(&app.config.selected_input)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for dev in &app.available_inputs {
                        ui.selectable_value(&mut app.config.selected_input, dev.clone(), dev);
                    }
                });

            ui.label(tr("output", lang));
            egui::ComboBox::from_id_source("out")
                .selected_text(&app.config.selected_output)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for dev in &app.available_outputs {
                        ui.selectable_value(&mut app.config.selected_output, dev.clone(), dev);
                    }
                });

            ui.add_space(5.0);
            if ui
                .add_sized(
                    [ui.available_width(), 30.0],
                    egui::Button::new(tr("update_list", lang)),
                )
                .clicked()
            {
                let (ins, outs) = audio::get_audio_devices();
                app.available_inputs = ins;
                app.available_outputs = outs;
            }
            ui.separator();

            ui.checkbox(&mut app.config.show_overlay, tr("overlay", lang));
            ui.separator();

            if ui
                .add_sized(
                    [ui.available_width(), 30.0],
                    egui::Button::new(tr("logout", lang)),
                )
                .clicked()
            {
                app.config.auth_token.clear();
                app.current_screen = AppScreen::Login;
                app.show_settings = false;
            }
        });
}

fn draw_auth_screen(ctx: &egui::Context, app: &mut P2PApp, is_login: bool) {
    let lang = app.config.language;

    egui::TopBottomPanel::top("auth_top")
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut app.config.language, Language::Russian, "RU");
                    ui.selectable_value(&mut app.config.language, Language::English, "EN");
                    ui.selectable_value(&mut app.config.language, Language::Japanese, "日本語");
                });
            });
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            let title = if is_login {
                tr("login_title", lang)
            } else {
                tr("register_title", lang)
            };
            ui.label(egui::RichText::new(title).size(24.0).strong());
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 125.0);
                ui.vertical(|ui| {
                    ui.label(tr("server", lang));
                    ui.add_sized(
                        [250.0, 25.0],
                        egui::TextEdit::singleline(&mut app.config.server_url),
                    );

                    ui.add_space(10.0);
                    ui.label(tr("login", lang));
                    ui.add_sized(
                        [250.0, 25.0],
                        egui::TextEdit::singleline(&mut app.config.username),
                    );

                    ui.add_space(10.0);
                    ui.label(tr("password", lang));
                    ui.add_sized(
                        [250.0, 25.0],
                        egui::TextEdit::singleline(&mut app.password_input).password(true),
                    );
                });
            });

            ui.add_space(25.0);

            if app.is_authenticating {
                ui.spinner();
            } else {
                let btn_text = if is_login {
                    tr("btn_login", lang)
                } else {
                    tr("btn_register", lang)
                };
                if ui
                    .add_sized(
                        [250.0, 35.0],
                        egui::Button::new(egui::RichText::new(btn_text).size(16.0)),
                    )
                    .clicked()
                {
                    app.is_authenticating = true;
                    app.auth_message.clear();

                    let (tx, rx) = std::sync::mpsc::channel();
                    app.auth_rx = Some(rx);
                    spawn_auth_request(
                        is_login,
                        app.config.server_url.clone(),
                        app.config.username.clone(),
                        app.password_input.clone(),
                        tx,
                    );
                }
            }

            ui.add_space(10.0);

            if !app.auth_message.is_empty() {
                let color = if app.auth_message.contains("успешна")
                    || app.auth_message.contains("Success")
                {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                ui.label(egui::RichText::new(&app.auth_message).color(color));
            }

            ui.add_space(20.0);

            if is_login {
                if ui.link(tr("no_account", lang)).clicked() {
                    app.current_screen = AppScreen::Register;
                    app.auth_message.clear();
                }
            } else {
                if ui.link(tr("have_account", lang)).clicked() {
                    app.current_screen = AppScreen::Login;
                    app.auth_message.clear();
                }
            }
        });
    });
}

fn spawn_auth_request(
    is_login: bool,
    server: String,
    user: String,
    pass: String,
    tx: std::sync::mpsc::Sender<AuthResponse>,
) {
    std::thread::spawn(move || {
        let scheme = if server.contains("localhost") || server.contains("127.0.0.1") {
            "http"
        } else {
            "https"
        };
        let endpoint = if is_login { "login" } else { "register" };
        let url = format!("{}://{}/api/{}", scheme, server, endpoint);

        let body = serde_json::json!({
            "username": user,
            "password": pass
        });

        match ureq::post(&url).send_json(body) {
            Ok(response) => {
                if let Ok(auth_res) = response.into_json::<AuthResponse>() {
                    let _ = tx.send(auth_res);
                } else {
                    let _ = tx.send(AuthResponse {
                        success: false,
                        message: "Ошибка обработки ответа".into(),
                        token: None,
                        config_json: None,
                    });
                }
            }
            Err(ureq::Error::Status(400..=599, response)) => {
                if let Ok(auth_res) = response.into_json::<AuthResponse>() {
                    let _ = tx.send(auth_res);
                } else {
                    let _ = tx.send(AuthResponse {
                        success: false,
                        message: "Ошибка на сервере".into(),
                        token: None,
                        config_json: None,
                    });
                }
            }
            Err(_) => {
                let _ = tx.send(AuthResponse {
                    success: false,
                    message: "Сервер недоступен".into(),
                    token: None,
                    config_json: None,
                });
            }
        }
    });
}

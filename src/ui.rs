use crate::app::P2PApp;
use crate::audio;
use crate::engine;
use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub fn render(ctx: &egui::Context, app: &mut P2PApp) {
    egui::SidePanel::left("controls")
        .exact_width(280.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.add_space(11.0);
            ui.heading(egui::RichText::new("⚙ Настройки").strong());
            ui.add_space(5.0);
            ui.separator();
            ui.add_space(5.0);

            egui::TopBottomPanel::bottom("status_panel")
                .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(0.0, 8.0)))
                .show_inside(ui, |ui| {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        let status = app.status_text.lock().unwrap().clone();
                        ui.label(
                            egui::RichText::new(status)
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_space(5.0);
                        draw_connection(ui, app);
                        ui.add_space(15.0);
                        draw_devices(ui, app);
                        ui.add_space(15.0);
                        draw_controls(ui, app);
                        ui.add_space(10.0);
                    });
                });
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(5.0);
        ui.heading(egui::RichText::new("👥 Участники").strong());
        ui.add_space(5.0);
        ui.separator();
        ui.add_space(5.0);

        draw_peers(ui, app);
    });

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
    }

    if app.config.show_overlay && app.is_connected {
        let peers = app.active_peers.lock().unwrap();
        let now = Instant::now();

        let mut speaking_peers = Vec::new();
        for (_addr, state) in peers.iter() {
            if now.duration_since(state.last_spoken).as_millis() < 300 {
                speaking_peers.push(state.name.clone());
            }
        }

        if !speaking_peers.is_empty() {
            let overlay_builder = egui::ViewportBuilder::default()
                .with_title("P2P Overlay")
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
                    let mut visuals = egui::Visuals::dark();
                    visuals.panel_fill = egui::Color32::TRANSPARENT;
                    overlay_ctx.set_visuals(visuals);

                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                        .show(overlay_ctx, |ui| {
                            for name in speaking_peers {
                                egui::Frame::none()
                                    .fill(egui::Color32::from_black_alpha(180))
                                    .rounding(6.0)
                                    .inner_margin(6.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("🔊 {}", name))
                                                .color(egui::Color32::GREEN)
                                                .strong()
                                                .size(16.0),
                                        );
                                    });
                                ui.add_space(4.0);
                            }
                        });
                },
            );
        }
    }
}

fn draw_connection(ui: &mut egui::Ui, app: &mut P2PApp) {
    ui.label("🌐 Сервер (URL):");
    ui.add(egui::TextEdit::singleline(&mut app.config.server_url).desired_width(f32::INFINITY));

    ui.add_space(5.0);

    ui.label("👤 Никнейм:");
    ui.add(egui::TextEdit::singleline(&mut app.config.username).desired_width(f32::INFINITY));

    ui.add_space(5.0);

    ui.label("🚪 Название комнаты:");
    ui.add(egui::TextEdit::singleline(&mut app.room_name).desired_width(f32::INFINITY));
}

fn draw_devices(ui: &mut egui::Ui, app: &mut P2PApp) {
    ui.label("🎤 Микрофон:");
    egui::ComboBox::from_id_source("mic")
        .selected_text(&app.config.selected_input)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for dev in &app.available_inputs {
                ui.selectable_value(&mut app.config.selected_input, dev.clone(), dev);
            }
        });

    ui.add_space(5.0);

    ui.label("🎧 Динамики:");
    egui::ComboBox::from_id_source("out")
        .selected_text(&app.config.selected_output)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for dev in &app.available_outputs {
                ui.selectable_value(&mut app.config.selected_output, dev.clone(), dev);
            }
        });

    ui.add_space(5.0);

    if ui.button("🔄 Обновить список устройств").clicked() {
        let (ins, outs) = audio::get_audio_devices();
        app.available_inputs = ins;
        app.available_outputs = outs;
    }
}

fn draw_controls(ui: &mut egui::Ui, app: &mut P2PApp) {
    let mut muted = app.is_muted.load(Ordering::Relaxed);
    if ui.checkbox(&mut muted, "🔇 Выключить микрофон").changed() {
        app.is_muted.store(muted, Ordering::Relaxed);
    }

    let mut deafened = app.is_deafened.load(Ordering::Relaxed);
    if ui
        .checkbox(&mut deafened, "🔈 Выключить динамики")
        .changed()
    {
        app.is_deafened.store(deafened, Ordering::Relaxed);
    }

    ui.add_space(5.0);

    ui.checkbox(&mut app.config.show_overlay, "🔲 Игровой оверлей");

    ui.add_space(15.0);

    let btn_height = 35.0;

    if app.is_connected {
        if ui
            .add_sized(
                [ui.available_width(), btn_height],
                egui::Button::new(egui::RichText::new("Отключиться").size(16.0)),
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
            .add_sized(
                [ui.available_width(), btn_height],
                egui::Button::new(egui::RichText::new("Подключиться").size(16.0)),
            )
            .clicked()
        {
            app.is_connected = true;
            app.kill_signal.store(false, Ordering::Relaxed);
            *app.status_text.lock().unwrap() = "Инициализация...".to_string();
            engine::start_voice_engine(
                app.config.server_url.clone(),
                app.config.username.clone(),
                app.room_name.clone(),
                app.config.selected_input.clone(),
                app.config.selected_output.clone(),
                app.volume_level.clone(),
                app.status_text.clone(),
                app.kill_signal.clone(),
                app.is_muted.clone(),
                app.is_deafened.clone(),
                app.active_peers.clone(),
            );
        }
    }
}

fn draw_peers(ui: &mut egui::Ui, app: &mut P2PApp) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut peers = app.active_peers.lock().unwrap();
        if peers.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 2.0 - 20.0);
                ui.label(egui::RichText::new("Ждем собеседников...").color(egui::Color32::GRAY));
            });
        } else {
            let now = Instant::now();

            for (_addr, state) in peers.iter_mut() {
                egui::Frame::group(ui.style())
                    .rounding(egui::Rounding::same(8.0))
                    .fill(ui.visuals().faint_bg_color)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        let is_speaking = now.duration_since(state.last_spoken).as_millis() < 300;

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
                                    egui::RichText::new(format!("Пинг: {} мс", state.ping_ms))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            });
                        });

                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label("Громкость:");
                            ui.add(
                                egui::Slider::new(&mut state.volume, 0.0..=2.0).show_value(false),
                            );
                        });
                    });
                ui.add_space(8.0);
            }
        }
    });
}

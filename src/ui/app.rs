use super::*;
use eframe::egui;
use egui::{
    Align2, CentralPanel, Color32, ProgressBar, ScrollArea,
    SidePanel, Spinner, Window,
};
use std::path::Path;
use std::time::Duration;

/// Действия контекстного меню хранилища
enum RemoteMenuAction {
    Open,
    Check,
    About,
    Delete,
}

impl eframe::App for RcloneUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_rclone_init();
        self.poll_background_operation();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label("🚀 Rclone GUI");
                ui.separator();
                ui.add_enabled_ui(!self.history_back.is_empty(), |ui| {
                    if ui.button("⬅").clicked() {
                        log::debug!("Кнопка 'Назад' нажата");
                        self.go_back();
                    }
                });
                ui.add_enabled_ui(!self.history_forward.is_empty(), |ui| {
                    if ui.button("➡").clicked() {
                        log::debug!("Кнопка 'Вперед' нажата");
                        self.go_forward();
                    }
                });
                if self.active_task_count > 0 {
                    ui.add(Spinner::new().size(16.0));
                }
            });
        });

        SidePanel::left("left")
            .default_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Хранилища");
                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    for remote in self.remote_list.clone() {
                        let selected = self.current_path.starts_with(&remote.name);
                        let response =
                            ui.selectable_label(selected, format!("📡 {}", remote.name));
                        if response.clicked() {
                            log::debug!("Открытие remote: {}", remote.name);
                            self.navigate_to(format!("{}:", remote.name));
                        }
                        if response.secondary_clicked() {
                            log::debug!("Контекстное меню remote: {}", remote.name);
                            let pos = ctx
                                .input(|i| i.pointer.interact_pos())
                                .unwrap_or_default();
                            self.context_menu = Some((remote.name.clone(), pos));
                            self.context_menu_requested = true;
                        }
                    }
                });
                ui.separator();
                ui.add_enabled_ui(self.rclone.is_some(), |ui| {
                    if ui.button("➕ Добавить облако").clicked() {
                        log::debug!("Открытие диалога добавления облака");
                        self.show_add_remote_dialog = true;
                    }
                });
            });

        SidePanel::right("right")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Задачи");
                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    for op in &self.active_operations {
                        ui.group(|ui| {
                            ui.label(&op.description);
                            ui.add(
                                ProgressBar::new(op.progress)
                                    .text(format!("{:.0}%", op.progress * 100.0)),
                            );
                            ui.small(&op.status);
                        });
                    }
                    if self.active_operations.is_empty() {
                        ui.weak("Нет активных задач");
                    }
                });
            });

        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("📍 {}", self.current_path));
                ui.add_enabled_ui(self.active_task_count == 0, |ui| {
                    if ui.button("🔄").clicked() {
                        log::debug!("Обновление текущего пути: {}", self.current_path);
                        self.pending_load_path = Some(self.current_path.clone());
                    }
                });
            });
            ui.separator();

            ScrollArea::vertical().id_source("files").show(ui, |ui| {
                if !self.current_path.is_empty() {
                    if self.current_path.contains('/') && ui.button("📁 .. (Вверх)").clicked()
                    {
                        log::debug!("Переход вверх по директории");
                        if let Some((p, _)) = self.current_path.rsplit_once('/') {
                            self.navigate_to(p.into());
                        } else if let Some((p, _)) = self.current_path.rsplit_once(':') {
                            self.navigate_to(format!("{}:", p));
                        }
                    }

                    for file in self.current_files.clone() {
                        let full = if self.current_path.ends_with(':') {
                            format!("{}{}", self.current_path, file.name)
                        } else {
                            format!("{}/{}", self.current_path, file.name)
                        };
                        let sel = self.selected_paths.contains(&full);

                        ui.horizontal(|ui| {
                            if ui.selectable_label(sel, file.icon()).clicked() {
                                if sel {
                                    log::debug!("Снят выбор с {}", full);
                                    self.selected_paths.retain(|p| p != &full);
                                } else {
                                    log::debug!("Выбран {}", full);
                                    self.selected_paths.push(full.clone());
                                }
                            }
                            if file.is_dir {
                                if ui.link(&file.name).clicked() {
                                    log::debug!("Открыта директория: {}", full);
                                    self.navigate_to(full);
                                }
                            } else {
                                ui.label(&file.name);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.small(file.format_size());
                                },
                            );
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Выберите хранилище слева");
                    });
                }
            });

            if !self.selected_paths.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {}", self.selected_paths.len()));
                    ui.add_enabled_ui(self.active_task_count == 0, |ui| {
                        if ui.button("📋 Копировать").clicked() {
                            log::info!("Начало операции копирования выбранных элементов");
                            self.transfer_source_info = self.get_selected_info();
                            self.is_move_mode = false;
                            self.show_transfer_dialog = true;
                        }
                        if ui.button("✂ Переместить").clicked() {
                            log::info!("Начало операции перемещения выбранных элементов");
                            self.transfer_source_info = self.get_selected_info();
                            self.is_move_mode = true;
                            self.show_transfer_dialog = true;
                        }
                        if ui.button("🗑 Удалить").clicked() {
                            log::info!("Начало операции удаления выбранных элементов");
                            self.delete_selected();
                            self.selected_paths.clear();
                        }
                    });
                });
            }
        });

        // Диалог добавления нового облака
        if self.show_add_remote_dialog {
            Window::new("Новое облако")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .fixed_size([480.0, 420.0])
                .show(ctx, |ui| {
                    match self.add_remote_step.clone() {
                        AddRemoteStep::Form => {
                            ui.label("Имя:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_remote_name)
                                    .hint_text("например: mydrive")
                                    .desired_width(240.0),
                            );
                            ui.add_space(6.0);
                            ui.label("Тип:");
                            egui::ComboBox::from_id_source("remote_type")
                                .selected_text(self.new_remote_type.as_str())
                                .width(240.0)
                                .show_ui(ui, |ui| {
                                    for t in REMOTE_TYPES {
                                        ui.selectable_value(
                                            &mut self.new_remote_type,
                                            t.to_string(),
                                            *t,
                                        );
                                    }
                                });
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(
                                    "Для облаков с авторизацией (Google Drive, Dropbox и др.) при создании откроется браузер для входа в аккаунт.",
                                )
                                .wrap(true),
                            );
                            ui.add_space(10.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                let can_create = !self.new_remote_name.trim().is_empty()
                                    && !self.new_remote_type.trim().is_empty()
                                    && self.active_task_count == 0;
                                if ui
                                    .add_enabled(
                                        can_create,
                                        egui::Button::new("✅ Создать"),
                                    )
                                    .clicked()
                                {
                                    log::debug!("Кнопка 'Создать' нажата");
                                    self.start_add_remote();
                                }
                                if ui.button("Отмена").clicked() {
                                    self.show_add_remote_dialog = false;
                                }
                            });
                        }
                        AddRemoteStep::Question(question) => {
                            ui.heading(format!("❓ {}", question.name));
                            ui.add(egui::Label::new(&question.help).wrap(true));
                            if !question.default.is_empty() {
                                ui.weak(format!("По умолчанию: {}", question.default));
                            }
                            if !question.examples.is_empty() {
                                ui.add_space(6.0);
                                ui.label("Варианты:");
                                for (value, help) in &question.examples {
                                    let selected = self.add_remote_answer == *value;
                                    let label = if help.is_empty() {
                                        value.clone()
                                    } else {
                                        format!("{} — {}", value, help)
                                    };
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.add_remote_answer = value.clone();
                                    }
                                }
                            }
                            ui.add_space(6.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.add_remote_answer)
                                    .password(question.is_password)
                                    .hint_text(if question.is_password { "секрет" } else { "ответ" })
                                    .desired_width(320.0),
                            );
                            ui.add_space(10.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        self.active_task_count == 0,
                                        egui::Button::new("✅ Ответить"),
                                    )
                                    .clicked()
                                {
                                    log::debug!("Ответ на вопрос rclone отправлен");
                                    self.answer_add_remote();
                                }
                                if ui.button("Отмена").clicked() {
                                    self.show_add_remote_dialog = false;
                                }
                            });
                        }
                        AddRemoteStep::Busy => {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new());
                                ui.label(&self.add_remote_status);
                            });
                            ui.add(
                                egui::Label::new(
                                    "Если страница авторизации не открылась — скопируйте ссылку из статуса ниже в браузер.",
                                )
                                .wrap(true),
                            );
                            ui.add_space(10.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Отмена").clicked() {
                                    self.show_add_remote_dialog = false;
                                }
                            });
                        }
                    }
                });
        }

        // Модальное окно трансфера
        if self.show_transfer_dialog {
            Window::new("Трансфер")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.active_transfer_tab,
                            TransferTab::Remote,
                            "☁ Облако",
                        );
                        ui.selectable_value(
                            &mut self.active_transfer_tab,
                            TransferTab::Local,
                            "💻 ПК",
                        );
                    });
                    ui.separator();

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.transfer_dest)
                                    .hint_text("Путь назначения..."),
                            );
                            if self.active_transfer_tab == TransferTab::Local
                                && ui.button("📂 Обзор").clicked()
                            {
                                log::debug!("Открыт локальный браузер файлов");
                                self.refresh_local_list();
                                self.show_local_browser = true;
                            }
                        });

                        if !self.transfer_dest.is_empty() && !self.is_path_valid() {
                            ui.colored_label(
                                Color32::KHAKI,
                                if self.active_transfer_tab == TransferTab::Remote {
                                    "⚠ Путь облака должен содержать ':'"
                                } else {
                                    "⚠ Укажите локальный путь без ':'"
                                },
                            );
                        }
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let can_start = self.is_path_valid() && self.active_task_count == 0;
                        if ui
                            .add_enabled(can_start, egui::Button::new("🚀 Начать"))
                            .clicked()
                        {
                            log::info!("Запуск трансфера в {}", self.transfer_dest);
                            self.perform_transfer(self.is_move_mode);
                            self.show_transfer_dialog = false;
                        }
                        if ui.button("Отмена").clicked() {
                            log::debug!("Отмена трансфера");
                            self.show_transfer_dialog = false;
                        }
                    });
                });
        }

        // Внутренний браузер папок
        if self.show_local_browser {
            Window::new("Выбор локальной папки")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([400.0, 300.0])
                .show(ctx, |ui| {
                    ui.label(format!("📍 {}", self.local_browser_path));
                    ui.horizontal(|ui| {
                        if ui.button("⬅ Наверх").clicked() {
                            if let Some(parent) = Path::new(&self.local_browser_path).parent() {
                                self.local_browser_path = parent.to_string_lossy().into();
                                self.refresh_local_list();
                            }
                        }
                        if ui.button("🔄").clicked() {
                            self.refresh_local_list();
                        }
                    });
                    ui.separator();
                    
                    // Проверяем, пуста ли папка
                    let is_empty = self.local_browser_files.is_empty();
                    
                    // Создаем рамку для контента
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            if is_empty {
                                // Если папка пуста, показываем сообщение по центру с иконкой
                                let available_height = ui.available_height();
                                ui.centered_and_justified(|ui| {
                                    ui.add_space(available_height / 3.0);
                                    ui.vertical(|ui| {
                                        ui.label("📂");
                                        ui.add_space(8.0);
                                        ui.colored_label(
                                            egui::Color32::GRAY,
                                            "Эта папка пуста"
                                        );
                                    });
                                });
                            } else {
                                // Если есть файлы, показываем список
                                ScrollArea::vertical()
                                    .max_height(200.0)
                                    .auto_shrink([false; 2])
                                    .show(ui, |ui| {
                                        let files = self.local_browser_files.clone();
                                        for file in files {
                                            if file.is_dir {
                                                ui.horizontal(|ui| {
                                                    let response = ui.button("📁");
                                                    ui.add_space(4.0);
                                                    let link_response = ui.link(&file.name);
                                                    
                                                    if response.clicked() || link_response.clicked() {
                                                        self.local_browser_path = file.path;
                                                        self.refresh_local_list();
                                                    }
                                                });
                                            } else {
                                                ui.horizontal(|ui| {
                                                    ui.label("📄");
                                                    ui.add_space(4.0);
                                                    ui.label(&file.name);
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(egui::Align::Center),
                                                        |ui| {
                                                            ui.small(file.format_size());
                                                        },
                                                    );
                                                });
                                            }
                                        }
                                    });
                            }
                        });
                    
                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Выбрать").clicked() {
                            log::debug!("Выбрана локальная папка: {}", self.local_browser_path);
                            self.transfer_dest = self.local_browser_path.clone();
                            self.show_local_browser = false;
                        }
                        if ui.button("Отмена").clicked() {
                            self.show_local_browser = false;
                        }
                    });
                });
        }

        if let Some(msg) = &self.error_message {
            let mut close = false;
            Window::new("Внимание")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(msg);
                    if ui.button("ОК").clicked() {
                        close = true;
                    }
                });
            if close {
                log::debug!("Ошибка закрыта: {}", msg);
                self.error_message = None;
            }
        }

        // Подтверждение удаления хранилища
        if let Some(remote_name) = self.remote_to_delete.clone() {
            let mut close = false;
            let mut confirm = false;
            Window::new("Удаление хранилища")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .fixed_size([430.0, 170.0])
                .show(ctx, |ui| {
                    ui.label(format!("Удалить хранилище «{}»?", remote_name));
                    ui.add(
                        egui::Label::new(
                            "Файлы в облаке не удаляются — удаляется только запись из конфигурации rclone.",
                        )
                        .wrap(true),
                    );
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("🗑️ Удалить").clicked() {
                            confirm = true;
                        }
                        if ui.button("Отмена").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.remote_to_delete = None;
            }
            if confirm {
                self.remote_to_delete = None;
                self.delete_remote(remote_name);
            }
        }

        // Окно с результатом проверки или информацией о хранилище
        if let Some(info) = &self.remote_info {
            let mut close = false;
            Window::new(&info.title)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .fixed_size([430.0, 260.0])
                .show(ctx, |ui| {
                    let color = if info.success {
                        Color32::LIGHT_GREEN
                    } else {
                        Color32::KHAKI
                    };
                    ui.add(
                        egui::Label::new(egui::RichText::new(&info.content).color(color))
                            .wrap(true),
                    );
                    ui.separator();
                    if ui.button("ОК").clicked() {
                        close = true;
                    }
                });
            if close {
                self.remote_info = None;
            }
        }

        // Контекстное меню хранилища (правая кнопка мыши)
        if let Some((menu_name, menu_pos)) = self.context_menu.clone() {
            let remote_type = self
                .remote_list
                .iter()
                .find(|r| r.name == menu_name)
                .map(|r| r.r#type.clone())
                .unwrap_or_default();
            let mut action: Option<RemoteMenuAction> = None;
            Window::new("remote_menu")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .fixed_pos(menu_pos)
                .show(ctx, |ui| {
                    ui.weak(format!("Тип: {}", remote_type));
                    ui.separator();
                    if ui.button("📂 Открыть").clicked() {
                        action = Some(RemoteMenuAction::Open);
                    }
                    if ui.button("🔍 Проверить доступность").clicked() {
                        action = Some(RemoteMenuAction::Check);
                    }
                    if ui.button("📊 Использование").clicked() {
                        action = Some(RemoteMenuAction::About);
                    }
                    ui.separator();
                    if ui.button("🗑️ Удалить").clicked() {
                        action = Some(RemoteMenuAction::Delete);
                    }
                });
            if let Some(action) = action {
                self.context_menu = None;
                match action {
                    RemoteMenuAction::Open => {
                        log::debug!("Открытие remote из меню: {}", menu_name);
                        self.navigate_to(format!("{}:", menu_name));
                    }
                    RemoteMenuAction::Check => {
                        log::debug!("Проверка remote из меню: {}", menu_name);
                        self.check_remote(menu_name.clone());
                    }
                    RemoteMenuAction::About => {
                        log::debug!("Использование remote из меню: {}", menu_name);
                        self.about_remote(menu_name.clone());
                    }
                    RemoteMenuAction::Delete => {
                        log::debug!("Запрос удаления remote из меню: {}", menu_name);
                        self.remote_to_delete = Some(menu_name.clone());
                    }
                }
            } else {
                let just_opened = std::mem::take(&mut self.context_menu_requested);
                if !just_opened && ctx.input(|i| i.pointer.any_click()) {
                    log::debug!("Контекстное меню закрыто");
                    self.context_menu = None;
                }
            }
        }

        if let Some(path) = self.pending_load_path.take() {
            self.current_path = path.clone();
            self.selected_paths.clear();
            self.load_files(&path);
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

//! Графический интерфейс для операций rclone.

use eframe::egui;
use egui::{CentralPanel, ScrollArea, Window, Align2, Color32, Spinner};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
use crate::rclone_install::RcloneApp;
use crate::operations::{self, FileInfo, Remote, CopyOptions, MoveOptions, DeleteOptions, FindOptions};

/// Состояние приложения.
#[derive(Clone)]
pub enum AppState {
    Initializing,
    Ready,
    Error(String),
    Copying,
    Syncing,
    Moving,
    Deleting,
    Loading,
}

/// Информация о ходе передачи.
#[derive(Clone)]
pub struct TransferProgress {
    pub current: u64,
    pub total: u64,
    pub speed: f64,
    pub file_name: String,
}

/// Результат длительной операции.
enum OperationResult {
    None,
    Success(String),
    Failure(String),
}

/// Главная структура UI.
pub struct RcloneUI {
    rclone: Option<Arc<RcloneApp>>,
    state: AppState,
    error_message: Option<String>,
    
    current_path: String,
    remote_list: Vec<Remote>,
    current_files: Vec<FileInfo>,
    selected_paths: Vec<String>,
    
    transfer_source: String,
    transfer_dest: String,
    transfer_progress: Option<TransferProgress>,
    
    search_pattern: String,
    search_results: Vec<FileInfo>,
    
    show_transfer_dialog: bool,
    show_new_remote_dialog: bool,
    new_remote_name: String,
    new_remote_type: String,
    available_remote_types: Vec<String>,
    new_remote_config: std::collections::HashMap<String, String>,
    
    settings: AppSettings,
    
    background_working: bool,
    operation_result: Arc<Mutex<OperationResult>>,
    operation_id: u32,
    show_delete_remote_dialog: bool,
    remote_to_delete: Option<String>,
    show_browser_warning: bool,
    pending_remote_creation: Option<(String, String)>,
    
    // Отложенные действия
    pending_load_path: Option<String>,
    pending_search_pattern: Option<String>,
}

#[derive(Clone)]
pub struct AppSettings {
    pub show_hidden: bool,
    pub confirm_before_transfer: bool,
    pub max_concurrent_transfers: u32,
    pub bandwidth_limit: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_hidden: false,
            confirm_before_transfer: true,
            max_concurrent_transfers: 4,
            bandwidth_limit: None,
        }
    }
}

impl RcloneUI {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            rclone: None,
            state: AppState::Initializing,
            error_message: None,
            current_path: String::new(),
            remote_list: Vec::new(),
            current_files: Vec::new(),
            selected_paths: Vec::new(),
            transfer_source: String::new(),
            transfer_dest: String::new(),
            transfer_progress: None,
            search_pattern: String::new(),
            search_results: Vec::new(),
            show_transfer_dialog: false,
            show_new_remote_dialog: false,
            new_remote_name: String::new(),
            new_remote_type: String::new(),
            available_remote_types: Vec::new(),
            new_remote_config: std::collections::HashMap::new(),
            settings: AppSettings::default(),
            background_working: false,
            operation_result: Arc::new(Mutex::new(OperationResult::None)),
            operation_id: 0,
            show_delete_remote_dialog: false,
            remote_to_delete: None,
            show_browser_warning: false,
            pending_remote_creation: None,
            pending_load_path: None,
            pending_search_pattern: None,
        }
    }
    
    fn init_rclone(&mut self) {
        let rt = Runtime::new().unwrap();
        let rclone_result = rt.block_on(async { RcloneApp::new().await });
        match rclone_result {
            Ok(rclone) => {
                self.rclone = Some(Arc::new(rclone));
                self.load_remote_types();
                self.load_remotes();
            }
            Err(e) => {
                let error_msg = format!("Не удалось инициализировать rclone: {}", e);
                self.error_message = Some(error_msg.clone());
                self.state = AppState::Error(error_msg);
            }
        }
    }
    
    fn load_remote_types(&mut self) {
        if let Some(rclone) = &self.rclone {
            match operations::info::backends(rclone) {
                Ok(types) => self.available_remote_types = types,
                Err(_) => {
                    self.available_remote_types = vec![
                        "drive".to_string(), "s3".to_string(), "dropbox".to_string(),
                        "local".to_string(), "onedrive".to_string(), "webdav".to_string(),
                    ];
                }
            }
        }
    }
    
    fn load_remotes(&mut self) {
        if let Some(rclone) = &self.rclone {
            match operations::remotes::list(rclone) {
                Ok(remotes) => {
                    self.remote_list = remotes;
                    self.state = AppState::Ready;
                }
                Err(e) => {
                    self.error_message = Some(e.clone());
                    self.state = AppState::Error(e);
                }
            }
        }
    }
    
    fn load_files(&mut self, path: &str) {
        if let Some(rclone) = &self.rclone {
            match operations::files::list(rclone, path) {
                Ok(files) => {
                    self.current_files = files;
                    self.current_path = path.to_string();
                    self.search_results.clear();
                    self.search_pattern.clear();
                }
                Err(e) => self.error_message = Some(e),
            }
        }
    }
    
    fn start_background_operation<F>(&mut self, f: F) -> u32
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        self.background_working = true;
        self.operation_id = self.operation_id.wrapping_add(1);
        let op_id = self.operation_id;
        let result_holder = self.operation_result.clone();
        std::thread::spawn(move || {
            let result = f();
            let mut holder = result_holder.lock().unwrap();
            *holder = match result {
                Ok(msg) => OperationResult::Success(msg),
                Err(e) => OperationResult::Failure(e),
            };
        });
        op_id
    }
    
    fn poll_background_operation(&mut self) {
        if !self.background_working {
            return;
        }
        let result = {
            let mut holder = self.operation_result.lock().unwrap();
            match std::mem::replace(&mut *holder, OperationResult::None) {
                OperationResult::Success(msg) => Some(Ok(msg)),
                OperationResult::Failure(e) => Some(Err(e)),
                _ => None,
            }
        };
        if let Some(res) = result {
            self.background_working = false;
            match res {
                Ok(msg) => {
                    println!("Операция успешна: {}", msg);
                    self.load_remotes();
                    let current_path = self.current_path.clone();
                    if !current_path.is_empty() {
                        self.load_files(&current_path);
                    }
                    self.error_message = None;
                }
                Err(e) => {
                    self.error_message = Some(e);
                }
            }
        }
    }
    
    fn delete_remote(&mut self, name: &str) {
        if let Some(rclone) = self.rclone.clone() {
            let name = name.to_string();
            self.start_background_operation(move || {
                operations::remotes::delete(&rclone, &name)
                    .map(|_| format!("Хранилище '{}' удалено", name))
                    .map_err(|e| format!("Ошибка удаления: {}", e))
            });
        }
    }
    
    fn perform_move(&mut self, source: &str, dest: &str) {
        if let Some(rclone) = self.rclone.clone() {
            self.state = AppState::Moving;
            let options = MoveOptions {
                verbose: true,
                dry_run: false,
                delete_empty_src_dirs: true,
            };
            let source = source.to_string();
            let dest = dest.to_string();
            self.start_background_operation(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    operations::sync::move_files(&rclone, &source, &dest, &options)
                })
                .map(|stats| format!("Перемещено {} файлов, {} байт", stats.files, stats.transferred))
                .map_err(|e| format!("Ошибка перемещения: {}", e))
            });
        }
    }
    
    fn perform_copy(&mut self, source: &str, dest: &str) {
        if let Some(rclone) = self.rclone.clone() {
            self.state = AppState::Copying;
            let options = CopyOptions {
                verbose: true,
                dry_run: false,
                bandwidth_limit: self.settings.bandwidth_limit.clone(),
                no_traverse: false,
            };
            let source = source.to_string();
            let dest = dest.to_string();
            self.start_background_operation(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    operations::sync::copy(&rclone, &source, &dest, &options)
                })
                .map(|stats| format!("Скопировано {} файлов", stats.files))
                .map_err(|e| format!("Ошибка копирования: {}", e))
            });
        }
    }
    
    fn create_remote_with_warning(&mut self, name: &str, r#type: &str) {
        self.pending_remote_creation = Some((name.to_string(), r#type.to_string()));
        self.show_browser_warning = true;
    }
    
    fn execute_remote_creation(&mut self, name: &str, r#type: &str) {
        if let Some(rclone) = self.rclone.clone() {
            let name = name.to_string();
            let r#type = r#type.to_string();
            let config = std::collections::HashMap::new();
            self.start_background_operation(move || {
                operations::remotes::create(&rclone, &name, &r#type, &config)
                    .map(|_| format!("Хранилище '{}' создано", name))
                    .map_err(|e| format!("Ошибка создания: {}", e))
            });
        }
    }
    
    fn search_files(&mut self, pattern: &str) {
        if let Some(rclone) = &self.rclone {
            let options = FindOptions {
                recursive: true,
                max_results: 100,
            };
            match operations::search::by_name(rclone, &self.current_path, pattern, &options) {
                Ok(results) => self.search_results = results,
                Err(e) => self.error_message = Some(e),
            }
        }
    }
    
    fn validate_remote_name(name: &str) -> bool {
        if name.is_empty() || name.starts_with(' ') || name.starts_with('-') || name.ends_with(' ') {
            return false;
        }
        name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '+' || ch == '@' || ch == ' ')
    }
    
    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        if bytes >= GB { format!("{:.2} ГБ", bytes as f64 / GB as f64) }
        else if bytes >= MB { format!("{:.2} МБ", bytes as f64 / MB as f64) }
        else if bytes >= KB { format!("{:.2} КБ", bytes as f64 / KB as f64) }
        else { format!("{} Б", bytes) }
    }
    
    fn format_speed(speed: f64) -> String {
        if speed < 1024.0 { format!("{:.0} Б/с", speed) }
        else if speed < 1024.0 * 1024.0 { format!("{:.1} КБ/с", speed / 1024.0) }
        else if speed < 1024.0 * 1024.0 * 1024.0 { format!("{:.1} МБ/с", speed / (1024.0 * 1024.0)) }
        else { format!("{:.2} ГБ/с", speed / (1024.0 * 1024.0 * 1024.0)) }
    }
}

impl eframe::App for RcloneUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background_operation();
        
        if matches!(self.state, AppState::Initializing) && self.rclone.is_none() {
            self.init_rclone();
        }
        
        // Верхняя панель меню
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Файл", |ui| {
                    if ui.button("Выход").clicked() { std::process::exit(0); }
                });
                ui.menu_button("Хранилища", |ui| {
                    if ui.button("Добавить новое хранилище").clicked() {
                        self.show_new_remote_dialog = true;
                    }
                    if ui.button("Обновить список").clicked() {
                        self.load_remotes();
                    }
                });
                ui.menu_button("Настройки", |ui| {
                    ui.checkbox(&mut self.settings.show_hidden, "Показывать скрытые файлы");
                    ui.checkbox(&mut self.settings.confirm_before_transfer, "Подтверждать перед передачей");
                    ui.add(egui::Slider::new(&mut self.settings.max_concurrent_transfers, 1..=10).text("Макс. одновременных передач"));
                    ui.horizontal(|ui| {
                        ui.label("Ограничение скорости:");
                        let mut limit_str = self.settings.bandwidth_limit.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut limit_str).changed() {
                            self.settings.bandwidth_limit = if limit_str.is_empty() { None } else { Some(limit_str) };
                        }
                    });
                });
                ui.separator();
                // Индикатор состояния с анимацией
                if self.background_working {
                    ui.horizontal(|ui| {
                        ui.add(Spinner::new().size(16.0));
                        ui.colored_label(egui::Color32::YELLOW, " Работа в фоне...");
                    });
                } else {
                    match &self.state {
                        AppState::Ready => { ui.colored_label(Color32::GREEN, "● Готов"); }
                        AppState::Initializing => { ui.colored_label(Color32::YELLOW, "● Инициализация..."); }
                        AppState::Error(_) => { ui.colored_label(Color32::RED, "● Ошибка"); }
                        _ => { ui.colored_label(Color32::BLUE, "● Занят"); }
                    }
                };
                if let Some(rclone) = &self.rclone {
                    ui.separator();
                    ui.label(format!("rclone: {}", rclone.get_rclone_path().display()));
                }
            });
        });
        
        // Левая панель со списком хранилищ
        egui::SidePanel::left("remotes_panel")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Хранилища");
                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    let remotes = self.remote_list.clone();
                    if remotes.is_empty() && matches!(self.state, AppState::Ready) {
                        ui.colored_label(Color32::GRAY, "Хранилища не найдены");
                        if ui.button("➕ Добавить хранилище").clicked() {
                            self.show_new_remote_dialog = true;
                        }
                    } else {
                        for remote in &remotes {
                            ui.horizontal(|ui| {
                                if ui.button(format!("📡 {}", remote.name)).clicked() {
                                    self.pending_load_path = Some(format!("{}:", remote.name));
                                }
                                if ui.button("🗑️").on_hover_text("Удалить хранилище").clicked() {
                                    self.remote_to_delete = Some(remote.name.clone());
                                    self.show_delete_remote_dialog = true;
                                }
                            });
                            ui.add_space(2.0);
                        }
                    }
                });
            });
        
        // Центральная панель
        CentralPanel::default().show(ctx, |ui| {
            let current_path = self.current_path.clone();
            let search_pattern = self.search_pattern.clone();
            let mut new_search = search_pattern.clone();
            ui.horizontal(|ui| {
                ui.label("📍 Путь:");
                ui.label(if current_path.is_empty() { "Выберите хранилище" } else { &current_path });
                ui.separator();
                ui.label("🔍 Поиск:");
                if ui.text_edit_singleline(&mut new_search).changed() {
                    self.search_pattern = new_search.clone();
                    if !self.search_pattern.is_empty() && !self.current_path.is_empty() {
                        self.pending_search_pattern = Some(self.search_pattern.clone());
                    } else {
                        self.search_results.clear();
                    }
                }
                if ui.button("🔄 Обновить").clicked() && !self.current_path.is_empty() {
                    self.pending_load_path = Some(self.current_path.clone());
                }
            });
            ui.separator();
            
            // Анимированная загрузка в центральной панели
            if self.background_working {
                ui.centered_and_justified(|ui| {
                    ui.add(Spinner::new().size(64.0));
                    ui.add_space(10.0);
                    ui.colored_label(
                        egui::Color32::LIGHT_BLUE,
                        "⟳ Выполняется операция... Пожалуйста, подождите."
                    );
                });
                ctx.request_repaint_after(Duration::from_millis(16));
                return;
            }
            
            if let AppState::Error(msg) = &self.state {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(Color32::RED, format!("Ошибка: {}", msg));
                });
                return;
            }
            
            if !self.search_results.is_empty() {
                let search_results = self.search_results.clone();
                let current_path = self.current_path.clone();
                ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Результаты поиска");
                    for file in &search_results {
                        ui.horizontal(|ui| {
                            ui.label(file.icon());
                            if ui.link(&file.name).clicked() && file.is_dir {
                                let new_path = if current_path.ends_with(':') {
                                    format!("{}{}", current_path, file.name)
                                } else {
                                    format!("{}/{}", current_path, file.name)
                                };
                                self.pending_load_path = Some(new_path);
                            }
                            ui.label(Self::format_size(file.size));
                        });
                    }
                });
            } else if !self.current_path.is_empty() {
                let current_files = self.current_files.clone();
                let current_path = self.current_path.clone();
                ScrollArea::vertical().show(ui, |ui| {
                    if current_path.contains('/') && ui.button("📁 .. (Родительская папка)").clicked() {
                        let parent = current_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                        self.pending_load_path = Some(parent.to_string());
                    }
                    ui.heading("Файлы и папки");
                    for file in &current_files {
                        let selected = self.selected_paths.contains(&file.path);
                        ui.horizontal(|ui| {
                            if ui.selectable_label(selected, file.icon()).clicked() {
                                if selected { self.selected_paths.retain(|p| p != &file.path); }
                                else { self.selected_paths.push(file.path.clone()); }
                            }
                            if file.is_dir {
                                if ui.link(&file.name).clicked() {
                                    let new_path = if current_path.ends_with(':') {
                                        format!("{}{}", current_path, file.name)
                                    } else {
                                        format!("{}/{}", current_path, file.name)
                                    };
                                    self.pending_load_path = Some(new_path);
                                }
                            } else {
                                ui.label(&file.name);
                            }
                            ui.label(Self::format_size(file.size));
                        });
                    }
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("Добро пожаловать в Rclone UI Manager");
                    ui.label("Выберите хранилище из левой панели или добавьте новое");
                    if ui.button("➕ Добавить новое хранилище").clicked() {
                        self.show_new_remote_dialog = true;
                    }
                });
            }
            
            ui.separator();
            if !self.selected_paths.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {}", self.selected_paths.len()));
                    if ui.button("📋 Копировать").clicked() {
                        self.transfer_source = self.selected_paths[0].clone();
                        self.show_transfer_dialog = true;
                    }
                    if ui.button("✂️ Переместить").clicked() {
                        self.transfer_source = self.selected_paths[0].clone();
                        self.show_transfer_dialog = true;
                    }
                    if ui.button("🗑️ Удалить").clicked() {
                        if let Some(rclone) = self.rclone.clone() {
                            let path = self.selected_paths[0].clone();
                            let options = DeleteOptions { recursive: true, verbose: true, dry_run: false };
                            self.start_background_operation(move || {
                                operations::sync::delete(&rclone, &path, &options)
                                    .map(|_| format!("Удалено: {}", path))
                                    .map_err(|e| format!("Ошибка удаления: {}", e))
                            });
                            self.selected_paths.clear();
                        }
                    }
                });
            }
        });
        
        // Обработка отложенных действий
        if let Some(path) = self.pending_load_path.take() {
            self.load_files(&path);
        }
        if let Some(pattern) = self.pending_search_pattern.take() {
            self.search_files(&pattern);
        }
        
        // Диалог передачи файлов
        if self.show_transfer_dialog {
            let source = self.transfer_source.clone();
            let dest = self.transfer_dest.clone();
            Window::new("Передача файлов").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(format!("Источник: {}", source));
                ui.label("Назначение:");
                let mut dest_edit = dest.clone();
                if ui.text_edit_singleline(&mut dest_edit).changed() {
                    self.transfer_dest = dest_edit;
                }
                ui.separator();
                let current_dest = self.transfer_dest.clone();
                ui.horizontal(|ui| {
                    if ui.button("✅ Копировать").clicked() {
                        self.perform_copy(&source, &current_dest);
                        self.show_transfer_dialog = false;
                        self.selected_paths.clear();
                    }
                    if ui.button("✂️ Переместить").clicked() {
                        self.perform_move(&source, &current_dest);
                        self.show_transfer_dialog = false;
                        self.selected_paths.clear();
                    }
                    if ui.button("❌ Отмена").clicked() {
                        self.show_transfer_dialog = false;
                    }
                });
            });
        }
        
        // Диалог добавления хранилища
        if self.show_new_remote_dialog {
            let mut name = self.new_remote_name.clone();
            let mut typ = self.new_remote_type.clone();
            let available_types = self.available_remote_types.clone();
            Window::new("Добавление хранилища").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Имя хранилища:");
                if ui.text_edit_singleline(&mut name).changed() {
                    self.new_remote_name = name;
                }
                ui.label("Тип хранилища:");
                egui::ComboBox::from_label("").selected_text(&typ).show_ui(ui, |ui| {
                    for t in &available_types {
                        if ui.selectable_value(&mut typ, t.clone(), t).clicked() {
                            self.new_remote_type = typ.clone();
                        }
                    }
                });
                ui.label("⚠️ Некоторые типы (Google Drive, Dropbox и др.) откроют браузер для авторизации.");
                let name_clone = self.new_remote_name.clone();
                let type_clone = self.new_remote_type.clone();
                ui.horizontal(|ui| {
                    if ui.button("✅ Создать").clicked() {
                        if !Self::validate_remote_name(&name_clone) {
                            self.error_message = Some("Недопустимое имя хранилища".to_string());
                        } else if type_clone.is_empty() {
                            self.error_message = Some("Выберите тип".to_string());
                        } else {
                            self.create_remote_with_warning(&name_clone, &type_clone);
                            self.show_new_remote_dialog = false;
                        }
                    }
                    if ui.button("❌ Отмена").clicked() {
                        self.show_new_remote_dialog = false;
                        self.new_remote_name.clear();
                        self.new_remote_type.clear();
                    }
                });
            });
        }
        
        // Диалог предупреждения о браузере
        if self.show_browser_warning {
            Window::new("Внимание").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.colored_label(Color32::YELLOW, "🌐 Будет открыт браузер для авторизации");
                ui.label("После завершения авторизации в браузере вернитесь в приложение.");
                ui.label("Операция выполняется в фоне, интерфейс не зависнет.");
                ui.separator();
                if ui.button("Продолжить").clicked() {
                    if let Some((name, typ)) = self.pending_remote_creation.take() {
                        self.execute_remote_creation(&name, &typ);
                    }
                    self.show_browser_warning = false;
                }
                if ui.button("Отмена").clicked() {
                    self.pending_remote_creation = None;
                    self.show_browser_warning = false;
                }
            });
        }
        
        // Диалог подтверждения удаления хранилища
        if self.show_delete_remote_dialog {
            let remote_name = self.remote_to_delete.clone();
            Window::new("Удаление хранилища").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                if let Some(ref name) = remote_name {
                    ui.label(format!("Удалить хранилище '{}'?", name));
                    ui.label("Это действие не удаляет данные, только конфигурацию.");
                    let name_clone = name.clone();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Удалить").clicked() {
                            self.delete_remote(&name_clone);
                            self.show_delete_remote_dialog = false;
                            self.remote_to_delete = None;
                        }
                        if ui.button("❌ Отмена").clicked() {
                            self.show_delete_remote_dialog = false;
                            self.remote_to_delete = None;
                        }
                    });
                } else {
                    self.show_delete_remote_dialog = false;
                }
            });
        }
        
        // Диалог ошибок
        if let Some(error) = &self.error_message {
            let error_clone = error.clone();
            Window::new("Ошибка").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.colored_label(Color32::RED, "⚠️ Произошла ошибка:");
                ui.label(&error_clone);
                if ui.button("OK").clicked() {
                    self.error_message = None;
                    if let AppState::Error(_) = &self.state {
                        self.state = AppState::Ready;
                    }
                }
            });
        }
        
        ctx.request_repaint();
    }
}

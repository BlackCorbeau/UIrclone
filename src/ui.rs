//! Графический интерфейс для операций rclone.

use eframe::egui;
use egui::{CentralPanel, ScrollArea, Window, Align2, Color32, Spinner, ProgressBar, SidePanel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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

/// Операция для отображения в правой панели.
#[derive(Clone)]
pub struct Operation {
    pub id: u32,
    pub description: String,
    pub progress: f32,
    pub status: String,
    pub start_time: Instant,
}

/// Результат длительной операции.
enum OperationResult {
    None,
    Success(u32, String),        // (operation_id, message)
    Failure(u32, String),        // (operation_id, error)
    FileList(Vec<FileInfo>),
    RemoteList(Vec<Remote>),
    SearchResults(Vec<FileInfo>),
    ProgressUpdate(u32, f32, String),
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

    transfer_source_list: Vec<String>,
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

    active_task_count: u32,
    operation_result: Arc<Mutex<OperationResult>>,
    show_delete_remote_dialog: bool,
    remote_to_delete: Option<String>,
    show_browser_warning: bool,
    pending_remote_creation: Option<(String, String)>,

    pending_load_path: Option<String>,
    pending_search_pattern: Option<String>,
    rclone_init_receiver: Option<std::sync::mpsc::Receiver<Result<Arc<RcloneApp>, String>>>,
    active_operations: Vec<Operation>,
    next_op_id: u32,
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
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let result = rt.block_on(async { RcloneApp::new().await });
            let result_string = match result {
                Ok(app) => Ok(Arc::new(app)),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result_string);
        });

        Self {
            rclone: None,
            state: AppState::Initializing,
            error_message: None,
            current_path: String::new(),
            remote_list: Vec::new(),
            current_files: Vec::new(),
            selected_paths: Vec::new(),
            transfer_source_list: Vec::new(),
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
            active_task_count: 0,
            operation_result: Arc::new(Mutex::new(OperationResult::None)),
            show_delete_remote_dialog: false,
            remote_to_delete: None,
            show_browser_warning: false,
            pending_remote_creation: None,
            pending_load_path: None,
            pending_search_pattern: None,
            rclone_init_receiver: Some(rx),
            active_operations: Vec::new(),
            next_op_id: 1,
        }
    }

    fn poll_rclone_init(&mut self) {
        if let Some(receiver) = &self.rclone_init_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok(rclone) => {
                        self.rclone = Some(rclone);
                        self.load_remote_types();
                        self.load_remotes();
                        self.state = AppState::Ready;
                    }
                    Err(e) => {
                        let error_msg = format!("Не удалось инициализировать rclone: {}", e);
                        self.error_message = Some(error_msg.clone());
                        self.state = AppState::Error(error_msg);
                    }
                }
                self.rclone_init_receiver = None;
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
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let remotes = operations::remotes::list(&rclone);
                let mut holder = result_holder.lock().unwrap();
                *holder = match remotes {
                    Ok(list) => OperationResult::RemoteList(list),
                    Err(e) => OperationResult::Failure(0, e),
                };
            });
        }
    }

    fn load_files(&mut self, path: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            self.active_task_count += 1;
            let path = path.to_string();
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let files = operations::files::list(&rclone, &path);
                let mut holder = result_holder.lock().unwrap();
                *holder = match files {
                    Ok(list) => OperationResult::FileList(list),
                    Err(e) => OperationResult::Failure(0, e),
                };
            });
        }
    }

    fn search_files(&mut self, pattern: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            self.active_task_count += 1;
            let pattern = pattern.to_string();
            let current_path = self.current_path.clone();
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let options = FindOptions {
                    recursive: true,
                    max_results: 100,
                };
                let results = operations::search::by_name(&rclone, &current_path, &pattern, &options);
                let mut holder = result_holder.lock().unwrap();
                *holder = match results {
                    Ok(list) => OperationResult::SearchResults(list),
                    Err(e) => OperationResult::Failure(0, e),
                };
            });
        }
    }

    fn poll_background_operation(&mut self) {
        if self.active_task_count == 0 {
            // Если нет активных задач, но состояние всё ещё "Занят" — сбрасываем
            if matches!(self.state, AppState::Copying | AppState::Moving | AppState::Deleting | AppState::Loading) {
                self.state = AppState::Ready;
            }
            return;
        }
        let result = {
            let mut holder = self.operation_result.lock().unwrap();
            std::mem::replace(&mut *holder, OperationResult::None)
        };
    
        match result {
            OperationResult::Success(op_id, msg) => {
                println!("✅ Операция {} успешна: {}", op_id, msg);
                self.active_task_count = self.active_task_count.saturating_sub(1);
                self.active_operations.retain(|op| op.id != op_id);
                self.error_message = Some(msg);
                self.state = AppState::Ready;
            }
            OperationResult::Failure(op_id, e) => {
                eprintln!("❌ Операция {} ошибка: {}", op_id, e);
                self.active_task_count = self.active_task_count.saturating_sub(1);
                self.active_operations.retain(|op| op.id != op_id);
                self.error_message = Some(e);
                self.state = AppState::Ready;
            }
            OperationResult::FileList(files) => {
                self.current_files = files;
                self.active_task_count = self.active_task_count.saturating_sub(1);
                self.state = AppState::Ready;
            }
            OperationResult::RemoteList(remotes) => {
                self.remote_list = remotes;
                self.active_task_count = self.active_task_count.saturating_sub(1);
                self.state = AppState::Ready;
            }
            OperationResult::SearchResults(results) => {
                self.search_results = results;
                self.active_task_count = self.active_task_count.saturating_sub(1);
                self.state = AppState::Ready;
            }
            OperationResult::ProgressUpdate(op_id, progress, status) => {
                if let Some(op) = self.active_operations.iter_mut().find(|op| op.id == op_id) {
                    op.progress = progress;
                    op.status = status;
                }
            }
            OperationResult::None => {}
        }
    }

    fn delete_remote(&mut self, name: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            let name = name.to_string();
            let op_id = self.add_operation(format!("Удаление хранилища '{}'", name));
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let result = operations::remotes::delete(&rclone, &name)
                    .map(|_| format!("Хранилище '{}' удалено", name))
                    .map_err(|e| format!("Ошибка удаления: {}", e));
                let mut holder = result_holder.lock().unwrap();
                *holder = match result {
                    Ok(msg) => OperationResult::Success(op_id, msg),
                    Err(e) => OperationResult::Failure(op_id, e),
                };
            });
        }
    }

    fn perform_move_multiple(&mut self, sources: Vec<String>, dest: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            self.state = AppState::Moving;
            let options = MoveOptions {
                verbose: true,
                dry_run: false,
                delete_empty_src_dirs: true,
            };
            let dest = dest.to_string();
            let op_id = self.add_operation(format!("Перемещение {} элементов", sources.len()));
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let _rt = Runtime::new().unwrap();
                let mut errors = Vec::new();
                let mut moved_count = 0;
                let mut total_bytes = 0;
                let total = sources.len() as f32;

                for (idx, source) in sources.iter().enumerate() {
                    match operations::sync::move_files(&rclone, source, &dest, &options) {
                        Ok(stats) => {
                            moved_count += 1;
                            total_bytes += stats.transferred;
                        }
                        Err(e) => {
                            errors.push(format!("{}: {}", source, e));
                        }
                    }
                    let progress = (idx + 1) as f32 / total;
                    let _ = result_holder.lock().map(|mut holder| {
                        *holder = OperationResult::ProgressUpdate(op_id, progress, format!("Перемещено {} из {}", idx + 1, total));
                    });
                    std::thread::sleep(std::time::Duration::from_millis(10)); // небольшая пауза для UI
                }

                let result = if errors.is_empty() {
                    Ok(format!("Перемещено {} элементов, {} байт", moved_count, total_bytes))
                } else {
                    Err(format!("Перемещено {}, ошибки:\n{}", moved_count, errors.join("\n")))
                };
                let mut holder = result_holder.lock().unwrap();
                *holder = match result {
                    Ok(msg) => OperationResult::Success(op_id, msg),
                    Err(e) => OperationResult::Failure(op_id, e),
                };
            });
        }
    }

    fn perform_copy_multiple(&mut self, sources: Vec<String>, dest: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            self.state = AppState::Copying;
            let options = CopyOptions {
                verbose: true,
                dry_run: false,
                bandwidth_limit: self.settings.bandwidth_limit.clone(),
                no_traverse: false,
            };
            let dest = dest.to_string();
            let op_id = self.add_operation(format!("Копирование {} элементов", sources.len()));
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let _rt = Runtime::new().unwrap();
                let mut errors = Vec::new();
                let mut copied_count = 0;
                let total = sources.len() as f32;

                for (idx, source) in sources.iter().enumerate() {
                    match operations::sync::copy(&rclone, source, &dest, &options) {
                        Ok(_stats) => {
                            copied_count += 1;
                        }
                        Err(e) => {
                            errors.push(format!("{}: {}", source, e));
                        }
                    }
                    let progress = (idx + 1) as f32 / total;
                    let _ = result_holder.lock().map(|mut holder| {
                        *holder = OperationResult::ProgressUpdate(op_id, progress, format!("Скопировано {} из {}", idx + 1, total));
                    });
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                let result = if errors.is_empty() {
                    Ok(format!("Скопировано {} элементов", copied_count))
                } else {
                    Err(format!("Скопировано {}, ошибки:\n{}", copied_count, errors.join("\n")))
                };
                let mut holder = result_holder.lock().unwrap();
                *holder = match result {
                    Ok(msg) => OperationResult::Success(op_id, msg),
                    Err(e) => OperationResult::Failure(op_id, e),
                };
            });
        }
    }

    fn delete_selected_paths(&mut self, paths: Vec<String>) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            let mut is_dir_map = std::collections::HashMap::new();
            for file in &self.current_files {
                let full_path = if self.current_path.ends_with(':') {
                    format!("{}{}", self.current_path, file.name)
                } else {
                    format!("{}/{}", self.current_path, file.name)
                };
                is_dir_map.insert(full_path, file.is_dir);
            }
            let op_id = self.add_operation(format!("Удаление {} элементов", paths.len()));
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let mut errors = Vec::new();
                let mut deleted_count = 0;
                let total = paths.len() as f32;

                for (idx, path) in paths.iter().enumerate() {
                    let is_dir = is_dir_map.get(path).cloned().unwrap_or(false);
                    let options = DeleteOptions {
                        recursive: is_dir,
                        verbose: false,
                        dry_run: false,
                    };
                    match operations::sync::delete(&rclone, path, &options) {
                        Ok(stats) => {
                            deleted_count += 1;
                            println!("Удалено: {} ({} элементов)", path, stats.files);
                        }
                        Err(e) => {
                            errors.push(format!("{}: {}", path, e));
                        }
                    }
                    let progress = (idx + 1) as f32 / total;
                    let _ = result_holder.lock().map(|mut holder| {
                        *holder = OperationResult::ProgressUpdate(op_id, progress, format!("Удалено {} из {}", idx + 1, total));
                    });
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                let result = if errors.is_empty() {
                    Ok(format!("Успешно удалено {} элементов", deleted_count))
                } else {
                    Err(format!("Удалено {} элементов, но возникли ошибки:\n{}", deleted_count, errors.join("\n")))
                };
                let mut holder = result_holder.lock().unwrap();
                *holder = match result {
                    Ok(msg) => OperationResult::Success(op_id, msg),
                    Err(e) => OperationResult::Failure(op_id, e),
                };
            });
        }
    }

    fn add_operation(&mut self, description: String) -> u32 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        self.active_operations.push(Operation {
            id,
            description,
            progress: 0.0,
            status: "Начато".to_string(),
            start_time: Instant::now(),
        });
        id
    }

    fn create_remote_with_warning(&mut self, name: &str, r#type: &str) {
        self.pending_remote_creation = Some((name.to_string(), r#type.to_string()));
        self.show_browser_warning = true;
    }

    fn execute_remote_creation(&mut self, name: &str, r#type: &str) {
        if self.active_task_count > 0 || self.rclone.is_none() {
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            let name = name.to_string();
            let r#type = r#type.to_string();
            let config = std::collections::HashMap::new();
            let op_id = self.add_operation(format!("Создание хранилища '{}'", name));
            self.active_task_count += 1;
            let result_holder = self.operation_result.clone();
            std::thread::spawn(move || {
                let _ = result_holder.lock().map(|mut holder| {
                    *holder = OperationResult::ProgressUpdate(op_id, 0.5, "Открытие браузера для авторизации...".to_string());
                });
                let result = operations::remotes::create(&rclone, &name, &r#type, &config)
                    .map(|_| format!("Хранилище '{}' создано", name))
                    .map_err(|e| format!("Ошибка создания: {}", e));
                let mut holder = result_holder.lock().unwrap();
                *holder = match result {
                    Ok(msg) => OperationResult::Success(op_id, msg),
                    Err(e) => OperationResult::Failure(op_id, e),
                };
            });
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
        if bytes >= GB {
            format!("{:.2} ГБ", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} МБ", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} КБ", bytes as f64 / KB as f64)
        } else {
            format!("{} Б", bytes)
        }
    }

    #[allow(dead_code)]
    fn format_speed(speed: f64) -> String {
        if speed < 1024.0 {
            format!("{:.0} Б/с", speed)
        } else if speed < 1024.0 * 1024.0 {
            format!("{:.1} КБ/с", speed / 1024.0)
        } else if speed < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} МБ/с", speed / (1024.0 * 1024.0))
        } else {
            format!("{:.2} ГБ/с", speed / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

impl eframe::App for RcloneUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_rclone_init();
        self.poll_background_operation();

        // Верхняя панель меню
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Файл", |ui| {
                    if ui.button("Выход").clicked() {
                        std::process::exit(0);
                    }
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
                if self.active_task_count > 0 {
                    ui.horizontal(|ui| {
                        ui.add(Spinner::new().size(16.0));
                        ui.colored_label(egui::Color32::YELLOW, format!(" Работа в фоне... ({} задач)", self.active_task_count));
                    });
                } else {
                    match &self.state {
                        AppState::Ready => {
                            ui.colored_label(Color32::GREEN, "● Готов");
                        }
                        AppState::Initializing => {
                            ui.colored_label(Color32::YELLOW, "● Инициализация...");
                        }
                        AppState::Error(_) => {
                            ui.colored_label(Color32::RED, "● Ошибка");
                        }
                        _ => {
                            ui.colored_label(Color32::BLUE, "● Занят");
                        }
                    }
                }
                if let Some(rclone) = &self.rclone {
                    ui.separator();
                    ui.label(format!("rclone: {}", rclone.get_rclone_path().display()));
                }
            });
        });

        // Левая панель со списком хранилищ
        SidePanel::left("remotes_panel")
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
                                let btn = ui.button(format!("📡 {}", remote.name));
                                if btn.clicked() && self.active_task_count == 0 {
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

        // Правая панель с операциями
        SidePanel::right("operations_panel")
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Операции");
                ui.separator();
                if self.active_operations.is_empty() {
                    ui.colored_label(Color32::GRAY, "Нет активных операций");
                } else {
                    ScrollArea::vertical().show(ui, |ui| {
                        for op in &self.active_operations {
                            ui.group(|ui| {
                                ui.label(&op.description);
                                ui.add(ProgressBar::new(op.progress).text(format!("{:.0}%", op.progress * 100.0)));
                                ui.colored_label(Color32::LIGHT_BLUE, &op.status);
                                ui.label(format!("Время: {:.0} сек", op.start_time.elapsed().as_secs()));
                            });
                            ui.add_space(5.0);
                        }
                    });
                }
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
                    if !self.search_pattern.is_empty() && !self.current_path.is_empty() && self.active_task_count == 0 {
                        self.pending_search_pattern = Some(self.search_pattern.clone());
                    } else {
                        self.search_results.clear();
                    }
                }
                if ui.button("🔄 Обновить").clicked() && !self.current_path.is_empty() && self.active_task_count == 0 {
                    self.pending_load_path = Some(self.current_path.clone());
                }
            });
            ui.separator();

            if self.active_task_count > 0 {
                ui.centered_and_justified(|ui| {
                    ui.add(Spinner::new().size(64.0));
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::LIGHT_BLUE, "⟳ Выполняется операция... Пожалуйста, подождите.");
                });
                ctx.request_repaint_after(Duration::from_millis(50));
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
                            if ui.link(&file.name).clicked() && file.is_dir && self.active_task_count == 0 {
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
                    if current_path.contains('/') && ui.button("📁 .. (Родительская папка)").clicked() && self.active_task_count == 0 {
                        let parent = current_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                        self.pending_load_path = Some(parent.to_string());
                    }
                    ui.heading("Файлы и папки");
                    for file in &current_files {
                        let full_path = if current_path.ends_with(':') {
                            format!("{}{}", current_path, file.name)
                        } else {
                            format!("{}/{}", current_path, file.name)
                        };
                        let selected = self.selected_paths.contains(&full_path);
                        ui.horizontal(|ui| {
                            if ui.selectable_label(selected, file.icon()).clicked() && self.active_task_count == 0 {
                                if selected {
                                    self.selected_paths.retain(|p| p != &full_path);
                                } else {
                                    self.selected_paths.push(full_path.clone());
                                }
                            }
                            if file.is_dir {
                                if ui.link(&file.name).clicked() && self.active_task_count == 0 {
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
            if !self.selected_paths.is_empty() && self.active_task_count == 0 {
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {}", self.selected_paths.len()));
                    if ui.button("📋 Копировать").clicked() {
                        self.transfer_source_list = self.selected_paths.clone();
                        self.show_transfer_dialog = true;
                    }
                    if ui.button("✂️ Переместить").clicked() {
                        self.transfer_source_list = self.selected_paths.clone();
                        self.show_transfer_dialog = true;
                    }
                    if ui.button("🗑️ Удалить").clicked() {
                        let paths_to_delete = self.selected_paths.clone();
                        self.delete_selected_paths(paths_to_delete);
                        self.selected_paths.clear();
                    }
                });
            }
        });

        // Обработка отложенных действий
        if let Some(path) = self.pending_load_path.take() {
            self.current_path = path.clone();
            self.search_results.clear();
            self.search_pattern.clear();
            self.selected_paths.clear();
            self.load_files(&path);
        }
        if let Some(pattern) = self.pending_search_pattern.take() {
            self.search_files(&pattern);
        }

        // Диалог передачи файлов (копирование/перемещение)
        if self.show_transfer_dialog {
            let source_count = self.transfer_source_list.len();
            let dest = self.transfer_dest.clone();
            Window::new("Передача файлов")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("Выбрано элементов для передачи: {}", source_count));
                    if source_count <= 3 {
                        for src in &self.transfer_source_list {
                            ui.label(format!("📄 {}", src));
                        }
                    } else {
                        ui.label(format!("и ещё {} элементов", source_count - 3));
                    }
                    ui.separator();
                    ui.label("Назначение (директория):");
                    let mut dest_edit = dest.clone();
                    if ui.text_edit_singleline(&mut dest_edit).changed() {
                        self.transfer_dest = dest_edit;
                    }
                    ui.separator();
                    let current_dest = self.transfer_dest.clone();
                    let sources = self.transfer_source_list.clone();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Копировать").clicked() {
                            self.perform_copy_multiple(sources.clone(), &current_dest);
                            self.show_transfer_dialog = false;
                            self.selected_paths.clear();
                            self.transfer_source_list.clear();
                            self.transfer_dest.clear();
                        }
                        if ui.button("✂️ Переместить").clicked() {
                            self.perform_move_multiple(sources.clone(), &current_dest);
                            self.show_transfer_dialog = false;
                            self.selected_paths.clear();
                            self.transfer_source_list.clear();
                            self.transfer_dest.clear();
                        }
                        if ui.button("❌ Отмена").clicked() {
                            self.show_transfer_dialog = false;
                            self.transfer_source_list.clear();
                            self.transfer_dest.clear();
                        }
                    });
                });
        }

        // Диалог добавления хранилища
        if self.show_new_remote_dialog {
            let mut name = self.new_remote_name.clone();
            let mut typ = self.new_remote_type.clone();
            let available_types = self.available_remote_types.clone();
            Window::new("Добавление хранилища")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Имя хранилища:");
                    if ui.text_edit_singleline(&mut name).changed() {
                        self.new_remote_name = name;
                    }
                    ui.label("Тип хранилища:");
                    egui::ComboBox::from_label("")
                        .selected_text(&typ)
                        .show_ui(ui, |ui| {
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
            Window::new("Внимание")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
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
            Window::new("Удаление хранилища")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
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

        // Диалог ошибок / уведомлений
        if let Some(error) = &self.error_message {
            let error_clone = error.clone();
            Window::new("Сообщение")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if error_clone.starts_with("✅") || error_clone.contains("успеш") || error_clone.contains("скопировано") {
                        ui.colored_label(Color32::GREEN, "✅ Успешно:");
                    } else {
                        ui.colored_label(Color32::RED, "⚠️ Произошла ошибка:");
                    }
                    ui.label(&error_clone);
                    if ui.button("OK").clicked() {
                        self.error_message = None;
                        if let AppState::Error(_) = &self.state {
                            self.state = AppState::Ready;
                        }
                    }
                });
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

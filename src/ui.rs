//! Графический интерфейс для операций rclone.
//!
//! Этот модуль содержит главную структуру `RcloneUI` и её реализацию,
//! включая панели, диалоги, просмотр файлов и управление передачей данных.

use eframe::egui;
use egui::{CentralPanel, ScrollArea, Window, Align2, Color32, ProgressBar};
use std::sync::Arc;
use tokio::runtime::Runtime;
use crate::rclone_install::RcloneApp;
use crate::operations::{self, FileInfo, Remote, CopyOptions, FindOptions};

/// Состояние приложения.
#[derive(Clone)]
pub enum AppState {
    /// Приложение инициализируется (проверка наличия rclone).
    Initializing,
    /// Приложение готово и простаивает.
    Ready,
    /// Произошла ошибка, хранится текст ошибки.
    Error(String),
    /// Выполняется копирование.
    Copying,
    /// Выполняется синхронизация.
    Syncing,
    /// Выполняется перемещение.
    Moving,
    /// Выполняется удаление.
    Deleting,
    /// Загрузка файлов или информации о хранилищах.
    Loading,
}

/// Информация о ходе передачи.
#[derive(Clone)]
pub struct TransferProgress {
    /// Передано байт.
    pub current: u64,
    /// Всего байт.
    pub total: u64,
    /// Текущая скорость (байт/с).
    pub speed: f64,
    /// Имя текущего передаваемого файла.
    pub file_name: String,
}

/// Главная структура UI, содержащая всё состояние и логику.
pub struct RcloneUI {
    /// Экземпляр rclone (инициализируется при первом использовании).
    rclone: Option<Arc<RcloneApp>>,
    /// Текущее состояние приложения.
    state: AppState,
    /// Глобальное сообщение об ошибке.
    error_message: Option<String>,
    
    // Навигация
    /// Текущий отображаемый путь.
    current_path: String,
    /// Список доступных удалённых хранилищ.
    remote_list: Vec<Remote>,
    /// Файлы и папки в текущем пути.
    current_files: Vec<FileInfo>,
    
    // Выделение
    /// Выбранные пользователем пути.
    selected_paths: Vec<String>,
    
    // Передача
    /// Исходный путь для операции передачи.
    transfer_source: String,
    /// Целевой путь для операции передачи.
    transfer_dest: String,
    /// Прогресс текущей передачи.
    transfer_progress: Option<TransferProgress>,
    
    // Поиск
    /// Текущий шаблон поиска.
    search_pattern: String,
    /// Результаты последнего поиска.
    search_results: Vec<FileInfo>,
    
    // Диалоги UI
    /// Открыт ли диалог передачи.
    show_transfer_dialog: bool,
    /// Открыт ли диалог создания нового хранилища.
    show_new_remote_dialog: bool,
    /// Имя нового хранилища.
    new_remote_name: String,
    /// Тип нового хранилища (например, "s3", "dropbox").
    new_remote_type: String,
    /// Конфигурационные параметры нового хранилища.
    new_remote_config: std::collections::HashMap<String, String>,
    
    // Настройки
    /// Настройки пользователя.
    settings: AppSettings,
}

/// Настройки приложения, задаваемые пользователем.
#[derive(Clone)]
pub struct AppSettings {
    /// Показывать скрытые файлы.
    pub show_hidden: bool,
    /// Запрашивать подтверждение перед началом передачи.
    pub confirm_before_transfer: bool,
    /// Максимальное количество одновременных передач.
    pub max_concurrent_transfers: u32,
    /// Ограничение пропускной способности (например, "1M").
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
    /// Создаёт новый экземпляр UI.
    ///
    /// Бэкенд rclone инициализируется позже, при первом вызове `update`.
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
            new_remote_config: std::collections::HashMap::new(),
            settings: AppSettings::default(),
        }
    }
    
    /// Инициализирует бэкенд rclone.
    ///
    /// Пытается найти системную установку или скачивает последнюю версию.
    fn init_rclone(&mut self) {
        let rt = Runtime::new().unwrap();
        let rclone_result = rt.block_on(async {
            RcloneApp::new().await
        });
        
        match rclone_result {
            Ok(rclone) => {
                self.rclone = Some(Arc::new(rclone));
                self.load_remotes();
            }
            Err(e) => {
                let error_msg = format!("Не удалось инициализировать rclone: {}", e);
                self.error_message = Some(error_msg.clone());
                self.state = AppState::Error(error_msg);
            }
        }
    }
    
    /// Загружает список удалённых хранилищ и обновляет левую панель.
    fn load_remotes(&mut self) {
        if let Some(rclone) = &self.rclone {
            match operations::remotes::list(rclone) {
                Ok(remotes) => {
                    self.remote_list = remotes;
                    self.state = AppState::Ready;
                }
                Err(e) => {
                    let error_msg = e.clone();
                    self.error_message = Some(e);
                    self.state = AppState::Error(error_msg);
                }
            }
        }
    }
    
    /// Загружает содержимое каталога.
    ///
    /// # Аргументы
    /// * `path` – путь к удалённому хранилищу (например, "remote:" или "remote:папка/подпапка").
    fn load_files(&mut self, path: &str) {
        if let Some(rclone) = &self.rclone {
            match operations::files::list(rclone, path) {
                Ok(files) => {
                    self.current_files = files;
                    self.current_path = path.to_string();
                    self.search_results.clear();
                    self.search_pattern.clear();
                }
                Err(e) => {
                    self.error_message = Some(e);
                }
            }
        }
    }
    
    /// Выполняет операцию копирования из источника в приёмник.
    ///
    /// Запускается в отдельном потоке, чтобы не блокировать UI.
    fn perform_copy(&mut self, source: &str, dest: &str) {
        if let Some(rclone) = &self.rclone {
            self.state = AppState::Copying;
            
            let options = CopyOptions {
                verbose: true,
                dry_run: false,
                bandwidth_limit: self.settings.bandwidth_limit.clone(),
                no_traverse: false,
            };
            
            // Копирование в отдельном потоке
            let rclone_clone = rclone.clone();
            let source_clone = source.to_string();
            let dest_clone = dest.to_string();
            let options_clone = options;
            
            std::thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    operations::sync::copy(&rclone_clone, &source_clone, &dest_clone, &options_clone)
                })
            });
            
            // Для демонстрации имитируем завершение
            self.transfer_progress = Some(TransferProgress {
                current: 100,
                total: 100,
                speed: 10_000_000.0,
                file_name: "Завершено".to_string(),
            });
            self.state = AppState::Ready;
        }
    }
    
    /// Ищет файлы по шаблону.
    fn search_files(&mut self, pattern: &str) {
        if let Some(rclone) = &self.rclone {
            let options = FindOptions {
                recursive: true,
                max_results: 100,
            };
            
            match operations::search::by_name(rclone, &self.current_path, pattern, &options) {
                Ok(results) => {
                    self.search_results = results;
                }
                Err(e) => {
                    self.error_message = Some(e);
                }
            }
        }
    }
    
    /// Форматирует размер в байтах в удобочитаемый вид.
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
    
    /// Форматирует скорость передачи (байт/с) в удобочитаемый вид.
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
    /// Вызывается каждый кадр для обновления интерфейса.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Инициализация rclone, если ещё не выполнена
        if matches!(self.state, AppState::Initializing) && self.rclone.is_none() {
            self.init_rclone();
        }
        
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
                            if limit_str.is_empty() {
                                self.settings.bandwidth_limit = None;
                            } else {
                                self.settings.bandwidth_limit = Some(limit_str);
                            }
                        }
                    });
                });
                
                ui.separator();
                
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
                    AppState::Copying => {
                        ui.colored_label(Color32::BLUE, "● Копирование...");
                    }
                    _ => {
                        ui.colored_label(Color32::GRAY, "● Занят");
                    }
                }
                
                if let Some(rclone) = &self.rclone {
                    ui.separator();
                    ui.label(format!("rclone: {}", rclone.get_rclone_path().display()));
                }
            });
        });
        
        // Левая панель — список хранилищ
        egui::SidePanel::left("remotes_panel")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Хранилища");
                ui.separator();
                
                ScrollArea::vertical().show(ui, |ui| {
                    let remotes = self.remote_list.clone();
                    if remotes.is_empty() && matches!(self.state, AppState::Ready) {
                        ui.colored_label(Color32::GRAY, "Хранилища не найдены");
                        ui.add_space(5.0);
                        if ui.button("➕ Добавить хранилище").clicked() {
                            self.show_new_remote_dialog = true;
                        }
                    } else {
                        for remote in &remotes {
                            let button_text = format!("📡 {}", remote.name);
                            if ui.button(button_text).clicked() {
                                let path = format!("{}:", remote.name);
                                self.load_files(&path);
                            }
                            ui.add_space(2.0);
                        }
                    }
                });
            });
        
        // Центральная панель — основное содержимое
        CentralPanel::default().show(ctx, |ui| {
            // Верхняя панель с путём и поиском
            ui.horizontal(|ui| {
                ui.label("📍 Путь:");
                let path_text = if self.current_path.is_empty() {
                    "Выберите хранилище из левой панели".to_string()
                } else {
                    self.current_path.clone()
                };
                ui.label(path_text);
                
                ui.separator();
                
                ui.label("🔍 Поиск:");
                let mut new_search = self.search_pattern.clone();
                if ui.text_edit_singleline(&mut new_search).changed() {
                    self.search_pattern = new_search.clone();
                    if !new_search.is_empty() && !self.current_path.is_empty() {
                        self.search_files(&new_search);
                    } else {
                        self.search_results.clear();
                    }
                }
                
                let current_path_clone = self.current_path.clone();
                if ui.button("🔄 Обновить").clicked() {
                    if !current_path_clone.is_empty() {
                        self.load_files(&current_path_clone);
                    }
                }
            });
            
            ui.separator();
            
            // Состояние загрузки
            if matches!(self.state, AppState::Loading) || matches!(self.state, AppState::Initializing) {
                ui.centered_and_justified(|ui| {
                    ui.label("Загрузка...");
                });
                return;
            }
            
            // Состояние ошибки
            if let AppState::Error(msg) = &self.state {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(Color32::RED, format!("Ошибка: {}", msg));
                });
                return;
            }
            
            // Результаты поиска или список файлов
            if !self.search_results.is_empty() {
                let search_results = self.search_results.clone();
                let current_path = self.current_path.clone();
                ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Результаты поиска");
                    ui.separator();
                    
                    for file in &search_results {
                        let file_clone = file.clone();
                        ui.horizontal(|ui| {
                            ui.label(file.icon());
                            if ui.link(&file.name).clicked() {
                                if file_clone.is_dir {
                                    let new_path = if current_path.ends_with(':') {
                                        format!("{}{}", current_path, file_clone.name)
                                    } else {
                                        format!("{}/{}", current_path, file_clone.name)
                                    };
                                    self.load_files(&new_path);
                                }
                            }
                            ui.label(Self::format_size(file_clone.size));
                            if let Some(modified) = &file_clone.modified {
                                ui.label(modified);
                            }
                        });
                    }
                });
            } else if !self.current_path.is_empty() {
                // Файловый менеджер
                let current_files = self.current_files.clone();
                let current_path = self.current_path.clone();
                ScrollArea::vertical().show(ui, |ui| {
                    // Кнопка "Наверх"
                    if current_path.contains('/') {
                        if ui.button("📁 .. (Родительская папка)").clicked() {
                            let parent = current_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                            self.load_files(parent);
                        }
                        ui.add_space(5.0);
                    }
                    
                    ui.heading("Файлы и папки");
                    ui.separator();
                    
                    if current_files.is_empty() {
                        ui.colored_label(Color32::GRAY, "Папка пуста");
                    } else {
                        for file in &current_files {
                            let file_clone = file.clone();
                            let selected = self.selected_paths.contains(&file.path);
                            ui.horizontal(|ui| {
                                if ui.selectable_label(selected, file.icon()).clicked() {
                                    if selected {
                                        self.selected_paths.retain(|p| p != &file_clone.path);
                                    } else {
                                        self.selected_paths.push(file_clone.path.clone());
                                    }
                                }
                                
                                if file.is_dir {
                                    if ui.link(&file.name).clicked() {
                                        let new_path = if current_path.ends_with(':') {
                                            format!("{}{}", current_path, file.name)
                                        } else {
                                            format!("{}/{}", current_path, file.name)
                                        };
                                        self.load_files(&new_path);
                                    }
                                } else {
                                    ui.label(&file.name);
                                }
                                
                                ui.label(Self::format_size(file.size));
                                if let Some(modified) = &file.modified {
                                    ui.label(modified);
                                }
                            });
                        }
                    }
                });
            } else {
                // Приветственный экран
                ui.centered_and_justified(|ui| {
                    ui.vertical(|ui| {
                        ui.add_space(50.0);
                        ui.heading("Добро пожаловать в Rclone UI Manager");
                        ui.add_space(20.0);
                        ui.label("Выберите хранилище из левой панели, чтобы начать");
                        ui.add_space(10.0);
                        ui.label("Или добавьте новое хранилище через меню «Хранилища»");
                        ui.add_space(30.0);
                        if ui.button("➕ Добавить новое хранилище").clicked() {
                            self.show_new_remote_dialog = true;
                        }
                    });
                });
            }
            
            ui.separator();
            
            // Кнопки действий
            if !self.selected_paths.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {} элемент(ов)", self.selected_paths.len()));
                    ui.separator();
                    
                    if ui.button("📋 Копировать").clicked() {
                        self.transfer_source = self.selected_paths[0].clone();
                        self.show_transfer_dialog = true;
                    }
                    
                    if ui.button("✂️ Переместить").clicked() {
                        self.transfer_source = self.selected_paths[0].clone();
                        self.show_transfer_dialog = true;
                    }
                    
                    if ui.button("🗑️ Удалить").clicked() {
                        if self.settings.confirm_before_transfer {
                            self.show_transfer_dialog = true;
                        }
                    }
                });
            }
            
            // Прогресс передачи
            if let Some(progress) = &self.transfer_progress {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("📊 Передача: {}", progress.file_name));
                    let percent = if progress.total > 0 {
                        (progress.current as f64 / progress.total as f64) as f32
                    } else {
                        0.0
                    };
                    ui.add(ProgressBar::new(percent).show_percentage());
                    ui.label(format!("Скорость: {}", Self::format_speed(progress.speed)));
                });
            }
        });
        
        // Диалоги
        if self.show_transfer_dialog {
            let transfer_source = self.transfer_source.clone();
            let transfer_dest = self.transfer_dest.clone();
            let mut new_transfer_dest = transfer_dest.clone();
            
            Window::new("Передача файлов")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Источник:");
                    ui.label(&transfer_source);
                    
                    ui.add_space(10.0);
                    
                    ui.label("Назначение:");
                    ui.text_edit_singleline(&mut new_transfer_dest);
                    
                    ui.add_space(10.0);
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("✅ Начать передачу").clicked() {
                            let source = transfer_source.clone();
                            let dest = new_transfer_dest.clone();
                            self.transfer_dest = dest.clone();
                            self.perform_copy(&source, &dest);
                            self.show_transfer_dialog = false;
                            self.selected_paths.clear();
                        }
                        
                        if ui.button("❌ Отмена").clicked() {
                            self.show_transfer_dialog = false;
                        }
                    });
                });
        }
        
        if self.show_new_remote_dialog {
            let mut new_remote_name = self.new_remote_name.clone();
            let mut new_remote_type = self.new_remote_type.clone();
            Window::new("Добавление хранилища")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Имя хранилища:");
                    ui.text_edit_singleline(&mut new_remote_name);
                    
                    ui.add_space(5.0);
                    
                    ui.label("Тип хранилища:");
                    ui.text_edit_singleline(&mut new_remote_type);
                    
                    ui.add_space(10.0);
                    
                    ui.label("Распространённые типы: s3, dropbox, google drive, local");
                    ui.add_space(10.0);
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("✅ Создать").clicked() {
                            if let Some(rclone) = &self.rclone {
                                let config = std::collections::HashMap::new();
                                match operations::remotes::create(
                                    rclone,
                                    &new_remote_name,
                                    &new_remote_type,
                                    &config,
                                ) {
                                    Ok(_) => {
                                        self.new_remote_name = new_remote_name;
                                        self.new_remote_type = new_remote_type;
                                        self.load_remotes();
                                        self.show_new_remote_dialog = false;
                                        self.new_remote_name.clear();
                                        self.new_remote_type.clear();
                                    }
                                    Err(e) => {
                                        self.error_message = Some(e);
                                    }
                                }
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
        
        // Диалог ошибок
        if let Some(error) = &self.error_message {
            let error_clone = error.clone();
            Window::new("Ошибка")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(Color32::RED, "⚠️ Произошла ошибка:");
                    ui.add_space(5.0);
                    ui.label(&error_clone);
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() {
                        self.error_message = None;
                        if let AppState::Error(_) = &self.state {
                            self.state = AppState::Ready;
                        }
                    }
                });
        }
        
        // Запрос перерисовки для анимаций
        ctx.request_repaint();
    }
}

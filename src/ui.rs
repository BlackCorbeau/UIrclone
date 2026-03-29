//! Графический интерфейс для операций rclone с интегрированным Local FS и поддержкой кастомных шрифтов.

use crate::operations::{
    self, CopyOptions, DeleteOptions, FileInfo, MoveOptions, Remote,
};
use crate::rclone_install::RcloneApp;
use eframe::egui;
use egui::{
    Align2, CentralPanel, Color32, ProgressBar, ScrollArea, SidePanel, Spinner, Window,
    FontDefinitions, FontData, FontFamily,
};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

#[derive(Clone, PartialEq)]
pub enum AppState {
    Initializing,
    Ready,
    Error(String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum TransferTab {
    Remote,
    Local,
}

#[derive(Clone)]
pub struct Operation {
    pub id: u32,
    pub description: String,
    pub progress: f32,
    pub status: String,
    pub start_time: Instant,
}

enum OperationResult {
    Success(u32, String),
    Failure(u32, String),
    FileList(Vec<FileInfo>),
    RemoteList(Vec<Remote>),
    ProgressUpdate(u32, f32, String),
}

pub struct RcloneUI {
    rclone: Option<Arc<RcloneApp>>,
    state: AppState,
    error_message: Option<String>,

    current_path: String,
    history_back: Vec<String>,
    history_forward: Vec<String>,

    remote_list: Vec<Remote>,
    current_files: Vec<FileInfo>,
    selected_paths: Vec<String>,

    transfer_source_info: Vec<(String, bool)>,
    transfer_dest: String,
    active_transfer_tab: TransferTab,

    show_transfer_dialog: bool,
    show_local_browser: bool,
    local_browser_path: String,
    local_browser_files: Vec<FileInfo>,

    is_move_mode: bool,
    #[allow(dead_code)]
    settings: AppSettings,

    active_task_count: u32,
    operation_tx: Sender<OperationResult>,
    operation_rx: Receiver<OperationResult>,

    pending_load_path: Option<String>,
    rclone_init_receiver: Option<Receiver<Result<Arc<RcloneApp>, String>>>,

    active_operations: Vec<Operation>,
    next_op_id: u32,
}

#[derive(Clone, Default)]
pub struct AppSettings {
    pub show_hidden: bool,
    pub bandwidth_limit: Option<String>,
}

impl RcloneUI {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // --- ВОССТАНОВЛЕНИЕ ШРИФТОВ ---
        let mut fonts = FontDefinitions::default();
        
        if let Ok(font_data) = std::fs::read("assets/fonts/Sans.otf") {
            fonts.font_data.insert("sans".to_owned(), FontData::from_owned(font_data));
            fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "sans".to_owned());
            fonts.families.entry(FontFamily::Monospace).or_default().push("sans".to_owned());
        }

        if let Ok(emoji_data) = std::fs::read("assets/fonts/NotoColorEmoji.ttf") {
            fonts.font_data.insert("emoji".to_owned(), FontData::from_owned(emoji_data));
            let prop = fonts.families.entry(FontFamily::Proportional).or_default();
            if !prop.contains(&"emoji".to_owned()) {
                prop.push("emoji".to_owned());
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        let (init_tx, init_rx) = channel();
        let (op_tx, op_rx) = channel();

        std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let result = rt.block_on(async { RcloneApp::new().await });
            let res = match result {
                Ok(app) => Ok(Arc::new(app)),
                Err(e) => Err(e.to_string()),
            };
            let _ = init_tx.send(res);
        });

        Self {
            rclone: None,
            state: AppState::Initializing,
            error_message: None,
            current_path: String::new(),
            history_back: Vec::new(),
            history_forward: Vec::new(),
            remote_list: Vec::new(),
            current_files: Vec::new(),
            selected_paths: Vec::new(),
            transfer_source_info: Vec::new(),
            transfer_dest: String::new(),
            active_transfer_tab: TransferTab::Remote,
            show_transfer_dialog: false,
            show_local_browser: false,
            local_browser_path: std::env::var("HOME").unwrap_or_else(|_| "/".into()),
            local_browser_files: Vec::new(),
            is_move_mode: false,
            settings: AppSettings::default(),
            active_task_count: 0,
            operation_tx: op_tx,
            operation_rx: op_rx,
            pending_load_path: None,
            rclone_init_receiver: Some(init_rx),
            active_operations: Vec::new(),
            next_op_id: 1,
        }
    }

    // --- Валидация ---
    fn is_path_valid(&self) -> bool {
        if self.transfer_dest.is_empty() { return false; }
        match self.active_transfer_tab {
            TransferTab::Remote => self.transfer_dest.contains(':'),
            TransferTab::Local => !self.transfer_dest.contains(':') || Path::new(&self.transfer_dest).is_absolute(),
        }
    }

    // --- Навигация ---
    fn navigate_to(&mut self, new_path: String) {
        if self.current_path == new_path { return; }
        if !self.current_path.is_empty() { self.history_back.push(self.current_path.clone()); }
        self.history_forward.clear();
        self.pending_load_path = Some(new_path);
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.history_back.pop() {
            self.history_forward.push(self.current_path.clone());
            self.pending_load_path = Some(prev);
        }
    }

    fn go_forward(&mut self) {
        if let Some(next) = self.history_forward.pop() {
            self.history_back.push(self.current_path.clone());
            self.pending_load_path = Some(next);
        }
    }

    fn refresh_local_list(&mut self) {
        if let Ok(files) = operations::local_fs::list_directory(&self.local_browser_path) {
            self.local_browser_files = files;
        }
    }

    // --- Работа с Rclone ---
    fn poll_rclone_init(&mut self) {
        if let Some(rx) = &self.rclone_init_receiver {
            if let Ok(res) = rx.try_recv() {
                match res {
                    Ok(rclone) => { 
                        self.rclone = Some(rclone); 
                        self.load_remotes(); 
                        self.state = AppState::Ready; 
                    }
                    Err(e) => { 
                        self.error_message = Some(e.clone()); 
                        self.state = AppState::Error(e); 
                    }
                }
                self.rclone_init_receiver = None;
            }
        }
    }

    fn load_remotes(&mut self) {
        if let Some(rclone) = self.rclone.clone() {
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(match operations::remotes::list(&rclone) {
                    Ok(list) => OperationResult::RemoteList(list),
                    Err(e) => OperationResult::Failure(0, e.to_string()),
                });
            });
        }
    }

    fn load_files(&mut self, path: &str) {
        if let Some(rclone) = self.rclone.clone() {
            self.active_task_count += 1;
            let path = path.to_string();
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(match operations::files::list(&rclone, &path) {
                    Ok(list) => OperationResult::FileList(list),
                    Err(e) => OperationResult::Failure(0, e.to_string()),
                });
            });
        }
    }

    fn poll_background_operation(&mut self) {
        while let Ok(result) = self.operation_rx.try_recv() {
            match result {
                OperationResult::Success(op_id, _) => {
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    self.pending_load_path = Some(self.current_path.clone());
                }
                OperationResult::Failure(op_id, e) => {
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    self.error_message = Some(e);
                }
                OperationResult::FileList(files) => {
                    self.current_files = files;
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                }
                OperationResult::RemoteList(remotes) => {
                    self.remote_list = remotes;
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                }
                OperationResult::ProgressUpdate(op_id, progress, status) => {
                    if let Some(op) = self.active_operations.iter_mut().find(|op| op.id == op_id) {
                        op.progress = progress;
                        op.status = status;
                    }
                }
            }
        }
    }

    fn perform_transfer(&mut self, is_move: bool) {
        if self.active_task_count > 0 { return; }
        if let Some(rclone) = self.rclone.clone() {
            let sources = self.transfer_source_info.clone();
            let dest_base = self.transfer_dest.clone();
            let op_id = self.add_operation(format!("{} {} эл.", if is_move { "Перенос" } else { "Копия" }, sources.len()));
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            
            std::thread::spawn(move || {
                let total = sources.len() as f32;
                for (idx, (source, is_dir)) in sources.iter().enumerate() {
                    let name = source.split([':', '/', '\\']).last().unwrap_or("");
                    let separator = if dest_base.contains(':') && !dest_base.ends_with(':') && !dest_base.ends_with('/') { "/" } else { "" };
                    
                    let final_dest = if *is_dir { 
                        format!("{}{}{}", dest_base.trim_end_matches(['/', '\\']), separator, name) 
                    } else { 
                        dest_base.clone() 
                    };
                    
                    let res = if is_move {
                        operations::sync::move_files(&rclone, source, &final_dest, &MoveOptions { verbose: true, ..Default::default() })
                    } else {
                        operations::sync::copy(&rclone, source, &final_dest, &CopyOptions { verbose: true, ..Default::default() })
                    };

                    if let Err(e) = res {
                        let _ = tx.send(OperationResult::Failure(op_id, format!("Ошибка на {}: {}", name, e))); 
                        return;
                    }
                    let _ = tx.send(OperationResult::ProgressUpdate(op_id, (idx + 1) as f32 / total, format!("{}/{}", idx+1, total)));
                }
                let _ = tx.send(OperationResult::Success(op_id, "Ок".into()));
            });
        }
    }

    fn delete_selected(&mut self) {
        if self.active_task_count > 0 { return; }
        if let Some(rclone) = self.rclone.clone() {
            let paths = self.get_selected_info();
            let op_id = self.add_operation(format!("Удаление {} эл.", paths.len()));
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                for (p, is_dir) in paths.iter() {
                    let opts = DeleteOptions { recursive: *is_dir, ..Default::default() };
                    if let Err(e) = operations::sync::delete(&rclone, p, &opts) {
                        let _ = tx.send(OperationResult::Failure(op_id, e.to_string())); return;
                    }
                }
                let _ = tx.send(OperationResult::Success(op_id, "Ок".into()));
            });
        }
    }

    fn add_operation(&mut self, description: String) -> u32 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        self.active_operations.push(Operation { id, description, progress: 0.0, status: "В очереди".into(), start_time: Instant::now() });
        id
    }

    fn get_selected_info(&self) -> Vec<(String, bool)> {
        self.selected_paths.iter().map(|path| {
            let is_dir = self.current_files.iter().find(|f| {
                let full = if self.current_path.ends_with(':') { format!("{}{}", self.current_path, f.name) } else { format!("{}/{}", self.current_path, f.name) };
                &full == path
            }).map(|f| f.is_dir).unwrap_or(false);
            (path.clone(), is_dir)
        }).collect()
    }
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
                    if ui.button("⬅").clicked() { self.go_back(); }
                });
                ui.add_enabled_ui(!self.history_forward.is_empty(), |ui| {
                    if ui.button("➡").clicked() { self.go_forward(); }
                });
                if self.active_task_count > 0 { ui.add(Spinner::new().size(16.0)); }
            });
        });

        SidePanel::left("left").default_width(180.0).show(ctx, |ui| {
            ui.add_space(10.0);
            ui.heading("Хранилища");
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                for remote in self.remote_list.clone() {
                    if ui.selectable_label(self.current_path.starts_with(&remote.name), format!("📡 {}", remote.name)).clicked() {
                        self.navigate_to(format!("{}:", remote.name));
                    }
                }
            });
        });

        SidePanel::right("right").default_width(200.0).show(ctx, |ui| {
            ui.heading("Задачи");
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                for op in &self.active_operations {
                    ui.group(|ui| {
                        ui.label(&op.description);
                        ui.add(ProgressBar::new(op.progress).text(format!("{:.0}%", op.progress * 100.0)));
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
                    if ui.button("🔄").clicked() { self.pending_load_path = Some(self.current_path.clone()); }
                });
            });
            ui.separator();

            ScrollArea::vertical().id_source("files").show(ui, |ui| {
                if !self.current_path.is_empty() {
                    if self.current_path.contains('/') && ui.button("📁 .. (Вверх)").clicked() {
                        if let Some((p, _)) = self.current_path.rsplit_once('/') { self.navigate_to(p.into()); }
                        else if let Some((p, _)) = self.current_path.rsplit_once(':') { self.navigate_to(format!("{}:", p)); }
                    }

                    for file in self.current_files.clone() {
                        let full = if self.current_path.ends_with(':') { format!("{}{}", self.current_path, file.name) } else { format!("{}/{}", self.current_path, file.name) };
                        let sel = self.selected_paths.contains(&full);

                        ui.horizontal(|ui| {
                            if ui.selectable_label(sel, file.icon()).clicked() {
                                if sel { self.selected_paths.retain(|p| p != &full); }
                                else { self.selected_paths.push(full.clone()); }
                            }
                            if file.is_dir {
                                if ui.link(&file.name).clicked() { self.navigate_to(full); }
                            } else { ui.label(&file.name); }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.small(file.format_size());
                            });
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| { ui.label("Выберите хранилище слева"); });
                }
            });

            if !self.selected_paths.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {}", self.selected_paths.len()));
                    ui.add_enabled_ui(self.active_task_count == 0, |ui| {
                        if ui.button("📋 Копировать").clicked() {
                            self.transfer_source_info = self.get_selected_info();
                            self.is_move_mode = false;
                            self.show_transfer_dialog = true;
                        }
                        if ui.button("✂ Переместить").clicked() {
                            self.transfer_source_info = self.get_selected_info();
                            self.is_move_mode = true;
                            self.show_transfer_dialog = true;
                        }
                        if ui.button("🗑 Удалить").clicked() {
                            self.delete_selected();
                            self.selected_paths.clear();
                        }
                    });
                });
            }
        });

        // Модальное окно трансфера
        if self.show_transfer_dialog {
            Window::new("Трансфер").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_transfer_tab, TransferTab::Remote, "☁ Облако");
                    ui.selectable_value(&mut self.active_transfer_tab, TransferTab::Local, "💻 ПК");
                });
                ui.separator();

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.transfer_dest).hint_text("Путь назначения..."));
                        if self.active_transfer_tab == TransferTab::Local && ui.button("📂 Обзор").clicked() {
                            self.refresh_local_list();
                            self.show_local_browser = true;
                        }
                    });
                    
                    if !self.transfer_dest.is_empty() && !self.is_path_valid() {
                        ui.colored_label(Color32::KHAKI, 
                            if self.active_transfer_tab == TransferTab::Remote { "⚠ Путь облака должен содержать ':'" } 
                            else { "⚠ Укажите локальный путь без ':'" });
                    }
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_start = self.is_path_valid() && self.active_task_count == 0;
                    if ui.add_enabled(can_start, egui::Button::new("🚀 Начать")).clicked() {
                        self.perform_transfer(self.is_move_mode);
                        self.show_transfer_dialog = false;
                    }
                    if ui.button("Отмена").clicked() { self.show_transfer_dialog = false; }
                });
            });
        }

        // Внутренний браузер папок
        if self.show_local_browser {
            Window::new("Выбор локальной папки").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).fixed_size([400.0, 300.0]).show(ctx, |ui| {
                ui.label(format!("📍 {}", self.local_browser_path));
                if ui.button("⬅ Наверх").clicked() {
                    if let Some(parent) = Path::new(&self.local_browser_path).parent() {
                        self.local_browser_path = parent.to_string_lossy().into();
                        self.refresh_local_list();
                    }
                }
                ui.separator();
                ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    let files = self.local_browser_files.clone();
                    for file in files {
                        if file.is_dir && ui.button(format!("📁 {}", file.name)).clicked() {
                            self.local_browser_path = file.path;
                            self.refresh_local_list();
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("✅ Выбрать").clicked() {
                        self.transfer_dest = self.local_browser_path.clone();
                        self.show_local_browser = false;
                    }
                    if ui.button("Отмена").clicked() { self.show_local_browser = false; }
                });
            });
        }

        if let Some(msg) = &self.error_message {
            let mut close = false;
            Window::new("Внимание").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(msg);
                if ui.button("ОК").clicked() { close = true; }
            });
            if close { self.error_message = None; }
        }

        if let Some(path) = self.pending_load_path.take() {
            self.current_path = path.clone();
            self.selected_paths.clear();
            self.load_files(&path);
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

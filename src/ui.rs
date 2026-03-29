//! Графический интерфейс для операций rclone.

use eframe::egui;
use egui::{CentralPanel, ScrollArea, Window, Align2, Color32, Spinner, ProgressBar, SidePanel};
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use crate::rclone_install::RcloneApp;
use crate::operations::{self, FileInfo, Remote, CopyOptions, MoveOptions, DeleteOptions};

#[derive(Clone, PartialEq)]
pub enum AppState {
    Initializing,
    Ready,
    Error(String),
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
    // История переходов
    history_back: Vec<String>,
    history_forward: Vec<String>,

    remote_list: Vec<Remote>,
    current_files: Vec<FileInfo>,
    selected_paths: Vec<String>,

    transfer_source_info: Vec<(String, bool)>,
    transfer_dest: String,

    show_transfer_dialog: bool,
    is_move_mode: bool, 
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
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
            show_transfer_dialog: false,
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

    // --- Логика навигации ---
    fn navigate_to(&mut self, new_path: String) {
        if self.current_path == new_path { return; }
        if !self.current_path.is_empty() {
            self.history_back.push(self.current_path.clone());
        }
        self.history_forward.clear();
        self.pending_load_path = Some(new_path);
    }

    fn go_back(&mut self) {
        if let Some(prev_path) = self.history_back.pop() {
            self.history_forward.push(self.current_path.clone());
            self.pending_load_path = Some(prev_path);
        }
    }

    fn go_forward(&mut self) {
        if let Some(next_path) = self.history_forward.pop() {
            self.history_back.push(self.current_path.clone());
            self.pending_load_path = Some(next_path);
        }
    }

    // --- Фоновые процессы ---
    fn poll_rclone_init(&mut self) {
        if let Some(receiver) = &self.rclone_init_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok(rclone) => {
                        self.rclone = Some(rclone);
                        self.load_remotes();
                        self.state = AppState::Ready;
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Критическая ошибка: {}", e));
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
                    Err(e) => OperationResult::Failure(0, format!("Ошибка списка хранилищ: {}", e)),
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
                    Err(e) => OperationResult::Failure(0, format!("Ошибка доступа к папке: {}", e)),
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

    // --- Операции ---
    fn perform_copy_multiple(&mut self, sources: Vec<(String, bool)>, dest: &str) {
        if let Some(rclone) = self.rclone.clone() {
            let options = CopyOptions { verbose: true, dry_run: false, bandwidth_limit: self.settings.bandwidth_limit.clone(), no_traverse: false };
            let dest_base = dest.to_string();
            let op_id = self.add_operation(format!("Копирование {} эл.", sources.len()));
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                let total = sources.len() as f32;
                for (idx, (source, is_dir)) in sources.iter().enumerate() {
                    let name = source.split([':', '/']).last().unwrap_or("");
                    let final_dest = if *is_dir { format!("{}/{}", dest_base.trim_end_matches('/'), name) } else { dest_base.clone() };
                    if let Err(e) = operations::sync::copy(&rclone, source, &final_dest, &options) {
                        let _ = tx.send(OperationResult::Failure(op_id, format!("Ошибка копирования {}: {}", source, e))); return;
                    }
                    let _ = tx.send(OperationResult::ProgressUpdate(op_id, (idx + 1) as f32 / total, format!("{}/{}", idx+1, total)));
                }
                let _ = tx.send(OperationResult::Success(op_id, "Завершено".into()));
            });
        }
    }

    fn perform_move_multiple(&mut self, sources: Vec<(String, bool)>, dest: &str) {
        if let Some(rclone) = self.rclone.clone() {
            let options = MoveOptions { verbose: true, dry_run: false, delete_empty_src_dirs: true };
            let dest_base = dest.to_string();
            let op_id = self.add_operation(format!("Перемещение {} эл.", sources.len()));
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                let total = sources.len() as f32;
                for (idx, (source, is_dir)) in sources.iter().enumerate() {
                    let name = source.split([':', '/']).last().unwrap_or("");
                    let final_dest = if *is_dir { format!("{}/{}", dest_base.trim_end_matches('/'), name) } else { dest_base.clone() };
                    if let Err(e) = operations::sync::move_files(&rclone, source, &final_dest, &options) {
                        let _ = tx.send(OperationResult::Failure(op_id, format!("Ошибка перемещения {}: {}", source, e))); return;
                    }
                    let _ = tx.send(OperationResult::ProgressUpdate(op_id, (idx + 1) as f32 / total, format!("{}/{}", idx+1, total)));
                }
                let _ = tx.send(OperationResult::Success(op_id, "Завершено".into()));
            });
        }
    }

    fn delete_selected_paths(&mut self, paths: Vec<String>) {
        if let Some(rclone) = self.rclone.clone() {
            let mut path_info = Vec::new();
            for path in &paths {
                let is_dir = self.current_files.iter().find(|f| {
                    let full = if self.current_path.ends_with(':') { format!("{}{}", self.current_path, f.name) } 
                               else { format!("{}/{}", self.current_path, f.name) };
                    &full == path
                }).map(|f| f.is_dir).unwrap_or(false);
                path_info.push((path.clone(), is_dir));
            }
            let op_id = self.add_operation(format!("Удаление {} эл.", paths.len()));
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                let total = path_info.len() as f32;
                for (idx, (p, is_dir)) in path_info.iter().enumerate() {
                    let opts = DeleteOptions { recursive: *is_dir, verbose: false, dry_run: false };
                    if let Err(e) = operations::sync::delete(&rclone, p, &opts) {
                        let _ = tx.send(OperationResult::Failure(op_id, format!("Ошибка удаления {}: {}", p, e))); return;
                    }
                    let _ = tx.send(OperationResult::ProgressUpdate(op_id, (idx + 1) as f32 / total, format!("{}/{}", idx+1, total)));
                }
                let _ = tx.send(OperationResult::Success(op_id, "Удалено".into()));
            });
        }
    }

    fn add_operation(&mut self, description: String) -> u32 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        self.active_operations.push(Operation { id, description, progress: 0.0, status: "В очереди".into(), start_time: Instant::now() });
        id
    }

    fn format_size(bytes: u64) -> String {
        if bytes >= 1073741824 { format!("{:.2} ГБ", bytes as f64 / 1073741824.0) }
        else if bytes >= 1048576 { format!("{:.2} МБ", bytes as f64 / 1048576.0) }
        else if bytes >= 1024 { format!("{:.2} КБ", bytes as f64 / 1024.0) }
        else { format!("{} Б", bytes) }
    }

    fn get_selected_info(&self) -> Vec<(String, bool)> {
        self.selected_paths.iter().map(|path| {
            let is_dir = self.current_files.iter().find(|f| {
                let full = if self.current_path.ends_with(':') { format!("{}{}", self.current_path, f.name) } 
                           else { format!("{}/{}", self.current_path, f.name) };
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

        let is_busy = !self.active_operations.is_empty();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label("🚀 UI rclone");
                ui.separator();
                
                // Навигация
                ui.add_enabled_ui(!self.history_back.is_empty(), |ui| {
                    if ui.button("⬅").clicked() { self.go_back(); }
                });
                ui.add_enabled_ui(!self.history_forward.is_empty(), |ui| {
                    if ui.button("➡").clicked() { self.go_forward(); }
                });

                if self.active_task_count > 0 { ui.add(Spinner::new().size(16.0)); }
            });
        });

        SidePanel::left("left").default_width(200.0).show(ctx, |ui| {
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

        SidePanel::right("right").default_width(220.0).show(ctx, |ui| {
            ui.heading("Активность");
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                for op in &self.active_operations {
                    ui.group(|ui| {
                        ui.label(&op.description);
                        ui.add(ProgressBar::new(op.progress).text(format!("{:.0}%", op.progress * 100.0)));
                        ui.small(&op.status);
                    });
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("📍 {}", self.current_path));
                if ui.button("🔄").clicked() { self.pending_load_path = Some(self.current_path.clone()); }
            });
            ui.separator();

            ScrollArea::vertical().id_source("files").show(ui, |ui| {
                if !self.current_path.is_empty() {
                    if self.current_path.contains('/') && ui.button("📁 .. (Вверх)").clicked() {
                        if let Some((p, _)) = self.current_path.rsplit_once('/') { self.navigate_to(p.into()); }
                    }

                    for file in self.current_files.clone() {
                        let full = if self.current_path.ends_with(':') { format!("{}{}", self.current_path, file.name) } 
                                   else { format!("{}/{}", self.current_path, file.name) };
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
                                ui.small(Self::format_size(file.size));
                            });
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| { ui.label("Выберите диск в левой панели"); });
                }
            });

            if !self.selected_paths.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("Выбрано: {}", self.selected_paths.len()));
                    ui.add_enabled_ui(!is_busy, |ui| {
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
                            self.delete_selected_paths(self.selected_paths.clone());
                            self.selected_paths.clear();
                        }
                    });
                });
            }
        });

        // Модальные окна
        if self.show_transfer_dialog {
            Window::new(if self.is_move_mode { "Перемещение" } else { "Копирование" })
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label("Куда отправить:");
                    ui.text_edit_singleline(&mut self.transfer_dest);
                    if !self.transfer_dest.contains(':') && !self.transfer_dest.is_empty() {
                        ui.colored_label(Color32::YELLOW, "⚠ Путь rclone должен содержать ':'");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("ОК").clicked() && !self.transfer_dest.is_empty() {
                            let (s, d) = (self.transfer_source_info.clone(), self.transfer_dest.clone());
                            if self.is_move_mode { self.perform_move_multiple(s, &d); }
                            else { self.perform_copy_multiple(s, &d); }
                            self.show_transfer_dialog = false;
                        }
                        if ui.button("Отмена").clicked() { self.show_transfer_dialog = false; }
                    });
                });
        }

        if let Some(msg) = &self.error_message {
            let mut close = false;
            Window::new("Сообщение").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.colored_label(Color32::LIGHT_RED, "⚠️ Событие:");
                ui.label(msg);
                if ui.button("Закрыть").clicked() { close = true; }
            });
            if close { self.error_message = None; }
        }

        // Выполнение отложенной загрузки
        if let Some(path) = self.pending_load_path.take() {
            self.current_path = path.clone();
            self.selected_paths.clear();
            self.load_files(&path);
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

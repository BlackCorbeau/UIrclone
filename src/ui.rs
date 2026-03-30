//! Графический интерфейс для операций rclone с интегрированным Local FS и поддержкой кастомных шрифтов.
use crate::operations::{FileInfo, Remote};
use crate::rclone_install::RcloneApp;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

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

pub mod app;
pub mod rcloneui;

#[derive(Clone, Default)]
pub struct AppSettings {
    pub show_hidden: bool,
    pub bandwidth_limit: Option<String>,
}

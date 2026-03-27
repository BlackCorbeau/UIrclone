use eframe::egui;
use egui::{CentralPanel, ScrollArea, Window, Align2, Color32, ProgressBar};
use std::sync::Arc;
use tokio::runtime::Runtime;
use crate::rclone_install::RcloneApp;
use crate::operations::{self, FileInfo, Remote, CopyOptions, FindOptions};

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
#[derive(Clone)]
pub struct TransferProgress {
    pub current: u64,
    pub total: u64,
    pub speed: f64,
    pub file_name: String,
}

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
    new_remote_config: std::collections::HashMap<String, String>,
    
    settings: AppSettings,
}

#[derive(Clone)]
pub struct AppSettings {
    pub show_hidden: bool,
    pub confirm_before_transfer: bool,
    pub max_concurrent_transfers: u32,
    pub bandwidth_limit: Option<String>,
}


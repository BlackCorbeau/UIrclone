use std::collections::HashMap;

/// === Модуль для работы с удаленными хранилищами ===

#[derive(Debug, Clone)]
pub struct Remote {
    pub name: String,
    pub r#type: String,
    pub config: HashMap<String, String>,
}

pub mod remotes;

/// === Модуль для работы с файлами и директориями ===

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<String>,
}

impl FileInfo {
    pub fn format_size(&self) -> String {
        let bytes = self.size;
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.is_dir { "📁" } else { "📄" }
    }
}

pub mod files;

/// === Модуль для операций синхронизации ===

#[derive(Debug, Clone)]
pub struct SyncStats {
    pub transferred: u64,
    pub files: u64,
    pub errors: u64,
    pub checks: u64,
    pub elapsed_time: f64,
    pub transfer_speed: f64, // в байтах в секунду
}

impl SyncStats {
    pub fn format_speed(&self) -> String {
        let speed = self.transfer_speed;
        if speed < 1024.0 {
            format!("{:.0} B/s", speed)
        } else if speed < 1024.0 * 1024.0 {
            format!("{:.1} KB/s", speed / 1024.0)
        } else if speed < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB/s", speed / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB/s", speed / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

#[derive(Debug, Default)]
pub struct CopyOptions {
    pub verbose: bool,
    pub dry_run: bool,
    pub bandwidth_limit: Option<String>,
    pub no_traverse: bool,
}

#[derive(Debug, Default)]
pub struct SyncOptions {
    pub verbose: bool,
    pub dry_run: bool,
    pub delete_excluded: bool,
    pub bandwidth_limit: Option<String>,
    pub existing_only: bool,
}

#[derive(Debug, Default)]
pub struct MoveOptions {
    pub verbose: bool,
    pub dry_run: bool,
    pub delete_empty_src_dirs: bool,
}

#[derive(Debug, Default)]
pub struct DeleteOptions {
    pub verbose: bool,
    pub dry_run: bool,
    pub recursive: bool,
}

pub mod sync;

/// === Модуль для поиска ===

#[derive(Debug, Default)]
pub struct FindOptions {
    pub recursive: bool,
    pub max_results: usize,
}

pub mod search;

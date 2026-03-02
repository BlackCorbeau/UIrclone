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

pub mod files;


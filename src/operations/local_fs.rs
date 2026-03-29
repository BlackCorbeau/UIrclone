
use std::fs;
use std::path::{Path, PathBuf};
use super::*;

pub fn list_directory(path: &str) -> Result<Vec<FileInfo>, String> {
    let p = if path.is_empty() {
        if cfg!(windows) { PathBuf::from("C:\\") } else { PathBuf::from("/") }
    } else {
        PathBuf::from(path)
    };

    let entries = fs::read_dir(&p).map_err(|e| e.to_string())?;
    let mut files = Vec::new();

    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            files.push(FileInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                size: meta.len(),
                is_dir: meta.is_dir(),
                modified: None,
            });
        }
    }
    // Сначала папки, потом файлы, по алфавиту
    files.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(files)
}

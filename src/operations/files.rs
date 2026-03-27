use super::*;
use crate::rclone_install::RcloneApp;
use serde_json::Value;

/// Получить содержимое директории
pub fn list(app: &RcloneApp, remote_path: &str) -> Result<Vec<FileInfo>, String> {
    let output = app.run_command(&["lsjson", remote_path])?;

    let files: Vec<Value> = serde_json::from_str(&output)
        .map_err(|e| format!("Ошибка парсинга списка файлов: {}", e))?;

    let mut file_infos = Vec::new();
    for item in files {
        let name = item["Name"].as_str().unwrap_or("").to_string();
        let path = item["Path"].as_str().unwrap_or("").to_string();
        let size = item["Size"].as_u64().unwrap_or(0);
        let is_dir = item["IsDir"].as_bool().unwrap_or(false);
        let modified = item["ModTime"].as_str().map(String::from);

        file_infos.push(FileInfo {
            name,
            path,
            size,
            is_dir,
            modified,
        });
    }

    Ok(file_infos)
}

/// Получить размер и количество файлов
pub fn size(app: &RcloneApp, path: &str) -> Result<(u64, u64), String> {
    let output = app.run_command(&["size", path, "--json"])?;

    let size_info: Value =
        serde_json::from_str(&output).map_err(|e| format!("Ошибка парсинга размера: {}", e))?;

    let count = size_info["count"].as_u64().unwrap_or(0);
    let bytes = size_info["bytes"].as_u64().unwrap_or(0);

    Ok((bytes, count))
}

/// Получить информацию о хранилище (общий объем, использовано, свободно)
pub fn about(app: &RcloneApp, remote: &str) -> Result<HashMap<String, u64>, String> {
    let output = app.run_command(&["about", remote, "--json"])?;

    let about_info: Value = serde_json::from_str(&output)
        .map_err(|e| format!("Ошибка парсинга информации о хранилище: {}", e))?;

    let mut info = HashMap::new();
    if let Some(obj) = about_info.as_object() {
        for (key, value) in obj {
            if let Some(num) = value.as_u64() {
                info.insert(key.clone(), num);
            } else if let Some(num) = value.as_f64() {
                info.insert(key.clone(), num as u64);
            }
        }
    }

    Ok(info)
}

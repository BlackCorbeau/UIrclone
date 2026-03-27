use super::*;
use crate::rclone_install::RcloneApp;
use serde_json::Value;

/// Найти файлы по имени
pub fn by_name(
    app: &RcloneApp,
    path: &str,
    pattern: &str,
    options: &FindOptions,
) -> Result<Vec<FileInfo>, String> {
    let mut args = vec!["lsjson", path];

    if options.recursive {
        args.push("-R");
    }

    let output = app.run_command(&args)?;
    let files: Vec<Value> =
        serde_json::from_str(&output).map_err(|e| format!("Ошибка парсинга: {}", e))?;

    let pattern = pattern.to_lowercase();
    let mut results = Vec::new();

    for item in files {
        let name = item["Name"].as_str().unwrap_or("");
        let path = item["Path"].as_str().unwrap_or("");

        if name.to_lowercase().contains(&pattern) || path.to_lowercase().contains(&pattern) {
            results.push(FileInfo {
                name: name.to_string(),
                path: path.to_string(),
                size: item["Size"].as_u64().unwrap_or(0),
                is_dir: item["IsDir"].as_bool().unwrap_or(false),
                modified: item["ModTime"].as_str().map(String::from),
            });
        }

        if results.len() >= options.max_results {
            break;
        }
    }

    Ok(results)
}

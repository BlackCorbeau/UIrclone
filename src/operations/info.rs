use super::*;
use crate::rclone_install::RcloneApp;
use serde_json::Value;
use std::collections::HashMap;

/// Получить версию в удобочитаемом формате
pub fn version(app: &RcloneApp) -> Result<HashMap<String, String>, String> {
    let output = app.run_command(&["version"])?;
    let mut version_info = HashMap::new();

    for line in output.lines() {
        if let Some((key, value)) = line.split_once(':') {
            version_info.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(version_info)
}

/// Получить статистику текущих операций
pub fn stats(app: &RcloneApp) -> Result<Value, String> {
    let output = app.run_command(&["core", "stats"])?;
    let stats: Value =
        serde_json::from_str(&output).map_err(|e| format!("Ошибка парсинга статистики: {}", e))?;
    Ok(stats)
}

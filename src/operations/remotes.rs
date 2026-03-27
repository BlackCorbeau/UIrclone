use super::*;
use crate::rclone_install::RcloneApp;
use serde_json::Value;

/// Получить список всех удаленных хранилищ
pub fn list(app: &RcloneApp) -> Result<Vec<Remote>, String> {
    let output = app.run_command(&["listremotes"])?;

    let mut remotes = Vec::new();
    for line in output.lines() {
        let name = line.trim_end_matches(':');
        if !name.is_empty() {
            // Получаем детальную информацию о каждом remote
            if let Ok(config) = get_config(app, name) {
                remotes.push(config);
            } else {
                // Если не удалось получить конфиг, добавляем базовую информацию
                remotes.push(Remote {
                    name: name.to_string(),
                    r#type: "unknown".to_string(),
                    config: HashMap::new(),
                });
            }
        }
    }

    Ok(remotes)
}

/// Получить конфигурацию конкретного remote
pub fn get_config(app: &RcloneApp, remote_name: &str) -> Result<Remote, String> {
    let output = app.run_command(&["config", "dump"])?;

    let configs: Value = serde_json::from_str(&output)
        .map_err(|e| format!("Ошибка парсинга конфигурации: {}", e))?;

    if let Some(remote_config) = configs.get(remote_name) {
        let r#type = remote_config
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut config_map = HashMap::new();
        if let Some(obj) = remote_config.as_object() {
            for (key, value) in obj {
                if key != "type" {
                    if let Some(val_str) = value.as_str() {
                        config_map.insert(key.clone(), val_str.to_string());
                    } else {
                        config_map.insert(key.clone(), value.to_string());
                    }
                }
            }
        }

        Ok(Remote {
            name: remote_name.to_string(),
            r#type,
            config: config_map,
        })
    } else {
        Err(format!("Remote '{}' не найден", remote_name))
    }
}

/// Создать новое удаленное хранилище
pub fn create(
    app: &RcloneApp,
    name: &str,
    r#type: &str,
    config: &HashMap<String, String>,
) -> Result<(), String> {
    let mut args = vec!["config", "create", name, r#type];

    for (key, value) in config {
        args.push(key.as_str());
        args.push(value.as_str());
    }

    app.run_command(&args)?;
    Ok(())
}

/// Обновить конфигурацию remote
pub fn update(app: &RcloneApp, name: &str, config: &HashMap<String, String>) -> Result<(), String> {
    let mut args = vec!["config", "update", name];

    for (key, value) in config {
        args.push(key.as_str());
        args.push(value.as_str());
    }

    app.run_command(&args)?;
    Ok(())
}

/// Удалить remote
pub fn delete(app: &RcloneApp, name: &str) -> Result<(), String> {
    app.run_command(&["config", "delete", name])?;
    Ok(())
}

/// Проверить доступность remote
pub fn check(app: &RcloneApp, remote_name: &str) -> Result<bool, String> {
    let result = app.run_command(&["lsd", remote_name, "--max-depth", "1"]);
    match result {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.contains("directory not found") {
                Ok(true) // Remote существует, но пустой
            } else {
                Err(e)
            }
        }
    }
}

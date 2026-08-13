use super::*;
use crate::rclone_install::RcloneApp;
use serde_json::Value;

/// Получить список всех удаленных хранилищ
pub fn list(app: &RcloneApp) -> Result<Vec<Remote>, String> {
    log::debug!("Получение списка удаленных хранилищ");
    let output = app.run_command(&["listremotes"])?;

    let mut remotes = Vec::new();
    for line in output.lines() {
        let name = line.trim_end_matches(':');
        if !name.is_empty() {
            // Получаем детальную информацию о каждом remote
            if let Ok(config) = get_config(app, name) {
                remotes.push(config);
            } else {
                log::warn!("Не удалось получить конфигурацию для remote '{}'", name);
                // Если не удалось получить конфиг, добавляем базовую информацию
                remotes.push(Remote {
                    name: name.to_string(),
                    r#type: "unknown".to_string(),
                    config: HashMap::new(),
                });
            }
        }
    }

    log::info!("Найдено {} удаленных хранилищ", remotes.len());
    Ok(remotes)
}

/// Получить конфигурацию конкретного remote
pub fn get_config(app: &RcloneApp, remote_name: &str) -> Result<Remote, String> {
    log::debug!("Получение конфигурации для remote: {}", remote_name);
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

        log::debug!("Получена конфигурация для {} типа {}", remote_name, r#type);
        Ok(Remote {
            name: remote_name.to_string(),
            r#type,
            config: config_map,
        })
    } else {
        let error_msg = format!("Remote '{}' не найден", remote_name);
        log::error!("{}", error_msg);
        Err(error_msg)
    }
}

/// Вопрос, который rclone задает в процессе создания remote
#[derive(Debug, Clone)]
pub struct ConfigQuestion {
    pub name: String,
    pub help: String,
    pub default: String,
    pub is_password: bool,
    pub examples: Vec<(String, String)>,
}

#[derive(Debug)]
pub enum ConfigStep {
    /// Создание завершено
    Done,
    /// rclone задает вопрос, нужен ответ пользователя
    Question {
        state: String,
        question: ConfigQuestion,
    },
}

/// Один шаг интерактивного создания remote через `config create --non-interactive`.
/// Вызывается в цикле: без state — первый вопрос, далее с state и ответом
/// пользователя через `--continue`. `on_stderr` получает строки stderr
/// (например, ссылку для авторизации в браузере).
pub fn config_create_step(
    app: &RcloneApp,
    name: &str,
    r#type: &str,
    state: Option<&str>,
    result: Option<&str>,
    on_stderr: &mut dyn FnMut(&str),
) -> Result<ConfigStep, String> {
    log::debug!("config create шаг: remote={} тип={} state={:?}", name, r#type, state);
    let mut args = vec!["config", "create", name, r#type, "--non-interactive"];
    if let Some(s) = state {
        args.extend([
            "--continue",
            "--state",
            s,
            "--result",
            result.unwrap_or_default(),
        ]);
    }

    let output = app.run_command_with_stderr_feed(&args, on_stderr)?;

    let value: Value = serde_json::from_str(&output)
        .map_err(|e| format!("Ошибка парсинга ответа rclone: {}", e))?;

    if let Some(err) = value.get("Error").and_then(|v| v.as_str()) {
        if !err.is_empty() {
            log::error!("rclone вернул ошибку: {}", err);
            return Err(err.to_string());
        }
    }

    if value["Option"].is_null() {
        return Ok(ConfigStep::Done);
    }

    let opt = &value["Option"];
    let examples = opt
        .get("Examples")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let val = match e.get("Value") {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => return None,
                    };
                    let help = e
                        .get("Help")
                        .and_then(|h| h.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some((val, help))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ConfigStep::Question {
        state: value["State"].as_str().unwrap_or("").to_string(),
        question: ConfigQuestion {
            name: opt["Name"].as_str().unwrap_or("").to_string(),
            help: opt["Help"].as_str().unwrap_or("").to_string(),
            default: opt["DefaultStr"].as_str().unwrap_or("").to_string(),
            is_password: opt["IsPassword"].as_bool().unwrap_or(false),
            examples,
        },
    })
}

/// Обновить конфигурацию remote
pub fn update(app: &RcloneApp, name: &str, config: &HashMap<String, String>) -> Result<(), String> {
    log::info!("Обновление конфигурации remote: {}", name);
    let mut args = vec!["config", "update", name];

    for (key, value) in config {
        args.push(key.as_str());
        args.push(value.as_str());
    }

    app.run_command(&args)?;
    log::info!("Конфигурация remote {} успешно обновлена", name);
    Ok(())
}

/// Удалить remote
pub fn delete(app: &RcloneApp, name: &str) -> Result<(), String> {
    log::info!("Удаление remote: {}", name);
    app.run_command(&["config", "delete", name])?;
    log::info!("Remote {} успешно удален", name);
    Ok(())
}

/// Проверить доступность remote
pub fn check(app: &RcloneApp, remote_name: &str) -> Result<bool, String> {
    log::debug!("Проверка доступности remote: {}", remote_name);
    let result = app.run_command(&["lsd", remote_name, "--max-depth", "1"]);
    match result {
        Ok(_) => {
            log::debug!("Remote {} доступен", remote_name);
            Ok(true)
        }
        Err(e) => {
            if e.contains("directory not found") {
                log::warn!("Remote {} существует, но пуст", remote_name);
                Ok(true) // Remote существует, но пустой
            } else {
                log::error!("Ошибка доступа к remote {}: {}", remote_name, e);
                Err(e)
            }
        }
    }
}

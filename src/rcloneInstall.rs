use flate2::read::GzDecoder;
use reqwest;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use tar::Archive;
use tokio;

pub struct RcloneApp {
    rclone_path: PathBuf,
    using_system_rclone: bool,
}

impl RcloneApp {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(system_rclone) = Self::check_system_rclone() {
            println!("Найден системный rclone: {:?}", system_rclone);
            return Ok(Self {
                rclone_path: system_rclone,
                using_system_rclone: true,
            });
        }
        println!("Системный rclone не найден, использую встроенную версию");

        let app_dir = dirs::data_dir()
            .ok_or("Не удалось найти папку для данных")?
            .join("rclone-ui");

        fs::create_dir_all(&app_dir)?;
        let rclone_path = if cfg!(windows) {
            app_dir.join("rclone.exe")
        } else {
            app_dir.join("rclone")
        };

        /*if !rclone_path.exists() {
            Self::extract_rclone(&rclone_path)?;
        }*/

        Ok(Self {
            rclone_path,
            using_system_rclone: false,
        })
    }

    fn check_system_rclone() -> Option<PathBuf> {
        if let Ok(path) = which::which("rclone") {
            return Some(path);
        }

        #[cfg(windows)]
        {
            if let Ok(path) = which::which("rclone.exe") {
                return Some(path);
            }
        }

        None
    }

    /*fn extract_rclone(dest_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        println!("Устанавливаю rclone в {:?}", dest_path);

        let rclone_bytes = unsafe {
            // unsafe блок, возвращающий значение
            if cfg!(windows) {
                include_bytes!("../bin/rclone.exe")
            } else if cfg!(target_os = "linux") {
                include_bytes!("../bin/rclone")
            } else if cfg!(target_os = "macos") {
                include_bytes!("../bin/rclone")
            } else {
                panic!("Неподдерживаемая ОС");
            }
        };

        fs::write(dest_path, rclone_bytes)?;

        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(dest_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(dest_path, perms)?;
        }

        println!("rclone успешно установлен!");
        Ok(())
    }*/

    fn run_command(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.rclone_path)
            .args(args)
            .output()
            .map_err(|e| format!("Ошибка запуска: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

/// Асинхронная функция для проверки наличия и версии rclone
/// Возвращает:
/// 0 - rclone установлен и актуален
/// 1 - требуется установка или обновление
pub async fn check_rclone_status_async() -> Result<i32, Box<dyn std::error::Error>> {
    if let Some(system_rclone) = RcloneApp::check_system_rclone() {
        println!("Найден системный rclone: n{:?}", system_rclone);

        let output = Command::new(&system_rclone)
            .arg("version")
            .output()
            .map_err(|e| format!("Ошибка получения версии: {}", e))?;

        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            println!("Версия rclone: {}", version_output);

            if is_version_recent(&version_output) {
                return Ok(0);
            } else {
                println!("Версия rclone устарела, требуется обновление");
                return Ok(1);
            }
        }
    }

    let app_dir = dirs::data_dir()
        .ok_or("Не удалось найти папку для данных")?
        .join("rclone-ui");

    let rclone_path = if cfg!(windows) {
        app_dir.join("rclone.exe")
    } else {
        app_dir.join("rclone")
    };

    if rclone_path.exists() {
        println!("Найден rclone в директории приложения");

        let output = Command::new(&rclone_path)
            .arg("version")
            .output()
            .map_err(|e| format!("Ошибка получения версии: {}", e))?;

        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);

            if is_version_recent(&version_output) {
                return Ok(0);
            } else {
                println!("Версия rclone в директории приложения устарела");
                return Ok(1);
            }
        }
    }

    println!("rclone не найден, требуется установка");
    Ok(1)
}

/// Вспомогательная функция для проверки актуальности версии
fn is_version_recent(version_output: &str) -> bool {
    !version_output.contains("rclone v1.55")
}

/// Асинхронная функция для установки последней версии rclone из GitHub
pub async fn install_latest_rclone() -> Result<RcloneApp, Box<dyn std::error::Error>> {
    println!("Начинаю установку последней версии rclone...");

    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        return Err("Неподдерживаемая операционная система".into());
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        return Err("Неподдерживаемая архитектура".into());
    };

    // Получаем актуальную версию через API GitHub
    let client = reqwest::Client::new();
    let latest_release: serde_json::Value = client
        .get("https://api.github.com/repos/rclone/rclone/releases/latest")
        .header("User-Agent", "rclone-ui-installer")
        .send()
        .await?
        .json()
        .await?;

    let version = latest_release["tag_name"]
        .as_str()
        .ok_or("Не удалось получить версию")?
        .trim_start_matches('v'); // Убираем 'v' из начала версии если есть

    // Всегда используем .zip формат для всех ОС
    let download_url = format!(
        "https://github.com/rclone/rclone/releases/download/v{}/rclone-v{}-{}-{}.zip",
        version, version, os, arch
    );

    println!("Скачиваю rclone с: {}", download_url);

    let response = client.get(&download_url).send().await?;

    if !response.status().is_success() {
        return Err(format!("Ошибка скачивания: HTTP {}", response.status()).into());
    }

    let bytes = response.bytes().await?;

    let app_dir = dirs::data_dir()
        .ok_or("Не удалось найти папку для данных")?
        .join("rclone-ui");

    fs::create_dir_all(&app_dir)?;

    // Определяем имя исполняемого файла в зависимости от ОС
    let rclone_filename = if cfg!(windows) {
        "rclone.exe"
    } else {
        "rclone"
    };
    let rclone_path = app_dir.join(rclone_filename);

    // Распаковываем zip архив (для всех ОС)
    use std::io::Cursor;
    use zip::read::ZipArchive;

    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;

    let mut extracted = false;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = file.name().to_string(); // Сохраняем имя в отдельную переменную

        // Ищем файл rclone или rclone.exe в архиве
        if file_name.ends_with("rclone") || file_name.ends_with("rclone.exe") {
            // Определяем путь для распаковки
            let output_path = if file_name.contains('/') || file_name.contains('\\') {
                // Если файл в подпапке, создаем временный путь
                let temp_path = app_dir.join("temp_rclone");
                fs::create_dir_all(&temp_path)?;
                temp_path.join(
                    file_name
                        .split(|c| c == '/' || c == '\\')
                        .last()
                        .unwrap_or(rclone_filename),
                )
            } else {
                app_dir.join(&file_name)
            };

            // Распаковываем файл
            let mut contents = Vec::new();
            std::io::copy(&mut file, &mut contents)?;
            fs::write(&output_path, contents)?;

            // Перемещаем в нужное место если необходимо
            if output_path != rclone_path {
                if rclone_path.exists() {
                    fs::remove_file(&rclone_path)?;
                }
                fs::rename(output_path, &rclone_path)?;
            }

            extracted = true;
            println!("Найден и распакован: {}", file_name);
            break;
        }
    }

    if !extracted {
        return Err("Не удалось найти исполняемый файл rclone в архиве".into());
    }

    // Удаляем временную папку если она была создана
    let temp_path = app_dir.join("temp_rclone");
    if temp_path.exists() {
        fs::remove_dir_all(temp_path)?;
    }

    // Устанавливаем права на выполнение для Unix систем
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&rclone_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&rclone_path, perms)?;
    }

    println!("rclone успешно установлен в {:?}", rclone_path);

    // Проверяем установку
    let output = Command::new(&rclone_path)
        .arg("version")
        .output()
        .map_err(|e| format!("Ошибка проверки установки: {}", e))?;

    if output.status.success() {
        let version_output = String::from_utf8_lossy(&output.stdout);
        println!("Установленная версия rclone:\n{}", version_output);
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Ошибка при проверке версии rclone: {}", error).into());
    }

    Ok(RcloneApp {
        rclone_path,
        using_system_rclone: false,
    })
}

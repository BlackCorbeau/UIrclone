use flate2::read::GzDecoder;
use reqwest;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use tar::Archive;
use tokio;

use super::RcloneApp;

impl RcloneApp {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Сначала проверяем системный rclone
        if let Some(system_rclone) = Self::check_system_rclone() {
            println!("Найден системный rclone: {:?}", system_rclone);
            return Ok(Self {
                rclone_path: system_rclone,
                using_system_rclone: true,
            });
        }

        // Проверяем в директории приложения
        let app_dir = dirs::data_dir()
            .ok_or("Не удалось найти папку для данных")?
            .join("rclone-ui");

        let rclone_path = if cfg!(windows) {
            app_dir.join("rclone.exe")
        } else {
            app_dir.join("rclone")
        };

        if rclone_path.exists() {
            println!("Найден rclone в директории приложения: {:?}", rclone_path);
            return Ok(Self {
                rclone_path,
                using_system_rclone: false,
            });
        }

        // Если ничего не найдено - устанавливаем
        println!("rclone не найден. Начинаю установку...");
        Self::install_latest_rclone().await
    }

    // Публичный конструктор для тестов и создания экземпляра с заданными параметрами
    pub fn new_with_path(rclone_path: PathBuf, using_system_rclone: bool) -> Self {
        Self {
            rclone_path,
            using_system_rclone,
        }
    }

    // Геттеры
    pub fn get_rclone_path(&self) -> &PathBuf {
        &self.rclone_path
    }

    pub fn is_using_system_rclone(&self) -> bool {
        self.using_system_rclone
    }

    // Сеттеры
    pub fn set_rclone_path(&mut self, path: PathBuf) {
        self.rclone_path = path;
    }

    pub fn set_using_system_rclone(&mut self, value: bool) {
        self.using_system_rclone = value;
    }

    pub fn check_system_rclone() -> Option<PathBuf> {
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

    async fn install_latest_rclone() -> Result<Self, Box<dyn std::error::Error>> {
        println!("Начинаю установку последней версии rclone...");

        // Определяем ОС и архитектуру
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

        // Получаем информацию о последнем релизе
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
            .trim_start_matches('v');

        // Формируем URL для скачивания
        let download_url = format!(
            "https://github.com/rclone/rclone/releases/download/v{}/rclone-v{}-{}-{}.zip",
            version, version, os, arch
        );

        println!("Скачиваю rclone версии {} с: {}", version, download_url);

        // Скачиваем архив
        let response = client.get(&download_url).send().await?;
        if !response.status().is_success() {
            return Err(format!("Ошибка скачивания: HTTP {}", response.status()).into());
        }

        let bytes = response.bytes().await?;

        // Создаем директорию приложения
        let app_dir = dirs::data_dir()
            .ok_or("Не удалось найти папку для данных")?
            .join("rclone-ui");
        fs::create_dir_all(&app_dir)?;

        // Определяем имя исполняемого файла
        let rclone_filename = if cfg!(windows) {
            "rclone.exe"
        } else {
            "rclone"
        };
        let rclone_path = app_dir.join(rclone_filename);

        // Распаковываем архив
        use zip::read::ZipArchive;
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)?;

        let mut extracted = false;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_name = file.name().to_string();

            if file_name.ends_with("rclone") || file_name.ends_with("rclone.exe") {
                let mut contents = Vec::new();
                std::io::copy(&mut file, &mut contents)?;
                fs::write(&rclone_path, contents)?;
                extracted = true;
                println!("Найден и распакован: {}", file_name);
                break;
            }
        }

        if !extracted {
            return Err("Не удалось найти исполняемый файл rclone в архиве".into());
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
        }

        Ok(Self {
            rclone_path,
            using_system_rclone: false,
        })
    }

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

    pub fn version(&self) -> Result<String, String> {
        self.run_command(&["version"])
    }
}

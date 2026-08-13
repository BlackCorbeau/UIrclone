use reqwest;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

use super::RcloneApp;

impl RcloneApp {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Инициализация RcloneApp");
        
        // Сначала проверяем системный rclone
        if let Some(system_rclone) = Self::check_system_rclone() {
            log::info!("Найден системный rclone: {:?}", system_rclone);
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
            log::info!("Найден rclone в директории приложения: {:?}", rclone_path);
            return Ok(Self {
                rclone_path,
                using_system_rclone: false,
            });
        }

        // Если ничего не найдено - устанавливаем
        log::warn!("rclone не найден. Начинаю установку...");
        Self::install_latest_rclone().await
    }

    // Публичный конструктор для тестов и создания экземпляра с заданными параметрами
    pub fn new_with_path(rclone_path: PathBuf, using_system_rclone: bool) -> Self {
        log::debug!("Создание RcloneApp с путем {:?}, системный: {}", rclone_path, using_system_rclone);
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
        log::debug!("Изменение пути rclone с {:?} на {:?}", self.rclone_path, path);
        self.rclone_path = path;
    }

    pub fn set_using_system_rclone(&mut self, value: bool) {
        log::debug!("Изменение флага using_system_rclone с {} на {}", self.using_system_rclone, value);
        self.using_system_rclone = value;
    }

    pub fn check_system_rclone() -> Option<PathBuf> {
        log::debug!("Проверка системного rclone");
        if let Ok(path) = which::which("rclone") {
            log::debug!("Найден системный rclone по пути: {:?}", path);
            return Some(path);
        }

        #[cfg(windows)]
        {
            if let Ok(path) = which::which("rclone.exe") {
                log::debug!("Найден системный rclone.exe по пути: {:?}", path);
                return Some(path);
            }
        }

        log::debug!("Системный rclone не найден");
        None
    }

    async fn install_latest_rclone() -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Начинаю установку последней версии rclone...");

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

        log::info!("ОС: {}, архитектура: {}", os, arch);

        // Получаем информацию о последнем релизе
        let client = reqwest::Client::new();
        log::debug!("Запрос последней версии rclone с GitHub API");
        
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

        log::info!("Найдена последняя версия rclone: {}", version);

        // Формируем URL для скачивания
        let download_url = format!(
            "https://github.com/rclone/rclone/releases/download/v{}/rclone-v{}-{}-{}.zip",
            version, version, os, arch
        );

        log::info!("Скачиваю rclone с: {}", download_url);

        // Скачиваем архив
        let response = client.get(&download_url).send().await?;
        if !response.status().is_success() {
            let error_msg = format!("Ошибка скачивания: HTTP {}", response.status());
            log::error!("{}", error_msg);
            return Err(error_msg.into());
        }

        let bytes = response.bytes().await?;
        log::info!("Архив скачан, размер: {} байт", bytes.len());

        // Создаем директорию приложения
        let app_dir = dirs::data_dir()
            .ok_or("Не удалось найти папку для данных")?
            .join("rclone-ui");
        fs::create_dir_all(&app_dir)?;
        log::debug!("Директория приложения: {:?}", app_dir);

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
                log::debug!("Найден исполняемый файл в архиве: {}", file_name);
                let mut contents = Vec::new();
                std::io::copy(&mut file, &mut contents)?;
                fs::write(&rclone_path, contents)?;
                extracted = true;
                log::info!("Распакован: {} в {:?}", file_name, rclone_path);
                break;
            }
        }

        if !extracted {
            log::error!("Не удалось найти исполняемый файл rclone в архиве");
            return Err("Не удалось найти исполняемый файл rclone в архиве".into());
        }

        // Устанавливаем права на выполнение для Unix систем
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&rclone_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&rclone_path, perms)?;
            log::debug!("Установлены права на выполнение для {:?}", rclone_path);
        }

        log::info!("rclone успешно установлен в {:?}", rclone_path);

        // Проверяем установку
        let output = Command::new(&rclone_path)
            .arg("version")
            .output()
            .map_err(|e| format!("Ошибка проверки установки: {}", e))?;

        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            log::info!("Установленная версия rclone:\n{}", version_output);
        } else {
            log::warn!("Не удалось проверить версию установленного rclone");
        }

        Ok(Self {
            rclone_path,
            using_system_rclone: false,
        })
    }

    pub fn run_command(&self, args: &[&str]) -> Result<String, String> {
        log::debug!("Выполнение команды rclone: {} {:?}", self.rclone_path.display(), args);
        
        let output = Command::new(&self.rclone_path)
            .args(args)
            .output()
            .map_err(|e| {
                log::error!("Ошибка запуска rclone: {}", e);
                format!("Ошибка запуска: {}", e)
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            log::debug!("Команда выполнена успешно, вывод: {} байт", stdout.len());
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            log::error!("Ошибка выполнения команды: {}", stderr);
            Err(stderr)
        }
    }

    /// Запуск команды с передачей строк stderr в колбэк (например, для показа
    /// ссылки авторизации rclone во время OAuth).
    pub fn run_command_with_stderr_feed(
        &self,
        args: &[&str],
        mut on_stderr: impl FnMut(&str),
    ) -> Result<String, String> {
        use std::io::{BufRead, Read};
        use std::process::{Command, Stdio};

        log::debug!("Выполнение команды rclone (с потоком stderr): {} {:?}", self.rclone_path.display(), args);

        let mut child = Command::new(&self.rclone_path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                log::error!("Ошибка запуска rclone: {}", e);
                format!("Ошибка запуска: {}", e)
            })?;

        let stderr = child.stderr.take().expect("stderr должен быть доступен");
        let mut stderr_text = String::new();
        {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        on_stderr(l.trim());
                        stderr_text.push_str(&l);
                        stderr_text.push('\n');
                    }
                    Err(_) => break,
                }
            }
        }

        let status = child.wait().map_err(|e| format!("Ошибка ожидания rclone: {}", e))?;

        let mut stdout = String::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut stdout);
        }

        if status.success() {
            Ok(stdout)
        } else {
            let err = if stderr_text.trim().is_empty() {
                stdout
            } else {
                stderr_text
            };
            log::error!("Ошибка выполнения команды: {}", err);
            Err(err)
        }
    }

    pub fn version(&self) -> Result<String, String> {
        log::debug!("Получение версии rclone");
        self.run_command(&["version"])
    }
}

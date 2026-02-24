use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

        if !rclone_path.exists() {
            Self::extract_rclone(&rclone_path)?;
        }

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

    fn extract_rclone(dest_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        println!("Устанавливаю rclone в {:?}", dest_path);

        let rclone_bytes = if cfg!(windows) {
            include_bytes!("../bin/rclone.exe")
        } else if cfg!(target_os = "linux") {
            include_bytes!("../bin/rclone")
        } else if cfg!(target_os = "macos") {
            include_bytes!("../bin/rclone")
        } else {
            panic!("Неподдерживаемая ОС");
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
}

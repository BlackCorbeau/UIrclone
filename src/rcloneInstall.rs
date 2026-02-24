use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct RcloneApp {
    rclone_path: PathBuf,
}

impl RcloneApp {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
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

        Ok(Self { rclone_path })
    }
}

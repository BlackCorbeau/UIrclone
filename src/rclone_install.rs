use std::path::PathBuf;

mod rclone_install_real;

pub struct RcloneApp {
    rclone_path: PathBuf,
    using_system_rclone: bool,
}

#[cfg(test)]
mod rclone_install_tests;

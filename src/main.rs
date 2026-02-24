mod rcloneInstall;

use rcloneInstall::RcloneApp;

fn main() {
    let _app = match RcloneApp::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Ошибка инициализации: {}", e);
            panic!("{}", e);
        }
    };
}

mod rclone_install;

use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;

    let rclone = rt.block_on(async {
        let rclone = rclone_install::RcloneApp::new().await?;
        println!("rclone готов к использованию");
        Ok::<_, Box<dyn std::error::Error>>(rclone)
    })?;

    let v = rclone.version().unwrap().to_string();
    println!("Rclone: {}", v);
    Ok::<_, Box<dyn std::error::Error>>(())
}

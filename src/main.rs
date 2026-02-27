mod rcloneInstall;

//use rcloneInstall::RcloneApp;

use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(rcloneInstall::install_latest_rclone())?;
    Ok(())
}

use crate::rclone_install::RcloneApp;

/// Проверить разницу между двумя директориями
pub fn diff(app: &RcloneApp, source: &str, dest: &str) -> Result<Vec<String>, String> {
    let output = app.run_command(&["check", source, dest, "--combined", "-"])?;

    let mut differences = Vec::new();
    for line in output.lines() {
        if !line.is_empty() {
            differences.push(line.to_string());
        }
    }

    Ok(differences)
}

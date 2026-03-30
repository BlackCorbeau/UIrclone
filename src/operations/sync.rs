use super::*;
use crate::rclone_install::RcloneApp;

/// Скопировать файлы/директории
pub fn copy(
    app: &RcloneApp,
    source: &str,
    dest: &str,
    options: &CopyOptions,
) -> Result<SyncStats, String> {
    let mut args = vec!["copy", source, dest, "--progress", "--stats-one-line"];

    if options.verbose {
        args.push("-v");
    }
    if options.dry_run {
        args.push("--dry-run");
    }
    if let Some(bandwidth) = &options.bandwidth_limit {
        args.push("--bwlimit");
        args.push(bandwidth.as_str());
    }
    if options.no_traverse {
        args.push("--no-traverse");
    }

    let output = app.run_command(&args)?;
    parse_sync_stats(&output)
}

/// Синхронизировать директории (dest станет идентичной source)
pub fn sync(
    app: &RcloneApp,
    source: &str,
    dest: &str,
    options: &SyncOptions,
) -> Result<SyncStats, String> {
    let mut args = vec!["sync", source, dest, "--progress", "--stats-one-line"];

    if options.verbose {
        args.push("-v");
    }
    if options.dry_run {
        args.push("--dry-run");
    }
    if options.delete_excluded {
        args.push("--delete-excluded");
    }
    if let Some(bandwidth) = &options.bandwidth_limit {
        args.push("--bwlimit");
        args.push(bandwidth.as_str());
    }
    if options.existing_only {
        args.push("--existing-only");
    }

    let output = app.run_command(&args)?;
    parse_sync_stats(&output)
}

/// Переместить файлы/директории
pub fn move_files(
    app: &RcloneApp,
    source: &str,
    dest: &str,
    options: &MoveOptions,
) -> Result<SyncStats, String> {
    let mut args = vec!["move", source, dest, "--progress", "--stats-one-line"];

    if options.verbose {
        args.push("-v");
    }
    if options.dry_run {
        args.push("--dry-run");
    }
    if options.delete_empty_src_dirs {
        args.push("--delete-empty-src-dirs");
    }

    let output = app.run_command(&args)?;
    parse_sync_stats(&output)
}

/// Удалить файлы/директории
pub fn delete(app: &RcloneApp, path: &str, options: &DeleteOptions) -> Result<SyncStats, String> {
    let command = if options.recursive { "purge" } else { "delete" };

    let mut args = vec![command, path, "--progress"];

    if options.verbose {
        args.push("-v");
    }
    if options.dry_run {
        args.push("--dry-run");
    }

    let output = app.run_command(&args)?;
    parse_sync_stats(&output)
}

/// Парсинг статистики синхронизации из вывода
fn parse_sync_stats(output: &str) -> Result<SyncStats, String> {
    let mut stats = SyncStats {
        transferred: 0,
        files: 0,
        errors: 0,
        checks: 0,
        elapsed_time: 0.0,
        transfer_speed: 0.0,
    };

    // Парсим последнюю строку со статистикой
    for line in output.lines().rev() {
        if line.contains("Transferred:") {
            // Пример: "Transferred:   	        5 / 5, 100%, 1.234 MB/s, ETA 0s"
            if let Some(speed_part) = line.split(',').nth(2) {
                if speed_part.contains("MB/s") {
                    let speed_str = speed_part.trim().replace("MB/s", "").trim().to_string();
                    if let Ok(speed_mb) = speed_str.parse::<f64>() {
                        stats.transfer_speed = speed_mb * 1024.0 * 1024.0;
                    }
                }
            }
            break;
        }
    }

    Ok(stats)
}

/// Получить список файлов, которые будут затронуты при синхронизации (dry-run)
pub fn preview(app: &RcloneApp, source: &str, dest: &str) -> Result<Vec<String>, String> {
    let output = app.run_command(&["copy", source, dest, "--dry-run", "--progress"])?;

    let mut files = Vec::new();
    for line in output.lines() {
        if line.contains(":") && !line.contains("Transferred:") && !line.contains("Errors:") {
            files.push(line.to_string());
        }
    }

    Ok(files)
}

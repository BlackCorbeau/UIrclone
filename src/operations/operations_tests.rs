use crate::operations::remotes::{config_create_step, ConfigStep};
use crate::operations::{FileInfo, SyncStats};
use crate::rclone_install::RcloneApp;
use std::fs;
use tempfile::tempdir;

#[cfg(not(windows))]
fn write_mock_rclone(script: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let path = temp_dir.path().join("rclone");
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    (temp_dir, path)
}

#[test]
fn test_file_info_formatting() {
    let file = FileInfo {
        name: "test.txt".to_string(),
        path: "/path/test.txt".to_string(),
        size: 1500,
        is_dir: false,
        modified: None,
    };

    assert_eq!(file.format_size(), "1.5 KB");
    assert_eq!(file.icon(), "📄");
}

#[test]
fn test_sync_stats_formatting() {
    let stats = SyncStats {
        transferred: 1024,
        files: 10,
        errors: 0,
        checks: 100,
        elapsed_time: 5.0,
        transfer_speed: 2_500_000.0, // Добавлен .0 для f64
    };

    assert_eq!(stats.format_speed(), "2.4 MB/s");
}

#[test]
#[cfg(not(windows))]
fn test_config_create_step_returns_question() {
    let script = r#"#!/bin/sh
cat <<'EOF'
{
    "State": "some-state",
    "Option": {
        "Name": "config_is_local",
        "Help": "Use web browser to automatically authenticate rclone with remote?",
        "Default": true,
        "DefaultStr": "true",
        "IsPassword": false,
        "Examples": [
            {"Value": "true", "Help": "Yes"},
            {"Value": "false", "Help": "No"}
        ]
    },
    "Error": "",
    "Result": ""
}
EOF
"#;
    let (_dir, path) = write_mock_rclone(script);
    let app = RcloneApp::new_with_path(path, false);

    let mut stderr_lines: Vec<String> = Vec::new();
    let step = config_create_step(&app, "mydrive", "drive", None, None, &mut |l| {
        stderr_lines.push(l.to_string())
    })
    .unwrap();

    match step {
        ConfigStep::Question { state, question } => {
            assert_eq!(state, "some-state");
            assert_eq!(question.name, "config_is_local");
            assert!(question.help.contains("web browser"));
            assert_eq!(question.default, "true");
            assert!(!question.is_password);
            assert_eq!(question.examples.len(), 2);
            assert_eq!(question.examples[0], ("true".to_string(), "Yes".to_string()));
        }
        _ => panic!("Ожидался вопрос, получено другое"),
    }
    assert!(stderr_lines.is_empty());
}

#[test]
#[cfg(not(windows))]
fn test_config_create_step_done() {
    let script = r#"#!/bin/sh
cat <<'EOF'
{
    "State": "",
    "Option": null,
    "Error": "",
    "Result": ""
}
EOF
"#;
    let (_dir, path) = write_mock_rclone(script);
    let app = RcloneApp::new_with_path(path, false);

    let step = config_create_step(&app, "local1", "local", None, None, &mut |_| {}).unwrap();
    assert!(matches!(step, ConfigStep::Done));
}

#[test]
#[cfg(not(windows))]
fn test_config_create_step_passes_continue_args() {
    let script = r#"#!/bin/sh
if [ "$1" = "config" ] && printf '%s\n' "$@" | grep -q -- "--continue" \
    && printf '%s\n' "$@" | grep -q -- "--state" \
    && printf '%s\n' "$@" | grep -q -- "--result"; then
    cat <<'EOF'
{"State": "next", "Option": null, "Error": "", "Result": ""}
EOF
else
    echo "no continue args" >&2
    exit 1
fi
"#;
    let (_dir, path) = write_mock_rclone(script);
    let app = RcloneApp::new_with_path(path, false);

    let step = config_create_step(
        &app,
        "mydrive",
        "drive",
        Some("st"),
        Some("answer"),
        &mut |_| {},
    )
    .unwrap();
    assert!(matches!(step, ConfigStep::Done));
}

#[test]
#[cfg(not(windows))]
fn test_config_create_step_error_from_stderr() {
    let script = r#"#!/bin/sh
echo "Error: couldn't find backend for type \"bogus\"" >&2
exit 1
"#;
    let (_dir, path) = write_mock_rclone(script);
    let app = RcloneApp::new_with_path(path, false);

    let result = config_create_step(&app, "x", "bogus", None, None, &mut |_| {});
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("couldn't find backend"));
}

#[test]
#[cfg(not(windows))]
fn test_config_create_step_error_from_json() {
    let script = r#"#!/bin/sh
cat <<'EOF'
{"State": "", "Option": null, "Error": "config already exists", "Result": ""}
EOF
"#;
    let (_dir, path) = write_mock_rclone(script);
    let app = RcloneApp::new_with_path(path, false);

    let result = config_create_step(&app, "x", "drive", None, None, &mut |_| {});
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already exists"));
}

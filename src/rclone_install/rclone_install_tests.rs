use super::RcloneApp;
use serial_test::serial;
use std::env;
use std::fs;
use tempfile::tempdir;

fn setup_test_dir() -> tempfile::TempDir {
    tempdir().expect("Failed to create temp dir")
}

#[test]
fn test_new_with_path_constructor() {
    let temp_dir = setup_test_dir();
    let rclone_path = temp_dir.path().join("rclone");

    let app = RcloneApp::new_with_path(rclone_path.clone(), true);

    assert_eq!(app.get_rclone_path(), &rclone_path);
    assert!(app.is_using_system_rclone());
}

#[test]
fn test_getters() {
    let temp_dir = setup_test_dir();
    let rclone_path = temp_dir.path().join("rclone");

    let app = RcloneApp::new_with_path(rclone_path.clone(), true);

    assert_eq!(app.get_rclone_path(), &rclone_path);
    assert!(app.is_using_system_rclone());
}

#[test]
fn test_setters() {
    let temp_dir = setup_test_dir();
    let rclone_path1 = temp_dir.path().join("rclone1");
    let rclone_path2 = temp_dir.path().join("rclone2");

    let mut app = RcloneApp::new_with_path(rclone_path1, false);

    assert!(!app.is_using_system_rclone());

    app.set_using_system_rclone(true);
    assert!(app.is_using_system_rclone());

    app.set_rclone_path(rclone_path2.clone());
    assert_eq!(app.get_rclone_path(), &rclone_path2);
}

#[test]
fn test_version_method_with_nonexistent_rclone() {
    let temp_dir = setup_test_dir();
    let rclone_path = if cfg!(windows) {
        temp_dir.path().join("rclone.exe")
    } else {
        temp_dir.path().join("rclone")
    };

    let app = RcloneApp::new_with_path(rclone_path, false);

    let result = app.version();
    assert!(result.is_err());
}

#[test]
fn test_version_method_with_mock_rclone() {
    let temp_dir = setup_test_dir();
    let rclone_path = if cfg!(windows) {
        temp_dir.path().join("rclone.exe")
    } else {
        temp_dir.path().join("rclone")
    };

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&rclone_path, "#!/bin/sh\necho 'rclone v1.60.0'").unwrap();
        let mut perms = fs::metadata(&rclone_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&rclone_path, perms).unwrap();
    }

    #[cfg(windows)]
    {
        fs::write(&rclone_path, "").unwrap();
    }

    let app = RcloneApp::new_with_path(rclone_path, false);

    if cfg!(not(windows)) {
        let result = app.version();
        println!("Version result: {:?}", result);
    }
}

#[test]
fn test_using_system_rclone_flag() {
    let temp_dir = setup_test_dir();
    let rclone_path = temp_dir.path().join("rclone");

    let app = RcloneApp::new_with_path(rclone_path.clone(), true);
    assert!(app.is_using_system_rclone());

    let app2 = RcloneApp::new_with_path(rclone_path, false);
    assert!(!app2.is_using_system_rclone());
}

#[test]
fn test_multiple_instances() {
    let temp_dir = setup_test_dir();

    let app1 = RcloneApp::new_with_path(temp_dir.path().join("rclone1"), true);
    let app2 = RcloneApp::new_with_path(temp_dir.path().join("rclone2"), false);
    let app3 = RcloneApp::new_with_path(temp_dir.path().join("rclone3"), true);

    assert!(app1.is_using_system_rclone());
    assert!(!app2.is_using_system_rclone());
    assert!(app3.is_using_system_rclone());

    assert_ne!(app1.get_rclone_path(), app2.get_rclone_path());
}

#[tokio::test]
#[serial]
async fn test_check_system_rclone() {
    let result = RcloneApp::check_system_rclone();
    println!("System rclone found: {:?}", result);
}

#[test]
fn test_path_manipulation() {
    let temp_dir = setup_test_dir();
    let original_path = temp_dir.path().join("original_rclone");
    let new_path = temp_dir.path().join("new_rclone");

    let mut app = RcloneApp::new_with_path(original_path.clone(), false);
    assert_eq!(app.get_rclone_path(), &original_path);

    app.set_rclone_path(new_path.clone());
    assert_eq!(app.get_rclone_path(), &new_path);
}

#[test]
fn test_os_arch_detection() {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        "unknown"
    };

    assert_ne!(os, "unknown", "ОС должна быть определена");
    assert_ne!(arch, "unknown", "Архитектура должна быть определена");

    println!("OS: {}, ARCH: {}", os, arch);
}

#[tokio::test]
#[ignore]
async fn test_actual_rclone_installation() {
    match RcloneApp::new().await {
        Ok(app) => {
            assert!(app.get_rclone_path().exists());
            let version = app.version();
            assert!(version.is_ok());
            println!("Установлен rclone по пути: {:?}", app.get_rclone_path());
            println!("Версия: {:?}", version);
            println!(
                "Используется системный rclone: {}",
                app.is_using_system_rclone()
            );
        }
        Err(e) => {
            panic!("Не удалось установить rclone: {}", e);
        }
    }
}

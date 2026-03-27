use crate::operations::{FileInfo, SyncStats};

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

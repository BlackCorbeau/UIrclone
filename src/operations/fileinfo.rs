use super::*;
impl FileInfo {
    pub fn format_size(&self) -> String {
        let bytes = self.size;
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.is_dir { "📁" } else { "📄" }
    }
}

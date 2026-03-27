//! Rclone UI Manager — графический интерфейс для rclone.
//!
//! Приложение предоставляет кроссплатформенную GUI для управления удалёнными
//! хранилищами rclone, просмотра файлов и выполнения операций копирования,
//! синхронизации, перемещения.

mod rclone_install;
mod operations;
mod ui;

use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};

/// Точка входа в приложение.
///
/// Загружает иконку окна, подключает пользовательские шрифты (Sans.otf и NotoColorEmoji.ttf)
/// и запускает нативное окно eframe с приложением RcloneUI.
fn main() -> Result<(), eframe::Error> {
    // Попытка загрузить иконку окна из assets/icon.png
    let icon = std::fs::read("assets/icon.png")
        .ok()
        .and_then(|bytes| eframe::icon_data::from_png_bytes(&bytes).ok())
        .map(std::sync::Arc::new);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Rclone UI Manager");

    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Rclone UI Manager",
        options,
        Box::new(|cc| {
            // Настройка пользовательских шрифтов
            let mut fonts = FontDefinitions::default();

            // 1. Основной шрифт без засечек (Sans.otf) делаем шрифтом по умолчанию для пропорционального текста.
            if let Ok(font_data) = std::fs::read("assets/fonts/Sans.otf") {
                fonts.font_data.insert(
                    "sans".to_owned(),
                    FontData::from_owned(font_data),
                );
                if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
                    proportional.insert(0, "sans".to_owned());
                } else {
                    fonts.families.insert(
                        FontFamily::Proportional,
                        vec!["sans".to_owned()],
                    );
                }
            }

            // 2. Шрифт с эмодзи (NotoColorEmoji.ttf) подключаем как запасной.
            if let Ok(emoji_data) = std::fs::read("assets/fonts/NotoColorEmoji.ttf") {
                fonts.font_data.insert(
                    "emoji".to_owned(),
                    FontData::from_owned(emoji_data),
                );
                if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
                    if !proportional.contains(&"emoji".to_owned()) {
                        proportional.push("emoji".to_owned());
                    }
                }
            }

            // Применяем настройки шрифтов к egui-контексту.
            cc.egui_ctx.set_fonts(fonts);

            Box::new(ui::RcloneUI::new(cc))
        }),
    )
}

mod rclone_install;
mod operations;
mod ui;

use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};

fn main() -> Result<(), eframe::Error> {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Rclone UI Manager");

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Rclone UI Manager",
        options,
        Box::new(|cc| {
            let mut fonts = FontDefinitions::default();
            
            if let Ok(font_data) = std::fs::read("assets/fonts/Sans.otf") {
                fonts.font_data.insert("sans".to_owned(), FontData::from_owned(font_data));
                fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "sans".to_owned());
            }

            if let Ok(emoji_data) = std::fs::read("assets/fonts/NotoColorEmoji.ttf") {
                fonts.font_data.insert("emoji".to_owned(), FontData::from_owned(emoji_data));
                let prop = fonts.families.entry(FontFamily::Proportional).or_default();
                if !prop.contains(&"emoji".to_owned()) {
                    prop.push("emoji".to_owned());
                }
            }

            cc.egui_ctx.set_fonts(fonts);
            Box::new(ui::RcloneUI::new(cc))
        }),
    )
}

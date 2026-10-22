mod app;
mod gui;

use ferriso_gui::Settings;

fn main() -> ferriso_gui::iced::Result {
    ferriso_gui::run(
        app::Dashboard::new(),
        gui::HerdrGui::new(),
        Settings {
            title: "herdr · gui".into(),
            size: (1200.0, 780.0),
            application_id: "io.github.gblmx.herdr-gui".into(),
        },
    )
}

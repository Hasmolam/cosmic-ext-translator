use cosmic_applet_translator::{app, config, i18n};

fn main() -> cosmic::iced::Result {
    let is_applet = std::env::args().any(|arg| arg == "--applet");
    // Panel applet and settings window run as separate processes; separate files keep
    // their lines apart.
    if let Some(path) =
        translator_protocol::logging::init(if is_applet { "applet" } else { "settings" })
    {
        tracing::info!("Logging to {}", path.display());
    }
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);
    config::migrate_legacy_config();

    if is_applet {
        cosmic::applet::run::<app::AppModel>(())
    } else {
        let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(900.0, 700.0));
        cosmic::app::run::<app::AppModel>(settings, ())
    }
}

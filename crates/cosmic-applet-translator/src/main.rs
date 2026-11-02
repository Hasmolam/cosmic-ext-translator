use cosmic_applet_translator::{app, i18n};

fn main() -> cosmic::iced::Result {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    if std::env::args().any(|arg| arg == "--applet") {
        cosmic::applet::run::<app::AppModel>(())
    } else {
        let settings = cosmic::app::Settings::default()
            .size(cosmic::iced::Size::new(900.0, 700.0));
        cosmic::app::run::<app::AppModel>(settings, ())
    }
}

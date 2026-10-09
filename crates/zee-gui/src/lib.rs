#![recursion_limit = "512"]

pub mod app;
pub mod window_view;
pub mod widgets;
pub mod workspace;

use gpui::*;

/// Launch and run the Zee hardware-accelerated desktop GUI application.
pub fn run_gui() {
    let (tx, rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    let app = gpui_platform::application();
    
    let tx_urls = tx.clone();
    app.on_open_urls(move |urls| {
        let _ = tx_urls.unbounded_send(urls);
    });

    app.on_reopen(move |cx| {
        if cx.windows().is_empty() {
            let mut config = zee_core::config::Config::load();
            config.vi_mode = false;
            let i18n = zee_core::i18n::I18n::load(&config.language);
            crate::app::new_window(config, i18n, cx);
        }
    });

    app.run(|app: &mut App| {
        crate::app::setup_app(app, rx);
    });
}

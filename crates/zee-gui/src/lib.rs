#![recursion_limit = "512"]

pub mod app;
pub mod window_view;
pub mod widgets;
pub mod workspace;

use gpui::*;

/// Launch and run the Zee hardware-accelerated desktop GUI application.
pub fn run_gui() {
    let _lock = match zee_core::single_instance::SingleInstanceLock::try_acquire("zee-gui") {
        Some(lock) => lock,
        None => {
            // An instance is already running!
            // On macOS, activate the running application and pass any target files
            #[cfg(target_os = "macos")]
            {
                let raw_args: Vec<String> = std::env::args().skip(1).collect();
                let cli_targets = zee_core::cli::parse_file_targets(&raw_args);
                let mut cmd = std::process::Command::new("open");
                cmd.arg("-a").arg("Zee");
                for target in cli_targets {
                    cmd.arg(&target.path);
                }
                let _ = cmd.spawn();
            }
            return;
        }
    };

    let (tx, rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    let app = gpui_platform::application();
    
    let tx_urls = tx.clone();
    app.on_open_urls(move |urls| {
        let _ = tx_urls.unbounded_send(urls);
    });

    app.on_reopen(move |cx| {
        if cx.windows().is_empty() && !zee_core::session::UpdateSession::exists() {
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

pub mod app;
pub mod renderer;
pub mod layout;
pub mod clipboard;
pub mod widgets;

use anyhow::Result;

/// Run the Zee Terminal User Interface (CLI/TUI) with the provided argument list.
pub fn run_tui(args: Vec<String>) -> Result<()> {
    let targets = zee_core::cli::parse_file_targets(&args);
    let mut app = app::App::with_targets(targets)?;
    app.run()
}

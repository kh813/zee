mod app;
mod renderer;
mod layout;
mod clipboard;
mod widgets;

use app::App;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let targets = zee_core::cli::parse_file_targets(&args);
    
    let mut app = App::with_targets(targets)?;
    app.run()
}

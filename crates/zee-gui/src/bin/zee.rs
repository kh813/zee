#![windows_subsystem = "windows"]

use std::io::IsTerminal;
use anyhow::Result;

fn main() -> Result<()> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Check for explicit --gui or -g flag
    let mut force_gui = false;
    let mut filtered_args = Vec::new();
    for arg in raw_args {
        if arg == "--gui" || arg == "-g" {
            force_gui = true;
        } else {
            filtered_args.push(arg);
        }
    }

    // Check if running in a terminal emulator
    let is_terminal = std::io::stdin().is_terminal();

    // On Linux/Unix desktop, check DISPLAY or WAYLAND_DISPLAY
    #[cfg(target_os = "linux")]
    let has_display = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();
    #[cfg(not(target_os = "linux"))]
    let has_display = true;

    // Determine whether to launch GUI or CLI:
    // 1. Explicit --gui / -g flag -> GUI
    // 2. Not in a terminal (e.g. launched from .desktop or file manager) and display available -> GUI
    // 3. Otherwise (in terminal without --gui) -> CLI
    let should_launch_gui = force_gui || (!is_terminal && has_display);

    if should_launch_gui {
        zee_gui::run_gui();
        Ok(())
    } else {
        zee_tui::run_tui(filtered_args)
    }
}

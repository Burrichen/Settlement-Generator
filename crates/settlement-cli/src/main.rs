//! `settlement-cli`: terminal interface for Settlement Generator.
//!
//! With no arguments this launches the full-screen TUI. All data and rules
//! come from `settlement-core` (the same service the GUI uses); this binary
//! only handles input and presentation.

mod tui;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "settlement-generator-cli",
    about = "Settlement Generator (terminal interface)",
    version = settlement_core::VERSION
)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    tui::run()
}

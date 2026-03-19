use std::path::PathBuf;

use clap::Parser;

mod dts;
mod tui;
mod west;

#[derive(Parser)]
#[command(name = "zdtwalk")]
#[command(about = "Device Tree Source TUI explorer for Zephyr projects")]
#[command(version)]
struct Cli {
    /// Path to the Zephyr workspace root (auto-discovered if omitted).
    #[arg(long)]
    workspace: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = tui::run_tui(cli.workspace).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

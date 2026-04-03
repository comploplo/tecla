//! Tecla keyboard firmware CLI tool.
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tecla", about = "rmk keyboard firmware toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch a QMK keyboard definition and flash rmk firmware.
    Flash {
        /// QMK keyboard identifier (e.g. crkbd, lily58, planck/rev6)
        #[arg(long)]
        keyboard: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Flash { keyboard } => {
            eprintln!("flash: {keyboard}");
        }
    }
}

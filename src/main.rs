use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dedupfs")]
#[command(about = "A local content-aware storage engine")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// initializing a DedupFS repository
    Init,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            println!("DedupFS initialization will be implemented here.");
        }
    }
}

mod cmd;
mod opts;

use clap::Parser;
use mimalloc::MiMalloc;
use opts::{TempoBench, TempoBenchSubcommand};
use tracing_subscriber::EnvFilter;

#[global_allocator]
// Increases RPS by ~5.5% at the time of
// writing. ~3.3% faster than jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Initialize tracing subscriber
    // Default level is info, can be overridden with RUST_LOG env var
    // e.g., RUST_LOG=debug tempo-bench run-max-tps ...
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    let args = TempoBench::parse();

    match args.cmd {
        TempoBenchSubcommand::RunMaxTps(cmd) => cmd.run().await,
    }
}

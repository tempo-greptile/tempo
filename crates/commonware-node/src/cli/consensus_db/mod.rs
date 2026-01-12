//! `tempo consensus-db` command - utilities for the commonware consensus database.

mod stats;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Consensus database utilities for inspecting commonware storage.
#[derive(Debug, Parser)]
pub struct Command {
    /// The path to the data directory.
    #[arg(long, value_name = "DATA_DIR", global = true, default_value_t)]
    datadir: PlatformPath,

    /// The chain this node is running.
    #[arg(long, value_name = "CHAIN", global = true, default_value = "tempo")]
    chain: String,

    #[command(subcommand)]
    command: Subcommands,
}

/// Wrapper for platform-specific default data directory.
#[derive(Debug, Clone)]
struct PlatformPath(PathBuf);

impl Default for PlatformPath {
    fn default() -> Self {
        Self(
            dirs::data_dir()
                .map(|p| p.join("reth"))
                .unwrap_or_else(|| PathBuf::from(".reth")),
        )
    }
}

impl std::fmt::Display for PlatformPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl std::str::FromStr for PlatformPath {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(PathBuf::from(s)))
    }
}

#[derive(Debug, Subcommand)]
enum Subcommands {
    /// Display statistics about the consensus database.
    Stats(stats::Command),
}

impl Command {
    /// Execute the `consensus-db` command.
    pub fn execute(self) -> eyre::Result<()> {
        // Resolve the consensus storage path
        let data_dir = self.datadir.0.join(&self.chain);
        let consensus_dir = data_dir.join("consensus");

        match self.command {
            Subcommands::Stats(cmd) => cmd.execute(&consensus_dir),
        }
    }
}

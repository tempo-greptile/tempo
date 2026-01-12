//! `tempo consensus-db stats` command implementation.

use clap::Parser;
use comfy_table::{Cell, Row, Table as ComfyTable};
use human_bytes::human_bytes;
use std::{fs, path::Path};

/// Display statistics about the consensus database.
#[derive(Debug, Parser)]
pub(crate) struct Command;

impl Command {
    pub(crate) fn execute(self, consensus_dir: &Path) -> eyre::Result<()> {
        if !consensus_dir.exists() {
            eyre::bail!(
                "Consensus directory does not exist: {}\n\
                 Run the node with consensus enabled first.",
                consensus_dir.display()
            );
        }

        println!("Consensus Storage Statistics");
        println!("Path: {}\n", consensus_dir.display());

        // Get block count from ordinal file
        let blocks_ordinal = consensus_dir.join("engine-finalized_blocks-ordinal");
        let block_count = ordinal_count(&blocks_ordinal);

        // Collect partition stats
        let mut partitions: Vec<(String, u64)> = fs::read_dir(consensus_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let size = dir_size(&e.path());
                (name, size)
            })
            .collect();
        partitions.sort_by(|a, b| a.0.cmp(&b.0));

        // Build table
        let mut table = ComfyTable::new();
        table.load_preset(comfy_table::presets::ASCII_MARKDOWN);
        table.set_header(["Partition", "Size"]);

        for (name, size) in &partitions {
            table.add_row([name, &human_bytes(*size as f64)]);
        }

        add_separator(&mut table);
        let total: u64 = partitions.iter().map(|(_, s)| s).sum();
        table.add_row(["Total", &human_bytes(total as f64)]);

        println!("{table}\n");

        // Per-entry stats if we have blocks
        if let Some(count) = block_count {
            let blocks_journal = dir_size(&consensus_dir.join("engine-finalized_blocks-freezer-journal"));
            let certs_journal = dir_size(&consensus_dir.join("engine-finalizations-by-height-freezer-journal"));

            println!("Per-Entry Statistics ({} blocks):\n", count);

            let mut entry_table = ComfyTable::new();
            entry_table.load_preset(comfy_table::presets::ASCII_MARKDOWN);
            entry_table.set_header(["Data", "Total", "Per Entry"]);

            if blocks_journal > 0 {
                entry_table.add_row([
                    "Blocks (journal)",
                    &human_bytes(blocks_journal as f64),
                    &format!("{} B", blocks_journal / count),
                ]);
            }
            if certs_journal > 0 {
                entry_table.add_row([
                    "Finalization Certs",
                    &human_bytes(certs_journal as f64),
                    &format!("{} B", certs_journal / count),
                ]);
            }

            println!("{entry_table}");
        }

        Ok(())
    }
}

/// Get entry count from ordinal directory (u64 per entry).
fn ordinal_count(path: &Path) -> Option<u64> {
    if !path.exists() {
        return None;
    }
    let size: u64 = fs::read_dir(path).ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| fs::metadata(e.path()).ok())
        .map(|m| m.len())
        .sum();
    if size > 0 { Some(size / 8) } else { None }
}

/// Recursively calculate directory size.
fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut size = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                size += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                size += dir_size(&p);
            }
        }
    }
    size
}

/// Add a separator row.
fn add_separator(table: &mut ComfyTable) {
    let widths = table.column_max_content_widths();
    let mut row = Row::new();
    for w in widths {
        row.add_cell(Cell::new("-".repeat(w as usize)));
    }
    table.add_row(row);
}

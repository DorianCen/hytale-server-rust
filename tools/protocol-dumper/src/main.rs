//! `protocol-dumper` — stages 1 and 2 of the Hytale protocol pipeline.
//!
//! `inventory` walks the auto-generated Java protocol sources and emits a
//! deterministic class inventory (kind, packet id, direction, channel, sizes).
//! `connection` parses the `packets/connection` family into a field-level IR
//! used by `protocol-gen`.
//!
//! The tool never edits the Java sources; it only reads them.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use protocol_dumper::{connection, inventory, DumperError, Result};

/// Command-line interface.
#[derive(Debug, Parser)]
#[command(name = "protocol-dumper", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Emit a class inventory for the generated protocol tree.
    Inventory {
        /// Protocol source directory.
        #[arg(long)]
        source: PathBuf,
        /// Output JSON path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Emit the field-level IR for the connection packet family.
    Connection {
        /// `packets/connection` source directory.
        #[arg(long)]
        source: PathBuf,
        /// Output JSON path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Emit the field-level IR for the whole generated protocol tree.
    All {
        /// `com/hypixel/hytale/protocol` source directory.
        #[arg(long)]
        source: PathBuf,
        /// Output JSON path.
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("protocol-dumper: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inventory { source, out } => {
            let inventory = inventory::build(&source)?;
            write_json(&out, &inventory)?;
            let counts = &inventory.meta.counts;
            println!(
                "packets={} structs={} enums={} other={} total={}",
                counts.packets, counts.structs, counts.enums, counts.other, counts.total
            );
            for discrepancy in &inventory.meta.discrepancies {
                println!("note: {discrepancy}");
            }
        }
        Command::Connection { source, out } => {
            let ir = connection::build(&source)?;
            write_json(&out, &ir)?;
            println!("packets={} enums={}", ir.packets.len(), ir.enums.len());
        }
        Command::All { source, out } => {
            let ir = connection::build_all(&source)?;
            write_json(&out, &ir)?;
            println!(
                "packets={} structs={} enums={}",
                ir.packets.len(),
                ir.structs.len(),
                ir.enums.len()
            );
        }
    }
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| DumperError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    std::fs::write(path, json).map_err(|source| DumperError::Io {
        path: path.to_path_buf(),
        source,
    })
}

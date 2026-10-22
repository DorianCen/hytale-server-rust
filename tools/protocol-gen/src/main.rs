//! `protocol-gen` CLI.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use protocol_gen::{check, generate_all, generate_connection, write_files};

const DEFAULT_IR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/golden/server/protocol/protocol.json"
);
const DEFAULT_CONNECTION_IR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/golden/server/protocol/connection.json"
);
const DEFAULT_OUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/hytale-protocol/src/generated"
);

/// Command-line interface.
#[derive(Debug, Parser)]
#[command(name = "protocol-gen", version, about)]
struct Cli {
    /// Regenerate to a temporary directory and diff the committed sources.
    #[arg(long)]
    check: bool,
    /// Protocol IR input (used by `all` and `--check`).
    #[arg(long, global = true, default_value = DEFAULT_IR)]
    ir: PathBuf,
    /// Generated source directory (used by `all` and `--check`).
    #[arg(long, global = true, default_value = DEFAULT_OUT)]
    out: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate every packet family plus the registry and protocol settings.
    All,
    /// Generate the connection packet family.
    Connection,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("protocol-gen: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), protocol_gen::GenError> {
    let cli = Cli::parse();
    if cli.check {
        let files = generate_all(&cli.ir)?;
        let differences = check(&cli.out, &files)?;
        if differences.is_empty() {
            println!("generated sources are up to date");
            return Ok(());
        }
        for difference in differences {
            eprintln!("stale: {difference}");
        }
        return Err(protocol_gen::GenError::Ir(
            "generated sources are stale; run `protocol-gen all`".to_string(),
        ));
    }
    match cli.command.unwrap_or(Command::All) {
        Command::All => {
            let files = generate_all(&cli.ir)?;
            write_files(&cli.out, &files)?;
            println!("wrote {} files to {}", files.len(), cli.out.display());
        }
        Command::Connection => {
            let ir = if cli.ir.as_os_str() == DEFAULT_IR {
                PathBuf::from(DEFAULT_CONNECTION_IR)
            } else {
                cli.ir.clone()
            };
            let files = generate_connection(&ir)?;
            write_files(&cli.out, &files)?;
            println!("wrote {} files to {}", files.len(), cli.out.display());
        }
    }
    Ok(())
}

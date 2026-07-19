//! `cargo-dupes`: the Rust-only Cargo subcommand. Parses the shared CLI
//! surface and wires [`RustAnalyzer`] into `dupes_core::cli::run_analysis`.

use clap::Parser;
use dupes_core::cli::Command;
use dupes_core::cli::CommonCliArgs;
use dupes_core::cli::{
  self,
};
use dupes_rust::RustAnalyzer;

#[derive(Parser)]
#[command(name = "cargo-dupes", version, about = "Detect duplicate code in Rust codebases")]
struct Cli {
  /// When invoked as `cargo dupes`, cargo passes "dupes" as the first arg.
  #[arg(hide = true, default_value = "")]
  _cargo_subcommand: String,

  #[command(subcommand)]
  command: Option<Command>,

  #[command(flatten)]
  common: CommonCliArgs,
}

fn main() {
  let Cli {
    command,
    common,
    ..
  } = Cli::parse();

  let root = common.root();
  let command = command.unwrap_or(Command::Report);
  let stdout = std::io::stdout();
  let mut writer = stdout.lock();

  let result = cli::run_command_with_analysis(&root, &command, &mut writer, || {
    let analyzer = RustAnalyzer::new();
    let overrides = common.overrides(vec!["rs".to_string()]);
    cli::run_analysis(&analyzer, &root, common.format, &overrides)
  });

  if let Err(e) = result {
    cli::exit_with_error(&e);
  }
}

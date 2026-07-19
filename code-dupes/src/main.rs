//! `code-dupes`: the multi-language CLI binary. Parses the shared CLI
//! surface, auto-detects or accepts a `--language`, selects the matching
//! analyzer, and wires it into `dupes_core::cli::run_analysis`.

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use dupes_core::analyzer::LanguageAnalyzer;
use dupes_core::cli::CliError;
use dupes_core::cli::Command;
use dupes_core::cli::CommonCliArgs;
use dupes_core::cli::{
  self,
};
use dupes_core::code_unit::CodeUnit;
use dupes_core::config::AnalysisConfig;
use dupes_python::PythonAnalyzer;
use dupes_rust::RustAnalyzer;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
  name = "code-dupes",
  version,
  about = "Detect duplicate code across multiple languages"
)]
struct Cli {
  #[command(subcommand)]
  command: Option<Command>,

  /// Language to analyze. Auto-detected from file extensions if omitted.
  #[arg(short, long, global = true)]
  language: Option<Language>,

  #[command(flatten)]
  common: CommonCliArgs,
}

#[derive(Clone, ValueEnum)]
enum Language {
  Rust,
  Python,
  Generic,
}

impl Cli {
  /// Resolve the analysis root from CLI input.
  fn root(&self) -> PathBuf {
    self.common.root()
  }

  /// Resolve the requested command, defaulting to a full report.
  fn command(&self) -> Command {
    self.command.clone().unwrap_or(Command::Report)
  }

  /// Convert CLI options into shared core overrides.
  fn overrides(&self) -> cli::CliOverrides {
    self
      .common
      .overrides(GENERIC_EXTENSIONS.iter().map(ToString::to_string).collect())
  }
}

impl Language {
  /// Static file extension registry — avoids constructing analyzers for detection.
  const fn extensions(&self) -> &'static [&'static str] {
    match self {
      Self::Rust => &["rs"],
      Self::Python => &["py", "pyi"],
      Self::Generic => &[],
    }
  }

  const AST: &[Self] = &[Self::Rust, Self::Python];
}

impl std::fmt::Display for Language {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Rust => write!(f, "rust"),
      Self::Python => write!(f, "python"),
      Self::Generic => write!(f, "generic"),
    }
  }
}

/// Extensions scanned by generic token and line detection.
const GENERIC_EXTENSIONS: &[&str] = &[
  "rs", "py", "pyi", "md", "markdown", "toml", "yaml", "yml", "json", "js", "jsx", "ts", "tsx", "css", "scss", "sh", "bash", "zsh", "fish",
  "just", "txt",
];

/// Analyzer used when only generic token/line dimensions are needed.
struct GenericAnalyzer;

impl LanguageAnalyzer for GenericAnalyzer {
  fn file_extensions(&self) -> &[&str] {
    &[]
  }

  fn parse_file(
    &self,
    _path: &Path,
    _source: &str,
    _config: &AnalysisConfig,
  ) -> Result<Vec<CodeUnit>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(Vec::new())
  }
}

/// Create a language analyzer for the given language.
fn resolve_analyzer(language: &Language) -> Box<dyn LanguageAnalyzer> {
  match language {
    Language::Rust => Box::new(RustAnalyzer::new()),
    Language::Python => Box::new(PythonAnalyzer::new()),
    Language::Generic => Box::new(GenericAnalyzer),
  }
}

/// Auto-detect language by scanning for files matching known extensions.
///
/// Performs a single directory walk (instead of one per language) and collects
/// file extensions, then matches against `Language::AST`. Returns an error if
/// multiple AST languages are detected — the user must specify `--language` to
/// disambiguate. Directories with only generic extensions fall back to
/// `Language::Generic` (token/line detection without an AST analyzer).
fn auto_detect_language(root: &std::path::Path) -> Result<Language, CliError> {
  // Collect all file extensions found in a single walk.
  let mut found_extensions = HashSet::new();

  for entry in WalkDir::new(root)
    .into_iter()
    .filter_entry(|e| {
      let path = e.path();
      if path.is_dir()
        && let Some(name) = path.file_name().and_then(|n| n.to_str())
      {
        if name == "target" {
          return false;
        }
        if name.starts_with('.') && path != root {
          return false;
        }
      }
      true
    })
    .flatten()
  {
    let path = entry.path();
    if path.is_file()
      && let Some(ext) = path.extension().and_then(|e| e.to_str())
    {
      found_extensions.insert(ext.to_ascii_lowercase());
    }
  }

  // Match found extensions against known languages.
  let detected: Vec<Language> = Language::AST
    .iter()
    .filter(|lang| lang.extensions().iter().any(|ext| found_extensions.contains(*ext)))
    .cloned()
    .collect();

  match detected.len() {
    0 => {
      if GENERIC_EXTENSIONS.iter().any(|ext| found_extensions.contains(*ext)) {
        Ok(Language::Generic)
      } else {
        Err(CliError::NoRecognizedFiles)
      }
    }
    1 => Ok(detected.into_iter().next().unwrap()),
    _ => Err(CliError::AmbiguousLanguage(detected.iter().map(ToString::to_string).collect())),
  }
}

fn main() {
  let cli_args = Cli::parse();
  let root = cli_args.root();
  let command = cli_args.command();
  let stdout = std::io::stdout();
  let mut writer = stdout.lock();

  let result = cli::run_command_with_analysis(&root, &command, &mut writer, || {
    let language = cli_args.language.clone().map_or_else(|| auto_detect_language(&root), Ok)?;
    let analyzer = resolve_analyzer(&language);
    let overrides = cli_args.overrides();
    cli::run_analysis(analyzer.as_ref(), &root, cli_args.common.format, &overrides)
  });

  if let Err(e) = result {
    cli::exit_with_error(&e);
  }
}

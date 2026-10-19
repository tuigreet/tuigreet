use std::{
  env,
  error::Error,
  fs,
  io::{self, Write as _},
  path::{Path, PathBuf},
};

use caress::{Shell, man};
use jiff::{Timestamp, fmt::strtime};
use pound::Parse;
use tuigreet_config::cli::Cli;

const PROGRAM: &str = "tuigreet";
const COMPLETIONS: &[(Shell, &str)] = &[
  (Shell::Bash, "tuigreet.bash"),
  (Shell::Zsh, "_tuigreet"),
  (Shell::Fish, "tuigreet.fish"),
  (Shell::Nu, "tuigreet.nu"),
  (Shell::Elvish, "tuigreet.elv"),
  (Shell::PowerShell, "tuigreet.ps1"),
];

/// Packaging support tasks for tuigreet.
#[derive(Debug, Parse)]
#[pound(name = "xtask")]
struct Xtask {
  /// Task to run.
  #[pound(subcommand)]
  task: Task,
}

#[derive(Debug, Parse)]
enum Task {
  /// Generate shell completion scripts.
  Comp {
    /// Directory to write completion scripts into.
    #[pound(long, default = "target/xtask/completions")]
    out_dir: PathBuf,
  },
  /// Generate mdoc manual pages.
  Man {
    /// Directory to write manual pages into.
    #[pound(long, default = "target/xtask/man")]
    out_dir: PathBuf,
  },
}

fn main() -> Result<(), Box<dyn Error>> {
  let root = workspace_root()?;
  match Xtask::parse().task {
    Task::Comp { out_dir } => {
      generate_completions(&resolve_out_dir(&root, out_dir))?;
    },
    Task::Man { out_dir } => {
      generate_manpages(&resolve_out_dir(&root, out_dir))?;
    },
  }
  Ok(())
}

fn resolve_out_dir(root: &Path, out_dir: PathBuf) -> PathBuf {
  if out_dir.is_absolute() {
    out_dir
  } else {
    root.join(out_dir)
  }
}

fn workspace_root() -> io::Result<PathBuf> {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .map(Path::to_path_buf)
    .ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::NotFound,
        "xtask manifest has no workspace parent",
      )
    })
}

fn generate_manpages(output: &Path) -> Result<(), Box<dyn Error>> {
  fs::create_dir_all(output)?;
  let date = generation_date()?;
  for page in man::pages(Cli::spec(), &date) {
    write_generated(&output.join(page.file_name()), &page.content)?;
  }
  Ok(())
}

fn generation_date() -> Result<String, Box<dyn Error>> {
  let timestamp = match env::var("SOURCE_DATE_EPOCH") {
    Ok(value) => Timestamp::from_second(value.parse::<i64>()?)?,
    Err(env::VarError::NotPresent) => Timestamp::now(),
    Err(error) => return Err(error.into()),
  };
  Ok(strtime::format("%B %-d, %Y", timestamp)?)
}

fn generate_completions(output: &Path) -> io::Result<()> {
  fs::create_dir_all(output)?;
  for &(shell, file_name) in COMPLETIONS {
    write_generated(&output.join(file_name), &shell.script(PROGRAM))?;
  }
  Ok(())
}

fn write_generated(path: &Path, content: &str) -> io::Result<()> {
  match fs::read(path) {
    Ok(existing) if existing == content.as_bytes() => return Ok(()),
    Ok(_) => {},
    Err(error) if error.kind() == io::ErrorKind::NotFound => {},
    Err(error) => return Err(error),
  }

  fs::write(path, content)?;
  writeln!(io::stdout().lock(), "generated {}", path.display())
}

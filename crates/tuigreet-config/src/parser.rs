use std::{
  collections::HashSet,
  env,
  fs,
  io::{self, Write},
  path::{Path, PathBuf},
};

use codespan_reporting::{
  diagnostic::{Diagnostic, Label},
  files::{Files, SimpleFiles},
  term,
};
use dirs::config_dir;
use jiff::{Zoned, fmt::strtime};
use toml::de::Error as TomlError;

use crate::{Config, ConfigError, SecretMode, cli::Cli};

/// A validated effective configuration and its non-fatal warnings.
#[derive(Debug)]
pub struct LoadedConfig {
  pub config:   Config,
  pub warnings: Vec<String>,
}

/// Load and validate the effective configuration selected by `cli`.
///
/// System and user files are layered over defaults unless an explicit path is
/// selected. Environment and command-line overrides are then applied from the
/// same typed source, with command-line values taking precedence.
///
/// # Errors
/// Returns an error when a selected file cannot be read or parsed, an override
/// is malformed, or the resulting configuration is invalid.
pub fn load_config(cli: &Cli) -> Result<LoadedConfig, ConfigError> {
  let mut config = if cli.no_config {
    let mut config = Config::default();
    apply_system_uid_defaults(&mut config, (false, false));
    config
  } else if let Some(path) = cli.config_path() {
    load_config_from_path(path)?
  } else {
    load_default_file_layers()?
  };

  cli.apply_to(&mut config)?;
  normalize_legacy_asterisks(&mut config);
  let warnings = config.validate(false)?;
  Ok(LoadedConfig { config, warnings })
}

/// Load configuration from a specific path.
///
/// # Errors
///
/// Returns error if file cannot be read or contains invalid TOML
fn load_config_from_path(path: &Path) -> Result<Config, ConfigError> {
  let content = fs::read_to_string(path)?;
  let table = toml::from_str::<toml::Table>(&content)
    .map_err(|error| toml_error(path, &content, error))?;
  let explicit_uids = explicit_uid_bounds(&table);
  let secret_mode_specified = table
    .get("secret")
    .and_then(toml::Value::as_table)
    .is_some_and(|secret| secret.contains_key("mode"));
  let mut config = toml::from_str::<Config>(&content)
    .map_err(|error| toml_error(path, &content, error))?;
  config.secret_mode_specified = secret_mode_specified;
  apply_system_uid_defaults(&mut config, explicit_uids);
  Ok(config)
}

/// Load system and user config as TOML tables before deserializing them.
/// This preserves explicit default-valued settings in the higher-priority file.
fn load_default_file_layers() -> Result<Config, ConfigError> {
  let mut merged = toml::Table::new();
  let mut paths = vec![PathBuf::from("/etc/tuigreet/config.toml")];
  if let Some(config_dir) = config_dir() {
    paths.push(config_dir.join("tuigreet").join("config.toml"));
  }

  for path in paths {
    if !path.exists() {
      continue;
    }
    let content = fs::read_to_string(&path)?;
    let mut table = toml::from_str::<toml::Table>(&content)
      .map_err(|error| toml_error(&path, &content, error))?;
    normalize_legacy_asterisks_table(&mut table);
    merge_toml_tables(&mut merged, table);
  }

  let explicit_uids = explicit_uid_bounds(&merged);
  let config_value = toml::Value::Table(merged);
  let secret_mode_specified = config_value
    .get("secret")
    .and_then(toml::Value::as_table)
    .is_some_and(|secret| secret.contains_key("mode"));
  let mut config: Config = config_value.try_into()?;
  config.secret_mode_specified = secret_mode_specified;
  apply_system_uid_defaults(&mut config, explicit_uids);
  Ok(config)
}

fn merge_toml_tables(dest: &mut toml::Table, src: toml::Table) {
  for (key, value) in src {
    match (dest.get_mut(&key), value) {
      (Some(toml::Value::Table(dest)), toml::Value::Table(src)) => {
        merge_toml_tables(dest, src);
      },
      (_, value) => {
        dest.insert(key, value);
      },
    }
  }
}

const DEFAULT_MIN_UID: u32 = 1000;
const DEFAULT_MAX_UID: u32 = 60000;

fn explicit_uid_bounds(table: &toml::Table) -> (bool, bool) {
  let user_menu = table.get("user_menu").and_then(toml::Value::as_table);
  (
    user_menu.is_some_and(|table| table.contains_key("min_uid")),
    user_menu.is_some_and(|table| table.contains_key("max_uid")),
  )
}

fn apply_system_uid_defaults(
  config: &mut Config,
  (min_explicit, max_explicit): (bool, bool),
) {
  if min_explicit && max_explicit {
    return;
  }
  let defaults = fs::read_to_string("/etc/login.defs")
    .map_or((DEFAULT_MIN_UID, DEFAULT_MAX_UID), |content| {
      parse_login_defs_uid_bounds(&content)
    });
  apply_uid_defaults(config, (min_explicit, max_explicit), defaults);
}

const fn apply_uid_defaults(
  config: &mut Config,
  (min_explicit, max_explicit): (bool, bool),
  defaults: (u32, u32),
) {
  if !min_explicit {
    config.user_menu.min_uid = defaults.0;
  }
  if !max_explicit {
    config.user_menu.max_uid = defaults.1;
  }
}

fn parse_login_defs_uid_bounds(content: &str) -> (u32, u32) {
  content.lines().fold(
    (DEFAULT_MIN_UID, DEFAULT_MAX_UID),
    |mut bounds, line| {
      let mut tokens = line.split_whitespace();
      match (tokens.next(), tokens.next()) {
        (Some("UID_MIN"), Some(value)) => {
          bounds.0 = value.parse().unwrap_or(bounds.0);
        },
        (Some("UID_MAX"), Some(value)) => {
          bounds.1 = value.parse().unwrap_or(bounds.1);
        },
        _ => {},
      }
      bounds
    },
  )
}

fn normalize_legacy_asterisks_table(table: &mut toml::Table) {
  let Some(asterisks) = table
    .get("display")
    .and_then(toml::Value::as_table)
    .and_then(|display| display.get("asterisks"))
    .and_then(toml::Value::as_bool)
  else {
    return;
  };

  let secret = table
    .entry("secret")
    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
  let Some(secret) = secret.as_table_mut() else {
    return;
  };
  secret.entry("mode").or_insert_with(|| {
    toml::Value::String(
      if asterisks { "characters" } else { "hidden" }.to_string(),
    )
  });
}

const fn legacy_secret_mode(asterisks: bool) -> SecretMode {
  if asterisks {
    SecretMode::Characters
  } else {
    SecretMode::Hidden
  }
}

/// Applies the compatibility alias only if the canonical field was absent.
const fn normalize_legacy_asterisks(config: &mut Config) {
  if !config.secret_mode_specified
    && let Some(asterisks) = config.display.asterisks
  {
    config.secret.mode = legacy_secret_mode(asterisks);
  }
}

/// Create a TOML error with file/line context for better error messages.
///
/// # Arguments
///
/// * `path` - Path to the config file
/// * `content` - Full file content
/// * `original_error` - TOML parsing error
///
/// # Returns
///
/// [`ConfigError`] with line numbers and additiona context
fn toml_error(
  path: &Path,
  content: &str,
  original_error: TomlError,
) -> ConfigError {
  let Some(span) = original_error.span() else {
    return ConfigError::Parse(original_error);
  };

  let mut files = SimpleFiles::new();
  let file_id = files.add(path.to_string_lossy(), content);

  let diagnostic = Diagnostic::error()
    .with_message(original_error.message())
    .with_labels(vec![
      Label::primary(file_id, span.clone()).with_message("TOML parse error"),
    ]);

  let config = term::Config::default();
  let writer = term::termcolor::StandardStream::stderr(
    term::termcolor::ColorChoice::Always,
  );
  if let Err(error) =
    term::emit_to_io_write(&mut writer.lock(), &config, &files, &diagnostic)
      .or_else(|_| {
        writeln!(
          io::stderr().lock(),
          "TOML parse error: {}",
          original_error.message()
        )
      })
  {
    return ConfigError::ParseWithContext {
      message: format!(
        "TOML parse error: {} at {} (could not write diagnostic: {error})",
        original_error.message(),
        path.display()
      ),
    };
  }

  ConfigError::ParseWithContext {
    message: format!(
      "TOML parse error: {} at {}:{} (byte {})",
      original_error.message(),
      path.display(),
      files.line_index(file_id, span.start).unwrap_or(0) + 1,
      span.start
    ),
  }
}

impl Config {
  /// Validates cross-field configuration constraints.
  ///
  /// Returns non-fatal configuration warnings when validation succeeds.
  ///
  /// # Errors
  ///
  /// Returns an error when mutually exclusive settings, invalid ranges,
  /// malformed values, or invalid wrapper commands are configured.
  pub fn validate(
    &self,
    validate_wrappers: bool,
  ) -> Result<Vec<String>, ConfigError> {
    let mut warnings = Vec::new();
    if self.display.asterisks.is_some() {
      warnings.push(
        "display.asterisks is deprecated; use secret.mode = \"characters\" or \
         \"hidden\" instead"
          .to_string(),
      );
    }

    self.validate_relationships()?;
    self.validate_layout()?;
    self.validate_keybindings()?;
    self.validate_time_format()?;
    if validate_wrappers {
      self.validate_wrappers()?;
    }
    self.validate_outputs(&mut warnings)?;
    if let Some(reason) = self.terminal.invalid_reason() {
      return Err(ConfigError::Validation(reason));
    }
    self.check_warnings(&mut warnings);
    Ok(warnings)
  }

  fn validate_relationships(&self) -> Result<(), ConfigError> {
    if self.display.issue && self.display.greeting.is_some() {
      return Err(ConfigError::MutuallyExclusive(
        "display.issue".to_string(),
        "display.greeting".to_string(),
      ));
    }
    self.validate_remember_relationships()?;
    if self.session.environments.iter().any(|value| {
      value
        .split_once('=')
        .is_none_or(|(name, _)| name.trim().is_empty())
    }) {
      return Err(ConfigError::Validation(
        "session.environments entries must use KEY=VALUE".to_string(),
      ));
    }
    if matches!(self.secret.mode, SecretMode::Characters)
      && self.secret.characters.is_empty()
    {
      return Err(ConfigError::Validation(
        "secret.characters must not be empty in characters mode".to_string(),
      ));
    }
    if self.user_menu.min_uid > self.user_menu.max_uid {
      return Err(ConfigError::InvalidRange(
        "user_menu.min_uid must not exceed user_menu.max_uid".to_string(),
      ));
    }
    Ok(())
  }

  fn validate_remember_relationships(&self) -> Result<(), ConfigError> {
    if self.remember.session && self.remember.user_session {
      return Err(ConfigError::MutuallyExclusive(
        "remember.session".to_string(),
        "remember.user_session".to_string(),
      ));
    }
    if self.remember.user_session && !self.remember.username {
      return Err(ConfigError::Dependency(
        "remember.user_session requires remember.username".to_string(),
      ));
    }
    Ok(())
  }

  fn validate_layout(&self) -> Result<(), ConfigError> {
    if self.layout.width == 0 {
      return Err(ConfigError::Validation(
        "layout.width must be greater than 0".to_string(),
      ));
    }
    for (name, padding) in [
      ("layout.window_padding", self.layout.window_padding),
      ("layout.container_padding", self.layout.container_padding),
      ("layout.prompt_padding", self.layout.prompt_padding),
    ] {
      if padding.is_some_and(|padding| padding > 10) {
        return Err(ConfigError::Validation(format!(
          "{name} must not exceed 10"
        )));
      }
    }
    Ok(())
  }

  fn validate_keybindings(&self) -> Result<(), ConfigError> {
    let keys = [
      self.keybindings.command,
      self.keybindings.sessions,
      self.keybindings.power,
      self.keybindings.background,
    ];
    if keys.iter().collect::<HashSet<_>>().len() != keys.len() {
      return Err(ConfigError::DuplicateKeybindings);
    }
    for (name, key) in [
      ("command", self.keybindings.command),
      ("sessions", self.keybindings.sessions),
      ("power", self.keybindings.power),
      ("background", self.keybindings.background),
    ] {
      if !(1..=12).contains(&key) {
        return Err(ConfigError::InvalidFKey(name.to_string(), key));
      }
    }

    let mut custom_keys = HashSet::new();
    let mut custom_actions = HashSet::new();
    for binding in &self.keybindings.actions {
      if !custom_keys.insert(binding.key) {
        return Err(ConfigError::Validation(format!(
          "duplicate custom keybinding '{}'",
          binding.key
        )));
      }
      if !custom_actions.insert(binding.action) {
        return Err(ConfigError::Validation(format!(
          "action '{:?}' is bound more than once",
          binding.action
        )));
      }
    }
    Ok(())
  }

  fn validate_time_format(&self) -> Result<(), ConfigError> {
    if let Some(format) = &self.display.time_format
      && strtime::format(format, &Zoned::now()).is_err()
    {
      return Err(ConfigError::InvalidTimeFormat);
    }
    Ok(())
  }

  fn validate_wrappers(&self) -> Result<(), ConfigError> {
    if let Some(wrapper) = &self.session.session_wrapper {
      Self::validate_wrapper_command(wrapper)?;
    }
    if let Some(wrapper) = &self.session.xsession_wrapper {
      Self::validate_wrapper_command(wrapper)?;
    }
    Ok(())
  }

  fn validate_outputs(
    &self,
    warnings: &mut Vec<String>,
  ) -> Result<(), ConfigError> {
    let primary_count =
      self.outputs.iter().filter(|output| output.primary).count();
    if primary_count > 1 {
      return Err(ConfigError::Validation(format!(
        "At most one output may be marked `primary = true`, but \
         {primary_count} are"
      )));
    }
    for output in &self.outputs {
      if output.connector.contains('/') || output.connector.contains("..") {
        return Err(ConfigError::Validation(format!(
          "Output connector name '{}' must not contain path separators",
          output.connector
        )));
      }
      if output.connector.is_empty() {
        return Err(ConfigError::Validation(
          "Output connector name must not be empty".to_string(),
        ));
      }
    }
    if !self.outputs.is_empty() && self.outputs.iter().all(|o| !o.enabled) {
      warnings.push(
        "All [[outputs]] entries have `enabled = false`; no output will be \
         used for terminal sizing"
          .to_string(),
      );
    }
    Ok(())
  }

  /// Check for configuration warnings
  fn check_warnings(&self, warnings: &mut Vec<String>) {
    // Warn about excessively high padding values
    if let Some(padding) = self.layout.window_padding
      && padding > 10
    {
      warnings.push(format!(
        "window_padding is very high ({padding}), this may cause display \
         issues"
      ));
    }

    if let Some(padding) = self.layout.container_padding
      && padding > 10
    {
      warnings.push(format!(
        "container_padding is very high ({padding}), this may cause display \
         issues"
      ));
    }

    // Warn about very wide width settings
    if self.layout.width > 200 {
      warnings.push(format!(
        "width is very high ({}), this may cause display issues on smaller \
         terminals",
        self.layout.width
      ));
    }

    // Warn if user menu is enabled but UID range might be empty
    if self.user_menu.enabled && self.user_menu.min_uid > 65000 {
      warnings.push(
        "user_menu.min_uid is very high, you may not see any users".to_string(),
      );
    }

    // Warn about potentially conflicting session directories
    let mut all_session_dirs = self.session.sessions_dirs.clone();
    all_session_dirs.extend(self.session.xsessions_dirs.clone());

    if all_session_dirs.len()
      != all_session_dirs.iter().collect::<HashSet<_>>().len()
    {
      warnings.push(
        "Duplicate session directories detected in sessions_dirs and \
         xsessions_dirs"
          .to_string(),
      );
    }

    // Warn about hidden widgets but enabled features
    if matches!(
      self.layout.widgets.time_position,
      crate::WidgetPosition::Hidden
    ) && self.display.show_time
    {
      warnings
        .push("time widget is hidden but show_time is enabled".to_string());
    }

    // Warn about potential security issues with power commands
    if let Some(ref cmd) = self.power.shutdown
      && !self.power.use_setsid
      && !cmd.contains("sudo")
      && !cmd.contains("doas")
    {
      warnings.push(
        "shutdown command without setsid or privilege escalation may fail"
          .to_string(),
      );
    }

    if let Some(ref cmd) = self.power.reboot
      && !self.power.use_setsid
      && !cmd.contains("sudo")
      && !cmd.contains("doas")
    {
      warnings.push(
        "reboot command without setsid or privilege escalation may fail"
          .to_string(),
      );
    }

    if let Some(ref cmd) = self.power.suspend
      && !self.power.use_setsid
      && !cmd.contains("sudo")
      && !cmd.contains("doas")
    {
      warnings.push(
        "suspend command without setsid or privilege escalation may fail"
          .to_string(),
      );
    }

    if let Some(ref cmd) = self.power.hibernate
      && !self.power.use_setsid
      && !cmd.contains("sudo")
      && !cmd.contains("doas")
    {
      warnings.push(
        "hibernate command without setsid or privilege escalation may fail"
          .to_string(),
      );
    }

    // Warn about potentially invalid time formats
    if let Some(ref format) = self.display.time_format
      && format.is_empty()
    {
      warnings.push(
        "time_format is empty, this will result in no time display".to_string(),
      );
    }

    // Warn about conflicting remember options
    if self.remember.session && self.remember.user_session {
      // This should be caught as an error above, but just in case
      warnings.push(
        "Both remember.session and remember.user_session are enabled"
          .to_string(),
      );
    }

    // Warn about invalid fps settings
    if self.background.fps.is_some_and(|f| f == 0) {
      warnings
        .push("Background fps is set to 0, this will be ignored".to_string());
    }
  }

  /// Validate that a wrapper command exists and is executable
  fn validate_wrapper_command(command: &str) -> Result<(), ConfigError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
      return Err(ConfigError::WrapperExecutableNotFound(PathBuf::from(
        command,
      )));
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if let Some(executable) = parts.first() {
      // Check if it's an absolute path
      let path = PathBuf::from(executable);
      if path.is_absolute() {
        if !path.exists() || !is_executable(&path) {
          return Err(ConfigError::WrapperExecutableNotFound(path));
        }
      } else {
        // Search in PATH
        if !command_exists(executable) {
          return Err(ConfigError::WrapperExecutableNotFound(PathBuf::from(
            executable,
          )));
        }
      }
    }
    Ok(())
  }
}

/// Check if a file is executable
fn is_executable(path: &Path) -> bool {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
      let permissions = metadata.permissions();
      return permissions.mode() & 0o111 != 0;
    }
  }

  #[cfg(not(unix))]
  {
    // On non-Unix systems, just check if file exists
    return path.exists();
  }

  false
}

/// Check if a command exists in PATH
fn command_exists(command: &str) -> bool {
  if let Ok(path) = env::var("PATH") {
    for dir in env::split_paths(&path) {
      let full_path = dir.join(command);
      if full_path.exists() && is_executable(&full_path) {
        return true;
      }
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use std::error::Error;

  use tempfile::NamedTempFile;

  use super::*;
  use crate::{BindingAction, IdleAction, PromptAnchor, RegionItem};

  #[test]
  fn rust_defaults_match_empty_toml() -> Result<(), Box<dyn Error>> {
    let rust_defaults = toml::Value::try_from(Config::default())?;
    let parsed_defaults = toml::Value::try_from(toml::from_str::<Config>("")?)?;

    assert_eq!(rust_defaults, parsed_defaults);
    Ok(())
  }

  #[test]
  fn toml_layers_preserve_explicit_default_values() -> Result<(), ConfigError> {
    let mut system: toml::Table =
      toml::from_str("[display]\nshow_time = true\n[layout]\nwidth = 120\n")?;
    let user: toml::Table =
      toml::from_str("[display]\nshow_time = false\n[layout]\nwidth = 80\n")?;

    merge_toml_tables(&mut system, user);
    let config: Config = toml::Value::Table(system).try_into()?;

    assert!(!config.display.show_time);
    assert_eq!(config.layout.width, 80);
    Ok(())
  }

  #[test]
  fn typed_toml_errors_include_the_selected_path() -> Result<(), Box<dyn Error>>
  {
    let mut file = NamedTempFile::new()?;
    writeln!(file, "[layout]\nwidth = \"wide\"")?;

    let Err(error) = load_config_from_path(file.path()) else {
      return Err(io::Error::other("invalid typed TOML loaded").into());
    };

    assert!(
      error
        .to_string()
        .contains(file.path().to_string_lossy().as_ref())
    );
    Ok(())
  }

  #[test]
  fn login_defs_uid_bounds_preserve_fallbacks_for_invalid_values() {
    let bounds = parse_login_defs_uid_bounds("UID_MIN 500\nUID_MAX invalid\n");

    assert_eq!(bounds, (500, DEFAULT_MAX_UID));
  }

  #[test]
  fn explicit_uid_bounds_win_independently() {
    let mut config = Config::default();
    config.user_menu.min_uid = 111;
    config.user_menu.max_uid = 222;

    apply_uid_defaults(&mut config, (true, false), (333, 444));

    assert_eq!(config.user_menu.min_uid, 111);
    assert_eq!(config.user_menu.max_uid, 444);
  }

  #[test]
  fn canonical_secret_mode_wins_over_legacy_alias() -> Result<(), ConfigError> {
    let mut config: Config = toml::from_str(
      r#"
  [display]
  asterisks = true
  
  [secret]
  mode = "hidden"
  "#,
    )?;
    config.secret_mode_specified = true;

    normalize_legacy_asterisks(&mut config);

    assert_eq!(config.secret.mode, SecretMode::Hidden);
    Ok(())
  }

  #[test]
  fn test_mutual_exclusive_remember_flags() -> Result<(), ConfigError> {
    let config: Config = toml::from_str(
      r"
      [remember]
      username = true
      session = true
      user_session = true
      ",
    )?;

    assert!(matches!(
      config.validate(false),
      Err(ConfigError::MutuallyExclusive(left, right))
        if left == "remember.session" && right == "remember.user_session"
    ));
    Ok(())
  }

  #[test]
  fn test_keybindings_distinctness_in_config() -> Result<(), ConfigError> {
    let toml_content = r"
  [keybindings]
  command = 3
  sessions = 3
  power = 7
  ";

    let config: Config = toml::from_str(toml_content)?;
    let validation_result = config.validate(false);

    assert!(
      matches!(validation_result, Err(ConfigError::DuplicateKeybindings)),
      "Expected DuplicateKeybindings error, got: {validation_result:?}"
    );
    Ok(())
  }

  #[test]
  fn test_session_config_default_consistency() -> Result<(), ConfigError> {
    let default_config = Config::default();

    let partial_toml = r#"
  [session]
  command = "test"
  "#;
    let partial_config: Config = toml::from_str(partial_toml)?;

    assert_eq!(
      default_config.session.sessions_dirs,
      partial_config.session.sessions_dirs,
      "Default and partially deserialized sessions_dirs should match"
    );
    Ok(())
  }

  #[test]
  fn test_power_config_default_consistency() -> Result<(), ConfigError> {
    let default_config = Config::default();

    let partial_toml = r#"
  [power]
  shutdown = "poweroff"
  "#;
    let partial_config: Config = toml::from_str(partial_toml)?;

    assert_eq!(
      default_config.power.use_setsid, partial_config.power.use_setsid,
      "Default and partially deserialized use_setsid should match"
    );
    Ok(())
  }

  #[test]
  fn test_wrapper_validation_empty_string() -> Result<(), ConfigError> {
    let empty_wrapper = r#"
  [session]
  session_wrapper = ""
  "#;

    let mut config: Config = toml::from_str(empty_wrapper)?;

    config.session.xsession_wrapper = None;

    let result = config.validate(true);

    assert!(
      result.is_err(),
      "Empty wrapper command should fail validation"
    );
    Ok(())
  }

  // [[outputs]] validation
  #[test]
  fn test_outputs_toml_roundtrip() -> Result<(), ConfigError> {
    let toml_content = r#"
  [[outputs]]
  connector = "DP-1"
  primary = true
  
  [[outputs]]
  connector = "HDMI-A-1"
  enabled = false
  "#;
    let config: Config = toml::from_str(toml_content)?;

    assert_eq!(config.outputs.len(), 2);
    assert_eq!(config.outputs[0].connector, "DP-1");
    assert!(config.outputs[0].primary);
    assert!(config.outputs[0].enabled); // default = true
    assert_eq!(config.outputs[1].connector, "HDMI-A-1");
    assert!(!config.outputs[1].primary); // default = false
    assert!(!config.outputs[1].enabled);

    // Validation should pass
    assert!(config.validate(false).is_ok());
    Ok(())
  }

  #[test]
  fn test_outputs_multiple_primary_is_error() -> Result<(), ConfigError> {
    let toml_content = r#"
  [[outputs]]
  connector = "DP-1"
  primary = true
  
  [[outputs]]
  connector = "HDMI-A-1"
  primary = true
  "#;
    let config: Config = toml::from_str(toml_content)?;
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "Multiple primary outputs should be a Validation error, got: {result:?}"
    );
    Ok(())
  }

  #[test]
  fn test_outputs_empty_connector_is_error() -> Result<(), ConfigError> {
    let toml_content = r#"
  [[outputs]]
  connector = ""
  "#;
    let config: Config = toml::from_str(toml_content)?;
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "Empty connector name should be a Validation error, got: {result:?}"
    );
    Ok(())
  }

  #[test]
  fn test_outputs_path_separator_in_connector_is_error()
  -> Result<(), ConfigError> {
    for bad in &["../DP-1", "/sys/class/drm/DP-1", "foo/bar"] {
      let config: Config =
        toml::from_str(&format!("[[outputs]]\nconnector = \"{bad}\"\n"))?;
      let result = config.validate(false);
      assert!(
        matches!(result, Err(ConfigError::Validation(_))),
        "Connector '{bad}' with path separator should be a Validation error, \
         got: {result:?}"
      );
    }
    Ok(())
  }

  #[test]
  fn test_outputs_valid_connector_names() -> Result<(), ConfigError> {
    // Typical DRM connector name patterns that must pass
    for good in &[
      "DP-1",
      "HDMI-A-1",
      "DisplayPort-2",
      "eDP-1",
      "VGA-1",
      "DVI-D-1",
    ] {
      let config: Config =
        toml::from_str(&format!("[[outputs]]\nconnector = \"{good}\"\n"))?;
      assert!(
        config.validate(false).is_ok(),
        "Connector '{good}' should be valid, but validation failed"
      );
    }
    Ok(())
  }

  #[test]
  fn test_outputs_all_disabled_is_warning() -> Result<(), ConfigError> {
    let toml_content = r#"
  [[outputs]]
  connector = "DP-1"
  enabled = false
  
  [[outputs]]
  connector = "HDMI-A-1"
  enabled = false
  "#;
    let config: Config = toml::from_str(toml_content)?;
    let result = config.validate(false);
    assert!(
      result.is_ok(),
      "All-disabled outputs should not be an error"
    );
    let warnings = result?;
    assert!(
      warnings.iter().any(|w| w.contains("enabled = false")),
      "Expected a warning about all outputs being disabled, got: {warnings:?}"
    );
    Ok(())
  }

  #[test]
  fn test_outputs_single_primary_passes() -> Result<(), ConfigError> {
    let toml_content = r#"
  [[outputs]]
  connector = "DP-1"
  primary = true
  
  [[outputs]]
  connector = "HDMI-A-1"
  "#;
    let config: Config = toml::from_str(toml_content)?;
    assert!(config.validate(false).is_ok());
    Ok(())
  }

  // [terminal] validation
  #[test]
  fn test_terminal_both_set_passes() -> Result<(), ConfigError> {
    let toml_content = r"
  [terminal]
  cols = 237
  rows = 52
  ";
    let config: Config = toml::from_str(toml_content)?;
    assert_eq!(config.terminal.cols, Some(237));
    assert_eq!(config.terminal.rows, Some(52));
    assert!(config.validate(false).is_ok());
    Ok(())
  }

  #[test]
  fn test_terminal_cols_without_rows_is_error() -> Result<(), ConfigError> {
    let toml_content = r"
  [terminal]
  cols = 237
  ";
    let config: Config = toml::from_str(toml_content)?;
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "cols without rows should be a Validation error, got: {result:?}"
    );
    Ok(())
  }

  #[test]
  fn test_terminal_rows_without_cols_is_error() -> Result<(), ConfigError> {
    let toml_content = r"
  [terminal]
  rows = 52
  ";
    let config: Config = toml::from_str(toml_content)?;
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "rows without cols should be a Validation error, got: {result:?}"
    );
    Ok(())
  }

  #[test]
  fn test_terminal_neither_set_passes() {
    let config = Config::default();
    assert!(config.validate(false).is_ok());
  }

  #[test]
  fn test_terminal_zero_cols_is_error() {
    let mut config = Config::default();
    config.terminal.cols = Some(0);
    config.terminal.rows = Some(52);
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "cols = 0 should be a Validation error, got: {result:?}"
    );
  }

  #[test]
  fn test_terminal_zero_rows_is_error() {
    let mut config = Config::default();
    config.terminal.cols = Some(237);
    config.terminal.rows = Some(0);
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "rows = 0 should be a Validation error, got: {result:?}"
    );
  }

  #[test]
  fn test_wrapper_validation_whitespace_only() -> Result<(), ConfigError> {
    let whitespace_wrapper = r#"
  [session]
  session_wrapper = "   "
  "#;

    let mut config: Config = toml::from_str(whitespace_wrapper)?;

    config.session.xsession_wrapper = None;

    let result = config.validate(true);

    assert!(
      result.is_err(),
      "Whitespace-only wrapper command should fail validation"
    );
    Ok(())
  }

  #[test]
  fn parses_high_value_interaction_configuration() -> Result<(), ConfigError> {
    let config: Config = toml::from_str(
      r#"
      [idle]
      timeout_seconds = 45
      action = "pause_animation"

      [layout.prompt]
      anchor = "bottom_right"
      offset_x = -3
      offset_y = 2
      edge_margin = 4

      [layout.widgets.regions]
      top_left = ["time", "battery"]
      bottom_right = ["session", "actions", "caps_lock"]

      [[keybindings.actions]]
      key = "alt+p"
      action = "power"

      [[keybindings.actions]]
      key = "ctrl+r"
      action = "toggle_password"
      "#,
    )?;

    assert_eq!(config.idle.timeout_seconds, 45);
    assert_eq!(config.idle.action, IdleAction::PauseAnimation);
    assert_eq!(config.layout.prompt.anchor, PromptAnchor::BottomRight);
    assert_eq!(config.layout.prompt.offset_x, -3);
    assert_eq!(config.layout.prompt.offset_y, 2);
    assert_eq!(config.layout.prompt.edge_margin, 4);
    assert_eq!(config.layout.widgets.regions.top_left, vec![
      RegionItem::Time,
      RegionItem::Battery
    ]);
    assert_eq!(config.layout.widgets.regions.bottom_right, vec![
      RegionItem::Session,
      RegionItem::Actions,
      RegionItem::CapsLock
    ]);
    let [power, toggle] = config.keybindings.actions.as_slice() else {
      return Err(ConfigError::Validation(
        "expected exactly two action bindings".to_string(),
      ));
    };
    assert_eq!(power.key.to_string(), "Alt+p");
    assert_eq!(power.action, BindingAction::Power);
    assert_eq!(toggle.key.to_string(), "Ctrl+r");
    assert_eq!(toggle.action, BindingAction::TogglePassword);
    assert!(config.validate(false).is_ok());
    Ok(())
  }

  #[test]
  fn rejects_unmodified_printable_action_keys_during_parsing()
  -> Result<(), ConfigError> {
    let Err(error) = toml::from_str::<Config>(
      r#"
      [[keybindings.actions]]
      key = "r"
      action = "toggle_password"
      "#,
    ) else {
      return Err(ConfigError::Validation(
        "unmodified printable action key was accepted".to_string(),
      ));
    };

    assert!(error.to_string().contains("printable action keys require"));
    Ok(())
  }

  #[test]
  fn rejects_duplicate_custom_action_chords() -> Result<(), ConfigError> {
    let config: Config = toml::from_str(
      r#"
      [[keybindings.actions]]
      key = "ctrl+b"
      action = "background"

      [[keybindings.actions]]
      key = "CTRL+B"
      action = "power"
      "#,
    )?;

    assert!(matches!(
      config.validate(false),
      Err(ConfigError::Validation(message)) if message.contains("duplicate custom keybinding")
    ));
    Ok(())
  }
}

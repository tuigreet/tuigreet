use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
};

use codespan_reporting::{
  diagnostic::{Diagnostic, Label},
  files::{Files, SimpleFiles},
  term,
};
use dirs::config_dir;

use crate::{
  AlignGreeting,
  Config,
  ConfigError,
  SecretMode,
  env::load_env_variables,
};

/// Loads the effective configuration from files, environment, and CLI options.
///
/// Without an explicit path, system and user files are layered over defaults.
/// An explicit path replaces those file layers. Environment variables and CLI
/// options are always applied last.
///
/// # Arguments
///
/// * `cli_config_path` - An optional configuration file. When present, it
///   replaces the system and user file layers.
/// * `cli_matches` - Optional parsed command-line options to apply after all
///   other configuration sources.
///
/// # Returns
///
/// The effective configuration after applying the selected file layers,
/// environment variables, and command-line options.
///
/// # Errors
///
/// Returns an error if a selected configuration file cannot be read or parsed.
pub fn load_config(
  cli_config_path: Option<&Path>,
  cli_matches: Option<&getopts::Matches>,
) -> Result<Config, ConfigError> {
  let mut config = if let Some(path) = cli_config_path {
    load_config_from_path(path)?
  } else {
    load_default_file_layers()?
  };

  let env_vars = load_env_variables();
  apply_config_layer(&mut config, env_vars);

  if let Some(cli) = cli_matches {
    let cli_config = extract_cli_config(cli);
    apply_config_layer(&mut config, cli_config);
  }

  normalize_legacy_asterisks(&mut config);
  Ok(config)
}

/// Merges src into dest, only overwriting fields that differ from their
/// defaults. Used for layering: defaults -> system -> user -> env -> CLI
fn apply_config_layer(dest: &mut Config, src: Config) {
  let defaults = Config::default();

  // General
  if src.general.debug != defaults.general.debug {
    dest.general.debug = src.general.debug;
  }
  if src.general.log_file != defaults.general.log_file {
    dest.general.log_file = src.general.log_file;
  }
  if src.general.numlock != defaults.general.numlock {
    dest.general.numlock = src.general.numlock;
  }

  // Session
  if src.session.command != defaults.session.command {
    dest.session.command = src.session.command;
  }
  if src.session.sessions_dirs != defaults.session.sessions_dirs {
    dest.session.sessions_dirs = src.session.sessions_dirs;
  }
  if src.session.xsessions_dirs != defaults.session.xsessions_dirs {
    dest.session.xsessions_dirs = src.session.xsessions_dirs;
  }
  if src.session.session_wrapper != defaults.session.session_wrapper {
    dest.session.session_wrapper = src.session.session_wrapper;
  }
  if src.session.xsession_wrapper != defaults.session.xsession_wrapper {
    dest.session.xsession_wrapper = src.session.xsession_wrapper;
  }
  if src.session.environments != defaults.session.environments {
    dest.session.environments = src.session.environments;
  }

  // Display
  if src.display.show_time != defaults.display.show_time {
    dest.display.show_time = src.display.show_time;
  }
  if src.display.time_format != defaults.display.time_format {
    dest.display.time_format = src.display.time_format;
  }
  if src.display.greeting != defaults.display.greeting {
    dest.display.greeting = src.display.greeting;
  }
  if src.display.show_title != defaults.display.show_title {
    dest.display.show_title = src.display.show_title;
  }
  if src.display.custom_title != defaults.display.custom_title {
    dest.display.custom_title = src.display.custom_title.clone();
  }
  if src.display.issue != defaults.display.issue {
    dest.display.issue = src.display.issue;
  }
  if src.display.battery != defaults.display.battery {
    dest.display.battery = src.display.battery;
  }
  if src.display.align_greeting != defaults.display.align_greeting {
    dest.display.align_greeting = src.display.align_greeting;
  }

  // Remember
  if src.remember.default_user != defaults.remember.default_user {
    dest.remember.default_user = src.remember.default_user;
  }
  if src.remember.username != defaults.remember.username {
    dest.remember.username = src.remember.username;
  }
  if src.remember.session != defaults.remember.session {
    dest.remember.session = src.remember.session;
  }
  if src.remember.user_session != defaults.remember.user_session {
    dest.remember.user_session = src.remember.user_session;
  }

  // User menu
  if src.user_menu.enabled != defaults.user_menu.enabled {
    dest.user_menu.enabled = src.user_menu.enabled;
  }
  if src.user_menu.min_uid != defaults.user_menu.min_uid {
    dest.user_menu.min_uid = src.user_menu.min_uid;
  }
  if src.user_menu.max_uid != defaults.user_menu.max_uid {
    dest.user_menu.max_uid = src.user_menu.max_uid;
  }

  // Secret
  if src.display.asterisks.is_some() {
    dest.display.asterisks = src.display.asterisks;
  }
  if src.secret_mode_specified || src.secret.mode != defaults.secret.mode {
    dest.secret.mode = src.secret.mode;
    dest.secret_mode_specified = true;
  } else if let Some(asterisks) = src.display.asterisks {
    dest.secret.mode = legacy_secret_mode(asterisks);
  }
  if src.secret.characters != defaults.secret.characters {
    dest.secret.characters = src.secret.characters;
  }

  // Layout
  if src.layout.width != defaults.layout.width {
    dest.layout.width = src.layout.width;
  }
  if src.layout.window_padding != defaults.layout.window_padding {
    dest.layout.window_padding = src.layout.window_padding;
  }
  if src.layout.container_padding != defaults.layout.container_padding {
    dest.layout.container_padding = src.layout.container_padding;
  }
  if src.layout.prompt_padding != defaults.layout.prompt_padding {
    dest.layout.prompt_padding = src.layout.prompt_padding;
  }
  if src.layout.widgets.time_position != defaults.layout.widgets.time_position {
    dest.layout.widgets.time_position = src.layout.widgets.time_position;
  }
  if src.layout.widgets.status_position
    != defaults.layout.widgets.status_position
  {
    dest.layout.widgets.status_position = src.layout.widgets.status_position;
  }
  if src.layout.widgets.battery_position
    != defaults.layout.widgets.battery_position
  {
    dest.layout.widgets.battery_position =
      src.layout.widgets.battery_position.clone();
  }

  // Status bar item visibility
  if src.layout.widgets.status_bar.show_reset
    != defaults.layout.widgets.status_bar.show_reset
  {
    dest.layout.widgets.status_bar.show_reset =
      src.layout.widgets.status_bar.show_reset;
  }
  if src.layout.widgets.status_bar.show_command
    != defaults.layout.widgets.status_bar.show_command
  {
    dest.layout.widgets.status_bar.show_command =
      src.layout.widgets.status_bar.show_command;
  }
  if src.layout.widgets.status_bar.show_session
    != defaults.layout.widgets.status_bar.show_session
  {
    dest.layout.widgets.status_bar.show_session =
      src.layout.widgets.status_bar.show_session;
  }
  if src.layout.widgets.status_bar.show_power
    != defaults.layout.widgets.status_bar.show_power
  {
    dest.layout.widgets.status_bar.show_power =
      src.layout.widgets.status_bar.show_power;
  }
  if src.layout.widgets.status_bar.show_background
    != defaults.layout.widgets.status_bar.show_background
  {
    dest.layout.widgets.status_bar.show_background =
      src.layout.widgets.status_bar.show_background;
  }
  if src.layout.widgets.status_bar.show_session_status
    != defaults.layout.widgets.status_bar.show_session_status
  {
    dest.layout.widgets.status_bar.show_session_status =
      src.layout.widgets.status_bar.show_session_status;
  }
  if src.layout.widgets.status_bar.show_caps_lock
    != defaults.layout.widgets.status_bar.show_caps_lock
  {
    dest.layout.widgets.status_bar.show_caps_lock =
      src.layout.widgets.status_bar.show_caps_lock;
  }

  // Power
  if src.power.shutdown != defaults.power.shutdown {
    dest.power.shutdown = src.power.shutdown;
  }
  if src.power.reboot != defaults.power.reboot {
    dest.power.reboot = src.power.reboot;
  }
  if src.power.suspend != defaults.power.suspend {
    dest.power.suspend = src.power.suspend;
  }
  if src.power.hibernate != defaults.power.hibernate {
    dest.power.hibernate = src.power.hibernate;
  }
  if src.power.use_setsid != defaults.power.use_setsid {
    dest.power.use_setsid = src.power.use_setsid;
  }

  // Keybindings
  if src.keybindings.command != defaults.keybindings.command {
    dest.keybindings.command = src.keybindings.command;
  }
  if src.keybindings.sessions != defaults.keybindings.sessions {
    dest.keybindings.sessions = src.keybindings.sessions;
  }
  if src.keybindings.power != defaults.keybindings.power {
    dest.keybindings.power = src.keybindings.power;
  }
  if src.keybindings.background != defaults.keybindings.background {
    dest.keybindings.background = src.keybindings.background;
  }

  // Background animation
  if src.background.kind.is_some() {
    dest.background.kind = src.background.kind;
  }
  if src.background.fps.is_some() {
    dest.background.fps = src.background.fps;
  }
  if src.background.doom.height.is_some() {
    dest.background.doom.height = src.background.doom.height;
  }
  if src.background.doom.spread.is_some() {
    dest.background.doom.spread = src.background.doom.spread;
  }
  if src.background.doom.top_color.is_some() {
    dest.background.doom.top_color = src.background.doom.top_color;
  }
  if src.background.doom.middle_color.is_some() {
    dest.background.doom.middle_color = src.background.doom.middle_color;
  }
  if src.background.doom.bottom_color.is_some() {
    dest.background.doom.bottom_color = src.background.doom.bottom_color;
  }
  if src.background.matrix.head_color.is_some() {
    dest.background.matrix.head_color = src.background.matrix.head_color;
  }
  if src.background.matrix.bright_color.is_some() {
    dest.background.matrix.bright_color = src.background.matrix.bright_color;
  }
  if src.background.matrix.dim_color.is_some() {
    dest.background.matrix.dim_color = src.background.matrix.dim_color;
  }
  if src.background.matrix.min_length.is_some() {
    dest.background.matrix.min_length = src.background.matrix.min_length;
  }
  if src.background.matrix.max_length.is_some() {
    dest.background.matrix.max_length = src.background.matrix.max_length;
  }
  if src.background.matrix.min_speed.is_some() {
    dest.background.matrix.min_speed = src.background.matrix.min_speed;
  }
  if src.background.matrix.max_speed.is_some() {
    dest.background.matrix.max_speed = src.background.matrix.max_speed;
  }
  if src.background.matrix.mutate_chance.is_some() {
    dest.background.matrix.mutate_chance = src.background.matrix.mutate_chance;
  }

  // Outputs: a non-empty list from a higher-priority layer fully replaces
  if !src.outputs.is_empty() {
    dest.outputs = src.outputs;
  }

  // Terminal: replace individual fields only if source specifies them
  if src.terminal.cols.is_some() {
    dest.terminal.cols = src.terminal.cols;
  }
  if src.terminal.rows.is_some() {
    dest.terminal.rows = src.terminal.rows;
  }

  // Theme: all fields are Option<String>, default is None
  if src.theme.border.is_some() {
    dest.theme.border = src.theme.border;
  }
  if src.theme.text.is_some() {
    dest.theme.text = src.theme.text;
  }
  if src.theme.time.is_some() {
    dest.theme.time = src.theme.time;
  }
  if src.theme.container.is_some() {
    dest.theme.container = src.theme.container;
  }
  if src.theme.title.is_some() {
    dest.theme.title = src.theme.title;
  }
  if src.theme.greet.is_some() {
    dest.theme.greet = src.theme.greet;
  }
  if src.theme.prompt.is_some() {
    dest.theme.prompt = src.theme.prompt;
  }
  if src.theme.input.is_some() {
    dest.theme.input = src.theme.input;
  }
  if src.theme.action.is_some() {
    dest.theme.action = src.theme.action;
  }
  if src.theme.button.is_some() {
    dest.theme.button = src.theme.button;
  }
}

/// Load configuration from a specific path.
///
/// # Errors
///
/// Returns error if file cannot be read or contains invalid TOML
fn load_config_from_path(path: &Path) -> Result<Config, ConfigError> {
  let content = fs::read_to_string(path)?;
  match toml::from_str::<Config>(&content) {
    Ok(mut config) => {
      config.secret_mode_specified = toml::from_str::<toml::Table>(&content)
        .is_ok_and(|table| {
          table
            .get("secret")
            .and_then(toml::Value::as_table)
            .is_some_and(|secret| secret.contains_key("mode"))
        });
      Ok(config)
    },
    Err(e) => Err(toml_error(path, &content, e)),
  }
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

  let config_value = toml::Value::Table(merged);
  let secret_mode_specified = config_value
    .get("secret")
    .and_then(toml::Value::as_table)
    .is_some_and(|secret| secret.contains_key("mode"));
  let mut config: Config = config_value.try_into()?;
  config.secret_mode_specified = secret_mode_specified;
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

fn legacy_secret_mode(asterisks: bool) -> SecretMode {
  if asterisks {
    SecretMode::Characters
  } else {
    SecretMode::Hidden
  }
}

/// Applies the compatibility alias only if the canonical field was absent.
fn normalize_legacy_asterisks(config: &mut Config) {
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
  original_error: toml::de::Error,
) -> ConfigError {
  let span = match original_error.span() {
    Some(span) => span,
    None => return ConfigError::Parse(original_error),
  };

  let mut files = SimpleFiles::new();
  let file_id = files.add(path.to_string_lossy(), content);

  let diagnostic = Diagnostic::error()
    .with_message(original_error.message())
    .with_labels(vec![
      Label::primary(file_id, span.start..span.end)
        .with_message("TOML parse error"),
    ]);

  let config = term::Config::default();
  let writer = term::termcolor::StandardStream::stderr(
    term::termcolor::ColorChoice::Always,
  );
  term::emit_to_io_write(&mut writer.lock(), &config, &files, &diagnostic)
    .unwrap_or_else(|_| {
      eprintln!("TOML parse error: {}", original_error.message())
    });

  ConfigError::ParseWithContext {
    source: format!(
      "TOML parse error: {} at {}:{} (byte {})",
      original_error.message(),
      path.display(),
      files.line_index(file_id, span.start).unwrap_or(0) + 1,
      span.start
    ),
  }
}

/// Extract CLI arguments into a Config struct (highest priority source)
pub fn extract_cli_config(matches: &getopts::Matches) -> Config {
  let mut config = Config::default();
  // General config
  if matches.opt_present("debug") {
    config.general.debug = true;
  }
  if matches.opt_present("numlock") {
    config.general.numlock = true;
  }
  // Display config
  if matches.opt_present("time") {
    config.display.show_time = true;
  }
  if let Some(format) = matches.opt_str("time-format") {
    config.display.time_format = Some(format);
  }
  if matches.opt_present("title") {
    config.display.show_title = true;
  }
  if let Some(custom_title) = matches.opt_str("custom-title") {
    config.display.custom_title = Some(custom_title);
  }
  if let Some(greeting) = matches.opt_str("greeting") {
    config.display.greeting = Some(greeting);
  }
  if matches.opt_present("issue") {
    config.display.issue = true;
  }
  if matches.opt_present("battery") {
    config.display.battery = true;
  }
  if let Some(align) = matches.opt_str("greet-align") {
    config.display.align_greeting = match align.as_str() {
      "left" => AlignGreeting::Left,
      "right" => AlignGreeting::Right,
      _ => AlignGreeting::Center,
    };
  }
  // Remember config
  if let Some(user) = matches.opt_str("user") {
    config.remember.default_user = Some(user);
  }
  if matches.opt_present("remember") {
    config.remember.username = true;
  }
  if matches.opt_present("remember-session") {
    config.remember.session = true;
  }
  if matches.opt_present("remember-user-session") {
    config.remember.user_session = true;
  }
  // User menu config
  if matches.opt_present("user-menu") {
    config.user_menu.enabled = true;
  }
  if let Some(min_uid) = matches.opt_str("user-menu-min-uid")
    && let Ok(uid) = min_uid.parse::<u32>()
  {
    config.user_menu.min_uid = uid;
  }
  if let Some(max_uid) = matches.opt_str("user-menu-max-uid")
    && let Ok(uid) = max_uid.parse::<u32>()
  {
    config.user_menu.max_uid = uid;
  }
  // Session config
  if let Some(cmd) = matches.opt_str("cmd") {
    config.session.command = Some(cmd);
  }
  if matches.opt_present("env") {
    config.session.environments = matches.opt_strs("env");
  }
  if let Some(dirs) = matches.opt_str("sessions") {
    config.session.sessions_dirs = dirs.split(':').map(String::from).collect();
  }
  if let Some(dirs) = matches.opt_str("xsessions") {
    config.session.xsessions_dirs = dirs.split(':').map(String::from).collect();
  }
  if let Some(wrapper) = matches.opt_str("session-wrapper") {
    config.session.session_wrapper = Some(wrapper);
  }
  if !matches.opt_present("no-xsession-wrapper")
    && let Some(wrapper) = matches.opt_str("xsession-wrapper")
  {
    config.session.xsession_wrapper = Some(wrapper);
  }
  // Layout config
  if let Some(width) = matches.opt_str("width")
    && let Ok(w) = width.parse::<u16>()
  {
    config.layout.width = w;
  }
  if let Some(padding) = matches.opt_str("window-padding")
    && let Ok(p) = padding.parse::<u16>()
  {
    config.layout.window_padding = Some(p);
  }
  if let Some(padding) = matches.opt_str("container-padding")
    && let Ok(p) = padding.parse::<u16>()
  {
    config.layout.container_padding = Some(p);
  }
  if let Some(padding) = matches.opt_str("prompt-padding")
    && let Ok(p) = padding.parse::<u16>()
  {
    config.layout.prompt_padding = Some(p);
  }
  // Keybindings config
  if let Some(key) = matches.opt_str("kb-command")
    && let Ok(k) = key.parse::<u8>()
  {
    config.keybindings.command = k;
  }
  if let Some(key) = matches.opt_str("kb-sessions")
    && let Ok(k) = key.parse::<u8>()
  {
    config.keybindings.sessions = k;
  }
  if let Some(key) = matches.opt_str("kb-power")
    && let Ok(k) = key.parse::<u8>()
  {
    config.keybindings.power = k;
  }
  if let Some(key) = matches.opt_str("kb-background")
    && let Ok(k) = key.parse::<u8>()
  {
    config.keybindings.background = k;
  }
  // Secret config
  if matches.opt_present("asterisks") {
    config.secret.mode = SecretMode::Characters;
  }
  if let Some(chars) = matches.opt_str("asterisks-char") {
    config.secret.characters = chars;
  }
  // Power config
  if let Some(cmd) = matches.opt_str("power-shutdown") {
    config.power.shutdown = Some(cmd);
  }
  if let Some(cmd) = matches.opt_str("power-reboot") {
    config.power.reboot = Some(cmd);
  }
  if let Some(cmd) = matches.opt_str("power-suspend") {
    config.power.suspend = Some(cmd);
  }
  if let Some(cmd) = matches.opt_str("power-hibernate") {
    config.power.hibernate = Some(cmd);
  }
  if matches.opt_present("power-no-setsid") {
    config.power.use_setsid = false;
  }
  // Background animation config
  if let Some(kind) = matches.opt_str("background") {
    config.background.kind = Some(kind);
  }
  if let Some(fps) = matches.opt_str("background-fps")
    && let Ok(v) = fps.parse::<u32>()
  {
    config.background.fps = Some(v);
  }
  if let Some(h) = matches.opt_str("doom-height")
    && let Ok(v) = h.parse::<u8>()
  {
    config.background.doom.height = Some(v);
  }
  if let Some(s) = matches.opt_str("doom-spread")
    && let Ok(v) = s.parse::<u8>()
  {
    config.background.doom.spread = Some(v);
  }
  if let Some(colors) = matches.opt_str("doom-colors") {
    let parts: Vec<&str> = colors.split(',').map(str::trim).collect();
    if parts.len() == 3 {
      config.background.doom.top_color = Some(parts[0].to_string());
      config.background.doom.middle_color = Some(parts[1].to_string());
      config.background.doom.bottom_color = Some(parts[2].to_string());
    }
  }
  if let Some(colors) = matches.opt_str("matrix-colors") {
    let parts: Vec<&str> = colors.split(',').map(str::trim).collect();
    if parts.len() == 3 {
      config.background.matrix.head_color = Some(parts[0].to_string());
      config.background.matrix.bright_color = Some(parts[1].to_string());
      config.background.matrix.dim_color = Some(parts[2].to_string());
    }
  }
  if let Some(s) = matches.opt_str("matrix-length") {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() == 2
      && let (Ok(lo), Ok(hi)) =
        (parts[0].parse::<u16>(), parts[1].parse::<u16>())
    {
      config.background.matrix.min_length = Some(lo);
      config.background.matrix.max_length = Some(hi);
    }
  }
  if let Some(s) = matches.opt_str("matrix-speed") {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() == 2
      && let (Ok(lo), Ok(hi)) =
        (parts[0].parse::<f32>(), parts[1].parse::<f32>())
    {
      config.background.matrix.min_speed = Some(lo);
      config.background.matrix.max_speed = Some(hi);
    }
  }
  config
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

    // Check mutually exclusive options
    if self.display.issue && self.display.greeting.is_some() {
      return Err(ConfigError::MutuallyExclusive(
        "display.issue".to_string(),
        "display.greeting".to_string(),
      ));
    }

    // Check dependencies
    if self.remember.user_session && !self.remember.username {
      return Err(ConfigError::Dependency(
        "remember.user_session requires remember.username".to_string(),
      ));
    }

    // Check UID ranges
    if self.user_menu.min_uid > self.user_menu.max_uid {
      return Err(ConfigError::InvalidRange(
        "user_menu.min_uid must not exceed user_menu.max_uid".to_string(),
      ));
    }

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

    // Check keybindings are distinct
    let keys = [
      self.keybindings.command,
      self.keybindings.sessions,
      self.keybindings.power,
      self.keybindings.background,
    ];
    if keys.iter().collect::<HashSet<_>>().len() != keys.len() {
      return Err(ConfigError::DuplicateKeybindings);
    }

    // Check F-key ranges
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

    // Validate time format if provided
    if let Some(ref format) = self.display.time_format
      && chrono::format::StrftimeItems::new(format)
        .any(|item| matches!(item, chrono::format::Item::Error))
    {
      return Err(ConfigError::InvalidTimeFormat);
    }

    // Validate session wrapper executables if requested
    if validate_wrappers {
      if let Some(ref wrapper) = self.session.session_wrapper {
        self.validate_wrapper_command(wrapper)?;
      }
      if let Some(ref wrapper) = self.session.xsession_wrapper {
        self.validate_wrapper_command(wrapper)?;
      }
    }

    // Validate [[outputs]] entries
    {
      let primary_count = self.outputs.iter().filter(|o| o.primary).count();
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

      // Warn if [[outputs]] is configured but all are disabled
      if !self.outputs.is_empty() && self.outputs.iter().all(|o| !o.enabled) {
        warnings.push(
          "All [[outputs]] entries have `enabled = false`; no output will be \
           used for terminal sizing"
            .to_string(),
        );
      }
    }

    // Validate [terminal].
    // Both cols and rows must be set together
    if let Some(reason) = self.terminal.invalid_reason() {
      return Err(ConfigError::Validation(reason));
    }

    // Add validation warnings for potentially problematic configurations
    self.check_warnings(&mut warnings);

    Ok(warnings)
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
      != all_session_dirs
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
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
  fn validate_wrapper_command(&self, command: &str) -> Result<(), ConfigError> {
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
  if let Ok(path) = std::env::var("PATH") {
    for dir in std::env::split_paths(&path) {
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
  use super::*;

  #[test]
  fn toml_layers_preserve_explicit_default_values() {
    let mut system: toml::Table =
      toml::from_str("[display]\nshow_time = true\n[layout]\nwidth = 120\n")
        .expect("system config is valid");
    let user: toml::Table =
      toml::from_str("[display]\nshow_time = false\n[layout]\nwidth = 80\n")
        .expect("user config is valid");

    merge_toml_tables(&mut system, user);
    let config: Config = toml::Value::Table(system)
      .try_into()
      .expect("merged config is valid");

    assert!(!config.display.show_time);
    assert_eq!(config.layout.width, 80);
  }

  #[test]
  fn canonical_secret_mode_wins_over_legacy_alias() {
    let mut config: Config = toml::from_str(
      r#"
[display]
asterisks = true

[secret]
mode = "hidden"
"#,
    )
    .expect("configuration must parse");
    config.secret_mode_specified = true;

    normalize_legacy_asterisks(&mut config);

    assert_eq!(config.secret.mode, SecretMode::Hidden);
  }

  #[test]
  fn legacy_false_overrides_a_lower_characters_layer() {
    let mut resolved = Config::default();
    resolved.secret.mode = SecretMode::Characters;

    let mut legacy_layer = Config::default();
    legacy_layer.display.asterisks = Some(false);
    apply_config_layer(&mut resolved, legacy_layer);

    assert_eq!(resolved.secret.mode, SecretMode::Hidden);
    assert!(
      resolved
        .validate(false)
        .expect("legacy configuration is valid")
        .iter()
        .any(|warning| warning.contains("display.asterisks is deprecated"))
    );
  }

  #[test]
  fn canonical_higher_layer_overrides_legacy_alias() {
    let mut resolved = Config::default();
    resolved.display.asterisks = Some(true);

    let mut canonical_layer = Config::default();
    canonical_layer.secret.mode = SecretMode::Hidden;
    canonical_layer.secret_mode_specified = true;
    apply_config_layer(&mut resolved, canonical_layer);
    normalize_legacy_asterisks(&mut resolved);

    assert_eq!(resolved.secret.mode, SecretMode::Hidden);
  }

  #[test]
  fn test_mutual_exclusive_remember_flags() {
    let toml_content = r"
[remember]
username = true
session = true
user_session = true
";

    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");

    assert!(config.remember.session);
    assert!(config.remember.user_session);

    // Both flags being set is now a warning, not a hard error, so that the
    // rest of the config is still applied. user_session takes behavioral
    // precedence when both are true.
    let result = config.validate(false);
    assert!(
      result.is_ok(),
      "Both remember.session and remember.user_session being true should \
       produce a warning, not an error"
    );

    let warnings = result.unwrap();
    assert!(
      warnings.iter().any(|w| {
        w.contains("remember.session") && w.contains("remember.user_session")
      }),
      "Expected a warning about conflicting remember options, got: \
       {warnings:?}"
    );
  }

  #[test]
  fn test_keybindings_distinctness_in_config() {
    let toml_content = r"
[keybindings]
command = 3
sessions = 3
power = 7
";

    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let validation_result = config.validate(false);

    match validation_result {
      Err(ConfigError::DuplicateKeybindings) => {},
      _ => {
        panic!(
          "Expected DuplicateKeybindings error, got: {validation_result:?}"
        );
      },
    }
  }

  #[test]
  fn test_session_config_default_consistency() {
    let default_config = Config::default();

    let partial_toml = r#"
[session]
command = "test"
"#;
    let partial_config: Config =
      toml::from_str(partial_toml).expect("Failed to parse partial TOML");

    assert_eq!(
      default_config.session.sessions_dirs,
      partial_config.session.sessions_dirs,
      "Default and partially deserialized sessions_dirs should match"
    );
  }

  #[test]
  fn test_power_config_default_consistency() {
    let default_config = Config::default();

    let partial_toml = r#"
[power]
shutdown = "poweroff"
"#;
    let partial_config: Config =
      toml::from_str(partial_toml).expect("Failed to parse partial TOML");

    assert_eq!(
      default_config.power.use_setsid, partial_config.power.use_setsid,
      "Default and partially deserialized use_setsid should match"
    );
  }

  #[test]
  fn test_wrapper_validation_empty_string() {
    let empty_wrapper = r#"
[session]
session_wrapper = ""
"#;

    let mut config: Config =
      toml::from_str(empty_wrapper).expect("Failed to parse TOML");

    config.session.xsession_wrapper = None;

    let result = config.validate(true);

    assert!(
      result.is_err(),
      "Empty wrapper command should fail validation"
    );
  }

  // [[outputs]] validation
  #[test]
  fn test_outputs_toml_roundtrip() {
    let toml_content = r#"
[[outputs]]
connector = "DP-1"
primary = true

[[outputs]]
connector = "HDMI-A-1"
enabled = false
"#;
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse [[outputs]] TOML");

    assert_eq!(config.outputs.len(), 2);
    assert_eq!(config.outputs[0].connector, "DP-1");
    assert!(config.outputs[0].primary);
    assert!(config.outputs[0].enabled); // default = true
    assert_eq!(config.outputs[1].connector, "HDMI-A-1");
    assert!(!config.outputs[1].primary); // default = false
    assert!(!config.outputs[1].enabled);

    // Validation should pass
    assert!(config.validate(false).is_ok());
  }

  #[test]
  fn test_outputs_multiple_primary_is_error() {
    let toml_content = r#"
[[outputs]]
connector = "DP-1"
primary = true

[[outputs]]
connector = "HDMI-A-1"
primary = true
"#;
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "Multiple primary outputs should be a Validation error, got: {result:?}"
    );
  }

  #[test]
  fn test_outputs_empty_connector_is_error() {
    let toml_content = r#"
[[outputs]]
connector = ""
"#;
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "Empty connector name should be a Validation error, got: {result:?}"
    );
  }

  #[test]
  fn test_outputs_path_separator_in_connector_is_error() {
    for bad in &["../DP-1", "/sys/class/drm/DP-1", "foo/bar"] {
      let config: Config =
        toml::from_str(&format!("[[outputs]]\nconnector = \"{bad}\"\n"))
          .expect("Failed to parse TOML");
      let result = config.validate(false);
      assert!(
        matches!(result, Err(ConfigError::Validation(_))),
        "Connector '{bad}' with path separator should be a Validation error, \
         got: {result:?}"
      );
    }
  }

  #[test]
  fn test_outputs_valid_connector_names() {
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
        toml::from_str(&format!("[[outputs]]\nconnector = \"{good}\"\n"))
          .expect("Failed to parse TOML");
      assert!(
        config.validate(false).is_ok(),
        "Connector '{good}' should be valid, but validation failed"
      );
    }
  }

  #[test]
  fn test_outputs_all_disabled_is_warning() {
    let toml_content = r#"
[[outputs]]
connector = "DP-1"
enabled = false

[[outputs]]
connector = "HDMI-A-1"
enabled = false
"#;
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let result = config.validate(false);
    assert!(
      result.is_ok(),
      "All-disabled outputs should not be an error"
    );
    let warnings = result.unwrap();
    assert!(
      warnings.iter().any(|w| w.contains("enabled = false")),
      "Expected a warning about all outputs being disabled, got: {warnings:?}"
    );
  }

  #[test]
  fn test_outputs_single_primary_passes() {
    let toml_content = r#"
[[outputs]]
connector = "DP-1"
primary = true

[[outputs]]
connector = "HDMI-A-1"
"#;
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    assert!(config.validate(false).is_ok());
  }

  // [terminal] validation
  #[test]
  fn test_terminal_both_set_passes() {
    let toml_content = r"
[terminal]
cols = 237
rows = 52
";
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    assert_eq!(config.terminal.cols, Some(237));
    assert_eq!(config.terminal.rows, Some(52));
    assert!(config.validate(false).is_ok());
  }

  #[test]
  fn test_terminal_cols_without_rows_is_error() {
    let toml_content = r"
[terminal]
cols = 237
";
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "cols without rows should be a Validation error, got: {result:?}"
    );
  }

  #[test]
  fn test_terminal_rows_without_cols_is_error() {
    let toml_content = r"
[terminal]
rows = 52
";
    let config: Config =
      toml::from_str(toml_content).expect("Failed to parse TOML");
    let result = config.validate(false);
    assert!(
      matches!(result, Err(ConfigError::Validation(_))),
      "rows without cols should be a Validation error, got: {result:?}"
    );
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
  fn test_wrapper_validation_whitespace_only() {
    let whitespace_wrapper = r#"
[session]
session_wrapper = "   "
"#;

    let mut config: Config =
      toml::from_str(whitespace_wrapper).expect("Failed to parse TOML");

    config.session.xsession_wrapper = None;

    let result = config.validate(true);

    assert!(
      result.is_err(),
      "Whitespace-only wrapper command should fail validation"
    );
  }

  // Config priority tests

  #[test]
  fn test_cli_only_no_config() {
    let mut config = Config::default();

    let mut cli = Config::default();
    cli.keybindings.sessions = 5;

    apply_config_layer(&mut config, cli);

    assert_eq!(config.keybindings.sessions, 5);
  }

  #[test]
  fn test_config_overrides_defaults() {
    let mut config = Config::default();

    let mut file_config = Config::default();
    file_config.keybindings.command = 3;

    apply_config_layer(&mut config, file_config);

    assert_eq!(config.keybindings.command, 3);
  }

  #[test]
  fn test_user_overrides_system() {
    let mut config = Config::default();
    config.keybindings.command = 5;

    let mut user = Config::default();
    user.keybindings.command = 7;

    apply_config_layer(&mut config, user);

    assert_eq!(config.keybindings.command, 7);
  }

  #[test]
  fn test_env_overrides_user() {
    let mut config = Config::default();
    config.keybindings.power = 10;

    let mut env = Config::default();
    env.keybindings.power = 8;

    apply_config_layer(&mut config, env);

    assert_eq!(config.keybindings.power, 8);
  }

  #[test]
  fn test_cli_overrides_env() {
    let mut config = Config::default();
    config.keybindings.sessions = 3;

    let mut cli = Config::default();
    cli.keybindings.sessions = 9;

    apply_config_layer(&mut config, cli);

    assert_eq!(config.keybindings.sessions, 9);
  }

  #[test]
  fn test_full_priority_chain() {
    let mut config = Config::default();
    assert_eq!(config.keybindings.command, 2);

    let mut system = Config::default();
    system.keybindings.command = 5;
    apply_config_layer(&mut config, system);
    assert_eq!(config.keybindings.command, 5);

    let mut user = Config::default();
    user.keybindings.command = 7;
    apply_config_layer(&mut config, user);
    assert_eq!(config.keybindings.command, 7);

    let mut env = Config::default();
    env.keybindings.command = 9;
    apply_config_layer(&mut config, env);
    assert_eq!(config.keybindings.command, 9);

    let mut cli = Config::default();
    cli.keybindings.command = 11;
    apply_config_layer(&mut config, cli);
    assert_eq!(config.keybindings.command, 11);
  }

  #[test]
  fn test_lower_layer_preserved_when_higher_layer_uses_defaults() {
    // System config sets a non-default value
    let mut config = Config::default();
    config.keybindings.power = 10;
    config.display.show_time = true;
    config.remember.username = true;

    // User config only touches one unrelated field; all others remain at
    // their defaults and must NOT overwrite the system values above.
    let mut user = Config::default();
    user.display.greeting = Some("hello".to_string());
    apply_config_layer(&mut config, user);

    assert_eq!(
      config.keybindings.power, 10,
      "system keybinding must survive"
    );
    assert!(config.display.show_time, "system show_time must survive");
    assert!(
      config.remember.username,
      "system remember.username must survive"
    );
    assert_eq!(config.display.greeting, Some("hello".to_string()));
  }
}

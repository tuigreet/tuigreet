use std::{
  fmt::Display,
  path::{Path, PathBuf},
  str::FromStr,
};

use pound::{Parse, Source, Sourced};

use crate::{
  AlignGreeting,
  Config,
  ConfigError,
  SecretMode,
  ThemeConfig,
  WidgetPosition,
};

/// Typed command-line and environment configuration for tuigreet.
#[expect(
  clippy::struct_excessive_bools,
  reason = "Each field represents an independent startup action"
)]
#[derive(Clone, Debug, Parse)]
#[pound(name = "tuigreet")]
pub struct Cli {
  /// print version information and exit
  #[pound(short = 'v', short_alias = 'V', long, exclusive)]
  pub version: bool,

  /// path to configuration file
  #[pound(long)]
  pub config: Option<PathBuf>,

  /// disable loading configuration files
  #[pound(long, conflicts_with = config)]
  pub no_config: bool,

  /// print effective configuration and exit
  #[pound(long)]
  pub dump_config: bool,

  /// list available DRM outputs and exit
  #[pound(long, exclusive)]
  pub list_outputs: bool,

  /// visual mock-up mode: skip the greetd socket and fake authentication
  #[pound(long = "mock")]
  pub mock_mode: bool,

  #[pound(flatten)]
  overrides: Overrides,
}

impl Cli {
  /// Parse the process command line, printing pound diagnostics and exiting on
  /// help or invalid arguments.
  #[must_use]
  pub fn parse() -> Self {
    <Self as Parse>::parse()
  }

  /// Parse an argument sequence that does not include the executable name.
  ///
  /// # Errors
  /// Returns a pound diagnostic for malformed or incompatible arguments.
  pub fn try_parse_from<'a, I>(args: I) -> Result<Self, pound::Error>
  where
    I: IntoIterator<Item = &'a str>,
  {
    <Self as Parse>::try_parse_from(args)
  }

  /// The explicitly selected configuration path, if any.
  #[must_use]
  pub fn config_path(&self) -> Option<&Path> {
    self.config.as_deref()
  }

  /// Apply environment and command-line values over a file-backed config.
  ///
  /// # Errors
  /// Returns an error when a compound override is malformed.
  pub(crate) fn apply_to(
    &self,
    config: &mut Config,
  ) -> Result<(), ConfigError> {
    self.overrides.apply_to(config)
  }
}

#[derive(Clone, Debug, Parse)]
struct Overrides {
  #[pound(flatten)]
  general:     GeneralOverrides,
  #[pound(flatten)]
  session:     SessionOverrides,
  #[pound(flatten)]
  display:     DisplayOverrides,
  #[pound(flatten)]
  remember:    RememberOverrides,
  #[pound(flatten)]
  user_menu:   UserMenuOverrides,
  #[pound(flatten)]
  secret:      SecretOverrides,
  #[pound(flatten)]
  layout:      LayoutOverrides,
  #[pound(flatten)]
  power:       PowerOverrides,
  #[pound(flatten)]
  keybindings: KeybindingOverrides,
  #[pound(flatten)]
  theme:       ThemeOverrides,
  #[pound(flatten)]
  background:  BackgroundOverrides,
}

impl Overrides {
  fn apply_to(&self, config: &mut Config) -> Result<(), ConfigError> {
    self.general.apply_to(config);
    self.session.apply_to(config);
    self.display.apply_to(config);
    self.remember.apply_to(config);
    self.user_menu.apply_to(config);
    self.secret.apply_to(config);
    self.layout.apply_to(config);
    self.power.apply_to(config);
    self.keybindings.apply_to(config);
    self.theme.apply_to(config)?;
    self.background.apply_to(config)?;
    Ok(())
  }
}

#[derive(Clone, Debug, Parse)]
struct GeneralOverrides {
  /// enable or disable debug logging
  #[pound(long = "debug-enabled", negate, env = "TUIGREET_DEBUG", hidden)]
  debug_enabled: Sourced<bool>,

  /// enable debug logging to FILE, or to /tmp/tuigreet.log when omitted
  #[pound(short = 'd', long = "debug", default_missing = "/tmp/tuigreet.log")]
  debug_log: Option<Sourced<String>>,

  /// path to the debug log file
  #[pound(long, env = "TUIGREET_LOG_FILE")]
  log_file: Option<Sourced<String>>,

  /// enable numlock on startup
  #[pound(long, negate, env = "TUIGREET_NUMLOCK")]
  numlock: Sourced<bool>,
}

impl GeneralOverrides {
  fn apply_to(&self, config: &mut Config) {
    if let Some(value) = flag(self.debug_enabled) {
      config.general.debug = value;
    }
    if let Some(Sourced { value, .. }) = &self.log_file {
      config.general.log_file.clone_from(value);
    }
    if let Some(Sourced { value, .. }) = &self.debug_log {
      config.general.debug = true;
      config.general.log_file.clone_from(value);
    }
    if let Some(value) = flag(self.numlock) {
      config.general.numlock = value;
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct SessionOverrides {
  /// command to run instead of a selected session
  #[pound(short = 'c', long = "cmd", env = "TUIGREET_SESSION_COMMAND")]
  command: Option<Sourced<String>>,

  /// environment variable for the default command; may be repeated
  #[pound(long = "env")]
  environments: Vec<String>,

  #[pound(
    long = "environments",
    env = "TUIGREET_ENVIRONMENTS",
    delimiter = ':',
    hidden
  )]
  environment_defaults: Vec<String>,

  /// colon-separated list of Wayland session paths
  #[pound(
    short = 's',
    long = "sessions",
    env = "TUIGREET_SESSIONS_DIRS",
    delimiter = ':'
  )]
  sessions_dirs: Vec<String>,

  /// colon-separated list of X11 session paths
  #[pound(
    short = 'x',
    long = "xsessions",
    env = "TUIGREET_XSESSIONS_DIRS",
    delimiter = ':'
  )]
  xsessions_dirs: Vec<String>,

  /// wrapper command used to initialize non-X11 sessions
  #[pound(long, env = "TUIGREET_SESSION_WRAPPER")]
  session_wrapper: Option<Sourced<String>>,

  /// wrapper command used to initialize X11 sessions
  #[pound(long, env = "TUIGREET_XSESSION_WRAPPER")]
  xsession_wrapper: Option<Sourced<String>>,

  /// do not wrap commands for X11 sessions
  #[pound(long)]
  no_xsession_wrapper: bool,

  /// regex of the path of the default selected session
  #[pound(long, env = "TUIGREET_DEFAULT_SESSION")]
  default_session: Option<Sourced<String>>,
}

impl SessionOverrides {
  fn apply_to(&self, config: &mut Config) {
    assign_option(&mut config.session.command, self.command.as_ref());
    assign_list(&mut config.session.environments, &self.environment_defaults);
    if !self.environments.is_empty() {
      config.session.environments.clone_from(&self.environments);
    }
    assign_list(&mut config.session.sessions_dirs, &self.sessions_dirs);
    assign_list(&mut config.session.xsessions_dirs, &self.xsessions_dirs);
    assign_option(
      &mut config.session.session_wrapper,
      self.session_wrapper.as_ref(),
    );
    assign_option(
      &mut config.session.xsession_wrapper,
      self.xsession_wrapper.as_ref(),
    );
    if self.no_xsession_wrapper {
      config.session.xsession_wrapper = None;
    }
    assign_option(
      &mut config.session.default_session,
      self.default_session.as_ref(),
    );
  }
}

#[derive(Clone, Debug, Parse)]
struct DisplayOverrides {
  /// display the current date and time
  #[pound(short = 't', long = "time", negate, env = "TUIGREET_TIME")]
  show_time: Sourced<bool>,

  /// custom strftime format for displaying date and time
  #[pound(long, env = "TUIGREET_TIME_FORMAT")]
  time_format: Option<Sourced<String>>,

  /// show custom text above the login prompt
  #[pound(short = 'g', long = "greeting", env = "TUIGREET_GREETING")]
  greeting: Option<Sourced<String>>,

  /// show the login form title
  #[pound(long = "title", negate, env = "TUIGREET_TITLE")]
  show_title: Sourced<bool>,

  /// custom title for the login form
  #[pound(long, env = "TUIGREET_CUSTOM_TITLE")]
  custom_title: Option<Sourced<String>>,

  /// show the host's issue file
  #[pound(short = 'i', long = "issue", negate, env = "TUIGREET_ISSUE")]
  issue: Sourced<bool>,

  /// display battery percentage
  #[pound(short = 'b', long = "battery", negate)]
  battery: Sourced<bool>,

  /// alignment of greeting text
  #[pound(long = "greet-align", env = "TUIGREET_ALIGN_GREETING")]
  align_greeting: Option<Sourced<AlignGreeting>>,
}

impl DisplayOverrides {
  fn apply_to(&self, config: &mut Config) {
    if let Some(value) = flag(self.show_time) {
      config.display.show_time = value;
    }
    assign_option(&mut config.display.time_format, self.time_format.as_ref());
    assign_option(&mut config.display.greeting, self.greeting.as_ref());
    if let Some(value) = flag(self.show_title) {
      config.display.show_title = value;
    }
    assign_option(&mut config.display.custom_title, self.custom_title.as_ref());
    if let Some(value) = flag(self.issue) {
      config.display.issue = value;
    }
    if let Some(value) = flag(self.battery) {
      config.display.battery = value;
    }
    if let Some(Sourced { value, .. }) = &self.align_greeting {
      config.display.align_greeting = value.clone();
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct RememberOverrides {
  /// pre-fill the username field
  #[pound(short = 'u', long = "user")]
  default_user: Option<Sourced<String>>,

  /// remember the last logged-in username
  #[pound(short = 'r', long = "remember", negate, env = "TUIGREET_REMEMBER")]
  username: Sourced<bool>,

  /// remember the last selected session
  #[pound(
    long = "remember-session",
    negate,
    env = "TUIGREET_REMEMBER_SESSION"
  )]
  session: Sourced<bool>,

  /// remember the last selected session for each user
  #[pound(
    long = "remember-user-session",
    negate,
    env = "TUIGREET_REMEMBER_USER_SESSION"
  )]
  user_session: Sourced<bool>,
}

impl RememberOverrides {
  fn apply_to(&self, config: &mut Config) {
    assign_option(
      &mut config.remember.default_user,
      self.default_user.as_ref(),
    );
    if let Some(value) = flag(self.username) {
      config.remember.username = value;
    }
    if let Some(value) = flag(self.session) {
      config.remember.session = value;
    }
    if let Some(value) = flag(self.user_session) {
      config.remember.user_session = value;
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct UserMenuOverrides {
  /// allow graphical selection of users from a menu
  #[pound(long = "user-menu", negate, env = "TUIGREET_USER_MENU")]
  enabled: Sourced<bool>,

  /// minimum UID to display in the user selection menu
  #[pound(long = "user-menu-min-uid", env = "TUIGREET_USER_MENU_MIN_UID")]
  min_uid: Option<Sourced<u32>>,

  /// maximum UID to display in the user selection menu
  #[pound(long = "user-menu-max-uid", env = "TUIGREET_USER_MENU_MAX_UID")]
  max_uid: Option<Sourced<u32>>,
}

impl UserMenuOverrides {
  fn apply_to(&self, config: &mut Config) {
    if let Some(value) = flag(self.enabled) {
      config.user_menu.enabled = value;
    }
    if let Some(Sourced { value, .. }) = &self.min_uid {
      config.user_menu.min_uid = *value;
    }
    if let Some(Sourced { value, .. }) = &self.max_uid {
      config.user_menu.max_uid = *value;
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct SecretOverrides {
  /// secret display mode
  #[pound(long = "secret-mode", env = "TUIGREET_SECRET_MODE")]
  mode: Option<Sourced<SecretMode>>,

  /// characters used to redact secrets
  #[pound(long = "secret-characters", env = "TUIGREET_SECRET_CHARACTERS")]
  characters: Option<Sourced<String>>,

  /// display characters when a secret is typed
  #[pound(long = "asterisks")]
  asterisks: bool,

  /// characters used by --asterisks
  #[pound(long = "asterisks-char")]
  asterisks_char: Option<Sourced<String>>,
}

impl SecretOverrides {
  fn apply_to(&self, config: &mut Config) {
    if let Some(Sourced { value, .. }) = &self.mode {
      config.secret.mode = value.clone();
      config.secret_mode_specified = true;
    }
    if let Some(Sourced { value, .. }) = &self.characters {
      config.secret.characters.clone_from(value);
    }
    if self.asterisks {
      config.secret.mode = SecretMode::Characters;
      config.secret_mode_specified = true;
    }
    if let Some(Sourced { value, .. }) = &self.asterisks_char {
      config.secret.characters.clone_from(value);
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct LayoutOverrides {
  /// width of the main prompt
  #[pound(short = 'w', long = "width", env = "TUIGREET_WIDTH", min = "1")]
  width: Option<Sourced<u16>>,

  /// padding inside the terminal area
  #[pound(long, env = "TUIGREET_WINDOW_PADDING", max = "10")]
  window_padding: Option<Sourced<u16>>,

  /// padding inside the main prompt container
  #[pound(long, env = "TUIGREET_CONTAINER_PADDING", max = "10")]
  container_padding: Option<Sourced<u16>>,

  /// padding between prompt rows
  #[pound(long, env = "TUIGREET_PROMPT_PADDING", max = "10")]
  prompt_padding: Option<Sourced<u16>>,

  /// time widget position
  #[pound(long, env = "TUIGREET_TIME_POSITION")]
  time_position: Option<Sourced<WidgetPosition>>,

  /// status widget position
  #[pound(long, env = "TUIGREET_STATUS_POSITION")]
  status_position: Option<Sourced<WidgetPosition>>,

  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_RESET", hidden)]
  status_show_reset:          Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_COMMAND", hidden)]
  status_show_command:        Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_SESSION", hidden)]
  status_show_session:        Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_POWER", hidden)]
  status_show_power:          Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_BACKGROUND", hidden)]
  status_show_background:     Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_SESSION_STATUS", hidden)]
  status_show_session_status: Sourced<bool>,
  #[pound(long, negate, env = "TUIGREET_STATUS_SHOW_CAPS_LOCK", hidden)]
  status_show_caps_lock:      Sourced<bool>,
}

impl LayoutOverrides {
  fn apply_to(&self, config: &mut Config) {
    if let Some(Sourced { value, .. }) = &self.width {
      config.layout.width = *value;
    }
    if let Some(Sourced { value, .. }) = &self.window_padding {
      config.layout.window_padding = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.container_padding {
      config.layout.container_padding = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.prompt_padding {
      config.layout.prompt_padding = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.time_position {
      config.layout.widgets.time_position = *value;
    }
    if let Some(Sourced { value, .. }) = &self.status_position {
      config.layout.widgets.status_position = *value;
    }
    for (target, value) in [
      (
        &mut config.layout.widgets.status_bar.show_reset,
        self.status_show_reset,
      ),
      (
        &mut config.layout.widgets.status_bar.show_command,
        self.status_show_command,
      ),
      (
        &mut config.layout.widgets.status_bar.show_session,
        self.status_show_session,
      ),
      (
        &mut config.layout.widgets.status_bar.show_power,
        self.status_show_power,
      ),
      (
        &mut config.layout.widgets.status_bar.show_background,
        self.status_show_background,
      ),
      (
        &mut config.layout.widgets.status_bar.show_session_status,
        self.status_show_session_status,
      ),
      (
        &mut config.layout.widgets.status_bar.show_caps_lock,
        self.status_show_caps_lock,
      ),
    ] {
      if let Some(value) = flag(value) {
        *target = value;
      }
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct PowerOverrides {
  /// command used to shut down the system
  #[pound(long, env = "TUIGREET_POWER_SHUTDOWN")]
  power_shutdown:  Option<Sourced<String>>,
  /// command used to reboot the system
  #[pound(long, env = "TUIGREET_POWER_REBOOT")]
  power_reboot:    Option<Sourced<String>>,
  /// command used to suspend the system
  #[pound(long, env = "TUIGREET_POWER_SUSPEND")]
  power_suspend:   Option<Sourced<String>>,
  /// command used to hibernate the system
  #[pound(long, env = "TUIGREET_POWER_HIBERNATE")]
  power_hibernate: Option<Sourced<String>>,

  /// prefix power commands with setsid
  #[pound(
    long = "power-setsid",
    negate = "power-no-setsid",
    env = "TUIGREET_POWER_USE_SETSID"
  )]
  use_setsid: Sourced<bool>,
}

impl PowerOverrides {
  fn apply_to(&self, config: &mut Config) {
    assign_option(&mut config.power.shutdown, self.power_shutdown.as_ref());
    assign_option(&mut config.power.reboot, self.power_reboot.as_ref());
    assign_option(&mut config.power.suspend, self.power_suspend.as_ref());
    assign_option(&mut config.power.hibernate, self.power_hibernate.as_ref());
    if let Some(value) = flag(self.use_setsid) {
      config.power.use_setsid = value;
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct KeybindingOverrides {
  /// F-key used to open the command menu
  #[pound(
    long = "kb-command",
    env = "TUIGREET_KB_COMMAND",
    min = "1",
    max = "12"
  )]
  command:    Option<Sourced<u8>>,
  /// F-key used to open the sessions menu
  #[pound(
    long = "kb-sessions",
    env = "TUIGREET_KB_SESSIONS",
    min = "1",
    max = "12"
  )]
  sessions:   Option<Sourced<u8>>,
  /// F-key used to open the power menu
  #[pound(long = "kb-power", env = "TUIGREET_KB_POWER", min = "1", max = "12")]
  power:      Option<Sourced<u8>>,
  /// F-key used to open the background menu
  #[pound(long = "kb-background", min = "1", max = "12")]
  background: Option<Sourced<u8>>,
}

impl KeybindingOverrides {
  const fn apply_to(&self, config: &mut Config) {
    if let Some(Sourced { value, .. }) = &self.command {
      config.keybindings.command = *value;
    }
    if let Some(Sourced { value, .. }) = &self.sessions {
      config.keybindings.sessions = *value;
    }
    if let Some(Sourced { value, .. }) = &self.power {
      config.keybindings.power = *value;
    }
    if let Some(Sourced { value, .. }) = &self.background {
      config.keybindings.background = *value;
    }
  }
}

#[derive(Clone, Debug, Parse)]
struct ThemeOverrides {
  /// application theme colors as semicolon-separated key=value pairs
  #[pound(long = "theme", env = "TUIGREET_THEME")]
  spec: Option<Sourced<String>>,

  #[pound(long = "theme-border", env = "TUIGREET_THEME_BORDER", hidden)]
  border:    Option<Sourced<String>>,
  #[pound(long = "theme-text", env = "TUIGREET_THEME_TEXT", hidden)]
  text:      Option<Sourced<String>>,
  #[pound(long = "theme-time", env = "TUIGREET_THEME_TIME", hidden)]
  time:      Option<Sourced<String>>,
  #[pound(long = "theme-container", env = "TUIGREET_THEME_CONTAINER", hidden)]
  container: Option<Sourced<String>>,
  #[pound(long = "theme-title", env = "TUIGREET_THEME_TITLE", hidden)]
  title:     Option<Sourced<String>>,
  #[pound(long = "theme-greet", env = "TUIGREET_THEME_GREET", hidden)]
  greet:     Option<Sourced<String>>,
  #[pound(long = "theme-prompt", env = "TUIGREET_THEME_PROMPT", hidden)]
  prompt:    Option<Sourced<String>>,
  #[pound(long = "theme-input", env = "TUIGREET_THEME_INPUT", hidden)]
  input:     Option<Sourced<String>>,
  #[pound(long = "theme-action", env = "TUIGREET_THEME_ACTION", hidden)]
  action:    Option<Sourced<String>>,
  #[pound(long = "theme-button", env = "TUIGREET_THEME_BUTTON", hidden)]
  button:    Option<Sourced<String>>,
}

impl ThemeOverrides {
  fn apply_to(&self, config: &mut Config) -> Result<(), ConfigError> {
    if let Some(spec) = self
      .spec
      .as_ref()
      .filter(|spec| matches!(spec.source, Source::Env(_)))
    {
      merge_theme(&mut config.theme, parse_theme(&spec.value)?);
    }

    assign_option(&mut config.theme.border, self.border.as_ref());
    assign_option(&mut config.theme.text, self.text.as_ref());
    assign_option(&mut config.theme.time, self.time.as_ref());
    assign_option(&mut config.theme.container, self.container.as_ref());
    assign_option(&mut config.theme.title, self.title.as_ref());
    assign_option(&mut config.theme.greet, self.greet.as_ref());
    assign_option(&mut config.theme.prompt, self.prompt.as_ref());
    assign_option(&mut config.theme.input, self.input.as_ref());
    assign_option(&mut config.theme.action, self.action.as_ref());
    assign_option(&mut config.theme.button, self.button.as_ref());

    if let Some(spec) = self
      .spec
      .as_ref()
      .filter(|spec| matches!(spec.source, Source::CommandLine))
    {
      config.theme = parse_theme(&spec.value)?;
    }
    Ok(())
  }
}

#[derive(Clone, Debug, Parse)]
struct BackgroundOverrides {
  /// background animation ('doom', 'matrix', or 'none')
  #[pound(long = "background")]
  kind:          Option<Sourced<String>>,
  /// render rate while a background animation is active
  #[pound(long = "background-fps", min = "1")]
  fps:           Option<Sourced<u32>>,
  /// DOOM fire decay control
  #[pound(long = "doom-height", min = "1", max = "9")]
  doom_height:   Option<Sourced<u8>>,
  /// DOOM fire horizontal jitter
  #[pound(long = "doom-spread", max = "4")]
  doom_spread:   Option<Sourced<u8>>,
  /// DOOM fire colors as TOP,MIDDLE,BOTTOM
  #[pound(long = "doom-colors")]
  doom_colors:   Option<Sourced<String>>,
  /// matrix colors as HEAD,BRIGHT,DIM
  #[pound(long = "matrix-colors")]
  matrix_colors: Option<Sourced<String>>,
  /// matrix trail length as MIN,MAX
  #[pound(long = "matrix-length")]
  matrix_length: Option<Sourced<String>>,
  /// matrix fall speed as MIN,MAX
  #[pound(long = "matrix-speed")]
  matrix_speed:  Option<Sourced<String>>,
}

impl BackgroundOverrides {
  fn apply_to(&self, config: &mut Config) -> Result<(), ConfigError> {
    assign_option(&mut config.background.kind, self.kind.as_ref());
    if let Some(Sourced { value, .. }) = &self.fps {
      config.background.fps = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.doom_height {
      config.background.doom.height = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.doom_spread {
      config.background.doom.spread = Some(*value);
    }
    if let Some(Sourced { value, .. }) = &self.doom_colors {
      let [top, middle, bottom] = triple(value, "--doom-colors")?;
      config.background.doom.top_color = Some(top);
      config.background.doom.middle_color = Some(middle);
      config.background.doom.bottom_color = Some(bottom);
    }
    if let Some(Sourced { value, .. }) = &self.matrix_colors {
      let [head, bright, dim] = triple(value, "--matrix-colors")?;
      config.background.matrix.head_color = Some(head);
      config.background.matrix.bright_color = Some(bright);
      config.background.matrix.dim_color = Some(dim);
    }
    if let Some(Sourced { value, .. }) = &self.matrix_length {
      let (min, max) = pair::<u16>(value, "--matrix-length")?;
      config.background.matrix.min_length = Some(min);
      config.background.matrix.max_length = Some(max);
    }
    if let Some(Sourced { value, .. }) = &self.matrix_speed {
      let (min, max) = pair::<f32>(value, "--matrix-speed")?;
      config.background.matrix.min_speed = Some(min);
      config.background.matrix.max_speed = Some(max);
    }
    Ok(())
  }
}

fn flag(value: Sourced<bool>) -> Option<bool> {
  (!matches!(value.source, Source::Default)).then_some(value.value)
}

fn assign_option<T: Clone>(
  target: &mut Option<T>,
  source: Option<&Sourced<T>>,
) {
  if let Some(value) = source {
    target.clone_from(&Some(value.value.clone()));
  }
}

fn assign_list(target: &mut Vec<String>, source: &[String]) {
  if source.is_empty() {
    return;
  }
  target.clear();
  target.extend(
    source
      .iter()
      .map(|value| value.trim())
      .filter(|value| !value.is_empty())
      .map(str::to_owned),
  );
}

fn parse_theme(spec: &str) -> Result<ThemeConfig, ConfigError> {
  let mut theme = ThemeConfig::default();
  for directive in spec.split(';') {
    let (key, value) = directive.split_once('=').ok_or_else(|| {
      ConfigError::Validation(format!(
        "invalid theme directive '{directive}'; expected component=color"
      ))
    })?;
    let (key, value) = (key.trim(), value.trim());
    if value.is_empty() {
      return Err(ConfigError::Validation(format!(
        "theme component '{key}' must not be empty"
      )));
    }
    if !set_theme_component(&mut theme, key, value) {
      return Err(ConfigError::Validation(format!(
        "unknown theme component '{key}'"
      )));
    }
  }
  Ok(theme)
}

fn set_theme_component(
  theme: &mut ThemeConfig,
  key: &str,
  value: &str,
) -> bool {
  for (name, target) in [
    ("border", &mut theme.border),
    ("text", &mut theme.text),
    ("time", &mut theme.time),
    ("container", &mut theme.container),
    ("title", &mut theme.title),
    ("greet", &mut theme.greet),
    ("prompt", &mut theme.prompt),
    ("input", &mut theme.input),
    ("action", &mut theme.action),
    ("button", &mut theme.button),
  ] {
    if name == key {
      *target = Some(value.to_owned());
      return true;
    }
  }
  false
}

fn merge_theme(target: &mut ThemeConfig, source: ThemeConfig) {
  for (target, source) in [
    (&mut target.border, source.border),
    (&mut target.text, source.text),
    (&mut target.time, source.time),
    (&mut target.container, source.container),
    (&mut target.title, source.title),
    (&mut target.greet, source.greet),
    (&mut target.prompt, source.prompt),
    (&mut target.input, source.input),
    (&mut target.action, source.action),
    (&mut target.button, source.button),
  ] {
    if source.is_some() {
      *target = source;
    }
  }
}

fn triple(value: &str, option: &str) -> Result<[String; 3], ConfigError> {
  let mut values = value.split(',').map(str::trim);
  let result = [values.next(), values.next(), values.next()];
  if result.iter().any(Option::is_none) || values.next().is_some() {
    return Err(ConfigError::Validation(format!(
      "{option} requires exactly three comma-separated values"
    )));
  }
  Ok(result.map(|value| value.unwrap_or_default().to_string()))
}

fn pair<T>(value: &str, option: &str) -> Result<(T, T), ConfigError>
where
  T: FromStr,
  T::Err: Display,
{
  let Some((min, max)) = value.split_once(',') else {
    return Err(ConfigError::Validation(format!(
      "{option} requires two comma-separated values"
    )));
  };
  if max.contains(',') {
    return Err(ConfigError::Validation(format!(
      "{option} requires exactly two comma-separated values"
    )));
  }
  let parse = |part: &str| {
    part.trim().parse::<T>().map_err(|error| {
      ConfigError::Validation(format!("invalid {option} value: {error}"))
    })
  };
  Ok((parse(min)?, parse(max)?))
}

#[cfg(test)]
mod tests {
  use std::{env, error::Error, io::Write, process::Command};

  use tempfile::NamedTempFile;

  use super::Cli;
  use crate::{AlignGreeting, SecretMode, WidgetPosition, parser::load_config};

  #[test]
  fn command_line_values_flow_into_effective_config()
  -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from([
      "--no-config",
      "--cmd",
      "sway",
      "--env",
      "A=B",
      "--env",
      "C=D=E",
      "--sessions",
      "/one:/two",
      "--xsessions",
      "/x-one:/x-two",
      "--asterisks",
      "--asterisks-char",
      ".",
      "--issue",
      "--time",
      "--prompt-padding",
      "0",
      "--window-padding",
      "1",
      "--container-padding",
      "10",
      "--user-menu",
      "--time-position",
      "bottom",
      "--theme",
      "border=blue;text=cyan",
      "--background",
      "matrix",
      "--background-fps",
      "44",
      "--power-no-setsid",
      "--no-xsession-wrapper",
    ])?;

    let config = load_config(&cli)?.config;
    assert_eq!(config.session.command.as_deref(), Some("sway"));
    assert_eq!(config.session.environments, ["A=B", "C=D=E"]);
    assert_eq!(config.session.sessions_dirs, ["/one", "/two"]);
    assert_eq!(config.session.xsessions_dirs, ["/x-one", "/x-two"]);
    assert!(config.session.xsession_wrapper.is_none());
    assert_eq!(config.secret.mode, SecretMode::Characters);
    assert_eq!(config.secret.characters, ".");
    assert!(config.display.issue);
    assert!(config.display.show_time);
    assert_eq!(config.layout.prompt_padding, Some(0));
    assert_eq!(config.layout.window_padding, Some(1));
    assert_eq!(config.layout.container_padding, Some(10));
    assert_eq!(config.layout.widgets.time_position, WidgetPosition::Bottom);
    assert!(config.user_menu.enabled);
    assert_eq!(config.theme.border.as_deref(), Some("blue"));
    assert_eq!(config.theme.text.as_deref(), Some("cyan"));
    assert_eq!(config.background.kind.as_deref(), Some("matrix"));
    assert_eq!(config.background.fps, Some(44));
    assert!(!config.power.use_setsid);
    Ok(())
  }

  #[test]
  fn secret_mode_option_is_typed() -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from([
      "--no-config",
      "--secret-mode",
      "characters",
      "--secret-characters",
      "#",
    ])?;

    let config = load_config(&cli)?.config;
    assert_eq!(config.secret.mode, SecretMode::Characters);
    assert_eq!(config.secret.characters, "#");
    Ok(())
  }

  #[test]
  fn value_enums_accept_case_insensitive_names_and_aliases()
  -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from([
      "--no-config",
      "--secret-mode",
      "ChArAcTeR",
      "--greet-align",
      "RIGHT",
      "--time-position",
      "TOP",
    ])?;

    let config = load_config(&cli)?.config;
    assert_eq!(config.secret.mode, SecretMode::Characters);
    assert_eq!(config.display.align_greeting, AlignGreeting::Right);
    assert_eq!(config.layout.widgets.time_position, WidgetPosition::Top);
    assert!(Cli::try_parse_from(["--secret-mode", "dots"]).is_err());
    Ok(())
  }

  #[test]
  fn empty_environment_lists_clear_default_paths() -> Result<(), Box<dyn Error>>
  {
    const CHILD_MARKER: &str = "TUIGREET_EMPTY_LIST_ENV_TEST_CHILD";
    if env::var_os(CHILD_MARKER).is_none() {
      let status = Command::new(env::current_exe()?)
        .args([
          "--exact",
          "cli::tests::empty_environment_lists_clear_default_paths",
          "--nocapture",
        ])
        .env_clear()
        .env(CHILD_MARKER, "1")
        .env("TUIGREET_SESSIONS_DIRS", "")
        .env("TUIGREET_XSESSIONS_DIRS", "")
        .env("TUIGREET_ENVIRONMENTS", "")
        .status()?;
      assert!(status.success(), "environment child failed: {status}");
      return Ok(());
    }

    let config = load_config(&Cli::try_parse_from(["--no-config"])?)?.config;
    assert!(config.session.sessions_dirs.is_empty());
    assert!(config.session.xsessions_dirs.is_empty());
    assert!(config.session.environments.is_empty());
    Ok(())
  }

  #[test]
  fn parser_rejects_out_of_range_values() {
    for args in [
      &["--width", "0"][..],
      &["--window-padding", "11"][..],
      &["--kb-command", "13"][..],
      &["--background-fps", "0"][..],
      &["--doom-height", "10"][..],
      &["--doom-spread", "5"][..],
    ] {
      assert!(
        Cli::try_parse_from(args.iter().copied()).is_err(),
        "{args:?}"
      );
    }
  }

  #[test]
  fn effective_config_rejects_invalid_combinations()
  -> Result<(), Box<dyn Error>> {
    for args in [
      &["--no-config", "--issue", "--greeting", "hello"][..],
      &[
        "--no-config",
        "--remember",
        "--remember-session",
        "--remember-user-session",
      ][..],
      &["--no-config", "--remember-user-session"][..],
      &["--no-config", "--cmd", "sway", "--env", "BROKEN"][..],
      &["--no-config", "--time-format", "%i %"][..],
      &[
        "--no-config",
        "--user-menu-min-uid",
        "10000",
        "--user-menu-max-uid",
        "5000",
      ][..],
      &["--no-config", "--asterisks", "--asterisks-char", ""][..],
    ] {
      let cli = Cli::try_parse_from(args.iter().copied())?;
      assert!(load_config(&cli).is_err(), "{args:?}");
    }
    Ok(())
  }

  #[test]
  fn command_line_defaults_override_file_values() -> Result<(), Box<dyn Error>>
  {
    let mut file = NamedTempFile::new()?;
    write!(
      file,
      r#"
      [display]
      show_time = true

      [layout]
      width = 120

      [session]
      xsession_wrapper = "custom-startx"

      [theme]
      border = "red"
      text = "green"
      "#
    )?;
    let path = file.path().to_string_lossy().into_owned();
    let cli = Cli::try_parse_from([
      "--config",
      path.as_str(),
      "--no-time",
      "--width",
      "80",
      "--no-xsession-wrapper",
      "--theme",
      "border=blue",
    ])?;

    let config = load_config(&cli)?.config;
    assert!(!config.display.show_time);
    assert_eq!(config.layout.width, 80);
    assert!(config.session.xsession_wrapper.is_none());
    assert_eq!(config.theme.border.as_deref(), Some("blue"));
    assert!(config.theme.text.is_none());
    Ok(())
  }

  #[test]
  fn environment_and_command_line_precedence() -> Result<(), Box<dyn Error>> {
    const CHILD_MARKER: &str = "TUIGREET_POUND_ENV_TEST_CHILD";
    if env::var_os(CHILD_MARKER).is_none() {
      let status = Command::new(env::current_exe()?)
        .args([
          "--exact",
          "cli::tests::environment_and_command_line_precedence",
          "--nocapture",
        ])
        .env_clear()
        .env(CHILD_MARKER, "1")
        .env("TUIGREET_WIDTH", "70")
        .env("TUIGREET_TIME", "true")
        .env("TUIGREET_SESSIONS_DIRS", "/env-one:/env-two")
        .env("TUIGREET_ENVIRONMENTS", "LANG=C:DESKTOP=sway")
        .env("TUIGREET_XSESSION_WRAPPER", "env-startx")
        .env("TUIGREET_THEME", "border=red;text=green")
        .env("TUIGREET_THEME_TEXT", "cyan")
        .status()?;
      assert!(status.success(), "environment child failed: {status}");
      return Ok(());
    }

    let cli = Cli::try_parse_from([
      "--no-config",
      "--width",
      "90",
      "--no-time",
      "--sessions",
      "/cli-one:/cli-two",
      "--no-xsession-wrapper",
    ])?;
    let config = load_config(&cli)?.config;
    assert_eq!(config.layout.width, 90);
    assert!(!config.display.show_time);
    assert_eq!(config.session.sessions_dirs, ["/cli-one", "/cli-two"]);
    assert_eq!(config.session.environments, ["LANG=C", "DESKTOP=sway"]);
    assert!(config.session.xsession_wrapper.is_none());
    assert_eq!(config.theme.border.as_deref(), Some("red"));
    assert_eq!(config.theme.text.as_deref(), Some("cyan"));
    Ok(())
  }

  #[test]
  fn conflicting_config_source_flags_are_rejected() {
    assert!(
      Cli::try_parse_from(["--config", "/tmp/config.toml", "--no-config"])
        .is_err()
    );
  }

  #[test]
  fn malformed_compound_values_are_rejected() -> Result<(), Box<dyn Error>> {
    for args in [
      &["--no-config", "--theme", "border"][..],
      &["--no-config", "--doom-colors", "red,orange"][..],
      &["--no-config", "--matrix-length", "1,2,3"][..],
      &["--no-config", "--matrix-speed", "fast,slow"][..],
    ] {
      let cli = Cli::try_parse_from(args.iter().copied())?;
      assert!(load_config(&cli).is_err(), "{args:?}");
    }
    Ok(())
  }
}

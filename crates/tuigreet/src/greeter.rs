use std::{
  convert::TryInto,
  env,
  io::{self, Write},
  path::PathBuf,
  process,
  sync::Arc,
};

use crossterm::event::KeyEvent;
use i18n_embed::DesktopLanguageRequester;
use pure_rust_locales::Locale;
use tokio::{
  net::UnixStream,
  sync::{RwLock, RwLockWriteGuard, mpsc::Sender},
};
use tracing_appender::non_blocking::WorkerGuard;
use tui::style::Color;
use tuigreet_config::{
  BindingAction,
  Config,
  IdleAction,
  IdleConfig,
  KeybindingsConfig,
  SessionEntry,
  StatusBarConfig,
  ThemeConfig,
};
use tuigreet_theme::{Theme, config::theme_from_config};
use tuigreet_types::{
  AuthStatus,
  DEFAULT_LOG_FILE,
  GreetAlign,
  Mode,
  SecretDisplay,
};
use zeroize::Zeroize;

use crate::{
  action::{KeyBinding, bindings_from_config},
  event::Event,
  info::{
    configured_session_paths,
    get_issue,
    get_last_command,
    get_last_session_path,
    get_last_user_command,
    get_last_user_name,
    get_last_user_session,
    get_last_user_username,
    get_sessions,
  },
  power::PowerOption,
  ui::{
    background::{self, Background},
    bg_animation::{self, Animation, AnimationSpec},
    common::{masked::MaskedString, menu::Menu},
    power::Power,
    sessions::{Session, SessionSource, SessionType},
    users::User,
  },
};

/// Default render rate when animation is enabled
pub const ANIMATION_DEFAULT_FPS: u32 = 30;
/// Base/floor render rate
pub const DEFAULT_FPS: u32 = 2;

#[derive(Debug, Clone, Copy, Default)]
struct IdleState {
  config: IdleConfig,
  active: bool,
}
const DEFAULT_LOCALE: Locale = Locale::en_US;

/// Core greeter state managing authentication, UI, and session selection.
#[expect(
  clippy::struct_excessive_bools,
  reason = "Independent CLI/configuration flags and authentication state are \
            not mutually exclusive states"
)]
pub struct Greeter {
  pub debug:   bool,
  pub logfile: String,
  pub logger:  Option<WorkerGuard>,

  pub numlock:       bool,
  pub locale:        Locale,
  pub loaded_config: Option<Config>,
  pub socket:        String,
  pub stream:        Option<Arc<RwLock<UnixStream>>>,
  pub events:        Option<Sender<Event>>,

  // Current mode of the application, will define what actions are permitted.
  pub mode:          Mode,
  // Mode the application will return to when exiting the current mode.
  pub previous_mode: Mode,
  // Offset the cursor should be at from its base position for the current
  // mode.
  pub cursor_offset: isize,

  // Buffer to be used as a temporary editing zone for the various modes.
  // Previous buffer is saved when a transient screen has to use the buffer, to
  // be able to restore it when leaving the transient screen.
  pub previous_buffer: Option<String>,
  pub buffer:          String,

  // Define the selected session and how to resolve it.
  pub session_source:   SessionSource,
  // List of session files found on disk.
  pub session_paths:    Vec<(PathBuf, SessionType)>,
  // Menu for session selection.
  pub sessions:         Menu<Session>,
  // Wrapper command to prepend to non-X11 sessions.
  pub session_wrapper:  Option<String>,
  // Wrapper command to prepend to X11 sessions.
  pub xsession_wrapper: Option<String>,
  // Default session regex
  pub default_session:  Option<String>,
  // Visibility, order and names of discovered sessions.
  pub session_entries:  Vec<SessionEntry>,

  // Whether user menu is enabled.
  pub user_menu:    bool,
  // Menu for user selection.
  pub users:        Menu<User>,
  // Default user to pre-fill.
  pub default_user: Option<String>,
  // Current username. Masked to display the full name if available.
  pub username:     MaskedString,
  // Prompt that should be displayed to ask for entry.
  pub prompt:       Option<String>,

  // Whether the current edition prompt should be hidden.
  pub asking_for_secret: bool,
  // How should secrets be displayed?
  pub secret_display:    SecretDisplay,
  // Whether an active secret prompt is temporarily revealed.
  secret_revealed:       bool,

  // Whether last logged-in user should be remembered.
  pub remember:              bool,
  // Whether last launched session (regardless of user) should be remembered.
  pub remember_session:      bool,
  // Whether last launched session for the current user should be remembered.
  pub remember_user_session: bool,

  // Style object for the terminal UI
  pub theme:       Theme,
  // Display the current time
  pub time:        bool,
  // Time format
  pub time_format: Option<String>,
  // Display battery percentage
  pub battery:     bool,
  // Greeting message (MOTD) to use to welcome the user.
  pub greeting:    Option<String>,
  // Container's title configuration
  pub title:       TitleOption,
  // Transaction message to show to the user.
  pub message:     Option<String>,

  // Menu for power options.
  pub powers:       Menu<Power>,
  // Whether to prefix the power commands with `setsid`.
  pub power_setsid: bool,

  // Effective typed action bindings, including legacy F-key settings.
  bindings:   Vec<KeyBinding>,
  status_bar: StatusBarConfig,

  // Inactivity policy and current state.
  idle: IdleState,

  // Background animation, drawn before the login UI.
  pub animation:     Option<Box<dyn Animation>>,
  // Configured animation FPS, when an animation is active.
  pub animation_fps: Option<u32>,
  // Skip greetd socket and simulate auth flow locally for UI testing
  pub mock:          bool,
  // Menu for the on-the-fly background switcher (F4 by default).
  pub backgrounds:   Menu<Background>,

  // The software is waiting for a response from `greetd`.
  pub working: bool,
  // We are done working.
  pub done:    bool,
  // Should we exit?
  pub exit:    Option<AuthStatus>,
}

impl Default for Greeter {
  fn default() -> Self {
    Self {
      debug:                 false,
      logfile:               DEFAULT_LOG_FILE.to_string(),
      logger:                None,
      numlock:               false,
      locale:                DEFAULT_LOCALE,
      loaded_config:         None,
      socket:                String::new(),
      stream:                None,
      events:                None,
      mode:                  Mode::default(),
      previous_mode:         Mode::default(),
      cursor_offset:         0,
      previous_buffer:       None,
      buffer:                String::new(),
      session_source:        SessionSource::default(),
      session_paths:         Vec::new(),
      sessions:              Menu::default(),
      session_wrapper:       None,
      xsession_wrapper:      None,
      default_session:       None,
      session_entries:       Vec::new(),
      user_menu:             false,
      users:                 Menu::default(),
      default_user:          None,
      username:              MaskedString::default(),
      prompt:                None,
      asking_for_secret:     false,
      secret_display:        SecretDisplay::default(),
      secret_revealed:       false,
      remember:              false,
      remember_session:      false,
      remember_user_session: false,
      theme:                 Theme::default(),
      time:                  false,
      time_format:           None,
      battery:               false,
      greeting:              None,
      title:                 TitleOption::default(),
      message:               None,
      powers:                Menu::default(),
      power_setsid:          false,
      bindings:              bindings_from_config(&KeybindingsConfig::default()),
      status_bar:            StatusBarConfig::default(),
      idle:                  IdleState::default(),
      animation:             None,
      animation_fps:         None,
      mock:                  false,
      backgrounds:           Menu::default(),
      working:               false,
      done:                  false,
      exit:                  None,
    }
  }
}

impl Drop for Greeter {
  fn drop(&mut self) {
    self.scrub(true, false);
  }
}

impl Greeter {
  #[cfg_attr(
    test,
    expect(
      clippy::unused_async,
      reason = "Tests omit startup socket connection; production awaits \
                connect"
    )
  )]
  pub async fn new(events: Sender<Event>, config: Config, mock: bool) -> Self {
    let mut greeter = Self::default();

    greeter.events = Some(events);
    greeter.mock = mock;
    greeter.set_locale();
    greeter.powers = Menu {
      title:    fl!("title_power"),
      options:  Vec::new(),
      selected: 0,
    };
    greeter.apply_config(config);

    #[cfg(not(test))]
    {
      greeter.logger = crate::init_logger(&greeter);
      greeter.connect().await;
    }

    let mut sessions = get_sessions(&greeter);
    if greeter.mock && sessions.is_empty() {
      sessions = mock_sessions();
    }
    greeter.sessions = Menu {
      title:    fl!("title_session"),
      options:  sessions,
      selected: 0,
    };
    greeter.select_default_session();
    greeter.apply_default_user();
    greeter.restore_remembered_user();
    greeter.restore_remembered_session();
    greeter
  }

  fn select_default_session(&mut self) {
    if !matches!(self.session_source, SessionSource::None)
      || self.sessions.options.is_empty()
    {
      return;
    }
    self.session_source = SessionSource::Session(0);

    let Some(pattern) = self.default_session.as_deref() else {
      return;
    };
    match regex::Regex::new(pattern) {
      Ok(regex) => {
        if let Some(index) =
          self
            .sessions
            .options
            .iter()
            .position(|Session { path, .. }| {
              path
                .as_ref()
                .is_some_and(|path| regex.is_match(&path.to_string_lossy()))
            })
        {
          self.sessions.selected = index;
          self.session_source = SessionSource::Session(index);
        }
      },
      Err(error) => {
        tracing::warn!(
          "Invalid regex for default session: {pattern}. Error: {error}"
        );
      },
    }
  }

  fn apply_default_user(&mut self) {
    if let Some(default_user) = &self.default_user {
      self.username = MaskedString::from(default_user.clone(), None);
    }
  }

  fn restore_remembered_user(&mut self) {
    if !self.remember {
      return;
    }
    let Some(username) = get_last_user_username() else {
      return;
    };
    self.username = MaskedString::from(username, get_last_user_name());
    if !self.remember_user_session {
      return;
    }

    if let Ok(command) = get_last_user_command(self.username.get()) {
      self.session_source = SessionSource::Command(command);
    }
    if let Ok(session_path) = get_last_user_session(self.username.get())
      && let Some(index) = self
        .sessions
        .options
        .iter()
        .position(|Session { path, .. }| path.as_deref() == Some(&session_path))
    {
      self.sessions.selected = index;
      self.session_source = SessionSource::Session(index);
    }
  }

  fn restore_remembered_session(&mut self) {
    if !self.remember_session {
      return;
    }
    if let Ok(command) = get_last_command() {
      self.session_source = SessionSource::Command(command.trim().to_string());
    }
    if let Ok(session_path) = get_last_session_path()
      && let Some(index) = self
        .sessions
        .options
        .iter()
        .position(|Session { path, .. }| path.as_deref() == Some(&session_path))
    {
      self.sessions.selected = index;
      self.session_source = SessionSource::Session(index);
    }
  }

  // Scrub memory of all data, unless `soft` is true, in which case, we will
  // keep the username (can happen if a wrong password was entered, we want to
  // give the user another chance, as PAM would).
  fn scrub(&mut self, scrub_message: bool, soft: bool) {
    self.buffer.zeroize();
    self.secret_revealed = false;
    self.prompt.zeroize();

    if !soft {
      self.username.zeroize();
    }

    if scrub_message {
      self.message.zeroize();
    }
  }

  // Reset the software to its initial state.
  pub async fn reset(&mut self, soft: bool) {
    if soft {
      self.mode = Mode::Password;
      self.previous_mode = Mode::Password;
    } else {
      self.mode = Mode::Username;
      self.previous_mode = Mode::Username;
    }

    self.working = false;
    self.done = false;

    self.scrub(false, soft);
    self.connect().await;
  }

  // Connect to `greetd` and return a stream we can safely write to.
  pub async fn connect(&mut self) {
    if self.mock {
      tracing::info!("mock mode: skipping greetd socket connect");
      return;
    }

    // If socket is not already set (by tests), read from environment
    if self.socket.is_empty() {
      self.socket = env::var("GREETD_SOCK").unwrap_or_else(|_| {
        report_output_error(writeln!(
          io::stderr().lock(),
          "GREETD_SOCK must be defined"
        ));
        process::exit(1);
      });
    }

    match UnixStream::connect(&self.socket).await {
      Ok(stream) => {
        self.stream = Some(Arc::new(RwLock::new(stream)));
      },

      Err(err) => {
        report_output_error(writeln!(io::stderr().lock(), "{err}"));
        process::exit(1);
      },
    }
  }

  /// Borrow the connected greetd socket for writing.
  ///
  /// # Errors
  /// Returns `NotConnected` when no greetd connection has been established.
  pub async fn stream(&self) -> io::Result<RwLockWriteGuard<'_, UnixStream>> {
    let stream = self.stream.as_ref().ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::NotConnected,
        "greetd socket is not connected",
      )
    })?;
    Ok(stream.write().await)
  }

  /// Width of the main prompt from the effective configuration.
  #[must_use]
  pub fn width(&self) -> u16 {
    self
      .loaded_config
      .as_ref()
      .map_or(80, |config| config.layout.width)
  }

  /// Padding around the terminal window.
  #[must_use]
  pub fn window_padding(&self) -> u16 {
    self
      .loaded_config
      .as_ref()
      .and_then(|config| config.layout.window_padding)
      .unwrap_or(0)
  }

  /// Padding inside the main prompt container, including its border.
  #[must_use]
  pub fn container_padding(&self) -> u16 {
    self
      .loaded_config
      .as_ref()
      .and_then(|config| config.layout.container_padding)
      .unwrap_or(1)
      .saturating_add(1)
  }

  /// Spacing between prompt rows.
  #[must_use]
  pub fn prompt_padding(&self) -> u16 {
    self
      .loaded_config
      .as_ref()
      .and_then(|config| config.layout.prompt_padding)
      .unwrap_or(1)
  }

  #[must_use]
  pub fn greet_align(&self) -> GreetAlign {
    self
      .loaded_config
      .as_ref()
      .map_or_else(GreetAlign::default, |config| {
        match config.display.align_greeting {
          tuigreet_config::AlignGreeting::Left => GreetAlign::Left,
          tuigreet_config::AlignGreeting::Right => GreetAlign::Right,
          tuigreet_config::AlignGreeting::Center => GreetAlign::Center,
        }
      })
  }

  // Sets the locale that will be used for this invocation from environment.
  fn set_locale(&mut self) {
    let locale = DesktopLanguageRequester::requested_languages()
      .into_iter()
      .next()
      .and_then(|locale| {
        locale
          .region
          .map(|region| format!("{}_{region}", locale.language))
      })
      .and_then(|id| id.as_str().try_into().ok());

    if let Some(locale) = locale {
      self.locale = locale;
    }
  }

  fn populate_backgrounds_menu(&mut self) {
    let title = fl!("title_background");
    let none_label = fl!("background_none");
    let options = background::options(&none_label);
    // Highlight the first concrete kind when an animation is active,
    // otherwise the synthetic "None" at index 0.
    let selected = if self.animation.is_none() {
      0
    } else {
      1.min(options.len() - 1)
    };

    self.backgrounds = Menu {
      title,
      options,
      selected,
    };
  }

  pub fn apply_background_selection(&mut self) {
    let Some(item) = self.backgrounds.options.get(self.backgrounds.selected)
    else {
      return;
    };
    match item.kind {
      None => {
        self.animation = None;
        self.animation_fps = None;
      },
      Some(kind) => {
        self.animation = Some(bg_animation::build_default(kind));
        self.animation_fps = Some(ANIMATION_DEFAULT_FPS);
      },
    }
    self.push_frame_rate();
    if let Some(ref sender) = self.events {
      let sender = sender.clone();
      tokio::spawn(async move {
        let _ = sender.send(Event::Refresh).await;
      });
    }
  }

  fn set_background_from_config(
    &mut self,
    cfg: &tuigreet_config::BackgroundConfig,
  ) {
    use crate::ui::bg_animation::{Kind, doom, matrix};

    let Some(kind) = cfg.kind.as_deref().and_then(Kind::from_name) else {
      if let Some(name) = cfg.kind.as_deref()
        && !name.trim().is_empty()
        && !name.eq_ignore_ascii_case("none")
      {
        tracing::warn!(
          "unknown background animation kind '{}', ignoring",
          name
        );
      }
      self.animation = None;
      self.animation_fps = None;
      return;
    };

    let parse = |s: &Option<String>, fallback: Color| {
      s.as_deref()
        .and_then(bg_animation::parse_color)
        .unwrap_or(fallback)
    };

    let spec = match kind {
      Kind::Doom => {
        let d = doom::Options::default();
        AnimationSpec::Doom(doom::Options {
          height: cfg.doom.height.unwrap_or(d.height),
          spread: cfg.doom.spread.unwrap_or(d.spread),
          top:    parse(&cfg.doom.top_color, d.top),
          middle: parse(&cfg.doom.middle_color, d.middle),
          bottom: parse(&cfg.doom.bottom_color, d.bottom),
        })
      },
      Kind::Matrix => {
        let d = matrix::Options::default();
        AnimationSpec::Matrix(matrix::Options {
          head:          parse(&cfg.matrix.head_color, d.head),
          bright:        parse(&cfg.matrix.bright_color, d.bright),
          dim:           parse(&cfg.matrix.dim_color, d.dim),
          min_length:    cfg.matrix.min_length.unwrap_or(d.min_length),
          max_length:    cfg.matrix.max_length.unwrap_or(d.max_length),
          min_speed:     cfg.matrix.min_speed.unwrap_or(d.min_speed),
          max_speed:     cfg.matrix.max_speed.unwrap_or(d.max_speed),
          mutate_chance: cfg.matrix.mutate_chance.unwrap_or(d.mutate_chance),
        })
      },
    };
    self.animation = Some(bg_animation::build(&spec));
    self.animation_fps = Some(cfg.fps.unwrap_or(ANIMATION_DEFAULT_FPS));
  }

  /// Render rate for the event loop.
  #[must_use]
  pub fn frame_rate(&self) -> f64 {
    self.animation_fps.map_or_else(
      || f64::from(DEFAULT_FPS),
      |fps| f64::from(fps.max(DEFAULT_FPS)),
    )
  }

  /// Push the current frame rate to the event loop.
  fn push_frame_rate(&self) {
    if let Some(tx) = &self.events {
      let fps = self.frame_rate();
      let tx = tx.clone();
      tokio::spawn(async move {
        let _ = tx.send(Event::SetFrameRate(fps)).await;
      });
    }
  }

  fn set_power_command(
    &mut self,
    action: PowerOption,
    command: Option<String>,
  ) {
    if let Some(power) = self
      .powers
      .options
      .iter_mut()
      .find(|power| power.action == action)
    {
      power.command = command;
      return;
    }

    let label = match action {
      PowerOption::Shutdown => fl!("shutdown"),
      PowerOption::Reboot => fl!("reboot"),
      PowerOption::Suspend => fl!("suspend"),
      PowerOption::Hibernate => fl!("hibernate"),
    };

    self.powers.options.push(Power {
      action,
      label,
      command,
    });
  }

  pub fn set_prompt(&mut self, prompt: &str) {
    self.secret_revealed = false;
    self.prompt = if prompt.ends_with(' ') {
      Some(prompt.into())
    } else {
      Some(format!("{prompt} "))
    };
  }

  pub fn remove_prompt(&mut self) {
    self.prompt = None;
  }

  // Computes the size of the prompt to help determine where input should start.
  #[must_use]
  pub fn prompt_width(&self) -> usize {
    self
      .prompt
      .as_ref()
      .map_or(0, |prompt| prompt.chars().count())
  }

  /// Replace all configuration-owned runtime state.
  pub fn apply_config(&mut self, config: Config) {
    use tuigreet_config::SecretMode;
    // General
    self.debug = config.general.debug;
    self.logfile.clone_from(&config.general.log_file);
    self.numlock = config.general.numlock;
    // Session
    self.session_source = config.session.command.as_ref().map_or_else(
      SessionSource::default,
      |command| {
        SessionSource::DefaultCommand(
          command.clone(),
          (!config.session.environments.is_empty())
            .then(|| config.session.environments.clone()),
        )
      },
    );
    self.session_paths = configured_session_paths(&config.session);
    self
      .session_wrapper
      .clone_from(&config.session.session_wrapper);
    self
      .xsession_wrapper
      .clone_from(&config.session.xsession_wrapper);
    self
      .default_session
      .clone_from(&config.session.default_session);
    self.session_entries.clone_from(&config.session.entries);
    // Display
    self.time = config.display.show_time;
    self.time_format.clone_from(&config.display.time_format);
    self.battery = config.display.battery;
    self.greeting = if config.display.issue {
      get_issue()
    } else {
      config.display.greeting.clone()
    };
    self.title.enable = config.display.show_title;
    self.title.custom.clone_from(&config.display.custom_title);
    // Remember
    self.default_user.clone_from(&config.remember.default_user);
    self.remember = config.remember.username;
    self.remember_session = config.remember.session;
    self.remember_user_session = config.remember.user_session;
    // User menu
    self.user_menu = config.user_menu.enabled;
    if self.user_menu {
      use crate::info::get_users;
      self.users = Menu {
        title:    fl!("title_users"),
        options:  get_users(config.user_menu.min_uid, config.user_menu.max_uid),
        selected: 0,
      };
    } else {
      self.users = Menu::default();
    }
    // Power
    self
      .set_power_command(PowerOption::Shutdown, config.power.shutdown.clone());
    self.set_power_command(PowerOption::Reboot, config.power.reboot.clone());
    self.set_power_command(PowerOption::Suspend, config.power.suspend.clone());
    self.set_power_command(
      PowerOption::Hibernate,
      config.power.hibernate.clone(),
    );
    self.power_setsid = config.power.use_setsid;
    // Secret
    match config.secret.mode {
      SecretMode::Hidden => self.secret_display = SecretDisplay::Hidden,
      SecretMode::Characters => {
        self.secret_display =
          SecretDisplay::Character(config.secret.characters.clone());
      },
    }
    // Keybindings
    self.set_keybindings(&config.keybindings);
    self.status_bar = config.layout.widgets.status_bar;
    self.idle = IdleState {
      config: config.idle,
      active: false,
    };
    // Animation
    self.set_background_from_config(&config.background);
    self.populate_backgrounds_menu();
    self.apply_theme_config(&config.theme);
    self.loaded_config = Some(config);
  }

  pub fn set_keybindings(&mut self, config: &KeybindingsConfig) {
    self.bindings = bindings_from_config(config);
  }

  #[must_use]
  pub fn action_for_key(&self, key: KeyEvent) -> Option<BindingAction> {
    self
      .bindings
      .iter()
      .find(|binding| binding.matches(key))
      .map(KeyBinding::action)
  }

  #[must_use]
  pub fn key_for_action(&self, action: BindingAction) -> Option<&str> {
    self
      .bindings
      .iter()
      .find(|binding| binding.action() == action)
      .map(KeyBinding::label)
  }

  #[must_use]
  pub(crate) const fn status_bar(&self) -> &StatusBarConfig {
    &self.status_bar
  }

  #[must_use]
  pub const fn idle_timeout(&self) -> u64 {
    self.idle.config.timeout_seconds
  }

  #[must_use]
  pub const fn idle_action(&self) -> IdleAction {
    self.idle.config.action
  }

  #[must_use]
  pub const fn is_idle(&self) -> bool {
    self.idle.active
  }

  pub const fn enter_idle(&mut self) {
    self.idle.active = true;
  }

  /// Wakes the greeter and reports whether the triggering key should be
  /// consumed.
  pub const fn wake_from_idle(&mut self) -> bool {
    let was_idle = self.idle.active;
    self.idle.active = false;
    was_idle
  }

  #[must_use]
  pub const fn is_secret_revealed(&self) -> bool {
    self.secret_revealed
  }

  pub fn toggle_secret_visibility(&mut self) {
    if self.mode == Mode::Password && self.asking_for_secret {
      self.secret_revealed = !self.secret_revealed;
    }
  }

  pub fn reload_sessions(&mut self) {
    let selected_path = self
      .sessions
      .options
      .get(self.sessions.selected)
      .and_then(|session| session.path.clone());
    let sessions = get_sessions(self);
    self.sessions.options = sessions;
    self.sessions.selected = selected_path
      .as_ref()
      .and_then(|path| {
        self
          .sessions
          .options
          .iter()
          .position(|session| session.path.as_ref() == Some(path))
      })
      .unwrap_or(0);
    if matches!(self.session_source, SessionSource::Session(_)) {
      self.session_source = if self.sessions.options.is_empty() {
        SessionSource::default()
      } else {
        SessionSource::Session(self.sessions.selected)
      };
    }
  }

  /// Apply the theme from the effective configuration.
  pub fn apply_theme_config(&mut self, theme_config: &ThemeConfig) {
    self.theme = theme_from_config(theme_config);
  }
}

// Parsed title option
pub struct TitleOption {
  // Display the container's title
  pub enable: bool,
  // Custom title text
  pub custom: Option<String>,
}

impl Default for TitleOption {
  fn default() -> Self {
    Self {
      enable: true,
      custom: None,
    }
  }
}

// Mock session list used to pad otherwise empty session menu
fn mock_sessions() -> Vec<Session> {
  [
    ("mock-wayland", "Mock Wayland", SessionType::Wayland),
    ("mock-x11", "Mock X11", SessionType::X11),
    ("mock-shell", "Mock shell", SessionType::None),
  ]
  .into_iter()
  .map(|(slug, name, session_type)| {
    Session {
      slug: Some(slug.to_string()),
      name: name.to_string(),
      command: "true".to_string(),
      session_type,
      path: None,
      xdg_desktop_names: None,
    }
  })
  .collect()
}

fn report_output_error(result: io::Result<()>) {
  if let Err(error) = result {
    tracing::error!("Failed to write CLI output: {error}");
  }
}

#[cfg(test)]
mod test {
  use std::{error::Error, fs, path::PathBuf};

  use tempfile::tempdir;
  use tokio::sync::mpsc;
  use tuigreet_config::Config;

  use crate::{
    Greeter,
    power::PowerOption,
    ui::sessions::{SessionSource, SessionType},
  };

  #[test]
  fn test_prompt_width() {
    let mut greeter = Greeter::default();
    greeter.prompt = None;

    assert_eq!(greeter.prompt_width(), 0);

    greeter.prompt = Some("Hello:".into());

    assert_eq!(greeter.prompt_width(), 6);
  }

  #[test]
  fn test_set_prompt() {
    let mut greeter = Greeter::default();

    greeter.set_prompt("Hello:");

    assert_eq!(greeter.prompt, Some("Hello: ".into()));

    greeter.set_prompt("Hello World: ");

    assert_eq!(greeter.prompt, Some("Hello World: ".into()));

    greeter.remove_prompt();

    assert_eq!(greeter.prompt, None);
  }

  #[test]
  fn applying_config_replaces_session_state() {
    let mut greeter = Greeter::default();

    let mut configured = Config::default();
    configured.session.command = Some("first".into());
    configured.session.sessions_dirs = vec!["/first".into()];
    configured.session.session_wrapper = Some("wrapper".into());
    configured.user_menu.enabled = true;
    greeter.apply_config(configured);

    let mut replacement = Config::default();
    replacement.session.sessions_dirs = vec!["/second".into()];
    greeter.apply_config(replacement);

    assert!(matches!(greeter.session_source, SessionSource::None));
    assert_eq!(greeter.session_paths, [(
      PathBuf::from("/second"),
      SessionType::Wayland
    )]);
    assert!(greeter.session_wrapper.is_none());
    assert!(!greeter.user_menu);
    assert!(greeter.users.options.is_empty());
  }

  #[test]
  fn applying_config_updates_log_file() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.general.log_file = "/tmp/configured-tuigreet.log".into();

    greeter.apply_config(config);

    assert_eq!(greeter.logfile, "/tmp/configured-tuigreet.log");
  }

  #[test]
  fn session_paths_are_deduplicated_when_config_is_applied() {
    let sess_dir = "/usr/share/wayland-sessions";
    let mut greeter = Greeter::default();

    let mut config = Config::default();
    config.session.sessions_dirs = vec![sess_dir.to_string(); 2];
    greeter.apply_config(config);

    assert_eq!(greeter.session_paths, [(
      PathBuf::from(sess_dir),
      SessionType::Wayland
    )]);
  }

  #[test]
  fn config_session_environments_are_applied() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.session.command = Some("sway".to_string());
    config.session.environments = vec![
      "XDG_CURRENT_DESKTOP=sway".to_string(),
      "XDG_SESSION_TYPE=wayland".to_string(),
    ];
    greeter.apply_config(config);

    assert!(matches!(
      &greeter.session_source,
      SessionSource::DefaultCommand(command, Some(environments))
        if command == "sway"
          && environments
            == &vec![
              "XDG_CURRENT_DESKTOP=sway".to_string(),
              "XDG_SESSION_TYPE=wayland".to_string()
            ]
    ));
  }

  #[tokio::test]
  async fn default_session_selects_from_loaded_sessions()
  -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    for (file, name) in
      [("first.desktop", "First"), ("second.desktop", "Second")]
    {
      fs::write(
        directory.path().join(file),
        format!("[Desktop Entry]\nName={name}\nExec={name}\n"),
      )?;
    }

    let mut config = Config::default();
    config.session.sessions_dirs =
      vec![directory.path().to_string_lossy().into_owned()];
    config.session.xsessions_dirs.clear();
    config.session.default_session = Some("second\\.desktop$".to_string());
    let (events, _receiver) = mpsc::channel(1);

    let greeter = Greeter::new(events, config, false).await;
    let selected = &greeter.sessions.options[greeter.sessions.selected];

    assert_eq!(selected.name, "Second");
    assert!(matches!(
      greeter.session_source,
      SessionSource::Session(index) if index == greeter.sessions.selected
    ));
    Ok(())
  }

  #[test]
  fn config_power_options_are_applied() {
    let mut greeter = Greeter::default();

    let mut config = Config::default();
    config.power.shutdown = Some("loginctl poweroff".to_string());
    config.power.reboot = Some("loginctl reboot".to_string());
    config.power.suspend = Some("loginctl suspend".to_string());
    config.power.hibernate = Some("loginctl hibernate".to_string());
    config.power.use_setsid = false;
    greeter.apply_config(config);

    assert!(!greeter.power_setsid);
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Shutdown
        && power.command.as_deref() == Some("loginctl poweroff")
    }));
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Reboot
        && power.command.as_deref() == Some("loginctl reboot")
    }));
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Suspend
        && power.command.as_deref() == Some("loginctl suspend")
    }));
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Hibernate
        && power.command.as_deref() == Some("loginctl hibernate")
    }));
  }
}

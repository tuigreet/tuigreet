use std::{
  convert::TryInto,
  env,
  error::Error,
  ffi::OsStr,
  path::PathBuf,
  process,
  sync::Arc,
};

use chrono::{
  Locale,
  format::{Item, StrftimeItems},
};
use getopts::{Matches, Options};
use i18n_embed::DesktopLanguageRequester;
use tokio::{
  net::UnixStream,
  sync::{RwLock, RwLockWriteGuard, mpsc::Sender},
};
use tracing_appender::non_blocking::WorkerGuard;
use tuigreet_theme::Theme;
use tuigreet_types::{
  AuthStatus,
  DEFAULT_ASTERISKS_CHARS,
  DEFAULT_LOG_FILE,
  DEFAULT_XSESSION_WRAPPER,
  GreetAlign,
  Mode,
  SecretDisplay,
};
use zeroize::Zeroize;

use crate::{
  event::Event,
  info::{
    get_issue,
    get_last_command,
    get_last_session_path,
    get_last_user_command,
    get_last_user_name,
    get_last_user_session,
    get_last_user_username,
    get_min_max_uids,
    get_sessions,
    get_users,
  },
  power::PowerOption,
  ui::{
    background::Background,
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

const DEFAULT_LOCALE: Locale = Locale::en_US;

/// Core greeter state managing authentication, UI, and session selection.
pub struct Greeter {
  pub debug:   bool,
  pub logfile: String,
  pub logger:  Option<WorkerGuard>,

  pub locale:        Locale,
  pub config:        Option<Matches>,
  pub loaded_config: Option<tuigreet_config::Config>, /* store loaded TOML
                                                       * config */
  pub socket:        String,
  pub stream:        Option<Arc<RwLock<UnixStream>>>,
  pub events:        Option<Sender<Event>>,

  // Current mode of the application, will define what actions are permitted.
  pub mode:          Mode,
  // Mode the application will return to when exiting the current mode.
  pub previous_mode: Mode,
  // Offset the cursor should be at from its base position for the current
  // mode.
  pub cursor_offset: i16,

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

  pub kb_command:    u8,
  pub kb_sessions:   u8,
  pub kb_power:      u8,
  pub kb_background: u8,

  pub status_show_reset:          bool,
  pub status_show_command:        bool,
  pub status_show_session:        bool,
  pub status_show_power:          bool,
  pub status_show_background:     bool,
  pub status_show_session_status: bool,
  pub status_show_caps_lock:      bool,

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
      debug:                      false,
      logfile:                    DEFAULT_LOG_FILE.to_string(),
      logger:                     None,
      locale:                     DEFAULT_LOCALE,
      config:                     None,
      loaded_config:              None,
      socket:                     String::new(),
      stream:                     None,
      events:                     None,
      mode:                       Mode::default(),
      previous_mode:              Mode::default(),
      cursor_offset:              0,
      previous_buffer:            None,
      buffer:                     String::new(),
      session_source:             SessionSource::default(),
      session_paths:              Vec::new(),
      sessions:                   Menu::default(),
      session_wrapper:            None,
      xsession_wrapper:           None,
      user_menu:                  false,
      users:                      Menu::default(),
      default_user:               None,
      username:                   MaskedString::default(),
      prompt:                     None,
      asking_for_secret:          false,
      secret_display:             SecretDisplay::default(),
      remember:                   false,
      remember_session:           false,
      remember_user_session:      false,
      theme:                      Theme::default(),
      time:                       false,
      time_format:                None,
      battery:                    false,
      greeting:                   None,
      title:                      Default::default(),
      message:                    None,
      powers:                     Menu::default(),
      power_setsid:               false,
      kb_command:                 2,
      kb_sessions:                3,
      kb_power:                   12,
      kb_background:              4,
      status_show_reset:          true,
      status_show_command:        true,
      status_show_session:        true,
      status_show_power:          true,
      status_show_background:     true,
      status_show_session_status: true,
      status_show_caps_lock:      true,
      animation:                  None,
      animation_fps:              None,
      mock:                       false,
      backgrounds:                Menu::default(),
      working:                    false,
      done:                       false,
      exit:                       None,
    }
  }
}

impl Drop for Greeter {
  fn drop(&mut self) {
    self.scrub(true, false);
  }
}

impl Greeter {
  pub async fn new(events: Sender<Event>) -> Self {
    let mut greeter = Self::default();

    greeter.events = Some(events);
    greeter.set_locale();

    greeter.powers = Menu {
      title:    fl!("title_power"),
      options:  Default::default(),
      selected: 0,
    };

    #[cfg(not(test))]
    {
      let args = env::args().collect::<Vec<String>>();

      if let Err(err) = greeter.parse_options(&args).await {
        eprintln!("{err}");
        print_usage(Self::options());

        process::exit(1);
      }

      // Initialize logger immediately after CLI parsing so that any errors
      // during config loading are captured in the debug log.
      greeter.logger = crate::init_logger(&greeter);

      // Load configuration after CLI parsing
      if !greeter.config().opt_present("no-config") {
        let config_path = greeter
          .config()
          .opt_str("config")
          .map(std::path::PathBuf::from);
        match tuigreet_config::parser::load_config(
          config_path.as_deref(),
          greeter.config.as_ref(),
        ) {
          Ok(config) => {
            // Validate config
            match config.validate(false) {
              Ok(warnings) => {
                for warning in warnings {
                  tracing::warn!("Config warning: {}", warning);
                }

                // Apply config to greeter only if validation passed
                greeter.apply_config(&config);

                // Apply theme config
                let cli_theme = greeter.config().opt_str("theme");
                greeter.apply_theme_config(&config.theme, cli_theme.as_deref());

                // Store config for later use
                greeter.loaded_config = Some(config.clone());

                // Apply terminal sizing based on [[outputs]] / [terminal]
                // config.  This must happen before the TUI is started so
                // that ratatui sees the correct terminal dimensions.
                if !config.outputs.is_empty() || config.terminal.cols.is_some()
                {
                  tuigreet_system::apply_output_config(
                    &config.outputs,
                    &config.terminal,
                  );
                }

                // Handle --dump-config
                if greeter.config().opt_present("dump-config") {
                  let toml_str = toml::to_string_pretty(&config)
                    .unwrap_or_else(|_| {
                      "# Failed to serialize config".to_string()
                    });
                  println!("{toml_str}");
                  process::exit(0);
                }
              },
              Err(e) => {
                tracing::error!("Config validation failed: {}", e);
                tracing::error!(
                  "Skipping invalid configuration, using CLI options and \
                   defaults"
                );
                // Do not apply invalid config - continue with CLI options
              },
            }
          },
          Err(e) => {
            tracing::warn!("Failed to load config: {}", e);
          },
        }
      }

      greeter.connect().await;
    }

    let mut sessions = get_sessions(&greeter).unwrap_or_default();

    if greeter.mock && sessions.is_empty() {
      sessions = mock_sessions();
    }

    if matches!(greeter.session_source, SessionSource::None)
      && !sessions.is_empty()
    {
      greeter.session_source = SessionSource::Session(0);
    }

    greeter.sessions = Menu {
      title:    fl!("title_session"),
      options:  sessions,
      selected: 0,
    };

    // If a default user is specified, use it.
    if let Some(ref default_user) = greeter.default_user {
      greeter.username = MaskedString::from(default_user.clone(), None);
    }

    // If we should remember the last logged-in user.
    if greeter.remember
      && let Some(username) = get_last_user_username()
    {
      greeter.username = MaskedString::from(username, get_last_user_name());

      // If, on top of that, we should remember their last session.
      if greeter.remember_user_session {
        // See if we have the last free-form command from the user.
        if let Ok(command) = get_last_user_command(greeter.username.get()) {
          greeter.session_source = SessionSource::Command(command);
        }

        // If a session was saved, use it and its name.
        if let Ok(ref session_path) =
          get_last_user_session(greeter.username.get())
        {
          // Set the selected menu option and the session source.
          if let Some(index) = greeter.sessions.options.iter().position(
            |Session { path, .. }| path.as_deref() == Some(session_path),
          ) {
            greeter.sessions.selected = index;
            greeter.session_source =
              SessionSource::Session(greeter.sessions.selected);
          }
        }
      }
    }

    // Same thing, but not user specific.
    if greeter.remember_session {
      if let Ok(command) = get_last_command() {
        greeter.session_source =
          SessionSource::Command(command.trim().to_string());
      }

      if let Ok(ref session_path) = get_last_session_path()
        && let Some(index) = greeter.sessions.options.iter().position(
          |Session { path, .. }| path.as_deref() == Some(session_path),
        )
      {
        greeter.sessions.selected = index;
        greeter.session_source =
          SessionSource::Session(greeter.sessions.selected);
      }
    }

    greeter
  }

  // Scrub memory of all data, unless `soft` is true, in which case, we will
  // keep the username (can happen if a wrong password was entered, we want to
  // give the user another chance, as PAM would).
  fn scrub(&mut self, scrub_message: bool, soft: bool) {
    self.buffer.zeroize();
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
      self.socket = if let Ok(socket) = env::var("GREETD_SOCK") {
        socket
      } else {
        eprintln!("GREETD_SOCK must be defined");
        process::exit(1);
      };
    }

    match UnixStream::connect(&self.socket).await {
      Ok(stream) => {
        self.stream = Some(Arc::new(RwLock::new(stream)));
      },

      Err(err) => {
        eprintln!("{err}");
        process::exit(1);
      },
    }
  }

  #[must_use]
  pub const fn config(&self) -> &Matches {
    self
      .config
      .as_ref()
      .expect("config accessed before parse_options")
  }

  pub async fn stream(&self) -> RwLockWriteGuard<'_, UnixStream> {
    self
      .stream
      .as_ref()
      .expect("stream accessed before connect")
      .write()
      .await
  }

  #[must_use]
  pub fn option(&self, name: &str) -> Option<String> {
    self.config().opt_str(name)
  }

  #[must_use]
  pub fn options_multi(&self, name: &str) -> Option<Vec<String>> {
    if self.config().opt_present(name) {
      Some(self.config().opt_strs(name))
    } else {
      None
    }
  }

  // Returns the width of the main window where content is displayed from the
  // provided arguments.
  #[must_use]
  pub fn width(&self) -> u16 {
    if let Some(value) = self.option("width")
      && let Ok(width) = value.parse::<u16>()
    {
      return width;
    }

    if let Some(ref config) = self.loaded_config {
      return config.layout.width;
    }

    80
  }

  // Returns the padding of the screen from the provided arguments.
  #[must_use]
  pub fn window_padding(&self) -> u16 {
    // Check CLI override first
    if let Some(value) = self.option("window-padding")
      && let Ok(padding) = value.parse::<u16>()
    {
      return padding;
    }

    // Then check loaded config
    if let Some(ref config) = self.loaded_config
      && let Some(padding) = config.layout.window_padding
    {
      return padding;
    }

    0
  }

  // Returns the padding of the main window where content is displayed from the
  // provided arguments.
  #[must_use]
  pub fn container_padding(&self) -> u16 {
    // Check CLI override first
    if let Some(value) = self.option("container-padding")
      && let Ok(padding) = value.parse::<u16>()
    {
      return padding + 1;
    }

    // Then check loaded config
    if let Some(ref config) = self.loaded_config
      && let Some(padding) = config.layout.container_padding
    {
      return padding + 1;
    }

    2
  }

  // Returns the spacing between each prompt from the provided arguments.
  #[must_use]
  pub fn prompt_padding(&self) -> u16 {
    // Check CLI override first
    if let Some(value) = self.option("prompt-padding")
      && let Ok(padding) = value.parse::<u16>()
    {
      return padding;
    }

    // Then check loaded config
    if let Some(ref config) = self.loaded_config
      && let Some(padding) = config.layout.prompt_padding
    {
      return padding;
    }

    1
  }

  #[must_use]
  pub fn greet_align(&self) -> GreetAlign {
    if let Some(value) = self.option("greet-align") {
      return match value.as_str() {
        "left" => GreetAlign::Left,
        "right" => GreetAlign::Right,
        _ => GreetAlign::Center,
      };
    }

    if let Some(ref config) = self.loaded_config {
      return match config.display.align_greeting {
        tuigreet_config::AlignGreeting::Left => GreetAlign::Left,
        tuigreet_config::AlignGreeting::Right => GreetAlign::Right,
        tuigreet_config::AlignGreeting::Center => GreetAlign::Center,
      };
    }

    GreetAlign::default()
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

  #[must_use]
  pub fn options() -> Options {
    let mut opts = Options::new();

    let xsession_wrapper_desc = format!(
      "wrapper command to initialize X server and launch X11 sessions \
       (default: {DEFAULT_XSESSION_WRAPPER})"
    );

    opts.optflag("h", "help", "show this usage information");
    opts.optflag("v", "version", "print version information");
    opts.optflagopt(
      "d",
      "debug",
      "enable debug logging to the provided file, or to /tmp/tuigreet.log",
      "FILE",
    );
    opts.optopt("c", "cmd", "command to run", "COMMAND");
    opts.optmulti(
      "",
      "env",
      "environment variables to run the default session with (can appear more \
       than once)",
      "KEY=VALUE",
    );
    opts.optopt(
      "s",
      "sessions",
      "colon-separated list of Wayland session paths",
      "DIRS",
    );
    opts.optopt(
      "",
      "session-wrapper",
      "wrapper command to initialize the non-X11 session",
      "'CMD [ARGS]...'",
    );
    opts.optopt(
      "x",
      "xsessions",
      "colon-separated list of X11 session paths",
      "DIRS",
    );
    opts.optopt(
      "",
      "xsession-wrapper",
      xsession_wrapper_desc.as_str(),
      "'CMD [ARGS]...'",
    );
    opts.optflag(
      "",
      "no-xsession-wrapper",
      "do not wrap commands for X11 sessions",
    );
    opts.optopt(
      "w",
      "width",
      "width of the main prompt (default: 80)",
      "WIDTH",
    );
    opts.optflag("i", "issue", "show the host's issue file");
    opts.optopt(
      "g",
      "greeting",
      "show custom text above login prompt",
      "GREETING",
    );
    opts.optflag("", "title", "show the container's title");
    opts.optopt(
      "",
      "custom-title",
      "custom title for the login form",
      "TITLE",
    );
    opts.optflag("t", "time", "display the current date and time");
    opts.optopt(
      "",
      "time-format",
      "custom strftime format for displaying date and time",
      "FORMAT",
    );
    opts.optflag("b", "battery", "display battery percentage");
    opts.optopt("u", "user", "pre-fill username field", "USER");
    opts.optflag("r", "remember", "remember last logged-in username");
    opts.optflag("", "remember-session", "remember last selected session");
    opts.optflag(
      "",
      "remember-user-session",
      "remember last selected session for each user",
    );
    opts.optflag(
      "",
      "user-menu",
      "allow graphical selection of users from a menu",
    );
    opts.optopt(
      "",
      "user-menu-min-uid",
      "minimum UID to display in the user selection menu",
      "UID",
    );
    opts.optopt(
      "",
      "user-menu-max-uid",
      "maximum UID to display in the user selection menu",
      "UID",
    );
    opts.optopt("", "theme", "define the application theme colors", "THEME");
    opts.optflag("", "asterisks", "display asterisks when a secret is typed");
    opts.optopt(
      "",
      "asterisks-char",
      "characters to be used to redact secrets (default: *)",
      "CHARS",
    );
    opts.optopt(
      "",
      "window-padding",
      "padding inside the terminal area (default: 0)",
      "PADDING",
    );
    opts.optopt(
      "",
      "container-padding",
      "padding inside the main prompt container (default: 1)",
      "PADDING",
    );
    opts.optopt(
      "",
      "prompt-padding",
      "padding between prompt rows (default: 1)",
      "PADDING",
    );
    opts.optopt(
      "",
      "greet-align",
      "alignment of the greeting text in the main prompt container (default: \
       'center')",
      "[left|center|right]",
    );

    opts.optopt(
      "",
      "power-shutdown",
      "command to run to shut down the system",
      "'CMD [ARGS]...'",
    );
    opts.optopt(
      "",
      "power-reboot",
      "command to run to reboot the system",
      "'CMD [ARGS]...'",
    );
    opts.optflag(
      "",
      "power-no-setsid",
      "do not prefix power commands with setsid",
    );

    opts.optopt(
      "",
      "kb-command",
      "F-key to use to open the command menu",
      "[1-12]",
    );
    opts.optopt(
      "",
      "kb-sessions",
      "F-key to use to open the sessions menu",
      "[1-12]",
    );
    opts.optopt(
      "",
      "kb-power",
      "F-key to use to open the power menu",
      "[1-12]",
    );
    opts.optopt(
      "",
      "kb-background",
      "F-key to use to open the background animation switcher menu",
      "[1-12]",
    );

    opts.optopt("", "config", "path to configuration file", "PATH");
    opts.optflag("", "no-config", "disable loading configuration files");
    opts.optflag("", "dump-config", "print effective configuration and exit");
    opts.optflag("", "list-outputs", "list available DRM outputs and exit");

    opts.optopt(
      "",
      "background",
      "background animation to render behind the login UI ('doom' or 'none')",
      "NAME",
    );
    opts.optopt(
      "",
      "background-fps",
      "render rate when a background animation is active (default: 30)",
      "FPS",
    );
    opts.optopt(
      "",
      "doom-height",
      "DOOM fire decay control, higher means taller flames (1-9, default: 6)",
      "N",
    );
    opts.optopt(
      "",
      "doom-spread",
      "DOOM fire horizontal jitter (0-4, default: 2)",
      "N",
    );
    opts.optopt(
      "",
      "doom-colors",
      "DOOM fire colors as TOP,MIDDLE,BOTTOM (each #RRGGBB or named)",
      "TOP,MIDDLE,BOTTOM",
    );
    opts.optflag(
      "",
      "mock",
      "visual mock-up mode: skip the greetd socket and fake the auth flow \
       locally",
    );
    opts.optopt(
      "",
      "matrix-colors",
      "matrix rain colors as HEAD,BRIGHT,DIM (each #RRGGBB or named)",
      "HEAD,BRIGHT,DIM",
    );
    opts.optopt(
      "",
      "matrix-length",
      "matrix trail length range as MIN,MAX in rows (default: 6,18)",
      "MIN,MAX",
    );
    opts.optopt(
      "",
      "matrix-speed",
      "matrix fall speed range as MIN,MAX in rows-per-frame (default: \
       0.30,1.10)",
      "MIN,MAX",
    );

    opts
  }

  // Parses command line arguments to configured the software accordingly.
  pub async fn parse_options<S>(
    &mut self,
    args: &[S],
  ) -> Result<(), Box<dyn Error>>
  where
    S: AsRef<OsStr>,
  {
    let opts = Self::options();

    self.config = match opts.parse(args) {
      Ok(matches) => Some(matches),
      Err(err) => return Err(err.into()),
    };

    if self.config().opt_present("help") {
      print_usage(opts);
      process::exit(0);
    }
    if self.config().opt_present("version") {
      print_version();
      process::exit(0);
    }

    if self.config().opt_present("list-outputs") {
      tuigreet_system::list_outputs();
    }

    self.mock = self.config().opt_present("mock");

    if self.config().opt_present("debug") {
      self.debug = true;

      self.logfile = match self.config().opt_str("debug") {
        Some(file) => file,
        None => DEFAULT_LOG_FILE.to_string(),
      }
    }

    if self.config().opt_present("issue")
      && self.config().opt_present("greeting")
    {
      return Err(
        "Only one of --issue and --greeting may be used at the same time"
          .into(),
      );
    }

    if self.config().opt_present("theme")
      && let Some(spec) = self.config().opt_str("theme")
    {
      self.theme = Theme::parse(spec.as_str());
    }

    if self.config().opt_present("asterisks") {
      let asterisk =
        if let Some(value) = self.config().opt_str("asterisks-char") {
          if value.chars().count() < 1 {
            return Err(
              "--asterisks-char must have at least one character as its value"
                .into(),
            );
          }

          value
        } else {
          DEFAULT_ASTERISKS_CHARS.to_string()
        };

      self.secret_display = SecretDisplay::Character(asterisk);
    }

    self.time = self.config().opt_present("time");
    self.battery = self.config().opt_present("battery");

    if let Some(format) = self.config().opt_str("time-format") {
      if StrftimeItems::new(&format).any(|item| item == Item::Error) {
        return Err("Invalid strftime format provided in --time-format".into());
      }

      self.time_format = Some(format);
    }

    self.title.enable = self.config().opt_present("title");
    if let Some(custom_title) = self.option("custom-title") {
      self.title.custom = Some(custom_title);
    }

    if self.config().opt_present("user-menu") {
      self.user_menu = true;

      let min_uid = self
        .config()
        .opt_str("user-menu-min-uid")
        .and_then(|uid| uid.parse::<u32>().ok());
      let max_uid = self
        .config()
        .opt_str("user-menu-max-uid")
        .and_then(|uid| uid.parse::<u32>().ok());
      let (min_uid, max_uid) = get_min_max_uids(min_uid, max_uid);

      tracing::info!("min/max UIDs are {}/{}", min_uid, max_uid);

      if min_uid > max_uid {
        return Err(
          "Minimum UID ({min_uid}) must not exceed maximum UID ({max_uid})"
            .into(),
        );
      }

      self.users = Menu {
        title:    fl!("title_users"),
        options:  get_users(min_uid, max_uid),
        selected: 0,
      };

      tracing::info!("found {} users", self.users.options.len());
    }

    if self.config().opt_present("remember-session")
      && self.config().opt_present("remember-user-session")
    {
      return Err(
        "Only one of --remember-session and --remember-user-session may be \
         used at the same time"
          .into(),
      );
    }
    if self.config().opt_present("remember-user-session")
      && !self.config().opt_present("remember")
    {
      return Err("--remember-session must be used with --remember".into());
    }

    self.default_user = self.option("user");
    self.remember = self.config().opt_present("remember");
    self.remember_session = self.config().opt_present("remember-session");
    self.remember_user_session =
      self.config().opt_present("remember-user-session");
    self.greeting = self.option("greeting");

    // If the `--cmd` argument is provided, it will override the selected
    // session.
    if let Some(command) = self.option("cmd") {
      let envs = self.options_multi("env");

      if let Some(envs) = envs {
        for env in envs {
          if !env.contains('=') {
            return Err(
              format!("malformed environment variable definition for '{env}'")
                .into(),
            );
          }
        }
      }

      self.session_source =
        SessionSource::DefaultCommand(command, self.options_multi("env"));
    }

    if let Some(dirs) = self.option("sessions") {
      for dir in env::split_paths(&dirs) {
        self.add_session_path(dir, SessionType::Wayland);
      }
    }

    if let Some(dirs) = self.option("xsessions") {
      for dir in env::split_paths(&dirs) {
        self.add_session_path(dir, SessionType::X11);
      }
    }

    if let Some(wrapper) = self.option("session-wrapper") {
      self.session_wrapper = Some(wrapper);
    }

    if !self.config().opt_present("no-xsession-wrapper") {
      self.xsession_wrapper = self
        .option("xsession-wrapper")
        .or_else(|| Some(DEFAULT_XSESSION_WRAPPER.to_string()));
    }

    if self.config().opt_present("issue") {
      self.greeting = get_issue();
    }

    self.powers.options.push(Power {
      action:  PowerOption::Shutdown,
      label:   fl!("shutdown"),
      command: self.config().opt_str("power-shutdown"),
    });

    self.powers.options.push(Power {
      action:  PowerOption::Reboot,
      label:   fl!("reboot"),
      command: self.config().opt_str("power-reboot"),
    });

    self.power_setsid = !self.config().opt_present("power-no-setsid");

    self.kb_command = self
      .config()
      .opt_str("kb-command")
      .map_or(2, |i| i.parse::<u8>().unwrap_or_default());
    self.kb_sessions = self
      .config()
      .opt_str("kb-sessions")
      .map_or(3, |i| i.parse::<u8>().unwrap_or_default());
    self.kb_power = self
      .config()
      .opt_str("kb-power")
      .map_or(12, |i| i.parse::<u8>().unwrap_or_default());
    self.kb_background = self
      .config()
      .opt_str("kb-background")
      .map_or(4, |i| i.parse::<u8>().unwrap_or_default());

    let kbs = [
      self.kb_command,
      self.kb_sessions,
      self.kb_power,
      self.kb_background,
    ];
    for i in 0..kbs.len() {
      for j in (i + 1)..kbs.len() {
        if kbs[i] == kbs[j] {
          return Err("keybindings must all be distinct".into());
        }
      }
    }

    let cli_config = tuigreet_config::parser::extract_cli_config(self.config());
    self.set_background_from_config(&cli_config.background);
    self.populate_backgrounds_menu();

    Ok(())
  }

  fn populate_backgrounds_menu(&mut self) {
    let title = fl!("title_background");
    let none_label = fl!("background_none");
    let options = crate::ui::background::options(&none_label);
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
        let _ = sender.send(crate::event::Event::Refresh).await;
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
      self.push_frame_rate();
      return;
    };

    let parse = |s: &Option<String>, fallback: tui::style::Color| {
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
    self.push_frame_rate();
  }

  /// Render rate for the event loop.
  #[must_use]
  pub fn frame_rate(&self) -> f64 {
    self
      .animation_fps
      .map_or(DEFAULT_FPS as f64, |fps| fps.max(DEFAULT_FPS) as f64)
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

  fn add_session_path(&mut self, path: PathBuf, session_type: SessionType) {
    if self.session_paths.iter().any(|(known_path, known_type)| {
      known_path == &path && known_type == &session_type
    }) {
      return;
    }

    self.session_paths.push((path, session_type));
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
    };

    self.powers.options.push(Power {
      action,
      label,
      command,
    });
  }

  pub fn set_prompt(&mut self, prompt: &str) {
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
    match &self.prompt {
      None => 0,
      Some(prompt) => prompt.chars().count(),
    }
  }

  // Apply configuration settings to the greeter, respecting CLI overrides
  pub fn apply_config(&mut self, config: &tuigreet_config::Config) {
    use tuigreet_config::SecretMode;
    // General
    self.debug = config.general.debug;
    // Session
    if config.session.command.is_some() {
      self.session_source = SessionSource::DefaultCommand(
        config.session.command.clone().unwrap(),
        Some(config.session.environments.clone())
          .filter(|environments| !environments.is_empty()),
      );
    }
    for dir in &config.session.sessions_dirs {
      self.add_session_path(PathBuf::from(dir), SessionType::Wayland);
    }
    for dir in &config.session.xsessions_dirs {
      self.add_session_path(PathBuf::from(dir), SessionType::X11);
    }
    if config.session.session_wrapper.is_some() {
      self.session_wrapper = config.session.session_wrapper.clone();
    }
    if config.session.xsession_wrapper.is_some()
      && !self.config().opt_present("no-xsession-wrapper")
    {
      self.xsession_wrapper = config.session.xsession_wrapper.clone();
    }
    // Display
    self.time = config.display.show_time;
    self.time_format = config.display.time_format.clone();
    self.battery = config.display.battery;
    self.greeting = if config.display.issue {
      get_issue()
    } else {
      config.display.greeting.clone()
    };
    self.title.enable = config.display.show_title;
    self.title.custom = config.display.custom_title.clone();
    // Remember
    self.default_user = config.remember.default_user.clone();
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
    }
    // Power
    self
      .set_power_command(PowerOption::Shutdown, config.power.shutdown.clone());
    self.set_power_command(PowerOption::Reboot, config.power.reboot.clone());
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
    self.kb_command = config.keybindings.command;
    self.kb_sessions = config.keybindings.sessions;
    self.kb_power = config.keybindings.power;
    self.kb_background = config.keybindings.background;
    // Status bar visibility
    self.status_show_reset = config.layout.widgets.status_bar.show_reset;
    self.status_show_command = config.layout.widgets.status_bar.show_command;
    self.status_show_session = config.layout.widgets.status_bar.show_session;
    self.status_show_power = config.layout.widgets.status_bar.show_power;
    self.status_show_background =
      config.layout.widgets.status_bar.show_background;
    self.status_show_session_status =
      config.layout.widgets.status_bar.show_session_status;
    self.status_show_caps_lock =
      config.layout.widgets.status_bar.show_caps_lock;
    // Animation
    self.set_background_from_config(&config.background);
    self.populate_backgrounds_menu();
  }

  // Apply theme configuration
  pub fn apply_theme_config(
    &mut self,
    theme_config: &tuigreet_config::ThemeConfig,
    cli_theme: Option<&str>,
  ) {
    use tuigreet_theme::config::{apply_cli_theme, theme_from_config};

    // Start with theme from config
    let config_theme = theme_from_config(theme_config);

    // Apply CLI theme override if present
    self.theme = apply_cli_theme(config_theme, cli_theme);
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

fn print_usage(opts: Options) {
  eprint!("{}", opts.usage("Usage: tuigreet [OPTIONS]"));
}

fn print_version() {
  println!("tuigreet {} ({})", env!("VERSION"), env!("TARGET"));
  println!(
    "Copyright (C) 2020 Antoine POPINEAU <https://github.com/apognu/tuigreet>."
  );
  println!("Maintained by NotAShelf <https://github.com/NotAShelf/tuigreet>.");
  println!("Licensed under GPLv3+ (GNU GPL version 3 or later).");
  println!();
  println!(
    "This is free software, you are welcome to redistribute it under some \
     conditions."
  );
  println!("There is NO WARRANTY, to the extent provided by law.");
}

#[cfg(test)]
mod test {
  use tuigreet_config::Config;
  use tuigreet_types::SecretDisplay;

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

  #[tokio::test]
  async fn test_command_line_arguments() {
    let table: &[(&[&str], _, Option<fn(&Greeter)>)] = &[
      // No arguments
      (&[], true, None),
      // Valid combinations
      (&["--cmd", "hello"], true, None),
      (
        &[
          "--cmd",
          "uname",
          "--env",
          "A=B",
          "--env",
          "C=D=E",
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
          "12",
          "--user-menu",
        ],
        true,
        Some(|greeter| {
          assert!(
            matches!(&greeter.session_source, SessionSource::DefaultCommand(cmd, Some(env)) if cmd == "uname" && env.len() == 2)
          );

          if let SessionSource::DefaultCommand(_, Some(env)) =
            &greeter.session_source
          {
            assert_eq!(env[0], "A=B");
            assert_eq!(env[1], "C=D=E");
          }

          assert!(
            matches!(&greeter.secret_display, SecretDisplay::Character(c) if c == ".")
          );
          assert_eq!(greeter.prompt_padding(), 0);
          assert_eq!(greeter.window_padding(), 1);
          assert_eq!(greeter.container_padding(), 13);
          assert!(greeter.user_menu);
          assert!(matches!(
            greeter.xsession_wrapper.as_deref(),
            Some("startx")
          ));
        }),
      ),
      (
        &["--xsession-wrapper", "mywrapper.sh"],
        true,
        Some(|greeter| {
          assert!(matches!(
            greeter.xsession_wrapper.as_deref(),
            Some("mywrapper.sh")
          ));
        }),
      ),
      (
        &["--no-xsession-wrapper"],
        true,
        Some(|greeter| {
          assert!(greeter.xsession_wrapper.is_none());
        }),
      ),
      // Invalid combinations
      (
        &["--remember-session", "--remember-user-session"],
        false,
        None,
      ),
      (&["--asterisk-char", ""], false, None),
      (&["--remember-user-session"], false, None),
      (&["--min-uid", "10000", "--max-uid", "5000"], false, None),
      (&["--issue", "--greeting", "Hello, world!"], false, None),
      (&["--kb-command", "F2", "--kb-sessions", "F2"], false, None),
      (&["--time-format", "%i %"], false, None),
      (&["--cmd", "cmd", "--env"], false, None),
      (&["--cmd", "cmd", "--env", "A"], false, None),
    ];

    for (opts, valid, check) in table {
      let mut greeter = Greeter::default();

      match valid {
        true => {
          assert!(
            matches!(greeter.parse_options(opts).await, Ok(())),
            "{opts:?} cannot be parsed"
          );

          if let Some(check) = check {
            check(&greeter);
          }
        },
        false => assert!((greeter.parse_options(opts).await).is_err()),
      }
    }
  }

  #[tokio::test]
  async fn test_session_paths_are_deduplicated_after_config_overlay() {
    let sess_dir = "/usr/share/wayland-sessions";

    let mut greeter = Greeter::default();

    assert!(
      greeter
        .parse_options(&["--sessions", sess_dir])
        .await
        .is_ok()
    );

    // CLI options are merged into the configuration before it is applied, so
    // the same directory can appear several times.
    let mut config = Config::default();
    config.session.sessions_dirs = vec![sess_dir.to_string(); 2];
    greeter.apply_config(&config);

    assert_eq!(greeter.session_paths, [(
      std::path::PathBuf::from(sess_dir),
      SessionType::Wayland
    )]);
  }

  #[tokio::test]
  async fn test_config_session_environments_survive_cli_overlay() {
    let mut greeter = Greeter::default();

    assert!(
      greeter
        .parse_options(&[
          "--cmd",
          "sway",
          "--env",
          "XDG_CURRENT_DESKTOP=sway",
          "--env",
          "XDG_SESSION_TYPE=wayland",
        ])
        .await
        .is_ok()
    );

    let mut config = Config::default();
    config.session.command = Some("sway".to_string());
    config.session.environments = vec![
      "XDG_CURRENT_DESKTOP=sway".to_string(),
      "XDG_SESSION_TYPE=wayland".to_string(),
    ];
    greeter.apply_config(&config);

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
  async fn test_config_power_options_are_applied() {
    let mut greeter = Greeter::default();

    assert!(greeter.parse_options(&[] as &[&str]).await.is_ok());

    let mut config = Config::default();
    config.power.shutdown = Some("loginctl poweroff".to_string());
    config.power.reboot = Some("loginctl reboot".to_string());
    config.power.use_setsid = false;
    greeter.apply_config(&config);

    assert!(!greeter.power_setsid);
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Shutdown
        && power.command.as_deref() == Some("loginctl poweroff")
    }));
    assert!(greeter.powers.options.iter().any(|power| {
      power.action == PowerOption::Reboot
        && power.command.as_deref() == Some("loginctl reboot")
    }));
  }
}

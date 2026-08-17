use serde::{Deserialize, Serialize};

/// Root configuration structure
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
  /// Parser metadata used to preserve explicit default-valued secret modes
  /// during configuration layering.
  #[serde(skip)]
  pub(crate) secret_mode_specified: bool,

  #[serde(default)]
  pub general: GeneralConfig,

  #[serde(default)]
  pub session: SessionConfig,

  #[serde(default)]
  pub display: DisplayConfig,

  #[serde(default)]
  pub remember: RememberConfig,

  #[serde(default)]
  pub user_menu: UserMenuConfig,

  #[serde(default)]
  pub secret: SecretConfig,

  #[serde(default)]
  pub layout: LayoutConfig,

  #[serde(default)]
  pub power: PowerConfig,

  #[serde(default)]
  pub keybindings: KeybindingsConfig,

  #[serde(default)]
  pub theme: ThemeConfig,

  #[serde(default)]
  pub background: BackgroundConfig,

  /// Per-output (display/monitor) configuration.
  /// Use `[[outputs]]` array of tables in TOML.
  #[serde(default)]
  pub outputs: Vec<OutputConfig>,

  /// Explicit terminal size override. When both `cols` and `rows` are set
  /// they take precedence over output-derived sizing.
  #[serde(default)]
  pub terminal: TerminalConfig,
}

/// Configuration for a single DRM output (monitor/display).
///
/// Example:
/// ```toml
/// [[outputs]]
/// connector = "DP-1"
/// primary = true
///
/// [[outputs]]
/// connector = "HDMI-A-1"
/// enabled = false
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OutputConfig {
  /// DRM connector name as it appears in `/sys/class/drm/` (e.g. `"DP-1"`,
  /// `"HDMI-A-1"`).
  pub connector: String,

  /// Whether tuigreet should use this output. Defaults to `true`.
  #[serde(default = "default_true")]
  pub enabled: bool,

  /// If `true`, size the terminal to match this output's native resolution.
  /// At most one output should be marked primary. If none is marked primary
  /// the first enabled output is used for sizing.
  #[serde(default)]
  pub primary: bool,
}

/// Explicit terminal character-cell size override.
///
/// When both `cols` and `rows` are provided they take highest priority over
/// output-derived sizing. Providing only one of the two fields is an error.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct TerminalConfig {
  /// Number of character columns.
  pub cols: Option<u16>,
  /// Number of character rows.
  pub rows: Option<u16>,
}

impl TerminalConfig {
  /// Returns `true` when the config is consistent: either both fields are
  /// absent, or both are `Some(v)` with `v > 0`.  Any other combination is
  /// invalid.
  #[must_use]
  pub fn is_valid(&self) -> bool {
    self.invalid_reason().is_none()
  }

  /// Returns a human-readable error string describing the first invalid
  /// condition found, or `None` when the config is consistent.
  #[must_use]
  pub fn invalid_reason(&self) -> Option<String> {
    match (self.cols, self.rows) {
      (Some(_), None) => {
        Some(
          "`terminal.cols` is set but `terminal.rows` is missing; both must \
           be provided together"
            .to_string(),
        )
      },
      (None, Some(_)) => {
        Some(
          "`terminal.rows` is set but `terminal.cols` is missing; both must \
           be provided together"
            .to_string(),
        )
      },
      (Some(0), Some(_)) => {
        Some("`terminal.cols` must be greater than 0".to_string())
      },
      (Some(_), Some(0)) => {
        Some("`terminal.rows` must be greater than 0".to_string())
      },
      _ => None,
    }
  }
}

/// General configuration options
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GeneralConfig {
  /// Enable debug logging
  #[serde(default)]
  pub debug: bool,

  /// Log file path
  #[serde(default = "default_log_file")]
  pub log_file: String,

  /// Enable numlock on startup
  #[serde(default)]
  pub numlock: bool,
}

impl Default for GeneralConfig {
  fn default() -> Self {
    Self {
      debug:    false,
      log_file: default_log_file(),
      numlock:  false,
    }
  }
}

/// Session management configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionConfig {
  /// Override session with a specific command
  #[serde(default)]
  pub command: Option<String>,

  /// Directories containing Wayland session files
  #[serde(default)]
  pub sessions_dirs: Vec<String>,

  /// Directories containing X11 session files
  #[serde(default)]
  pub xsessions_dirs: Vec<String>,

  /// Wrapper command for non-X11 sessions
  #[serde(default)]
  pub session_wrapper: Option<String>,

  /// Wrapper command for X11 sessions
  #[serde(default = "default_xsession_wrapper")]
  pub xsession_wrapper: Option<String>,

  /// Environment variables for default session
  #[serde(default)]
  pub environments: Vec<String>,
}

impl Default for SessionConfig {
  fn default() -> Self {
    Self {
      command:          None,
      sessions_dirs:    vec![],
      xsessions_dirs:   vec![],
      session_wrapper:  None,
      xsession_wrapper: default_xsession_wrapper(),
      environments:     Vec::new(),
    }
  }
}

/// Display and visual configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct DisplayConfig {
  /// Legacy secret-mask toggle. Prefer `[secret] mode = "characters"`.
  #[serde(default, skip_serializing)]
  pub(crate) asterisks: Option<bool>,

  /// Show current time
  #[serde(default)]
  pub show_time: bool,

  /// Custom time format (strftime)
  #[serde(default)]
  pub time_format: Option<String>,

  /// Custom greeting message
  #[serde(default)]
  pub greeting: Option<String>,

  /// Show login form title
  #[serde(default = "default_show_title")]
  pub show_title: bool,

  /// Custom login form title
  #[serde(default)]
  pub custom_title: Option<String>,

  /// Show /etc/issue file
  #[serde(default)]
  pub issue: bool,

  /// Show battery percentage
  #[serde(default)]
  pub battery: bool,

  /// Greeting text alignment
  #[serde(default)]
  pub align_greeting: AlignGreeting,
}

/// Remember/cache configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RememberConfig {
  /// Default user to pre-fill
  #[serde(default)]
  pub default_user: Option<String>,

  /// Remember last logged-in username
  #[serde(default)]
  pub username: bool,

  /// Remember last selected session (global)
  #[serde(default)]
  pub session: bool,

  /// Remember last selected session per user
  #[serde(default)]
  pub user_session: bool,
}

/// User menu configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserMenuConfig {
  /// Enable user selection menu
  #[serde(default)]
  pub enabled: bool,

  /// Minimum UID to display in user menu
  #[serde(default = "default_min_uid")]
  pub min_uid: u32,

  /// Maximum UID to display in user menu
  #[serde(default = "default_max_uid")]
  pub max_uid: u32,
}

impl Default for UserMenuConfig {
  fn default() -> Self {
    Self {
      enabled: false,
      min_uid: default_min_uid(),
      max_uid: default_max_uid(),
    }
  }
}

/// Secret display configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SecretConfig {
  /// How to display secrets
  #[serde(default)]
  pub mode: SecretMode,

  /// Characters to use when mode is Characters
  #[serde(default = "default_secret_characters")]
  pub characters: String,
}

impl Default for SecretConfig {
  fn default() -> Self {
    Self {
      mode:       SecretMode::Hidden,
      characters: default_secret_characters(),
    }
  }
}

/// Layout and sizing configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LayoutConfig {
  /// Width of the main prompt container
  #[serde(default = "default_width")]
  pub width: u16,

  /// Padding around the terminal window
  #[serde(default)]
  pub window_padding: Option<u16>,

  /// Padding inside the main container
  #[serde(default)]
  pub container_padding: Option<u16>,

  /// Padding between prompt rows
  #[serde(default)]
  pub prompt_padding: Option<u16>,

  /// Widget positioning options
  #[serde(default)]
  pub widgets: WidgetConfig,
}

impl Default for LayoutConfig {
  fn default() -> Self {
    Self {
      width:             default_width(),
      window_padding:    None,
      container_padding: None,
      prompt_padding:    None,
      widgets:           WidgetConfig::default(),
    }
  }
}

/// Battery widget placement
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatteryPosition {
  #[default]
  Left,
  Right,
}

/// Widget positioning configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct WidgetConfig {
  /// Position of time widget
  #[serde(default)]
  pub time_position: WidgetPosition,

  /// Position of status bar widget
  #[serde(default)]
  pub status_position: WidgetPosition,

  /// Position of battery widget
  #[serde(default)]
  pub battery_position: BatteryPosition,

  /// Status bar item visibility
  #[serde(default)]
  pub status_bar: StatusBarConfig,
}

/// Status bar item visibility configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct StatusBarConfig {
  /// Show the ESC/Reset button
  #[serde(default = "default_true")]
  pub show_reset: bool,

  /// Show the command button
  #[serde(default = "default_true")]
  pub show_command: bool,

  /// Show the session button
  #[serde(default = "default_true")]
  pub show_session: bool,

  /// Show the power button
  #[serde(default = "default_true")]
  pub show_power: bool,

  /// Show the background button
  #[serde(default = "default_true")]
  pub show_background: bool,

  /// Show the current session/command indicator
  #[serde(default = "default_true")]
  pub show_session_status: bool,

  /// Show the caps lock indicator
  #[serde(default = "default_true")]
  pub show_caps_lock: bool,
}

impl Default for StatusBarConfig {
  fn default() -> Self {
    Self {
      show_reset:          default_true(),
      show_command:        default_true(),
      show_session:        default_true(),
      show_power:          default_true(),
      show_background:     default_true(),
      show_session_status: default_true(),
      show_caps_lock:      default_true(),
    }
  }
}

/// Power management configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PowerConfig {
  /// Custom shutdown command
  #[serde(default)]
  pub shutdown: Option<String>,

  /// Custom reboot command
  #[serde(default)]
  pub reboot: Option<String>,

  /// Custom suspend command
  #[serde(default)]
  pub suspend: Option<String>,

  /// Custom hibernate command
  #[serde(default)]
  pub hibernate: Option<String>,

  /// Use setsid to detach power commands
  #[serde(default = "default_use_setsid")]
  pub use_setsid: bool,
}

impl Default for PowerConfig {
  fn default() -> Self {
    Self {
      shutdown:   None,
      reboot:     None,
      suspend:    None,
      hibernate:  None,
      use_setsid: default_use_setsid(),
    }
  }
}

/// Keybindings configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct KeybindingsConfig {
  /// F-key for command menu (1-12)
  #[serde(default = "default_kb_command")]
  pub command: u8,

  /// F-key for sessions menu (1-12)
  #[serde(default = "default_kb_sessions")]
  pub sessions: u8,

  /// F-key for power menu (1-12)
  #[serde(default = "default_kb_power")]
  pub power: u8,

  /// F-key for the on-the-fly background switcher menu (1-12)
  #[serde(default = "default_kb_background")]
  pub background: u8,
}

impl Default for KeybindingsConfig {
  fn default() -> Self {
    Self {
      command:    default_kb_command(),
      sessions:   default_kb_sessions(),
      power:      default_kb_power(),
      background: default_kb_background(),
    }
  }
}

/// Theme/color configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct ThemeConfig {
  /// Border color
  #[serde(default)]
  pub border:    Option<String>,
  /// Base text color
  #[serde(default)]
  pub text:      Option<String>,
  /// Time display color
  #[serde(default)]
  pub time:      Option<String>,
  /// Container background color
  #[serde(default)]
  pub container: Option<String>,
  /// Container title color
  #[serde(default)]
  pub title:     Option<String>,
  /// Greeting text color
  #[serde(default)]
  pub greet:     Option<String>,
  /// Prompt text color
  #[serde(default)]
  pub prompt:    Option<String>,
  /// User input color
  #[serde(default)]
  pub input:     Option<String>,
  /// Action text color
  #[serde(default)]
  pub action:    Option<String>,
  /// Action button color
  #[serde(default)]
  pub button:    Option<String>,
}

/// Background animation configuration. Each animation kind owns its own
/// namespaced sub-section so new animations can be added without collisions.
/// Distinct from upstream's `[animations]` section, which configures the
/// foreground tachyonfx post-processor.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct BackgroundConfig {
  /// Which animation to run. `None` or `"none"` disables animations.
  /// Currently supported: `"doom"`, `"matrix"`.
  #[serde(default)]
  pub kind: Option<String>,

  /// Render frame rate when an animation is active. Defaults to 30 FPS when
  /// `kind` is set; the base UI tick (2 FPS) otherwise.
  #[serde(default)]
  pub fps: Option<u32>,

  /// Parameters for the DOOM-style fire effect.
  #[serde(default)]
  pub doom: DoomConfig,

  /// Parameters for the cmatrix-style digital rain effect.
  #[serde(default)]
  pub matrix: MatrixConfig,
}

/// Parameters for the DOOM-style fire animation. Field names mirror Ly's
/// `doom_fire_*` config keys.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct DoomConfig {
  /// Decay control (1..=9). Higher = taller flames.
  #[serde(default)]
  pub height: Option<u8>,

  /// Horizontal jitter range (0..=4).
  #[serde(default)]
  pub spread: Option<u8>,

  /// Color of the coolest flame tips. Accepts `#RRGGBB`, `0xRRGGBB`, or any
  /// color name accepted by ratatui (e.g. `red`, `magenta`).
  #[serde(default)]
  pub top_color: Option<String>,

  /// Color of the mid-band flames.
  #[serde(default)]
  pub middle_color: Option<String>,

  /// Color of the hottest flames at the base.
  #[serde(default)]
  pub bottom_color: Option<String>,
}

/// Parameters for the cmatrix-style digital rain animation.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct MatrixConfig {
  /// Color of the leading glyph in each falling stream. Accepts `#RRGGBB`,
  /// `0xRRGGBB`, or any color name accepted by ratatui.
  #[serde(default)]
  pub head_color: Option<String>,

  /// Color of the brightest part of the trail (just behind the head).
  #[serde(default)]
  pub bright_color: Option<String>,

  /// Color of the dim tail before each glyph fades out.
  #[serde(default)]
  pub dim_color: Option<String>,

  /// Inclusive minimum trail length in rows.
  #[serde(default)]
  pub min_length: Option<u16>,

  /// Inclusive maximum trail length in rows.
  #[serde(default)]
  pub max_length: Option<u16>,

  /// Inclusive minimum stream speed, in rows-per-frame. Lower = slower.
  #[serde(default)]
  pub min_speed: Option<f32>,

  /// Inclusive maximum stream speed, in rows-per-frame.
  #[serde(default)]
  pub max_speed: Option<f32>,

  /// Per-cell, per-frame probability of a glyph mutating (the faint trail
  /// shimmer). `0.0` disables.
  #[serde(default)]
  pub mutate_chance: Option<f32>,
}

/// Greeting alignment options
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignGreeting {
  Left,
  #[default]
  Center,
  Right,
}

/// Secret display modes
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretMode {
  #[default]
  Hidden,
  Characters,
}

/// Widget position options
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetPosition {
  #[default]
  Default,
  Top,
  Bottom,
  Hidden,
}

// Default value functions
const fn default_true() -> bool {
  true
}

const fn default_show_title() -> bool {
  true
}

fn default_log_file() -> String {
  "/tmp/tuigreet.log".to_string()
}

fn default_xsession_wrapper() -> Option<String> {
  Some("startx /usr/bin/env".to_string())
}

const fn default_min_uid() -> u32 {
  1000
}

const fn default_max_uid() -> u32 {
  60000
}

fn default_secret_characters() -> String {
  "*".to_string()
}

const fn default_width() -> u16 {
  80
}

const fn default_use_setsid() -> bool {
  true
}

const fn default_kb_command() -> u8 {
  2
}

const fn default_kb_sessions() -> u8 {
  3
}

const fn default_kb_power() -> u8 {
  12
}

const fn default_kb_background() -> u8 {
  4
}

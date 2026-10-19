use std::{borrow::Cow, error::Error as StdError, fmt, str::FromStr};

use pound::ValueEnum;
use serde::{
  Deserialize,
  Deserializer,
  Serialize,
  Serializer,
  de::Error as DeserializeError,
};

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
  pub idle: IdleConfig,

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

  /// Default session regex for the path name of a session
  #[serde(default)]
  pub default_session: Option<String>,
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
      default_session:  None,
    }
  }
}

/// Display and visual configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "Independent user-configurable display toggles form the serialized \
            configuration schema."
)]
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

impl Default for DisplayConfig {
  fn default() -> Self {
    Self {
      asterisks:      None,
      show_time:      false,
      time_format:    None,
      greeting:       None,
      show_title:     default_show_title(),
      custom_title:   None,
      issue:          false,
      battery:        false,
      align_greeting: AlignGreeting::default(),
    }
  }
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

  /// Prompt placement options
  #[serde(default)]
  pub prompt: PromptLayoutConfig,

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
      prompt:            PromptLayoutConfig::default(),
      widgets:           WidgetConfig::default(),
    }
  }
}

/// Anchor used to place the prompt inside the available terminal area.
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum PromptAnchor {
  /// Top-left corner.
  TopLeft,
  /// Center of the top edge.
  TopCenter,
  /// Top-right corner.
  TopRight,
  /// Center of the left edge.
  CenterLeft,
  /// Center of the available area.
  #[default]
  Center,
  /// Center of the right edge.
  CenterRight,
  /// Bottom-left corner.
  BottomLeft,
  /// Center of the bottom edge.
  BottomCenter,
  /// Bottom-right corner.
  BottomRight,
}

/// Prompt placement and safe-area configuration.
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq,
)]
pub struct PromptLayoutConfig {
  /// Point in the available terminal area to which the prompt is anchored.
  #[serde(default)]
  pub anchor: PromptAnchor,

  /// Signed horizontal adjustment after anchoring.
  #[serde(default)]
  pub offset_x: i16,

  /// Signed vertical adjustment after anchoring.
  #[serde(default)]
  pub offset_y: i16,

  /// Minimum distance between the prompt and an edge of the available area.
  #[serde(default)]
  pub edge_margin: u16,
}

/// Existing widgets that can be composed into terminal edge regions.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegionItem {
  /// Current time.
  Time,
  /// Battery charge and charging state.
  Battery,
  /// Configured action shortcuts.
  Actions,
  /// Current session or command.
  Session,
  /// Caps Lock state.
  CapsLock,
}

/// Composable one-line regions at the top and bottom edges of the UI.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RegionConfig {
  /// Items aligned to the top-left edge.
  #[serde(default)]
  pub top_left:     Vec<RegionItem>,
  /// Items aligned to the top-right edge.
  #[serde(default)]
  pub top_right:    Vec<RegionItem>,
  /// Items aligned to the bottom-left edge.
  #[serde(default)]
  pub bottom_left:  Vec<RegionItem>,
  /// Items aligned to the bottom-right edge.
  #[serde(default)]
  pub bottom_right: Vec<RegionItem>,
}

impl RegionConfig {
  #[must_use]
  pub const fn is_configured(&self) -> bool {
    !self.top_left.is_empty()
      || !self.top_right.is_empty()
      || !self.bottom_left.is_empty()
      || !self.bottom_right.is_empty()
  }
}

/// Battery widget placement
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq,
)]
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

  /// Composable edge regions. Any configured region supersedes legacy widget
  /// positioning while preserving the legacy defaults when all are empty.
  #[serde(default)]
  pub regions: RegionConfig,

  /// Status-bar item visibility.
  #[serde(default)]
  pub status_bar: StatusBarConfig,
}

/// Status bar item visibility configuration
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "Each independently configurable status-bar item has its own \
            serialized visibility toggle."
)]
pub struct StatusBarConfig {
  /// Show the Escape/reset action.
  #[serde(default = "default_true")]
  pub show_reset: bool,

  /// Show the command action.
  #[serde(default = "default_true")]
  pub show_command: bool,

  /// Show the session action.
  #[serde(default = "default_true")]
  pub show_session: bool,

  /// Show the power action.
  #[serde(default = "default_true")]
  pub show_power: bool,

  /// Show the background action.
  #[serde(default = "default_true")]
  pub show_background: bool,

  /// Show the current session or command.
  #[serde(default = "default_true")]
  pub show_session_status: bool,

  /// Show the Caps Lock indicator.
  #[serde(default = "default_true")]
  pub show_caps_lock: bool,

  /// Show the password visibility action when it is bound.
  #[serde(default = "default_true")]
  pub show_password: bool,
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
      show_password:       default_true(),
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

/// Action to apply after an inactivity timeout.
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum IdleAction {
  /// Clear the terminal surface until the next key press.
  #[default]
  Blank,
  /// Freeze the animation while keeping the interface visible.
  PauseAnimation,
}

/// Inactivity behavior. A zero timeout disables idle handling.
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq,
)]
pub struct IdleConfig {
  /// Seconds without keyboard input before the action runs; zero disables it.
  #[serde(default)]
  pub timeout_seconds: u64,

  /// Action applied after the timeout.
  #[serde(default)]
  pub action: IdleAction,
}

/// Built-in UI action available to configurable keybindings.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BindingAction {
  /// Open the command editor.
  Command,
  /// Open the session menu.
  Sessions,
  /// Open the power menu.
  Power,
  /// Open the background selector.
  Background,
  /// Reveal or mask an active secret prompt.
  TogglePassword,
}

/// One terminal key used by an action binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKey {
  /// One Unicode scalar value.
  Character(char),
  /// A function key in the range F1-F12.
  Function(u8),
  /// A supported named terminal key.
  Named(NamedBindingKey),
}

/// Named terminal keys supported by action bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NamedBindingKey {
  /// Escape.
  Escape,
  /// Enter or Return.
  Enter,
  /// Tab.
  Tab,
  /// Reverse Tab.
  BackTab,
  /// Backspace.
  Backspace,
  /// Delete.
  Delete,
  /// Insert.
  Insert,
  /// Home.
  Home,
  /// End.
  End,
  /// Page Up.
  PageUp,
  /// Page Down.
  PageDown,
  /// Up arrow.
  Up,
  /// Down arrow.
  Down,
  /// Left arrow.
  Left,
  /// Right arrow.
  Right,
}

const NAMED_KEY_LABELS: [&str; 15] = [
  "Esc",
  "Enter",
  "Tab",
  "BackTab",
  "Backspace",
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Up",
  "Down",
  "Left",
  "Right",
];

const NAMED_KEY_ALIASES: &[(&str, NamedBindingKey)] = &[
  ("esc", NamedBindingKey::Escape),
  ("escape", NamedBindingKey::Escape),
  ("enter", NamedBindingKey::Enter),
  ("tab", NamedBindingKey::Tab),
  ("backtab", NamedBindingKey::BackTab),
  ("backspace", NamedBindingKey::Backspace),
  ("delete", NamedBindingKey::Delete),
  ("insert", NamedBindingKey::Insert),
  ("home", NamedBindingKey::Home),
  ("end", NamedBindingKey::End),
  ("pageup", NamedBindingKey::PageUp),
  ("pagedown", NamedBindingKey::PageDown),
  ("up", NamedBindingKey::Up),
  ("down", NamedBindingKey::Down),
  ("left", NamedBindingKey::Left),
  ("right", NamedBindingKey::Right),
];

impl NamedBindingKey {
  const fn label(self) -> &'static str {
    NAMED_KEY_LABELS[self as usize]
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct BindingModifiers(u8);

impl BindingModifiers {
  const CONTROL: u8 = 1;
  const ALT: u8 = 1 << 1;
  const SHIFT: u8 = 1 << 2;

  const fn insert(&mut self, modifier: u8) -> bool {
    let inserted = self.0 & modifier == 0;
    self.0 |= modifier;
    inserted
  }

  const fn contains(self, modifier: u8) -> bool {
    self.0 & modifier != 0
  }

  const fn remove(&mut self, modifier: u8) {
    self.0 &= !modifier;
  }

  const fn is_empty(self) -> bool {
    self.0 == 0
  }
}

/// A validated terminal key chord used by a built-in action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
  key:       BindingKey,
  modifiers: BindingModifiers,
}

impl KeyChord {
  /// Default chord used to reveal an active password prompt.
  pub const DEFAULT_PASSWORD_TOGGLE: Self = Self {
    key:       BindingKey::Function(7),
    modifiers: BindingModifiers(0),
  };

  /// Creates an unmodified function-key chord when `number` is in `1..=12`.
  #[must_use]
  pub const fn function(number: u8) -> Option<Self> {
    if number >= 1 && number <= 12 {
      Some(Self {
        key:       BindingKey::Function(number),
        modifiers: BindingModifiers(0),
      })
    } else {
      None
    }
  }

  /// Returns the terminal key in this chord.
  #[must_use]
  pub const fn key(self) -> BindingKey {
    self.key
  }

  /// Returns whether the chord contains Control.
  #[must_use]
  pub const fn control(self) -> bool {
    self.modifiers.contains(BindingModifiers::CONTROL)
  }

  /// Returns whether the chord contains Alt.
  #[must_use]
  pub const fn alt(self) -> bool {
    self.modifiers.contains(BindingModifiers::ALT)
  }

  /// Returns whether the chord contains Shift.
  #[must_use]
  pub const fn shift(self) -> bool {
    self.modifiers.contains(BindingModifiers::SHIFT)
  }
}

/// Error returned when a key chord string is malformed or unsupported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyChordParseError {
  /// No key chord was provided.
  Empty,
  /// A leading, trailing, or repeated `+` created an empty segment.
  EmptySegment,
  /// A modifier appeared more than once, including through an alias.
  DuplicateModifier,
  /// More than one non-modifier key appeared.
  MultipleKeys,
  /// The chord contained modifiers but no key.
  MissingKey,
  /// The key name is not supported.
  UnsupportedKey,
  /// A function key was malformed or outside F1-F12.
  InvalidFunctionKey,
  /// An unmodified printable character would interfere with text entry.
  UnmodifiedCharacter,
}

impl fmt::Display for KeyChordParseError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let message = match self {
      Self::Empty => "key chord must not be empty",
      Self::EmptySegment => "key chord contains an empty segment",
      Self::DuplicateModifier => "key chord contains a duplicate modifier",
      Self::MultipleKeys => "key chord must contain exactly one key",
      Self::MissingKey => "key chord is missing its key",
      Self::UnsupportedKey => "key chord contains an unsupported key",
      Self::InvalidFunctionKey => "function key must be in the range F1-F12",
      Self::UnmodifiedCharacter => {
        "printable action keys require Control, Alt, or Shift"
      },
    };
    formatter.write_str(message)
  }
}

impl StdError for KeyChordParseError {}

impl FromStr for KeyChord {
  type Err = KeyChordParseError;

  fn from_str(source: &str) -> Result<Self, Self::Err> {
    if source.trim().is_empty() {
      return Err(KeyChordParseError::Empty);
    }
    let (key, mut modifiers) = parse_chord_parts(source)?;
    let key = match key {
      BindingKey::Named(NamedBindingKey::Tab)
        if modifiers.contains(BindingModifiers::SHIFT) =>
      {
        BindingKey::Named(NamedBindingKey::BackTab)
      },
      key => key,
    };
    if matches!(key, BindingKey::Named(NamedBindingKey::BackTab)) {
      modifiers.remove(BindingModifiers::SHIFT);
    }
    if matches!(key, BindingKey::Character(_)) && modifiers.is_empty() {
      return Err(KeyChordParseError::UnmodifiedCharacter);
    }
    Ok(Self { key, modifiers })
  }
}

fn parse_chord_parts(
  source: &str,
) -> Result<(BindingKey, BindingModifiers), KeyChordParseError> {
  let mut modifiers = BindingModifiers::default();
  let mut key = None;
  for part in source.split('+') {
    let part = part.trim();
    if part.is_empty() {
      return Err(KeyChordParseError::EmptySegment);
    }
    if let Some(modifier) = parse_modifier(part) {
      if !modifiers.insert(modifier) {
        return Err(KeyChordParseError::DuplicateModifier);
      }
    } else if key.replace(parse_binding_key(part)?).is_some() {
      return Err(KeyChordParseError::MultipleKeys);
    }
  }
  Ok((key.ok_or(KeyChordParseError::MissingKey)?, modifiers))
}

impl fmt::Display for KeyChord {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut first = true;
    for (enabled, label) in [
      (self.control(), "Ctrl"),
      (self.alt(), "Alt"),
      (self.shift(), "Shift"),
    ] {
      if enabled {
        write_key_part(formatter, &mut first, label)?;
      }
    }
    match self.key {
      BindingKey::Character(character) => {
        write_key_part(formatter, &mut first, character)
      },
      BindingKey::Function(number) => {
        if !first {
          formatter.write_str("+")?;
        }
        write!(formatter, "F{number}")
      },
      BindingKey::Named(key) => {
        write_key_part(formatter, &mut first, key.label())
      },
    }
  }
}

impl Serialize for KeyChord {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.collect_str(self)
  }
}

impl<'de> Deserialize<'de> for KeyChord {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Cow::<'de, str>::deserialize(deserializer)?
      .parse()
      .map_err(DeserializeError::custom)
  }
}

const fn parse_modifier(part: &str) -> Option<u8> {
  if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
    Some(BindingModifiers::CONTROL)
  } else if part.eq_ignore_ascii_case("alt") {
    Some(BindingModifiers::ALT)
  } else if part.eq_ignore_ascii_case("shift") {
    Some(BindingModifiers::SHIFT)
  } else {
    None
  }
}

fn parse_binding_key(part: &str) -> Result<BindingKey, KeyChordParseError> {
  if let Some(key) = NAMED_KEY_ALIASES
    .iter()
    .find_map(|(name, key)| part.eq_ignore_ascii_case(name).then_some(*key))
  {
    return Ok(BindingKey::Named(key));
  }

  if let Some(number) =
    part.strip_prefix('f').or_else(|| part.strip_prefix('F'))
  {
    let number = number
      .parse::<u8>()
      .map_err(|_| KeyChordParseError::InvalidFunctionKey)?;
    return KeyChord::function(number)
      .map(|chord| chord.key)
      .ok_or(KeyChordParseError::InvalidFunctionKey);
  }

  let mut characters = part.chars();
  let character = characters
    .next()
    .filter(|_| characters.next().is_none())
    .ok_or(KeyChordParseError::UnsupportedKey)?;
  Ok(BindingKey::Character(character.to_ascii_lowercase()))
}

fn write_key_part(
  formatter: &mut fmt::Formatter<'_>,
  first: &mut bool,
  part: impl fmt::Display,
) -> fmt::Result {
  if !*first {
    formatter.write_str("+")?;
  }
  *first = false;
  write!(formatter, "{part}")
}

/// A configurable key-to-action mapping.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ActionBindingConfig {
  /// Validated terminal chord that triggers the action.
  pub key:    KeyChord,
  /// Built-in action triggered by the chord.
  pub action: BindingAction,
}

/// Keybindings configuration.
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

  /// Additional or replacement built-in action bindings. Bindings here
  /// override a default binding with the same key or action.
  #[serde(default)]
  pub actions: Vec<ActionBindingConfig>,
}

impl Default for KeybindingsConfig {
  fn default() -> Self {
    Self {
      command:    default_kb_command(),
      sessions:   default_kb_sessions(),
      power:      default_kb_power(),
      background: default_kb_background(),
      actions:    Vec::new(),
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
#[derive(
  Debug, Clone, Deserialize, Serialize, ValueEnum, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
#[pound(ignore_case)]
pub enum AlignGreeting {
  Left,
  #[default]
  Center,
  Right,
}

/// Secret display modes
#[derive(
  Debug, Clone, Deserialize, Serialize, ValueEnum, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
#[pound(ignore_case)]
pub enum SecretMode {
  #[default]
  Hidden,
  #[pound(alias = "character")]
  Characters,
}

/// Widget position options
#[derive(
  Debug, Clone, Copy, Deserialize, Serialize, ValueEnum, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
#[pound(ignore_case)]
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

#[expect(
  clippy::unnecessary_wraps,
  reason = "Serde's default callback must return the field's Option<String> \
            type."
)]
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn key_chords_normalize_aliases_and_character_case()
  -> Result<(), KeyChordParseError> {
    let control: KeyChord = "Control + B".parse()?;
    let ctrl: KeyChord = "ctrl+b".parse()?;
    let backtab: KeyChord = "backtab".parse()?;
    let shifted_tab: KeyChord = "shift+tab".parse()?;

    assert_eq!(control, ctrl);
    assert_eq!(control.to_string(), "Ctrl+b");
    assert_eq!(backtab, shifted_tab);
    assert_eq!(shifted_tab.to_string(), "BackTab");
    Ok(())
  }

  #[test]
  fn key_chords_reject_ambiguous_or_malformed_input() {
    assert_eq!(
      "ctrl++b".parse::<KeyChord>(),
      Err(KeyChordParseError::EmptySegment)
    );
    assert_eq!(
      "ctrl+control+b".parse::<KeyChord>(),
      Err(KeyChordParseError::DuplicateModifier)
    );
    assert_eq!(
      "F13".parse::<KeyChord>(),
      Err(KeyChordParseError::InvalidFunctionKey)
    );
    assert_eq!(
      "x".parse::<KeyChord>(),
      Err(KeyChordParseError::UnmodifiedCharacter)
    );
  }

  #[test]
  fn key_chords_round_trip_through_toml() -> Result<(), Box<dyn StdError>> {
    let config: KeybindingsConfig = toml::from_str(
      r#"
      [[actions]]
      key = "shift+backtab"
      action = "sessions"
      "#,
    )?;

    let encoded = toml::to_string(&config)?;
    let decoded: KeybindingsConfig = toml::from_str(&encoded)?;
    assert_eq!(decoded, config);
    Ok(())
  }
}

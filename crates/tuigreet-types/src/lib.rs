use std::{
  error::Error,
  fmt::{self, Display},
};

/// Default log file path
pub const DEFAULT_LOG_FILE: &str = "/tmp/tuigreet.log";
/// Default asterisk character for password masking
pub const DEFAULT_ASTERISKS_CHARS: &str = "*";
/// Default X session wrapper command
pub const DEFAULT_XSESSION_WRAPPER: &str = "startx";

/// Authentication status returned from PAM operations
#[derive(Debug, Copy, Clone)]
pub enum AuthStatus {
  /// Authentication succeeded
  Success,

  /// Authentication failed
  Failure,

  /// User cancelled authentication
  Cancel,
}

impl Display for AuthStatus {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{self:?}")
  }
}

impl Error for AuthStatus {}

/// UI mode determining which input/menu is displayed
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum Mode {
  /// Username input field
  #[default]
  Username,

  /// Password input field
  Password,

  /// Action selection (login/cancel)
  Action,

  /// User selection menu
  Users,

  /// Session selection menu
  Sessions,

  /// Power menu (shutdown/reboot)
  Power,
  /// Background animation selection menu
  Background,
  /// Processing/authenticating state
  Processing,
}

/// How to display secret/password input
#[derive(Debug, Clone, Default)]
pub enum SecretDisplay {
  /// Hide all characters
  #[default]
  Hidden,

  /// Show replacement characters (e.g., asterisks)
  Character(String),
}

impl SecretDisplay {
  /// Returns true if secrets should be shown (as replacement characters)
  #[must_use]
  pub const fn show(&self) -> bool {
    match self {
      Self::Hidden => false,
      Self::Character(_) => true,
    }
  }
}

/// Text alignment for greeting message
#[derive(Debug, Clone, Default)]
pub enum GreetAlign {
  /// Center alignment
  #[default]
  Center,
  /// Left alignment
  Left,
  /// Right alignment
  Right,
}

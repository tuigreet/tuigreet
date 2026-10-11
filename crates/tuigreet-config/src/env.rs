use std::env;

use crate::{
  Config,
  schema::{AlignGreeting, SecretMode, WidgetPosition},
};

/// Apply environment variable overrides to configuration.
///
/// Supported variables: `TUIGREET_DEBUG`, `TUIGREET_LOG_FILE`,
/// `TUIGREET_SESSION_COMMAND`, etc. Invalid boolean values are logged as
/// warnings and ignored.
pub fn load_env_variables() -> Config {
  let mut config = Config::default();
  // General config
  if let Ok(value) = env::var("TUIGREET_DEBUG") {
    if let Ok(debug) = parse_bool(&value) {
      config.general.debug = debug;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_DEBUG value: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_LOG_FILE") {
    config.general.log_file = value;
  }

  if let Ok(value) = env::var("TUIGREET_NUMLOCK") {
    if let Ok(numlock) = parse_bool(&value) {
      config.general.numlock = numlock;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_NUMLOCK value: '{}', expected true/false",
        value
      );
    }
  }

  // Session config
  if let Ok(value) = env::var("TUIGREET_SESSION_COMMAND") {
    config.session.command = Some(value);
  }

  // Display config
  if let Ok(value) = env::var("TUIGREET_TIME") {
    if let Ok(show_time) = parse_bool(&value) {
      config.display.show_time = show_time;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_TIME value: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_TIME_FORMAT") {
    config.display.time_format = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_GREETING") {
    config.display.greeting = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_TITLE") {
    if let Ok(show_title) = parse_bool(&value) {
      config.display.show_title = show_title;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_TITLE: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_CUSTOM_TITLE") {
    config.display.custom_title = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_ISSUE") {
    if let Ok(issue) = parse_bool(&value) {
      config.display.issue = issue;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_ISSUE value: '{}', expected true/false",
        value
      );
    }
  }

  // Remember config
  if let Ok(value) = env::var("TUIGREET_REMEMBER") {
    if let Ok(remember) = parse_bool(&value) {
      config.remember.username = remember;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_REMEMBER value: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_REMEMBER_SESSION") {
    if let Ok(remember) = parse_bool(&value) {
      config.remember.session = remember;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_REMEMBER_SESSION value: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_REMEMBER_USER_SESSION") {
    if let Ok(remember) = parse_bool(&value) {
      config.remember.user_session = remember;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_REMEMBER_USER_SESSION value: '{}', expected \
         true/false",
        value
      );
    }
  }

  // User menu config
  if let Ok(value) = env::var("TUIGREET_USER_MENU") {
    if let Ok(enabled) = parse_bool(&value) {
      config.user_menu.enabled = enabled;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_USER_MENU value: '{}', expected true/false",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_USER_MENU_MIN_UID") {
    if let Ok(uid) = value.parse::<u32>() {
      config.user_menu.min_uid = uid;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_USER_MENU_MIN_UID value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_USER_MENU_MAX_UID") {
    if let Ok(uid) = value.parse::<u32>() {
      config.user_menu.max_uid = uid;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_USER_MENU_MAX_UID value: '{}', expected number",
        value
      );
    }
  }

  // Layout config
  if let Ok(value) = env::var("TUIGREET_WIDTH") {
    if let Ok(width) = value.parse::<u16>() {
      config.layout.width = width;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_WIDTH value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_WINDOW_PADDING") {
    if let Ok(padding) = value.parse::<u16>() {
      config.layout.window_padding = Some(padding);
    } else {
      tracing::warn!(
        "Invalid TUIGREET_WINDOW_PADDING value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_CONTAINER_PADDING") {
    if let Ok(padding) = value.parse::<u16>() {
      config.layout.container_padding = Some(padding);
    } else {
      tracing::warn!(
        "Invalid TUIGREET_CONTAINER_PADDING value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_PROMPT_PADDING") {
    if let Ok(padding) = value.parse::<u16>() {
      config.layout.prompt_padding = Some(padding);
    } else {
      tracing::warn!(
        "Invalid TUIGREET_PROMPT_PADDING value: '{}', expected number",
        value
      );
    }
  }

  // Power config
  if let Ok(value) = env::var("TUIGREET_POWER_SHUTDOWN") {
    config.power.shutdown = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_POWER_REBOOT") {
    config.power.reboot = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_POWER_SUSPEND") {
    config.power.suspend = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_POWER_HIBERNATE") {
    config.power.hibernate = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_POWER_USE_SETSID") {
    if let Ok(use_setsid) = parse_bool(&value) {
      config.power.use_setsid = use_setsid;
    } else {
      tracing::warn!(
        "Invalid TUIGREET_POWER_USE_SETSID value: '{}', expected true/false",
        value
      );
    }
  }

  // Keybindings config
  if let Ok(value) = env::var("TUIGREET_KB_COMMAND") {
    if let Ok(key) = value.parse::<u8>() {
      if (1..=12).contains(&key) {
        config.keybindings.command = key;
      } else {
        tracing::warn!(
          "Invalid TUIGREET_KB_COMMAND value: '{}', expected 1-12",
          value
        );
      }
    } else {
      tracing::warn!(
        "Invalid TUIGREET_KB_COMMAND value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_KB_SESSIONS") {
    if let Ok(key) = value.parse::<u8>() {
      if (1..=12).contains(&key) {
        config.keybindings.sessions = key;
      } else {
        tracing::warn!(
          "Invalid TUIGREET_KB_SESSIONS value: '{}', expected 1-12",
          value
        );
      }
    } else {
      tracing::warn!(
        "Invalid TUIGREET_KB_SESSIONS value: '{}', expected number",
        value
      );
    }
  }

  if let Ok(value) = env::var("TUIGREET_KB_POWER") {
    if let Ok(key) = value.parse::<u8>() {
      if (1..=12).contains(&key) {
        config.keybindings.power = key;
      } else {
        tracing::warn!(
          "Invalid TUIGREET_KB_POWER value: '{}', expected 1-12",
          value
        );
      }
    } else {
      tracing::warn!(
        "Invalid TUIGREET_KB_POWER value: '{}', expected number",
        value
      );
    }
  }

  // Theme config - parse TUIGREET_THEME environment variable
  if let Ok(value) = env::var("TUIGREET_THEME") {
    // Parse theme string using the existing `Theme::parse` logic
    // Format: "border=white;text=green;container=blue" etc.
    let mut applied = false;

    // Parse the semicolon-separated theme specification
    for part in value.split(';') {
      if let Some((key, color)) = part.split_once('=') {
        let key = key.trim();
        let color = color.trim();

        match key {
          "border" => config.theme.border = Some(color.to_string()),
          "text" => config.theme.text = Some(color.to_string()),
          "time" => config.theme.time = Some(color.to_string()),
          "container" => config.theme.container = Some(color.to_string()),
          "title" => config.theme.title = Some(color.to_string()),
          "greet" => config.theme.greet = Some(color.to_string()),
          "prompt" => config.theme.prompt = Some(color.to_string()),
          "input" => config.theme.input = Some(color.to_string()),
          "action" => config.theme.action = Some(color.to_string()),
          "button" => {
            config.theme.button = Some(color.to_string());
          },
          _ => {
            tracing::warn!(
              "Unknown theme component '{}' in TUIGREET_THEME environment \
               variable",
              key
            );
            continue;
          },
        }

        applied = true;
      } else {
        tracing::warn!(
          "Invalid theme specification '{}' in TUIGREET_THEME, expected \
           format 'component=color'",
          part.trim()
        );
      }
    }

    if applied {
      tracing::info!(
        "Applied theme from TUIGREET_THEME environment variable: {}",
        value
      );
    }
  }

  // Display configuration
  if let Ok(value) = env::var("TUIGREET_ALIGN_GREETING") {
    match value.to_lowercase().as_str() {
      "left" => config.display.align_greeting = AlignGreeting::Left,
      "center" => config.display.align_greeting = AlignGreeting::Center,
      "right" => config.display.align_greeting = AlignGreeting::Right,
      _ => {
        tracing::warn!(
          "Invalid TUIGREET_ALIGN_GREETING value: '{}', expected 'left', \
           'center', or 'right'",
          value
        );
      },
    }
  }

  // Secret configuration
  if let Ok(value) = env::var("TUIGREET_SECRET_MODE") {
    match value.to_lowercase().as_str() {
      "hidden" => {
        config.secret.mode = SecretMode::Hidden;
        config.secret_mode_specified = true;
      },
      "character" | "characters" => {
        config.secret.mode = SecretMode::Characters;
        config.secret_mode_specified = true;
        if let Ok(chars) = env::var("TUIGREET_SECRET_CHARACTERS") {
          config.secret.characters = chars;
        }
      },
      _ => {
        tracing::warn!(
          "Invalid TUIGREET_SECRET_MODE value: '{}', expected 'hidden' or \
           'characters'",
          value
        );
      },
    }
  }

  if let Ok(value) = env::var("TUIGREET_SECRET_CHARACTERS") {
    config.secret.characters = value;
  }

  // Session configuration
  if let Ok(value) = env::var("TUIGREET_SESSIONS_DIRS") {
    config.session.sessions_dirs = value
      .split(':')
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();
  }

  if let Ok(value) = env::var("TUIGREET_XSESSIONS_DIRS") {
    config.session.xsessions_dirs = value
      .split(':')
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();
  }

  if let Ok(value) = env::var("TUIGREET_SESSION_WRAPPER") {
    config.session.session_wrapper = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_XSESSION_WRAPPER") {
    config.session.xsession_wrapper = Some(value);
  }

  if let Ok(value) = env::var("TUIGREET_ENVIRONMENTS") {
    config.session.environments = value
      .split(':')
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();
  }

  if let Ok(value) = env::var("TUIGREET_DEFAULT_SESSION") {
    config.session.default_session = Some(value);
  }

  // Widget positioning configuration
  if let Ok(value) = env::var("TUIGREET_TIME_POSITION") {
    match value.to_lowercase().as_str() {
      "default" => {
        config.layout.widgets.time_position = WidgetPosition::Default;
      },
      "top" => config.layout.widgets.time_position = WidgetPosition::Top,
      "bottom" => config.layout.widgets.time_position = WidgetPosition::Bottom,
      "hidden" => config.layout.widgets.time_position = WidgetPosition::Hidden,
      _ => {
        tracing::warn!(
          "Invalid TUIGREET_TIME_POSITION value: '{}', expected 'default', \
           'top', 'bottom', or 'hidden'",
          value
        );
      },
    }
  }

  if let Ok(value) = env::var("TUIGREET_STATUS_POSITION") {
    match value.to_lowercase().as_str() {
      "default" => {
        config.layout.widgets.status_position = WidgetPosition::Default;
      },
      "top" => config.layout.widgets.status_position = WidgetPosition::Top,
      "bottom" => {
        config.layout.widgets.status_position = WidgetPosition::Bottom;
      },
      "hidden" => {
        config.layout.widgets.status_position = WidgetPosition::Hidden;
      },
      _ => {
        tracing::warn!(
          "Invalid TUIGREET_STATUS_POSITION value: '{}', expected 'default', \
           'top', 'bottom', or 'hidden'",
          value
        );
      },
    }
  }

  // Status bar item visibility
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_RESET") {
    config.layout.widgets.status_bar.show_reset =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_COMMAND") {
    config.layout.widgets.status_bar.show_command =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_SESSION") {
    config.layout.widgets.status_bar.show_session =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_POWER") {
    config.layout.widgets.status_bar.show_power =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_BACKGROUND") {
    config.layout.widgets.status_bar.show_background =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_SESSION_STATUS") {
    config.layout.widgets.status_bar.show_session_status =
      value.to_lowercase() == "true";
  }
  if let Ok(value) = env::var("TUIGREET_STATUS_SHOW_CAPS_LOCK") {
    config.layout.widgets.status_bar.show_caps_lock =
      value.to_lowercase() == "true";
  }

  // Individual theme component environment variables
  if let Ok(value) = env::var("TUIGREET_THEME_BORDER") {
    config.theme.border = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_TEXT") {
    config.theme.text = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_TIME") {
    config.theme.time = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_CONTAINER") {
    config.theme.container = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_TITLE") {
    config.theme.title = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_GREET") {
    config.theme.greet = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_PROMPT") {
    config.theme.prompt = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_INPUT") {
    config.theme.input = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_ACTION") {
    config.theme.action = Some(value);
  }
  if let Ok(value) = env::var("TUIGREET_THEME_BUTTON") {
    config.theme.button = Some(value);
  }
  config
}

/// Parse a boolean value from string
/// Accepts: "true", "1", "yes" for true; "false", "0", "no" for false
fn parse_bool(value: &str) -> Result<bool, ()> {
  match value.to_lowercase().as_str() {
    "true" | "1" | "yes" => Ok(true),
    "false" | "0" | "no" => Ok(false),
    _ => Err(()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_bool() {
    assert_eq!(parse_bool("true"), Ok(true));
    assert_eq!(parse_bool("TRUE"), Ok(true));
    assert_eq!(parse_bool("1"), Ok(true));
    assert_eq!(parse_bool("yes"), Ok(true));
    assert_eq!(parse_bool("YES"), Ok(true));

    assert_eq!(parse_bool("false"), Ok(false));
    assert_eq!(parse_bool("FALSE"), Ok(false));
    assert_eq!(parse_bool("0"), Ok(false));
    assert_eq!(parse_bool("no"), Ok(false));
    assert_eq!(parse_bool("NO"), Ok(false));

    assert_eq!(parse_bool("invalid"), Err(()));
    assert_eq!(parse_bool(""), Err(()));
  }

  #[test]
  fn test_env_vars_processed_once_with_multiple_theme_components() {
    unsafe {
      env::set_var("TUIGREET_THEME", "border=red;text=blue;container=green");
      env::set_var("TUIGREET_SESSIONS_DIRS", "/test:/usr/share");
      env::set_var("TUIGREET_ALIGN_GREETING", "center");
    }

    let config = load_env_variables();

    // Verify all theme components applied
    assert_eq!(config.theme.border, Some("red".to_string()));
    assert_eq!(config.theme.text, Some("blue".to_string()));
    assert_eq!(config.theme.container, Some("green".to_string()));

    // Verify other env vars applied correctly (once, not per-component)
    assert_eq!(config.session.sessions_dirs, vec![
      "/test".to_string(),
      "/usr/share".to_string()
    ]);
    assert_eq!(config.display.align_greeting, AlignGreeting::Center);

    unsafe {
      env::remove_var("TUIGREET_THEME");
      env::remove_var("TUIGREET_SESSIONS_DIRS");
      env::remove_var("TUIGREET_ALIGN_GREETING");
    }
  }
}

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tuigreet_config::{
  ActionBindingConfig,
  BindingAction,
  BindingKey,
  KeyChord,
  KeybindingsConfig,
};

const RELEVANT_MODIFIERS: KeyModifiers = KeyModifiers::CONTROL
  .union(KeyModifiers::ALT)
  .union(KeyModifiers::SHIFT);

const NAMED_KEY_CODES: [KeyCode; 15] = [
  KeyCode::Esc,
  KeyCode::Enter,
  KeyCode::Tab,
  KeyCode::BackTab,
  KeyCode::Backspace,
  KeyCode::Delete,
  KeyCode::Insert,
  KeyCode::Home,
  KeyCode::End,
  KeyCode::PageUp,
  KeyCode::PageDown,
  KeyCode::Up,
  KeyCode::Down,
  KeyCode::Left,
  KeyCode::Right,
];

/// Runtime keybinding converted once from validated configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
  code:      KeyCode,
  modifiers: KeyModifiers,
  action:    BindingAction,
  label:     String,
}

impl KeyBinding {
  fn new(chord: KeyChord, action: BindingAction) -> Self {
    let mut modifiers = KeyModifiers::NONE;
    if chord.control() {
      modifiers.insert(KeyModifiers::CONTROL);
    }
    if chord.alt() {
      modifiers.insert(KeyModifiers::ALT);
    }
    if chord.shift() {
      modifiers.insert(KeyModifiers::SHIFT);
    }
    Self {
      code: code_from_key(chord.key()),
      modifiers,
      action,
      label: chord.to_string(),
    }
  }

  #[must_use]
  pub fn matches(&self, event: KeyEvent) -> bool {
    let mut modifiers = event.modifiers.intersection(RELEVANT_MODIFIERS);
    if event.code == KeyCode::BackTab {
      modifiers.remove(KeyModifiers::SHIFT);
    }
    modifiers == self.modifiers && codes_match(self.code, event.code)
  }

  #[must_use]
  pub const fn action(&self) -> BindingAction {
    self.action
  }

  #[must_use]
  pub fn label(&self) -> &str {
    &self.label
  }

  fn same_chord(&self, other: &Self) -> bool {
    self.modifiers == other.modifiers && codes_match(self.code, other.code)
  }
}

/// Builds effective bindings. Legacy menu keys replace the password-toggle
/// default on chord collisions. Explicit actions then replace both the same
/// action and the occupant of the same chord.
#[must_use]
pub fn bindings_from_config(config: &KeybindingsConfig) -> Vec<KeyBinding> {
  let mut bindings = Vec::with_capacity(5 + config.actions.len());
  insert_override(
    &mut bindings,
    KeyBinding::new(
      KeyChord::DEFAULT_PASSWORD_TOGGLE,
      BindingAction::TogglePassword,
    ),
  );

  for (number, action) in [
    (config.command, BindingAction::Command),
    (config.sessions, BindingAction::Sessions),
    (config.power, BindingAction::Power),
    (config.background, BindingAction::Background),
  ] {
    if let Some(chord) = KeyChord::function(number) {
      insert_override(&mut bindings, KeyBinding::new(chord, action));
    }
  }

  for ActionBindingConfig { key, action } in &config.actions {
    insert_override(&mut bindings, KeyBinding::new(*key, *action));
  }
  bindings
}

fn insert_override(bindings: &mut Vec<KeyBinding>, binding: KeyBinding) {
  bindings.retain(|current| {
    current.action != binding.action && !current.same_chord(&binding)
  });
  bindings.push(binding);
}

const fn code_from_key(key: BindingKey) -> KeyCode {
  match key {
    BindingKey::Character(character) => KeyCode::Char(character),
    BindingKey::Function(number) => KeyCode::F(number),
    BindingKey::Named(key) => NAMED_KEY_CODES[key as usize],
  }
}

fn codes_match(left: KeyCode, right: KeyCode) -> bool {
  match (left, right) {
    (KeyCode::Char(left), KeyCode::Char(right)) => {
      left.eq_ignore_ascii_case(&right)
    },
    _ => left == right,
  }
}

#[cfg(test)]
mod tests {
  use std::str::FromStr;

  use tuigreet_config::KeyChordParseError;

  use super::*;

  fn chord(spec: &str) -> Result<KeyChord, KeyChordParseError> {
    KeyChord::from_str(spec)
  }

  #[test]
  fn explicit_action_replaces_default_chord_and_action()
  -> Result<(), KeyChordParseError> {
    let mut config = KeybindingsConfig::default();
    config.actions.push(ActionBindingConfig {
      key:    chord("Ctrl+B")?,
      action: BindingAction::Background,
    });

    let bindings = bindings_from_config(&config);
    assert!(!bindings.iter().any(|binding| {
      binding.action() == BindingAction::Background && binding.label() == "F4"
    }));
    assert!(bindings.iter().any(|binding| {
      binding.action() == BindingAction::Background
        && binding
          .matches(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))
    }));
    Ok(())
  }

  #[test]
  fn explicit_chord_replaces_default_occupant() -> Result<(), KeyChordParseError>
  {
    let mut config = KeybindingsConfig::default();
    config.actions.push(ActionBindingConfig {
      key:    chord("F2")?,
      action: BindingAction::TogglePassword,
    });

    let bindings = bindings_from_config(&config);
    assert!(
      !bindings
        .iter()
        .any(|binding| binding.action() == BindingAction::Command)
    );
    assert!(bindings.iter().any(|binding| {
      binding.action() == BindingAction::TogglePassword
        && binding.matches(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
    }));
    Ok(())
  }

  #[test]
  fn legacy_function_key_wins_default_password_collision() {
    let config = KeybindingsConfig {
      command: 7,
      ..KeybindingsConfig::default()
    };

    let bindings = bindings_from_config(&config);
    assert!(bindings.iter().any(|binding| {
      binding.action() == BindingAction::Command
        && binding.matches(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE))
    }));
    assert!(
      !bindings
        .iter()
        .any(|binding| { binding.action() == BindingAction::TogglePassword })
    );
  }

  #[test]
  fn shifted_tab_matches_crossterm_backtab_event()
  -> Result<(), KeyChordParseError> {
    let binding = KeyBinding::new(chord("shift+tab")?, BindingAction::Sessions);

    assert!(
      binding.matches(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT,))
    );
    Ok(())
  }
}

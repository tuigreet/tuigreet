use std::{error::Error, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use greetd_ipc::Request;
use tokio::sync::RwLock;
use tuigreet_types::Mode;

use crate::{
  Greeter,
  event::Event,
  info::{
    delete_last_command,
    delete_last_session,
    get_last_user_command,
    get_last_user_session,
    write_last_command,
    write_last_session_path,
  },
  ipc::Ipc,
  power::power,
  ui::{
    common::masked::MaskedString,
    sessions::{Session, SessionSource},
    users::User,
  },
};

/// Handle keyboard input events.
///
/// Processes user input based on current application mode (username, password,
/// menu navigation, etc).
///
/// # Arguments
/// * `greeter` - Shared greeter state
/// * `input` - Key event from terminal
/// * `ipc` - IPC client for greetd communication
///
/// # Errors
/// Returns error if IPC communication fails
pub async fn handle(
  greeter: Arc<RwLock<Greeter>>,
  input: KeyEvent,
  ipc: Ipc,
) -> Result<(), Box<dyn Error>> {
  let mut greeter = greeter.write().await;

  if greeter.working {
    return Ok(());
  }

  match input {
    // ^U should erase the current buffer.
    KeyEvent {
      code: KeyCode::Char('u'),
      modifiers: KeyModifiers::CONTROL,
      ..
    } => {
      match greeter.mode {
        Mode::Username => greeter.username = MaskedString::default(),
        Mode::Password => greeter.buffer = String::new(),
        Mode::Command => greeter.buffer = String::new(),
        _ => {},
      }
    },

    // Exit handler for integration tests.
    #[cfg(test)]
    KeyEvent {
      code: KeyCode::Char('x'),
      modifiers: KeyModifiers::CONTROL,
      ..
    } => {
      use crate::{AuthStatus, Event};

      if let Some(ref sender) = greeter.events {
        let _ = sender.send(Event::Exit(AuthStatus::Cancel)).await;
      }
    },

    // Depending on the active screen, pressing Escape will either return to the
    // previous mode (close a popup, for example), or cancel the `greetd`
    // session.
    KeyEvent {
      code: KeyCode::Esc, ..
    } => {
      greeter.needs_clear = true;

      match greeter.mode {
        Mode::Command => {
          greeter.mode = greeter.previous_mode;
          greeter.buffer = greeter.previous_buffer.take().unwrap_or_default();
          greeter.cursor_offset = 0;
        },

        Mode::Users | Mode::Sessions | Mode::Power | Mode::Background => {
          greeter.mode = greeter.previous_mode;
        },

        _ => {
          Ipc::cancel(&mut greeter).await;
          greeter.reset(false).await;
        },
      }

      if let Some(ref sender) = greeter.events {
        let _ = sender.send(Event::Render).await;
      }
    },

    // Simple cursor directions in text fields.
    KeyEvent {
      code: KeyCode::Left,
      ..
    } => greeter.cursor_offset -= 1,
    KeyEvent {
      code: KeyCode::Right,
      ..
    } => greeter.cursor_offset += 1,

    // F2 will display the command entry prompt. If we are already in one of the
    // popup screens, we set the previous screen as being the current previous
    // screen.
    KeyEvent {
      code: KeyCode::F(i),
      ..
    } if i == greeter.kb_command => {
      greeter.previous_mode = match greeter.mode {
        Mode::Users
        | Mode::Command
        | Mode::Sessions
        | Mode::Power
        | Mode::Background => greeter.previous_mode,
        _ => greeter.mode,
      };

      // Set the edition buffer to the current command.
      greeter.previous_buffer = Some(greeter.buffer.clone());
      greeter.buffer = greeter
        .session_source
        .command(&greeter)
        .map(str::to_string)
        .unwrap_or_default();
      greeter.cursor_offset = 0;
      greeter.mode = Mode::Command;
    },

    // F3 will display the session selection menu. If we are already in one of
    // the popup screens, we set the previous screen as being the current
    // previous screen.
    KeyEvent {
      code: KeyCode::F(i),
      ..
    } if i == greeter.kb_sessions && !greeter.sessions.options.is_empty() => {
      greeter.previous_mode = match greeter.mode {
        Mode::Users
        | Mode::Command
        | Mode::Sessions
        | Mode::Power
        | Mode::Background => greeter.previous_mode,
        _ => greeter.mode,
      };

      greeter.mode = Mode::Sessions;
    },

    // F12 will display the user selection menu. If we are already in one of the
    // popup screens, we set the previous screen as being the current previous
    // screen.
    KeyEvent {
      code: KeyCode::F(i),
      ..
    } if i == greeter.kb_power && !greeter.powers.options.is_empty() => {
      greeter.previous_mode = match greeter.mode {
        Mode::Users
        | Mode::Command
        | Mode::Sessions
        | Mode::Power
        | Mode::Background => greeter.previous_mode,
        _ => greeter.mode,
      };

      greeter.mode = Mode::Power;
    },

    // F4 (default) opens the background animation switcher.
    KeyEvent {
      code: KeyCode::F(i),
      ..
    } if i == greeter.kb_background => {
      greeter.previous_mode = match greeter.mode {
        Mode::Users
        | Mode::Command
        | Mode::Sessions
        | Mode::Power
        | Mode::Background => greeter.previous_mode,
        _ => greeter.mode,
      };

      greeter.mode = Mode::Background;
    },

    // Handle moving up in menus.
    KeyEvent {
      code: KeyCode::Up, ..
    } => {
      if greeter.mode == Mode::Users && greeter.users.selected > 0 {
        greeter.users.selected -= 1;
      }

      if greeter.mode == Mode::Sessions && greeter.sessions.selected > 0 {
        greeter.sessions.selected -= 1;
      }

      if greeter.mode == Mode::Power && greeter.powers.selected > 0 {
        greeter.powers.selected -= 1;
      }

      if greeter.mode == Mode::Background && greeter.backgrounds.selected > 0 {
        greeter.backgrounds.selected -= 1;
      }
    },

    // Handle moving down in menus.
    KeyEvent {
      code: KeyCode::Down,
      ..
    } => {
      if greeter.mode == Mode::Users
        && greeter.users.selected + 1 < greeter.users.options.len()
      {
        greeter.users.selected += 1;
      }

      if greeter.mode == Mode::Sessions
        && greeter.sessions.selected + 1 < greeter.sessions.options.len()
      {
        greeter.sessions.selected += 1;
      }

      if greeter.mode == Mode::Power
        && greeter.powers.selected + 1 < greeter.powers.options.len()
      {
        greeter.powers.selected += 1;
      }

      if greeter.mode == Mode::Background
        && !greeter.backgrounds.options.is_empty()
        && greeter.backgrounds.selected < greeter.backgrounds.options.len() - 1
      {
        greeter.backgrounds.selected += 1;
      }
    },

    // ^A should go to the start of the current prompt
    KeyEvent {
      code: KeyCode::Char('a'),
      modifiers: KeyModifiers::CONTROL,
      ..
    } => {
      let value = {
        match greeter.mode {
          Mode::Username => &greeter.username.value,
          _ => &greeter.buffer,
        }
      };

      greeter.cursor_offset = -(value.chars().count() as i16);
    },

    // ^A should go to the end of the current prompt
    KeyEvent {
      code: KeyCode::Char('e'),
      modifiers: KeyModifiers::CONTROL,
      ..
    } => greeter.cursor_offset = 0,

    // Tab should validate the username entry (same as Enter).
    KeyEvent {
      code: KeyCode::Tab, ..
    } => {
      match greeter.mode {
        Mode::Username if !greeter.username.value.is_empty() => {
          validate_username(&mut greeter, &ipc).await;
        },
        _ => {},
      }
    },

    // Enter validates the current entry, depending on the active mode.
    KeyEvent {
      code: KeyCode::Enter,
      ..
    } => {
      match greeter.mode {
        Mode::Username if !greeter.username.value.is_empty() => {
          validate_username(&mut greeter, &ipc).await;
        },

        Mode::Username
          if greeter.user_menu && !greeter.users.options.is_empty() =>
        {
          greeter.previous_mode = match greeter.mode {
            Mode::Users | Mode::Command | Mode::Sessions | Mode::Power => {
              greeter.previous_mode
            },
            _ => greeter.mode,
          };

          greeter.buffer = greeter.previous_buffer.take().unwrap_or_default();
          greeter.mode = Mode::Users;
        },

        Mode::Username => {},

        Mode::Password => {
          greeter.working = true;
          greeter.message = None;

          ipc
            .send(Request::PostAuthMessageResponse {
              response: Some(greeter.buffer.clone()),
            })
            .await;

          greeter.buffer = String::new();
        },

        Mode::Command => {
          greeter.sessions.selected = 0;
          greeter.session_source =
            SessionSource::Command(greeter.buffer.clone());

          if greeter.remember_session {
            write_last_command(&greeter.buffer);
            delete_last_session();
          }

          greeter.buffer = greeter.previous_buffer.take().unwrap_or_default();
          greeter.mode = greeter.previous_mode;
        },

        Mode::Users => {
          let username =
            greeter.users.options.get(greeter.users.selected).cloned();

          if let Some(User { username, name }) = username {
            greeter.username = MaskedString::from(username, name);
          }

          greeter.mode = greeter.previous_mode;

          validate_username(&mut greeter, &ipc).await;
        },

        Mode::Sessions => {
          let session = greeter
            .sessions
            .options
            .get(greeter.sessions.selected)
            .cloned();

          if let Some(Session { path, .. }) = session {
            if greeter.remember_session
              && let Some(ref path) = path
            {
              write_last_session_path(path);
              delete_last_command();
            }

            greeter.session_source =
              SessionSource::Session(greeter.sessions.selected);
          }

          greeter.mode = greeter.previous_mode;
        },

        Mode::Power => {
          let power_command =
            greeter.powers.options.get(greeter.powers.selected).cloned();

          if let Some(command) = power_command {
            power(&mut greeter, command.action).await;
          }

          greeter.mode = greeter.previous_mode;
        },

        Mode::Background => {
          greeter.apply_background_selection();
          greeter.mode = greeter.previous_mode;
        },

        _ => {},
      }
    },

    // Do not handle any other controls keybindings
    KeyEvent {
      modifiers: KeyModifiers::CONTROL,
      ..
    } => {},

    // Handle free-form entry of characters.
    KeyEvent {
      code: KeyCode::Char(c),
      ..
    } => insert_key(&mut greeter, c).await,

    // Handle deletion of characters.
    KeyEvent {
      code: KeyCode::Backspace,
      ..
    }
    | KeyEvent {
      code: KeyCode::Delete,
      ..
    } => delete_key(&mut greeter, input.code).await,

    _ => {},
  }

  Ok(())
}

// Handle insertion of characters into the proper buffer, depending on the
// current mode and the position of the cursor.
async fn insert_key(greeter: &mut Greeter, c: char) {
  let value = match greeter.mode {
    Mode::Username => &greeter.username.value,
    Mode::Password => &greeter.buffer,
    Mode::Command => &greeter.buffer,
    _ => return,
  };

  let index = (value.chars().count() as i16 + greeter.cursor_offset) as usize;
  let left = value.chars().take(index);
  let right = value.chars().skip(index);

  let value = left.chain(std::iter::once(c)).chain(right).collect();
  let mode = greeter.mode;

  match mode {
    Mode::Username => greeter.username.value = value,
    Mode::Password => greeter.buffer = value,
    Mode::Command => greeter.buffer = value,
    _ => {},
  }
}

// Handle deletion of characters from a prompt into the proper buffer, depending
// on the current mode, whether Backspace or Delete was pressed and the position
// of the cursor.
async fn delete_key(greeter: &mut Greeter, key: KeyCode) {
  let value = match greeter.mode {
    Mode::Username => &greeter.username.value,
    Mode::Password => &greeter.buffer,
    Mode::Command => &greeter.buffer,
    _ => return,
  };

  let index = match key {
    KeyCode::Backspace => {
      (value.chars().count() as i16 + greeter.cursor_offset - 1) as usize
    },
    KeyCode::Delete => {
      (value.chars().count() as i16 + greeter.cursor_offset) as usize
    },
    _ => 0,
  };

  if value.chars().nth(index).is_some() {
    let left = value.chars().take(index);
    let right = value.chars().skip(index + 1);

    let value = left.chain(right).collect();

    match greeter.mode {
      Mode::Username => greeter.username.value = value,
      Mode::Password => greeter.buffer = value,
      Mode::Command => greeter.buffer = value,
      _ => return,
    }

    if key == KeyCode::Delete {
      greeter.cursor_offset += 1;
    }
  }
}

// Creates a `greetd` session for the provided username.
async fn validate_username(greeter: &mut Greeter, ipc: &Ipc) {
  greeter.working = true;
  greeter.message = None;

  ipc
    .send(Request::CreateSession {
      username: greeter.username.value.clone(),
    })
    .await;
  greeter.buffer = String::new();

  if greeter.remember_user_session {
    if let Ok(last_session) = get_last_user_session(&greeter.username.value)
      && let Some(last_session) =
        Session::from_path(greeter, last_session).cloned()
    {
      tracing::info!("remembered user session is {}", last_session.name);

      greeter.sessions.selected = greeter
        .sessions
        .options
        .iter()
        .position(|sess| sess.path == last_session.path)
        .unwrap_or(0);
      greeter.session_source =
        SessionSource::Session(greeter.sessions.selected);
    }

    if let Ok(command) = get_last_user_command(&greeter.username.value) {
      tracing::info!("remembered user command is {}", command);

      greeter.session_source = SessionSource::Command(command);
    }
  }
}

#[cfg(test)]
mod test {
  use std::sync::Arc;

  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
  use tokio::sync::RwLock;
  use tuigreet_types::Mode;

  use super::handle;
  use crate::{
    Greeter,
    ipc::Ipc,
    power::PowerOption,
    ui::{
      common::masked::MaskedString,
      power::Power,
      sessions::{Session, SessionSource},
    },
  };

  #[tokio::test]
  async fn ctrl_u() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Username;
      greeter.username = MaskedString::from("apognu".to_string(), None);
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.username.value, String::new());
    }

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Password;
      greeter.buffer = "password".to_string();
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.buffer, String::new());
    }

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Command;
      greeter.buffer = "newcommand".to_string();
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.buffer, String::new());
    }
  }

  #[tokio::test]
  async fn escape() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.previous_mode = Mode::Username;
      greeter.mode = Mode::Command;
      greeter.previous_buffer = Some("apognu".to_string());
      greeter.buffer = "newcommand".to_string();
      greeter.cursor_offset = 2;
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.mode, Mode::Username);
      assert_eq!(status.buffer, "apognu".to_string());
      assert!(status.previous_buffer.is_none());
      assert_eq!(status.cursor_offset, 0);
    }

    for mode in [Mode::Users, Mode::Sessions, Mode::Power] {
      {
        let mut greeter = greeter.write().await;
        greeter.previous_mode = Mode::Username;
        greeter.mode = mode;
      }

      let result = handle(
        greeter.clone(),
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        Ipc::new(),
      )
      .await;

      {
        let status = greeter.read().await;

        assert!(matches!(result, Ok(())));
        assert_eq!(status.mode, Mode::Username);
      }
    }
  }

  #[tokio::test]
  async fn left_right() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.cursor_offset, -1);
    }

    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.cursor_offset, 1);
    }
  }

  #[tokio::test]
  async fn f2() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Username;
      greeter.buffer = "apognu".to_string();
      greeter.session_source = SessionSource::Command("thecommand".to_string());
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::F(2), KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.mode, Mode::Command);
      assert_eq!(status.previous_buffer, Some("apognu".to_string()));
      assert_eq!(status.buffer, "thecommand".to_string());
    }

    for mode in [Mode::Users, Mode::Sessions, Mode::Power] {
      {
        let mut greeter = greeter.write().await;
        greeter.previous_mode = Mode::Username;
        greeter.mode = mode;
      }

      let result = handle(
        greeter.clone(),
        KeyEvent::new(KeyCode::F(2), KeyModifiers::empty()),
        Ipc::new(),
      )
      .await;

      {
        let status = greeter.read().await;

        assert!(matches!(result, Ok(())));
        assert_eq!(status.mode, Mode::Command);
        assert_eq!(status.previous_mode, Mode::Username);
      }
    }
  }

  #[tokio::test]
  async fn f_menu() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.sessions.options.push(Session::default());
      greeter.powers.options.push(Power {
        action: PowerOption::Shutdown,
        ..Default::default()
      });
    }

    for (key, mode) in [
      (KeyCode::F(3), Mode::Sessions),
      (KeyCode::F(4), Mode::Background),
      (KeyCode::F(12), Mode::Power),
    ] {
      {
        let mut greeter = greeter.write().await;
        greeter.mode = Mode::Username;
        greeter.buffer = "apognu".to_string();
      }

      let result = handle(
        greeter.clone(),
        KeyEvent::new(key, KeyModifiers::empty()),
        Ipc::new(),
      )
      .await;

      {
        let status = greeter.read().await;

        assert!(matches!(result, Ok(())));
        assert_eq!(status.mode, mode);
        assert_eq!(status.buffer, "apognu".to_string());
      }

      for mode in [Mode::Users, Mode::Sessions, Mode::Power] {
        {
          let mut greeter = greeter.write().await;
          greeter.previous_mode = Mode::Username;
          greeter.mode = mode;
        }

        let result = handle(
          greeter.clone(),
          KeyEvent::new(KeyCode::F(2), KeyModifiers::empty()),
          Ipc::new(),
        )
        .await;

        {
          let status = greeter.read().await;

          assert!(matches!(result, Ok(())));
          assert_eq!(status.mode, Mode::Command);
          assert_eq!(status.previous_mode, Mode::Username);
        }
      }
    }
  }

  #[tokio::test]
  async fn f_menu_rebinded() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.sessions.options.push(Session::default());
      greeter.powers.options.push(Power {
        action: PowerOption::Shutdown,
        ..Default::default()
      });
    }

    for (key, mode) in [
      (KeyCode::F(1), Mode::Sessions),
      (KeyCode::F(11), Mode::Power),
    ] {
      {
        let mut greeter = greeter.write().await;
        greeter.kb_command = 3;
        greeter.kb_sessions = 1;
        greeter.kb_power = 11;
        greeter.mode = Mode::Username;
        greeter.buffer = "apognu".to_string();
      }

      let result = handle(
        greeter.clone(),
        KeyEvent::new(key, KeyModifiers::empty()),
        Ipc::new(),
      )
      .await;

      {
        let status = greeter.read().await;

        assert!(matches!(result, Ok(())));
        assert_eq!(status.mode, mode);
        assert_eq!(status.buffer, "apognu".to_string());
      }

      for mode in [Mode::Users, Mode::Sessions, Mode::Power] {
        {
          let mut greeter = greeter.write().await;
          greeter.previous_mode = Mode::Username;
          greeter.mode = mode;
        }

        let result = handle(
          greeter.clone(),
          KeyEvent::new(KeyCode::F(3), KeyModifiers::empty()),
          Ipc::new(),
        )
        .await;

        {
          let status = greeter.read().await;

          assert!(matches!(result, Ok(())));
          assert_eq!(status.mode, Mode::Command);
          assert_eq!(status.previous_mode, Mode::Username);
        }
      }
    }
  }

  #[tokio::test]
  async fn empty_menu_does_not_open_or_panic() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Username;
    }

    // F3 must not flip into an empty sessions menu.
    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::F(3), KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    assert!(matches!(result, Ok(())));
    assert_eq!(greeter.read().await.mode, Mode::Username);

    // Force-enter the empty sessions menu and verify Down is a no-op rather
    // than underflowing on `options.len() - 1`.
    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Sessions;
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    assert!(matches!(result, Ok(())));
    assert_eq!(greeter.read().await.sessions.selected, 0);
  }

  #[tokio::test]
  async fn background_switcher_selects_kind() {
    use crate::ui::{
      background::Background,
      bg_animation::{self as animation, KINDS, Kind},
      common::menu::Menu,
    };

    let greeter = Arc::new(RwLock::new(Greeter::default()));

    // Manually populate the menu (test_greeter doesn't run parse_options)
    {
      let mut g = greeter.write().await;
      g.mode = Mode::Username;
      let mut options = vec![Background {
        kind:  None,
        label: "None".into(),
      }];
      for entry in KINDS {
        options.push(Background {
          kind:  Some(entry.kind),
          label: entry.label.into(),
        });
      }
      g.backgrounds = Menu {
        title: "bg".into(),
        options,
        selected: 0,
      };
      // Start with an animation active so we can verify it gets cleared
      g.animation = Some(animation::build_default(Kind::Doom));
    }

    // Open background menu
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::F(4), KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    {
      let g = greeter.read().await;
      assert_eq!(g.mode, Mode::Background);
      assert_eq!(g.previous_mode, Mode::Username);
      assert_eq!(g.backgrounds.selected, 0);
    }

    // Move selection
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    {
      let g = greeter.read().await;
      assert_eq!(g.backgrounds.selected, 1);
    }

    // Apply the selection
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    {
      let g = greeter.read().await;
      assert_eq!(g.mode, Mode::Username);
      assert!(
        g.animation.is_some(),
        "selecting Doom should set an animation"
      );
    }

    // Select "None" to clear
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::F(4), KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    let _ = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
      Ipc::new(),
    )
    .await;
    {
      let g = greeter.read().await;
      assert_eq!(g.mode, Mode::Username);
      assert!(
        g.animation.is_none(),
        "selecting None should clear the animation"
      );
    }
  }

  #[tokio::test]
  async fn ctrl_a_e() {
    let greeter = Arc::new(RwLock::new(Greeter::default()));

    {
      let mut greeter = greeter.write().await;
      greeter.mode = Mode::Command;
      greeter.buffer = "123456789".to_string();
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.cursor_offset, -9);
    }

    let result = handle(
      greeter.clone(),
      KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
      Ipc::new(),
    )
    .await;

    {
      let status = greeter.read().await;

      assert!(matches!(result, Ok(())));
      assert_eq!(status.cursor_offset, 0);
    }
  }
}

use std::{
  borrow::Cow,
  path::{Path, PathBuf},
};

use super::common::menu::MenuItem;
use crate::Greeter;

// A session uses a command from the configuration or a desktop session file.
#[derive(Default)]
pub enum SessionSource {
  #[default]
  None,
  DefaultCommand(String, Option<Vec<String>>),
  Session(usize),
}

impl SessionSource {
  // Get the configured command or the name from the desktop session file.
  pub fn label<'g, 'ss: 'g>(
    &'ss self,
    greeter: &'g Greeter,
  ) -> Option<&'g str> {
    match self {
      Self::None => None,
      Self::DefaultCommand(command, _) => Some(command),
      Self::Session(index) => {
        greeter
          .sessions
          .options
          .get(*index)
          .map(|session| session.name.as_str())
      },
    }
  }

  // Returns the command that should be spawned when the selected session is
  // started.
  pub fn command<'g, 'ss: 'g>(
    &'ss self,
    greeter: &'g Greeter,
  ) -> Option<&'g str> {
    match self {
      Self::None => None,
      Self::DefaultCommand(command, _) => Some(command.as_str()),
      Self::Session(index) => {
        greeter
          .sessions
          .options
          .get(*index)
          .map(|session| session.command.as_str())
      },
    }
  }

  pub fn env<'g, 'ss: 'g>(&'ss self) -> Option<Vec<String>> {
    match self {
      Self::None | Self::Session(_) => None,
      Self::DefaultCommand(_, env) => env.clone(),
    }
  }
}

// Represents the XDG type of the selected session.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum SessionType {
  X11,
  Wayland,
  Tty,
  #[default]
  None,
}

impl SessionType {
  // Returns the value that should be set in `XDG_SESSION_TYPE` when the session
  // is started.
  pub const fn as_xdg_session_type(self) -> &'static str {
    match self {
      Self::X11 => "x11",
      Self::Wayland => "wayland",
      Self::Tty => "tty",
      Self::None => "unspecified",
    }
  }
}

// A session, as defined by an XDG session file.
#[derive(Clone, Default)]
pub struct Session {
  // Slug of the session, being the name of the desktop file without its
  // extension.
  pub slug: Option<String>,

  // Human-friendly name for the session, maps to the `Name` attribute.
  pub name: String,

  // Command used to start the session, maps to the `Exec` attribute.
  pub command: String,

  // XDG session type for the session, detected from the location of the
  // session file.
  #[expect(
    clippy::struct_field_names,
    reason = "session_type identifies the XDG_SESSION_TYPE domain, not a \
              generic Rust type"
  )]
  pub session_type: SessionType,

  // Path to the session file. Used to uniquely identify sessions, since names
  // and commands can be identital between two different sessions.
  pub path: Option<PathBuf>,

  // Desktop names as defined with the `DesktopNames` desktop file property
  pub xdg_desktop_names: Option<String>,
}

impl MenuItem for Session {
  fn format(&self) -> Cow<'_, str> {
    Cow::Borrowed(&self.name)
  }
}

impl Session {
  // Get a `Session` from the path of a session file.
  //
  // If the path maps to a valid session file, will return the associated
  // session. Otherwise, will return `None`.
  pub fn from_path<P>(greeter: &Greeter, path: P) -> Option<&Self>
  where
    P: AsRef<Path>,
  {
    greeter
      .sessions
      .options
      .iter()
      .find(|session| session.path.as_deref() == Some(path.as_ref()))
  }

  // Retrieves the `Session` that is currently selected.
  //
  // Note that this does not indicate which menu item is "highlighted", but the
  // session that was selected.
  pub fn get_selected(greeter: &Greeter) -> Option<&Self> {
    match greeter.session_source {
      SessionSource::Session(index) => greeter.sessions.options.get(index),
      _ => None,
    }
  }
}

#[cfg(test)]
mod test {
  use crate::{
    Greeter,
    ui::{
      common::menu::Menu,
      sessions::{Session, SessionSource, SessionType},
    },
  };

  #[test]
  fn from_path_existing() -> Result<(), &'static str> {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title:    "Sessions".into(),
      selected: 1,
      options:  vec![
        Session {
          name: "Session1".into(),
          command: "Session1Cmd".into(),
          session_type: super::SessionType::Wayland,
          path: Some("/Session1Path".into()),
          ..Default::default()
        },
        Session {
          name: "Session2".into(),
          command: "Session2Cmd".into(),
          session_type: super::SessionType::X11,
          path: Some("/Session2Path".into()),
          ..Default::default()
        },
      ],
    };

    let session = Session::from_path(&greeter, "/Session2Path")
      .ok_or("the configured session path must be found")?;

    assert_eq!(session.name, "Session2");
    assert_eq!(session.session_type, SessionType::X11);
    Ok(())
  }

  #[test]
  fn from_path_non_existing() {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title:    "Sessions".into(),
      selected: 1,
      options:  vec![Session {
        name: "Session1".into(),
        command: "Session1Cmd".into(),
        session_type: super::SessionType::Wayland,
        path: Some("/Session1Path".into()),
        ..Default::default()
      }],
    };

    let session = Session::from_path(&greeter, "/Session2Path");

    assert!(session.is_none());
  }

  #[test]
  fn no_session() {
    let greeter = Greeter::default();

    assert!(Session::get_selected(&greeter).is_none());
  }

  #[test]
  fn distinct_session() -> Result<(), &'static str> {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title:    "Sessions".into(),
      selected: 1,
      options:  vec![
        Session {
          name: "Session1".into(),
          command: "Session1Cmd".into(),
          session_type: super::SessionType::Wayland,
          path: Some("/Session1Path".into()),
          ..Default::default()
        },
        Session {
          name: "Session2".into(),
          command: "Session2Cmd".into(),
          session_type: super::SessionType::X11,
          path: Some("/Session2Path".into()),
          ..Default::default()
        },
      ],
    };

    let session = Session::get_selected(&greeter)
      .ok_or("the selected session must be found")?;

    assert_eq!(session.name, "Session2");
    assert_eq!(session.session_type, SessionType::X11);
    Ok(())
  }

  #[test]
  fn same_name_session() -> Result<(), &'static str> {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title:    "Sessions".into(),
      selected: 1,
      options:  vec![
        Session {
          name: "Session".into(),
          command: "Session1Cmd".into(),
          session_type: super::SessionType::Wayland,
          path: Some("/Session1Path".into()),
          ..Default::default()
        },
        Session {
          name: "Session".into(),
          command: "Session2Cmd".into(),
          session_type: super::SessionType::X11,
          path: Some("/Session2Path".into()),
          ..Default::default()
        },
      ],
    };

    let session = Session::get_selected(&greeter)
      .ok_or("the selected session must be found")?;

    assert_eq!(session.name, "Session");
    assert_eq!(session.session_type, SessionType::X11);
    assert_eq!(session.command, "Session2Cmd");
    Ok(())
  }
}

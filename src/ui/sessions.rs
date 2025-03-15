use std::{
  borrow::Cow,
  path::{Path, PathBuf},
};

use crate::Greeter;

use super::common::menu::MenuItem;

use std::error::Error;
use ini::Ini;

// SessionSource models the selected session and where it comes from.
//
// A session can either come from a free-form command or an XDG-defined session
// file. Each variant contains a reference to the data required to create a
// session, either the String of the command or the index of the session in the
// session list.
#[derive(SmartDefault)]
pub enum SessionSource {
  #[default]
  None,
  DefaultCommand(String, Option<Vec<String>>),
  Command(String),
  Session(usize),
}

impl SessionSource {
  // Returns a human-readable label for the selected session.
  //
  // For free-form commands, this is the command itself. For session files, it
  // is the value of the `Name` attribute in that file.
  pub fn label<'g, 'ss: 'g>(&'ss self, greeter: &'g Greeter) -> Option<&'g str> {
    match self {
      SessionSource::None => None,
      SessionSource::DefaultCommand(command, _) => Some(command),
      SessionSource::Command(command) => Some(command),
      SessionSource::Session(index) => greeter.sessions.options.get(*index).map(|session| session.name.as_str()),
    }
  }

  // Returns the command that should be spawned when the selected session is
  // started.
  pub fn command<'g, 'ss: 'g>(&'ss self, greeter: &'g Greeter) -> Option<&'g str> {
    match self {
      SessionSource::None => None,
      SessionSource::DefaultCommand(command, _) => Some(command.as_str()),
      SessionSource::Command(command) => Some(command.as_str()),
      SessionSource::Session(index) => greeter.sessions.options.get(*index).map(|session| session.command.as_str()),
    }
  }

  pub fn env<'g, 'ss: 'g>(&'ss self) -> Option<Vec<String>> {
    match self {
      SessionSource::None => None,
      SessionSource::DefaultCommand(_, env) => env.clone(),
      SessionSource::Command(_) => None,
      SessionSource::Session(_) => None,
    }
  }
}

// Represents the XDG type of the selected session.
#[derive(SmartDefault, Debug, Copy, Clone, PartialEq)]
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
  pub fn as_xdg_session_type(&self) -> &'static str {
    match self {
      SessionType::X11 => "x11",
      SessionType::Wayland => "wayland",
      SessionType::Tty => "tty",
      SessionType::None => "unspecified",
    }
  }
}

// A session, as defined by an XDG session file.
#[derive(SmartDefault, Clone)]
pub struct Session {
  // Slug of the session, being the name of the desktop file without its
  // extension.
  pub slug: Option<String>,
  // Human-friendly name for the session, maps to the `Name` attribute.
  pub name: String,
  // Command used to start the session, maps to the `Exec` attribute.
  pub command: String,
  // XDG session type for the session, detected from the location of the session
  // file.
  pub session_type: SessionType,
  // Path to the session file. Used to uniquely identify sessions, since names
  // and commands can be identital between two different sessions.
  pub path: Option<PathBuf>,
  // Desktop names as defined with the `DesktopNames` desktop file property
  pub xdg_desktop_names: Option<String>,
}

// Represents a session configuration entry from the TOML config file
// Used to customize which sessions appear in the F3 menu and their order
#[derive(Default, Debug, Clone)]
pub struct SessionConfig {
  pub name: String,
  pub path: PathBuf,
  pub enabled: bool,
  pub order: i32,
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
  pub fn from_path<P>(greeter: &Greeter, path: P) -> Option<&Session>
  where
    P: AsRef<Path>,
  {
    greeter.sessions.options.iter().find(|session| session.path.as_deref() == Some(path.as_ref()))
  }

  // Retrieves the `Session` that is currently selected.
  //
  // Note that this does not indicate which menu item is "highlighted", but the
  // session that was selected.
  pub fn get_selected(greeter: &Greeter) -> Option<&Session> {
    match greeter.session_source {
      SessionSource::Session(index) => greeter.sessions.options.get(index),
      _ => None,
    }
  }

  // Creates a new Session object from a desktop file path with a custom name.
  //
  // This allows loading sessions from config files where the name might be
  // customized by the system administrator.
  //
  // Arguments:
  // * `path` - Path to the desktop entry file
  // * `name` - Custom name to use for the session (overrides the Name in the desktop file)
  //
  // Returns:
  // * `Ok(Some(Session))` if the file exists and is a valid desktop entry
  // * `Ok(None)` if the file doesn't exist
  // * `Err(...)` if there's an error parsing the desktop file
  pub fn from_path_with_name<P>(path: P, name: String) -> Result<Option<Session>, Box<dyn Error>> 
  where
    P: AsRef<Path>,
  {
    let path_ref = path.as_ref();
    if !path_ref.exists() {
        return Ok(None);
    }

    let desktop = Ini::load_from_file(path_ref)?;
    let section = desktop.section(Some("Desktop Entry")).ok_or("no Desktop Entry section in desktop file")?;
    
    let slug = path_ref.file_stem().map(|slug| slug.to_string_lossy().to_string());
    let exec = section.get("Exec").ok_or("no Exec property in desktop file")?;
    let xdg_desktop_names = section.get("DesktopNames").map(str::to_string);
    
    // Determine session type from file path
    let session_type = if path_ref.to_string_lossy().contains("wayland-sessions") {
        SessionType::Wayland
    } else if path_ref.to_string_lossy().contains("xsessions") {
        SessionType::X11
    } else {
        SessionType::None
    };
    
    Ok(Some(Session {
        slug,
        name,
        command: exec.to_string(),
        session_type,
        path: Some(path_ref.into()),
        xdg_desktop_names,
    }))
  }
}

#[cfg(test)]
mod test {
  use crate::{
    ui::{
      common::menu::Menu,
      sessions::{Session, SessionSource, SessionType},
    },
    Greeter,
  };

  #[test]
  fn from_path_existing() {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title: "Sessions".into(),
      selected: 1,
      options: vec![
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

    let session = Session::from_path(&greeter, "/Session2Path");

    assert!(matches!(session, Some(_)));
    assert_eq!(session.unwrap().name, "Session2");
    assert_eq!(session.unwrap().session_type, SessionType::X11);
  }

  #[test]
  fn from_path_non_existing() {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title: "Sessions".into(),
      selected: 1,
      options: vec![Session {
        name: "Session1".into(),
        command: "Session1Cmd".into(),
        session_type: super::SessionType::Wayland,
        path: Some("/Session1Path".into()),
        ..Default::default()
      }],
    };

    let session = Session::from_path(&greeter, "/Session2Path");

    assert!(matches!(session, None));
  }

  #[test]
  fn no_session() {
    let greeter = Greeter::default();

    assert!(matches!(Session::get_selected(&greeter), None));
  }

  #[test]
  fn distinct_session() {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title: "Sessions".into(),
      selected: 1,
      options: vec![
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

    let session = Session::get_selected(&greeter);

    assert!(matches!(session, Some(_)));
    assert_eq!(session.unwrap().name, "Session2");
    assert_eq!(session.unwrap().session_type, SessionType::X11);
  }

  #[test]
  fn same_name_session() {
    let mut greeter = Greeter::default();
    greeter.session_source = SessionSource::Session(1);

    greeter.sessions = Menu::<Session> {
      title: "Sessions".into(),
      selected: 1,
      options: vec![
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

    let session = Session::get_selected(&greeter);

    assert!(matches!(session, Some(_)));
    assert_eq!(session.unwrap().name, "Session");
    assert_eq!(session.unwrap().session_type, SessionType::X11);
    assert_eq!(session.unwrap().command, "Session2Cmd");
  }
}

use std::{
  env,
  error::Error,
  fs::{self, File},
  io::{self, BufRead, BufReader},
  path::{Path, PathBuf},
  process::Command,
};

use chrono::Local;
use ini::Ini;
use lazy_static::lazy_static;
use nix::sys::utsname;
use utmp_rs::{UtmpEntry, UtmpParser};
use uzers::os::unix::UserExt;

use crate::{
  ui::{
    common::masked::MaskedString,
    sessions::{Session, SessionType},
    users::User,
  },
  Greeter,
};

use crate::ui::sessions::SessionConfig;

const LAST_USER_USERNAME: &str = "/var/cache/tuigreet/lastuser";
const LAST_USER_NAME: &str = "/var/cache/tuigreet/lastuser-name";
const LAST_COMMAND: &str = "/var/cache/tuigreet/lastsession";
const LAST_SESSION: &str = "/var/cache/tuigreet/lastsession-path";

const DEFAULT_MIN_UID: u16 = 1000;
const DEFAULT_MAX_UID: u16 = 60000;

lazy_static! {
  static ref XDG_DATA_DIRS: Vec<PathBuf> = {
    let value = env::var("XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share".to_string());
    env::split_paths(&value).filter(|p| p.is_absolute()).collect()
  };
  static ref DEFAULT_SESSION_PATHS: Vec<(PathBuf, SessionType)> = XDG_DATA_DIRS
    .iter()
    .map(|p| (p.join("wayland-sessions"), SessionType::Wayland))
    .chain(XDG_DATA_DIRS.iter().map(|p| (p.join("xsessions"), SessionType::X11)))
    .collect();
}

pub fn get_hostname() -> String {
  match utsname::uname() {
    Ok(uts) => uts.nodename().to_str().unwrap_or("").to_string(),
    _ => String::new(),
  }
}

pub fn get_issue() -> Option<String> {
  let (date, time) = {
    let now = Local::now();

    (now.format("%a %b %_d %Y").to_string(), now.format("%H:%M:%S").to_string())
  };

  let user_count = match UtmpParser::from_path("/var/run/utmp")
    .map(|utmp| {
      utmp.into_iter().fold(0, |acc, entry| match entry {
        Ok(UtmpEntry::UserProcess { .. }) => acc + 1,
        Ok(UtmpEntry::LoginProcess { .. }) => acc + 1,
        _ => acc,
      })
    })
    .unwrap_or(0)
  {
    n if n < 2 => format!("{n} user"),
    n => format!("{n} users"),
  };

  let vtnr: usize = env::var("XDG_VTNR").unwrap_or_else(|_| "0".to_string()).parse().unwrap_or(0);
  let uts = utsname::uname();

  if let Ok(issue) = fs::read_to_string("/etc/issue") {
    let issue = issue
      .replace("\\S", "Linux")
      .replace("\\l", &format!("tty{vtnr}"))
      .replace("\\d", &date)
      .replace("\\t", &time)
      .replace("\\U", &user_count);

    let issue = match uts {
      Ok(uts) => issue
        .replace("\\s", uts.sysname().to_str().unwrap_or(""))
        .replace("\\r", uts.release().to_str().unwrap_or(""))
        .replace("\\v", uts.version().to_str().unwrap_or(""))
        .replace("\\n", uts.nodename().to_str().unwrap_or(""))
        .replace("\\m", uts.machine().to_str().unwrap_or(""))
        .replace("\\o", uts.domainname().to_str().unwrap_or("")),

      _ => issue,
    };

    return Some(issue.replace("\\x1b", "\x1b").replace("\\033", "\x1b").replace("\\e", "\x1b").replace(r"\\", r"\"));
  }

  None
}

pub fn get_last_user_username() -> Option<String> {
  match fs::read_to_string(LAST_USER_USERNAME).ok() {
    None => None,
    Some(username) => {
      let username = username.trim();

      if username.is_empty() {
        None
      } else {
        Some(username.to_string())
      }
    }
  }
}

pub fn get_last_user_name() -> Option<String> {
  match fs::read_to_string(LAST_USER_NAME).ok() {
    None => None,
    Some(name) => {
      let name = name.trim();

      if name.is_empty() {
        None
      } else {
        Some(name.to_string())
      }
    }
  }
}

pub fn write_last_username(username: &MaskedString) {
  let _ = fs::write(LAST_USER_USERNAME, &username.value);

  if let Some(ref name) = username.mask {
    let _ = fs::write(LAST_USER_NAME, name);
  } else {
    let _ = fs::remove_file(LAST_USER_NAME);
  }
}

pub fn get_last_session_path() -> Result<PathBuf, io::Error> {
  Ok(PathBuf::from(fs::read_to_string(LAST_SESSION)?.trim()))
}

pub fn get_last_command() -> Result<String, io::Error> {
  Ok(fs::read_to_string(LAST_COMMAND)?.trim().to_string())
}

pub fn write_last_session_path<P>(session: &P)
where
  P: AsRef<Path>,
{
  let _ = fs::write(LAST_SESSION, session.as_ref().to_string_lossy().as_bytes());
}

pub fn write_last_command(session: &str) {
  let _ = fs::write(LAST_COMMAND, session);
}

pub fn get_last_user_session(username: &str) -> Result<PathBuf, io::Error> {
  Ok(PathBuf::from(fs::read_to_string(format!("{LAST_SESSION}-{username}"))?.trim()))
}

pub fn get_last_user_command(username: &str) -> Result<String, io::Error> {
  Ok(fs::read_to_string(format!("{LAST_COMMAND}-{username}"))?.trim().to_string())
}

pub fn write_last_user_session<P>(username: &str, session: P)
where
  P: AsRef<Path>,
{
  let _ = fs::write(format!("{LAST_SESSION}-{username}"), session.as_ref().to_string_lossy().as_bytes());
}

pub fn delete_last_session() {
  let _ = fs::remove_file(LAST_SESSION);
}

pub fn write_last_user_command(username: &str, session: &str) {
  let _ = fs::write(format!("{LAST_COMMAND}-{username}"), session);
}

pub fn delete_last_user_session(username: &str) {
  let _ = fs::remove_file(format!("{LAST_SESSION}-{username}"));
}

pub fn delete_last_command() {
  let _ = fs::remove_file(LAST_COMMAND);
}

pub fn delete_last_user_command(username: &str) {
  let _ = fs::remove_file(format!("{LAST_COMMAND}-{username}"));
}

pub fn get_users(min_uid: u16, max_uid: u16) -> Vec<User> {
  let users = unsafe { uzers::all_users() };

  let users: Vec<User> = users
    .filter(|user| user.uid() >= min_uid as u32 && user.uid() <= max_uid as u32)
    .map(|user| User {
      username: user.name().to_string_lossy().to_string(),
      name: match user.gecos() {
        name if name.is_empty() => None,
        name => {
          let name = name.to_string_lossy();

          match name.split_once(',') {
            Some((name, _)) => Some(name.to_string()),
            None => Some(name.to_string()),
          }
        }
      },
    })
    .collect();

  users
}

pub fn get_min_max_uids(min_uid: Option<u16>, max_uid: Option<u16>) -> (u16, u16) {
  if let (Some(min_uid), Some(max_uid)) = (min_uid, max_uid) {
    return (min_uid, max_uid);
  }

  let overrides = (min_uid, max_uid);
  let default = (min_uid.unwrap_or(DEFAULT_MIN_UID), max_uid.unwrap_or(DEFAULT_MAX_UID));

  match File::open("/etc/login.defs") {
    Err(_) => default,
    Ok(file) => {
      let file = BufReader::new(file);

      let uids: (u16, u16) = file.lines().fold(default, |acc, line| {
        line
          .map(|line| {
            let mut tokens = line.split_whitespace();

            match (overrides, tokens.next(), tokens.next()) {
              ((None, _), Some("UID_MIN"), Some(value)) => (value.parse::<u16>().unwrap_or(acc.0), acc.1),
              ((_, None), Some("UID_MAX"), Some(value)) => (acc.0, value.parse::<u16>().unwrap_or(acc.1)),
              _ => acc,
            }
          })
          .unwrap_or(acc)
      });

      uids
    }
  }
}

// Loads and parses a session configuration file
// The configuration defines which sessions to display and in what order
// Returns a list of Session objects based on the configuration
fn load_sessions_from_config<P: AsRef<Path>>(path: P) -> Result<Vec<Session>, Box<dyn Error>> {
  use std::fs;
  use toml::Value;
  
  // Read and parse the TOML file
  let content = fs::read_to_string(path)?;
  let config: Value = toml::from_str(&content)?;
  
  let mut session_configs = Vec::new();
  
  // Extract sessions from the TOML structure
  if let Some(sessions) = config.get("sessions").and_then(|s| s.get("session")).and_then(|s| s.as_array()) {
    for session_value in sessions {
      let name = session_value.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
      let path_str = session_value.get("path").and_then(|v| v.as_str()).unwrap_or_default();
      let enabled = session_value.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
      let order = session_value.get("order").and_then(|v| v.as_integer()).unwrap_or(0);
      
      // Only include enabled sessions
      if enabled {
        session_configs.push(SessionConfig {
          name,
          path: PathBuf::from(path_str),
          enabled,
          order: order as i32,
        });
      }
    }
  }
  
  // Sort sessions by their specified order
  session_configs.sort_by_key(|s| s.order);
  
  // Convert SessionConfig objects to Session objects
  let mut sessions = Vec::new();
  for config in session_configs {
    if let Ok(Some(session)) = Session::from_path_with_name(&config.path, config.name) {
      sessions.push(session);
    }
  }
  
  Ok(sessions)
}

pub fn get_sessions(greeter: &Greeter) -> Result<Vec<Session>, Box<dyn Error>> {
  // If a session configuration file is specified, use it instead of auto-discovery
  if let Some(config_path) = greeter.option("session-config") {
    return load_sessions_from_config(config_path);
  }

  // Use the discover_sessions function for the actual session discovery
  discover_sessions(greeter)
}

fn load_desktop_file<P>(path: P, session_type: SessionType) -> Result<Option<Session>, Box<dyn Error>>
where
  P: AsRef<Path>,
{
  let desktop = Ini::load_from_file(path.as_ref())?;
  let section = desktop.section(Some("Desktop Entry")).ok_or("no Desktop Entry section in desktop file")?;

  if let Some("true") = section.get("Hidden") {
    tracing::info!("ignoring session in '{}': Hidden=true", path.as_ref().display());
    return Ok(None);
  }
  if let Some("true") = section.get("NoDisplay") {
    tracing::info!("ignoring session in '{}': NoDisplay=true", path.as_ref().display());
    return Ok(None);
  }

  let slug = path.as_ref().file_stem().map(|slug| slug.to_string_lossy().to_string());
  let name = section.get("Name").ok_or("no Name property in desktop file")?;
  let exec = section.get("Exec").ok_or("no Exec property in desktop file")?;
  let xdg_desktop_names = section.get("DesktopNames").map(str::to_string);

  tracing::info!("got session '{}' in '{}'", name, path.as_ref().display());

  Ok(Some(Session {
    slug,
    name: name.to_string(),
    command: exec.to_string(),
    session_type,
    path: Some(path.as_ref().into()),
    xdg_desktop_names,
  }))
}

pub fn capslock_status() -> bool {
  let mut command = Command::new("kbdinfo");
  command.args(["gkbled", "capslock"]);

  match command.output() {
    Ok(output) => output.status.code() == Some(0),
    Err(_) => false,
  }
}

#[cfg(feature = "nsswrapper")]
#[cfg(test)]
mod nsswrapper_tests {
  #[test]
  fn nsswrapper_get_users_from_nss() {
    use super::get_users;

    let users = get_users(1000, 2000);

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].username, "joe");
    assert_eq!(users[0].name, Some("Joe".to_string()));
    assert_eq!(users[1].username, "bob");
    assert_eq!(users[1].name, None);
  }
}

// Function to generate a TOML configuration template with all available sessions
// This allows administrators to customize which sessions are displayed and in what order
pub fn show_all_sessions() -> Result<(), Box<dyn Error>> {
  // Create a minimal greeter without connecting to greetd
  let mut greeter = Greeter::default();
  
  // Initialize any needed fields
  if greeter.config.is_none() {
    greeter.config = Greeter::options().parse(&[""]).ok();
  }
  
  // Get sessions directly from the filesystem
  let sessions = discover_sessions(&greeter)?;
  
  println!("[sessions]");
  println!("# Configuration for session display");
  println!("# enabled = true|false - whether to display the session");
  println!("# order = <number> - display order (lower numbers shown first)");
  println!("");
  
  for (idx, session) in sessions.iter().enumerate() {
    let name = session.name.replace("\"", "\\\"");
    println!("[[sessions.session]]");
    println!("name = \"{}\"", name);
    if let Some(path) = &session.path {
      println!("path = \"{}\"", path.display());
    }
    println!("enabled = true");
    println!("order = {}", idx);
    println!("");
  }
  
  Ok(())
}

// Discovers available sessions from the filesystem
pub fn discover_sessions(greeter: &Greeter) -> Result<Vec<Session>, Box<dyn Error>> {
  let paths = if greeter.session_paths.is_empty() {
    DEFAULT_SESSION_PATHS.as_ref()
  } else {
    &greeter.session_paths
  };

  let mut files = vec![];

  for (path, session_type) in paths.iter() {
    if let Ok(entries) = fs::read_dir(path) {
      files.extend(entries.flat_map(|entry| entry.map(|entry| load_desktop_file(entry.path(), *session_type))).flatten().flatten());
    }
  }

  files.sort_by(|a, b| a.name.cmp(&b.name));

  Ok(files)
}

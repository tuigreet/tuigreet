use std::{
  env,
  error::Error,
  fs,
  io,
  path::{Path, PathBuf},
  process::Command,
  sync::{Mutex, OnceLock, PoisonError},
};

use chrono::Local;
use ini::Ini;
use nix::sys::utsname::uname;
use utmp_rs::{UtmpEntry, UtmpParser};
use uzers::os::unix::UserExt;

use crate::{
  Greeter,
  ui::{
    common::masked::MaskedString,
    sessions::{Session, SessionType},
    users::User,
  },
};

/// Cache file paths
const LAST_USER_USERNAME: &str = "/var/cache/tuigreet/lastuser";
const LAST_USER_NAME: &str = "/var/cache/tuigreet/lastuser-name";
const LAST_COMMAND: &str = "/var/cache/tuigreet/lastsession";
const LAST_SESSION: &str = "/var/cache/tuigreet/lastsession-path";

static XDG_DATA_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
static DEFAULT_SESSION_PATHS: OnceLock<Vec<(PathBuf, SessionType)>> =
  OnceLock::new();
static USER_ENUMERATION: Mutex<()> = Mutex::new(());

fn xdg_data_dirs() -> &'static Vec<PathBuf> {
  XDG_DATA_DIRS.get_or_init(|| {
    let value = env::var("XDG_DATA_DIRS")
      .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    env::split_paths(&value)
      .filter(|p| p.is_absolute())
      .collect()
  })
}

fn default_session_paths() -> &'static Vec<(PathBuf, SessionType)> {
  DEFAULT_SESSION_PATHS.get_or_init(|| {
    xdg_data_dirs()
      .iter()
      .map(|p| (p.join("wayland-sessions"), SessionType::Wayland))
      .chain(
        xdg_data_dirs()
          .iter()
          .map(|p| (p.join("xsessions"), SessionType::X11)),
      )
      .collect()
  })
}

pub fn get_hostname() -> String {
  uname().map_or_else(
    |_| String::new(),
    |uts| uts.nodename().to_str().unwrap_or("").to_string(),
  )
}

pub fn get_issue() -> Option<String> {
  let (date, time) = {
    let now = Local::now();

    (
      now.format("%a %b %_d %Y").to_string(),
      now.format("%H:%M:%S").to_string(),
    )
  };

  let user_count =
    match UtmpParser::from_path("/var/run/utmp").map_or(0, |utmp| {
      utmp.into_iter().fold(0, |acc, entry| {
        match entry {
          Ok(
            UtmpEntry::UserProcess { .. } | UtmpEntry::LoginProcess { .. },
          ) => acc + 1,
          _ => acc,
        }
      })
    }) {
      n if n < 2 => format!("{n} user"),
      n => format!("{n} users"),
    };

  let uts = uname();
  let vtnr: usize = env::var("XDG_VTNR")
    .unwrap_or_else(|_| "0".to_string())
    .parse()
    .unwrap_or(0);

  if let Ok(issue) = fs::read_to_string("/etc/issue") {
    let issue = issue
      .replace("\\S", "Linux")
      .replace("\\l", &format!("tty{vtnr}"))
      .replace("\\d", &date)
      .replace("\\t", &time)
      .replace("\\U", &user_count);

    let issue = match uts {
      Ok(uts) => {
        issue
          .replace("\\s", uts.sysname().to_str().unwrap_or(""))
          .replace("\\r", uts.release().to_str().unwrap_or(""))
          .replace("\\v", uts.version().to_str().unwrap_or(""))
          .replace("\\n", uts.nodename().to_str().unwrap_or(""))
          .replace("\\m", uts.machine().to_str().unwrap_or(""))
          .replace("\\o", uts.domainname().to_str().unwrap_or(""))
      },

      _ => issue,
    };

    return Some(
      issue
        .replace("\\x1b", "\x1b")
        .replace("\\033", "\x1b")
        .replace("\\e", "\x1b")
        .replace(r"\\", r"\"),
    );
  }

  None
}

fn read_trimmed_file(path: &str) -> Option<String> {
  let contents = fs::read_to_string(path).ok()?;
  let trimmed = contents.trim();
  if trimmed.is_empty() {
    None
  } else {
    Some(trimmed.to_string())
  }
}

pub fn get_last_user_username() -> Option<String> {
  read_trimmed_file(LAST_USER_USERNAME)
}

pub fn get_last_user_name() -> Option<String> {
  read_trimmed_file(LAST_USER_NAME)
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
  let _ =
    fs::write(LAST_SESSION, session.as_ref().to_string_lossy().as_bytes());
}

pub fn write_last_command(session: &str) {
  let _ = fs::write(LAST_COMMAND, session);
}

pub fn get_last_user_session(username: &str) -> Result<PathBuf, io::Error> {
  Ok(PathBuf::from(
    fs::read_to_string(format!("{LAST_SESSION}-{username}"))?.trim(),
  ))
}

pub fn get_last_user_command(username: &str) -> Result<String, io::Error> {
  Ok(
    fs::read_to_string(format!("{LAST_COMMAND}-{username}"))?
      .trim()
      .to_string(),
  )
}

pub fn write_last_user_session<P>(username: &str, session: P)
where
  P: AsRef<Path>,
{
  let _ = fs::write(
    format!("{LAST_SESSION}-{username}"),
    session.as_ref().to_string_lossy().as_bytes(),
  );
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

#[expect(
  unsafe_code,
  reason = "enumerating libc's passwd database requires unsafe FFI"
)]
pub fn get_users(min_uid: u32, max_uid: u32) -> Vec<User> {
  let enumeration = USER_ENUMERATION
    .lock()
    .unwrap_or_else(PoisonError::into_inner);
  // SAFETY: this is the only passwd enumeration in the program. The mutex
  // serializes all calls and remains held until the iterator is exhausted
  // and dropped; other account lookups use uzers' reentrant APIs.
  let users = unsafe { uzers::all_users() };

  let result = users
    .filter(|user| user.uid() >= min_uid && user.uid() <= max_uid)
    .map(|user| {
      User {
        username: user.name().to_string_lossy().to_string(),
        name:     match user.gecos() {
          name if name.is_empty() => None,
          name => {
            let name = name.to_string_lossy();

            match name.split_once(',') {
              Some((name, _)) => Some(name.to_string()),
              None => Some(name.to_string()),
            }
          },
        },
      }
    })
    .collect();
  drop(enumeration);
  result
}

pub fn get_sessions(greeter: &Greeter) -> Vec<Session> {
  let paths = if greeter.session_paths.is_empty() {
    default_session_paths()
  } else {
    &greeter.session_paths
  };

  let mut files = vec![];

  for (path, session_type) in paths {
    tracing::info!(
      "reading {:?} sessions from '{}'",
      session_type,
      path.display()
    );

    if let Ok(entries) = fs::read_dir(path) {
      files.extend(
        entries
          .flat_map(|entry| {
            entry.map(|entry| load_desktop_file(entry.path(), *session_type))
          })
          .flatten()
          .flatten(),
      );
    }
  }

  files.sort_by(|a, b| a.name.cmp(&b.name));

  tracing::info!("found {} sessions", files.len());

  files
}

fn load_desktop_file<P>(
  path: P,
  session_type: SessionType,
) -> Result<Option<Session>, Box<dyn Error>>
where
  P: AsRef<Path>,
{
  let desktop = Ini::load_from_file(path.as_ref())?;
  let section = desktop
    .section(Some("Desktop Entry"))
    .ok_or("no Desktop Entry section in desktop file")?;

  if section.get("Hidden") == Some("true") {
    tracing::info!(
      "ignoring session in '{}': Hidden=true",
      path.as_ref().display()
    );
    return Ok(None);
  }
  if section.get("NoDisplay") == Some("true") {
    tracing::info!(
      "ignoring session in '{}': NoDisplay=true",
      path.as_ref().display()
    );
    return Ok(None);
  }

  let slug = path
    .as_ref()
    .file_stem()
    .map(|slug| slug.to_string_lossy().to_string());
  let name = section
    .get("Name")
    .ok_or("no Name property in desktop file")?;
  let exec = section
    .get("Exec")
    .ok_or("no Exec property in desktop file")?;
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

pub struct BatteryInfo {
  pub percentage: u8,
  pub charging:   bool,
}

pub fn get_battery_info() -> Option<BatteryInfo> {
  let power_dir = Path::new("/sys/class/power_supply");
  let entries = fs::read_dir(power_dir).ok()?;

  for entry in entries.flatten() {
    let name = entry.file_name();
    if !name.to_string_lossy().starts_with("BAT") {
      continue;
    }
    let capacity_path = entry.path().join("capacity");
    let status_path = entry.path().join("status");
    if let Ok(content) = fs::read_to_string(&capacity_path)
      && let Ok(cap) = content.trim().parse::<u8>()
    {
      let charging =
        fs::read_to_string(&status_path).is_ok_and(|s| s.trim() == "Charging");
      return Some(BatteryInfo {
        percentage: cap,
        charging,
      });
    }
  }

  None
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

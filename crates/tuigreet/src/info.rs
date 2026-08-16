use std::{
  env,
  error::Error,
  fs::{self, File},
  io::{self, BufRead, BufReader},
  path::{Path, PathBuf},
  process::Command,
  sync::OnceLock,
};

use chrono::Local;
use ini::Ini;
use nix::{
  ifaddrs::getifaddrs,
  net::if_::InterfaceFlags,
  sys::{
    socket::{AddressFamily, SockaddrLike},
    termios, utsname,
  },
};
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

/// Default UID range for user menu
const DEFAULT_MIN_UID: u32 = 1000;
const DEFAULT_MAX_UID: u32 = 60000;

static XDG_DATA_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
static DEFAULT_SESSION_PATHS: OnceLock<Vec<(PathBuf, SessionType)>> =
  OnceLock::new();

fn xdg_data_dirs() -> &'static Vec<PathBuf> {
  XDG_DATA_DIRS.get_or_init(|| {
    let value = env::var("XDG_DATA_DIRS")
      .unwrap_or("/usr/local/share:/usr/share".to_string());
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
  match utsname::uname() {
    Ok(uts) => uts.nodename().to_str().unwrap_or("").to_string(),
    _ => String::new(),
  }
}

pub fn get_issue() -> Option<String> {
  let (date, time) = {
    let now = Local::now();

    (
      now.format("%a %b %_d %Y").to_string(),
      now.format("%H:%M:%S").to_string(),
    )
  };

  let user_count = UtmpParser::from_path("/var/run/utmp").map_or(0, |utmp| {
    utmp.into_iter().fold(0, |acc, entry| match entry {
      Ok(UtmpEntry::UserProcess { .. }) => acc + 1,
      Ok(UtmpEntry::LoginProcess { .. }) => acc + 1,
      _ => acc,
    })
  });

  let user_string = match user_count {
    n if n == 1 => format!("{n} user"),
    n => format!("{n} users"),
  };

  let uts = utsname::uname();
  let vtnr: usize = env::var("XDG_VTNR")
    .unwrap_or_else(|_| "0".to_string())
    .parse()
    .unwrap_or(0);

  if let Ok(issue) = fs::read_to_string("/etc/issue") {
    let mut pretty_issue: String = "".to_owned();

    let mut iter = issue.chars().peekable();
    while let Some(c) = iter.next() {
      if c == '\\' {
        let special_char = match iter.next() {
          Some(special_char) => special_char,
          None => break,
        };

        if special_char == '\\' {
          pretty_issue.push('\\');
        } else if special_char == '4' || special_char == '6' {
          let mut interface;

          if iter.peek() == Some(&'{') {
            iter.next(); // 4 -> {

            interface = iter.next().unwrap_or('}').to_string();
            while iter.peek() != Some(&'}') {
              interface.push(iter.next().unwrap_or('}'));
            }
            iter.next(); // } ->
          } else {
            interface = "".to_owned();
          }

          for ifaddr in getifaddrs().unwrap() {
            if let Some(address) = ifaddr.address {
              if address.family().unwrap_or(AddressFamily::Unspec)
                == (if special_char == '6' {
                  AddressFamily::Inet6
                } else {
                  AddressFamily::Inet
                })
                && ((interface.is_empty()
                  && !ifaddr.flags.contains(InterfaceFlags::IFF_LOOPBACK)
                  && ifaddr.flags.contains(InterfaceFlags::IFF_RUNNING)
                  && ifaddr.flags.contains(InterfaceFlags::IFF_UP))
                  || ifaddr.interface_name == *interface)
              {
                if special_char == '6' {
                  if let Some(ipv6) = address.as_sockaddr_in6() {
                    pretty_issue.push_str(&ipv6.ip().to_string());
                  }
                } else {
                  if let Some(ipv4) = address.as_sockaddr_in() {
                    pretty_issue.push_str(&ipv4.ip().to_string());
                  }
                }
                break;
              }
            }
          }
        } else if special_char == 'b' {
          if let Ok(dev_tty) =
            File::options().read(true).write(true).open("/dev/tty")
          {
            if let Ok(term) = termios::tcgetattr(dev_tty) {
              let baud = termios::cfgetispeed(&term);
              let mut baud_name = format!("{baud:?}");
              baud_name.remove(0);
              pretty_issue.push_str(&baud_name);
            }
          }
        } else if special_char == 'd' {
          pretty_issue.push_str(&date);
        } else if special_char == 'e' {
          pretty_issue.push('\x1b');
          if iter.peek() == Some(&'{') {
            iter.next(); // e -> {

            let mut name = iter.next().unwrap_or('}').to_string();
            while iter.peek() != Some(&'}') {
              name.push(iter.next().unwrap_or('}'));
            }
            iter.next(); // } ->

            pretty_issue.push('[');

            if name == "black" {
              pretty_issue.push_str("30");
            } else if name == "blink" {
              pretty_issue.push('5');
            } else if name == "blue" {
              pretty_issue.push_str("34");
            } else if name == "bold" {
              pretty_issue.push('1');
            } else if name == "brown" {
              pretty_issue.push_str("33");
            } else if name == "cyan" {
              pretty_issue.push_str("36");
            } else if name == "darkgray" {
              pretty_issue.push_str("1;30");
            } else if name == "gray" {
              pretty_issue.push_str("37");
            } else if name == "green" {
              pretty_issue.push_str("32");
            } else if name == "halfbright" {
              pretty_issue.push('2');
            } else if name == "lightblue" {
              pretty_issue.push_str("1;34");
            } else if name == "lightcyan" {
              pretty_issue.push_str("1;36");
            } else if name == "lightgray" {
              pretty_issue.push_str("37");
            } else if name == "lightgreen" {
              pretty_issue.push_str("1;32");
            } else if name == "lightmagenta" {
              pretty_issue.push_str("1;35");
            } else if name == "lightred" {
              pretty_issue.push_str("1;31");
            } else if name == "magenta" {
              pretty_issue.push_str("35");
            } else if name == "red" {
              pretty_issue.push_str("31");
            } else if name == "reset" {
              pretty_issue.push('0');
            } else if name == "reverse" {
              pretty_issue.push('7');
            } else if name == "yellow" {
              pretty_issue.push_str("1;33");
            } else if name == "white" {
              pretty_issue.push_str("1;37");
            }

            pretty_issue.push('m');
          }
        } else if special_char == 's' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.sysname().to_str().unwrap_or(""));
          }
        } else if special_char == 'S' {
          let mut os_release_path = "/etc/os-release";
          if !fs::exists(os_release_path).unwrap_or(false) {
            os_release_path = "/usr/lib/os-release"
          }

          if let Ok(os_release) = fs::read_to_string(os_release_path) {
            let mut variable_name;
            if iter.peek() == Some(&'{') {
              iter.next(); // S -> {

              variable_name = iter.next().unwrap_or('}').to_string();
              while iter.peek() != Some(&'}') {
                variable_name.push(iter.next().unwrap_or('}'));
              }
              iter.next(); // } ->
            } else {
              variable_name = "PRETTY_NAME".to_owned();
            }

            for line in os_release.lines() {
              if line.starts_with(&format!("{variable_name}=")) {
                let mut variable_value =
                  line.replace(&format!("{variable_name}="), "");
                if variable_value.starts_with('"')
                  && variable_value.ends_with('"')
                {
                  variable_value = variable_value.replace('"', "");
                }

                if variable_name == "ANSI_COLOR" {
                  pretty_issue.push_str(&format!("\x1b[{variable_value}m"));
                } else {
                  pretty_issue.push_str(&variable_value);
                }
              }
            }
          } else {
            match uts {
              Ok(uts) => {
                pretty_issue.push_str(uts.machine().to_str().unwrap_or(""))
              },
              _ => pretty_issue.push_str("Linux"),
            }
          }
        } else if special_char == 'l' {
          pretty_issue.push_str(&format!("tty{vtnr}"));
        } else if special_char == 'm' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.machine().to_str().unwrap_or(""));
          }
        } else if special_char == 'n' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.nodename().to_str().unwrap_or(""));
          }
        /* TODO: 'O' -> DNS address */
        } else if special_char == 'o' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.domainname().to_str().unwrap_or(""));
          }
        } else if special_char == 'r' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.release().to_str().unwrap_or(""));
          }
        } else if special_char == 't' {
          pretty_issue.push_str(&time);
        } else if special_char == 'u' {
          pretty_issue.push_str(&user_count.to_string());
        } else if special_char == 'U' {
          pretty_issue.push_str(&user_string);
        } else if special_char == 'v' {
          if let Ok(uts) = uts {
            pretty_issue.push_str(uts.version().to_str().unwrap_or(""));
          }
        } else {
          pretty_issue.push('\\');
          pretty_issue.push(special_char);
        }
      } else {
        pretty_issue.push(c);
      }
    }

    return Some(pretty_issue);
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

pub fn get_users(min_uid: u32, max_uid: u32) -> Vec<User> {
  // SAFETY: `uzers` documents this enumeration as unsafe because it consults
  // libc account databases; we only consume its returned iterator here.
  let users = unsafe { uzers::all_users() };

  users
    .filter(|user| user.uid() >= min_uid && user.uid() <= max_uid)
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
        },
      },
    })
    .collect()
}

pub fn get_min_max_uids(
  min_uid: Option<u32>,
  max_uid: Option<u32>,
) -> (u32, u32) {
  if let (Some(min_uid), Some(max_uid)) = (min_uid, max_uid) {
    return (min_uid, max_uid);
  }

  let overrides = (min_uid, max_uid);
  let default = (
    min_uid.unwrap_or(DEFAULT_MIN_UID),
    max_uid.unwrap_or(DEFAULT_MAX_UID),
  );

  match File::open("/etc/login.defs") {
    Err(_) => default,
    Ok(file) => {
      let file = BufReader::new(file);

      let uids: (u32, u32) = file.lines().fold(default, |acc, line| {
        line.map_or(acc, |line| {
          let mut tokens = line.split_whitespace();

          match (overrides, tokens.next(), tokens.next()) {
            ((None, _), Some("UID_MIN"), Some(value)) => {
              (value.parse::<u32>().unwrap_or(acc.0), acc.1)
            },
            ((_, None), Some("UID_MAX"), Some(value)) => {
              (acc.0, value.parse::<u32>().unwrap_or(acc.1))
            },
            _ => acc,
          }
        })
      });

      uids
    },
  }
}

pub fn get_sessions(greeter: &Greeter) -> Result<Vec<Session>, Box<dyn Error>> {
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

  Ok(files)
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
  pub charging: bool,
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
      let charging = fs::read_to_string(&status_path)
        .map(|s| s.trim() == "Charging")
        .unwrap_or(false);
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

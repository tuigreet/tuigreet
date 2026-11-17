use std::{path::Path, process::Stdio, sync::Arc};

use tokio::{process::Command, sync::RwLock};
use tuigreet_types::Mode;

use crate::{Greeter, event::Event, ui::power::Power};

/// Power management options
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PowerOption {
  #[default]
  Shutdown,
  Reboot,
  Suspend,
  Hibernate,
}

/// Execute a power command.
pub async fn power(greeter: &Greeter, option: PowerOption) {
  if greeter.mock {
    if let Some(ref sender) = greeter.events {
      let _ = sender
        .send(Event::Exit(tuigreet_types::AuthStatus::Cancel))
        .await;
    }
    return;
  }

  let command = match greeter
    .powers
    .options
    .iter()
    .find(|opt| opt.action == option)
  {
    None => None,

    Some(Power {
      command: Some(args),
      ..
    }) => {
      let command = if greeter.power_setsid {
        let mut command = Command::new("setsid");
        command.args(args.split(' '));
        command
      } else {
        let mut args = args.split(' ');

        let mut command = Command::new(args.next().unwrap_or_default());
        command.args(args);
        command
      };

      Some(command)
    },

    Some(_) => {
      let command = match option {
        PowerOption::Shutdown => {
          let mut command = Command::new("shutdown");
          command.args(["-h", "now"]);
          command
        },
        PowerOption::Reboot => {
          let mut command = Command::new("shutdown");
          command.args(["-r", "now"]);
          command
        },
        PowerOption::Suspend | PowerOption::Hibernate => {
          let mut command = if Path::new("/run/systemd/system").is_dir() {
            Command::new("systemctl")
          } else {
            Command::new("loginctl")
          };
          command.arg(match option {
            PowerOption::Suspend => "suspend",
            _ => "hibernate",
          });
          command
        },
      };

      Some(command)
    },
  };

  if let Some(mut command) = command {
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    if let Some(ref sender) = greeter.events {
      let _ = sender.send(Event::PowerCommand(command, option)).await;
    }
  }
}

pub enum PowerPostAction {
  Noop,
  ClearScreen,
}

pub async fn run(
  greeter: &Arc<RwLock<Greeter>>,
  mut command: Command,
  option: PowerOption,
) -> PowerPostAction {
  tracing::info!("executing power command: {:?}", command);

  greeter.write().await.mode = Mode::Processing;

  let message = match command.output().await {
    Ok(result) => {
      match (result.status, result.stderr) {
        (status, _) if status.success() => None,
        (status, output) => {
          let status = format!("{} {status}", fl!("command_exited"));
          let output = String::from_utf8(output).unwrap_or_default();

          Some(format!("{status}\n{output}"))
        },
      }
    },

    Err(err) => Some(format!("{}: {err}", fl!("command_failed"))),
  };

  tracing::info!("power command exited with: {:?}", message);

  let mut greeter = greeter.write().await;

  if message.is_none()
    && matches!(option, PowerOption::Shutdown | PowerOption::Reboot)
  {
    PowerPostAction::ClearScreen
  } else {
    greeter.mode = greeter.previous_mode;
    greeter.working = false;
    greeter.message = message;

    PowerPostAction::Noop
  }
}

#[cfg(test)]
mod test {
  use std::sync::Arc;

  use tokio::{process::Command, sync::RwLock};
  use tuigreet_types::Mode;

  use super::{PowerOption, PowerPostAction, run};
  use crate::Greeter;

  #[tokio::test]
  async fn successful_shutdown_and_reboot_still_handoff() {
    for option in [PowerOption::Shutdown, PowerOption::Reboot] {
      let greeter = Arc::new(RwLock::new(Greeter::default()));

      assert!(matches!(
        run(&greeter, Command::new("true"), option).await,
        PowerPostAction::ClearScreen
      ));
    }
  }

  #[tokio::test]
  async fn failed_power_action_restores_prompt_and_reports_error() {
    let mut state = Greeter::default();
    state.previous_mode = Mode::Password;
    state.mode = Mode::Power;
    state.working = true;
    state.buffer = "partial password".to_string();
    let greeter = Arc::new(RwLock::new(state));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 'sleep denied' >&2; exit 7"]);

    assert!(matches!(
      run(&greeter, command, PowerOption::Suspend).await,
      PowerPostAction::Noop
    ));

    let state = greeter.read().await;
    assert_eq!(state.mode, Mode::Password);
    assert!(!state.working);
    assert_eq!(state.buffer, "partial password");
    assert!(state.message.as_deref().is_some_and(|message| {
      message.contains('7') && message.contains("sleep denied")
    }));
    drop(state);
  }
}

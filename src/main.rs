#[macro_use]
extern crate smart_default;

#[macro_use]
mod macros;

mod event;
mod greeter;
mod info;
mod ipc;
mod keyboard;
mod power;
mod ui;

#[cfg(test)]
mod integration;

use std::{error::Error, fs::OpenOptions, io, process, sync::Arc};

use crossterm::{
  execute,
  terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use event::Event;
use greetd_ipc::Request;
use power::PowerPostAction;
use tokio::sync::RwLock;
use tracing_appender::non_blocking::WorkerGuard;
use tui::{backend::CrosstermBackend, Terminal};

#[cfg(not(test))]
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};

pub use self::greeter::*;
use self::{event::Events, ipc::Ipc};

#[tokio::main]
async fn main() {
  // Check for --show-all-sessions first, before requiring GREETD_SOCK
  let args = std::env::args().collect::<Vec<String>>();
  if args.iter().any(|arg| arg == "--show-all-sessions") {
    if let Err(error) = info::show_all_sessions() {
      eprintln!("Error: {}", error);
      std::process::exit(1);
    }
    return;
  }

  let backend = CrosstermBackend::new(io::stdout());
  let events = Events::new().await;
  let greeter = Greeter::new(events.sender()).await;

  if let Err(error) = run(backend, greeter, events).await {
    if let Some(AuthStatus::Success) = error.downcast_ref::<AuthStatus>() {
      return;
    }

    process::exit(1);
  }
}

async fn run<B>(backend: B, mut greeter: Greeter, mut events: Events) -> Result<(), Box<dyn Error>>
where
  B: tui::backend::Backend,
{
  tracing::info!("tuigreet started");

  register_panic_handler();

  #[cfg(not(test))]
  {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
  }

  let mut terminal = Terminal::new(backend)?;

  #[cfg(not(test))]
  terminal.clear()?;

  let ipc = Ipc::new();

  if greeter.remember && !greeter.username.value.is_empty() {
    greeter.working = true;

    tracing::info!("creating remembered session for user {}", greeter.username.value);

    ipc
      .send(Request::CreateSession {
        username: greeter.username.value.clone(),
      })
      .await;
  }

  let greeter = Arc::new(RwLock::new(greeter));

  tokio::task::spawn({
    let greeter = greeter.clone();
    let mut ipc = ipc.clone();

    async move {
      loop {
        let _ = ipc.handle(greeter.clone()).await;
      }
    }
  });

  loop {
    if let Some(status) = greeter.read().await.exit {
      tracing::info!("exiting main loop");

      return Err(status.into());
    }

    match events.next().await {
      Some(Event::Render) => ui::draw(greeter.clone(), &mut terminal).await?,
      Some(Event::Key(key)) => keyboard::handle(greeter.clone(), key, ipc.clone()).await?,

      Some(Event::Exit(status)) => {
        crate::exit(&mut *greeter.write().await, status).await;
      }

      Some(Event::PowerCommand(command)) => {
        if let PowerPostAction::ClearScreen = power::run(&greeter, command).await {
          execute!(io::stdout(), LeaveAlternateScreen)?;
          terminal.set_cursor(1, 1)?;
          terminal.clear()?;
          disable_raw_mode()?;

          break;
        }
      }

      _ => {}
    }
  }

  Ok(())
}

async fn exit(greeter: &mut Greeter, status: AuthStatus) {
  tracing::info!("preparing exit with status {}", status);

  match status {
    AuthStatus::Success => {}
    AuthStatus::Cancel | AuthStatus::Failure => Ipc::cancel(greeter).await,
  }

  #[cfg(not(test))]
  clear_screen();

  let _ = execute!(io::stdout(), LeaveAlternateScreen);
  let _ = disable_raw_mode();

  greeter.exit = Some(status);
}

fn register_panic_handler() {
  let hook = std::panic::take_hook();

  std::panic::set_hook(Box::new(move |info| {
    #[cfg(not(test))]
    clear_screen();

    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    let _ = disable_raw_mode();

    hook(info);
  }));
}

#[cfg(not(test))]
pub fn clear_screen() {
  let backend = CrosstermBackend::new(io::stdout());

  if let Ok(mut terminal) = Terminal::new(backend) {
    let _ = terminal.hide_cursor();
    let _ = terminal.clear();
  }
}

fn init_logger(greeter: &Greeter) -> Option<WorkerGuard> {
  use tracing_subscriber::filter::{LevelFilter, Targets};
  use tracing_subscriber::prelude::*;

  let logfile = OpenOptions::new().write(true).create(true).append(true).clone();

  match (greeter.debug, logfile.open(&greeter.logfile)) {
    (true, Ok(file)) => {
      let (appender, guard) = tracing_appender::non_blocking(file);
      let target = Targets::new().with_target("tuigreet", LevelFilter::DEBUG);

      tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(appender).with_line_number(true))
        .with(target)
        .init();

      Some(guard)
    }

    _ => None,
  }
}

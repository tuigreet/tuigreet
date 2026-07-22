#[macro_use]
mod macros;

mod event;
mod greeter;
mod info;
mod ipc;
mod keyboard;
mod power;
mod ui;
mod watcher;

#[cfg(test)] mod integration;

#[cfg(not(test))] use std::mem;
use std::{error::Error, io, process, sync::Arc};

#[cfg(not(test))]
use crossterm::terminal::{
  Clear,
  ClearType,
  EnterAlternateScreen,
  enable_raw_mode,
};
use crossterm::{
  cursor::Hide,
  execute,
  terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use event::Event;
use greetd_ipc::Request;
use power::PowerPostAction;
use tokio::sync::RwLock;
use tui::{Terminal, backend::CrosstermBackend};
use tuigreet_types::AuthStatus;

pub use self::greeter::*;
use self::{event::Events, ipc::Ipc};

#[tokio::main]
async fn main() {
  let backend = CrosstermBackend::new(io::stdout());
  let events = Events::new().await;
  let greeter = Greeter::new(events.sender()).await;
  events.set_frame_rate(greeter.frame_rate());

  if let Err(error) = run(backend, greeter, events).await {
    if matches!(
      error.downcast_ref::<AuthStatus>(),
      Some(AuthStatus::Success)
    ) {
      return;
    }

    process::exit(1);
  }
}

async fn run<B>(
  backend: B,
  mut greeter: Greeter,
  mut events: Events,
) -> Result<(), Box<dyn Error>>
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

    tracing::info!(
      "creating remembered session for user {}",
      greeter.username.value
    );

    ipc
      .send(Request::CreateSession {
        username: greeter.username.value.clone(),
      })
      .await;
  }

  if greeter.user_menu
    && greeter.users.options.len() == 1
    && let Some(user) = greeter.users.options.first().cloned()
  {
    tracing::info!("auto-selecting sole eligible user: {}", user.username);

    greeter.username =
      crate::ui::common::masked::MaskedString::from(user.username, user.name);
    greeter.working = true;

    ipc
      .send(Request::CreateSession {
        username: greeter.username.value.clone(),
      })
      .await;
  }

  let greeter = Arc::new(RwLock::new(greeter));

  // Initialize config watcher for hot reloading
  #[cfg(not(test))]
  let _config_watcher = {
    let config_path = {
      let greeter_guard = greeter.read().await;
      greeter_guard
        .config()
        .opt_str("config")
        .map(std::path::PathBuf::from)
    };

    match crate::watcher::ConfigWatcher::new(
      config_path,
      greeter.clone(),
      events.sender(),
    ) {
      Ok(watcher) => Some(watcher),
      Err(e) => {
        tracing::warn!("Failed to initialize config watcher: {}", e);
        None
      },
    }
  };

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

      // Skip `Terminal`'s `Drop`, it would re-emit `\x1b[?25h` whenever the
      // last frame ran with `hidden_cursor = true`, un-hiding the cursor right
      // after `exit` hid it. Leaking the buffers is harmless at process exit.
      #[cfg(not(test))]
      mem::forget(terminal);

      return Err(status.into());
    }

    match events.next().await {
      Some(Event::Render) => ui::draw(greeter.clone(), &mut terminal).await?,
      Some(Event::Key(key)) => {
        keyboard::handle(greeter.clone(), key, ipc.clone()).await?;
      },

      Some(Event::Exit(status)) => {
        crate::exit(&mut *greeter.write().await, status).await;
      },

      Some(Event::PowerCommand(command)) => {
        if matches!(
          power::run(&greeter, command).await,
          PowerPostAction::ClearScreen
        ) {
          execute!(io::stdout(), LeaveAlternateScreen)?;
          terminal.set_cursor_position((1, 1))?;
          terminal.clear()?;
          execute!(io::stdout(), Hide)?;
          disable_raw_mode()?;

          // Same rationale as the exit-status branch above.
          #[cfg(not(test))]
          mem::forget(terminal);

          break;
        }
      },

      Some(Event::Refresh) => {
        // Config was hot reloaded, force a render
        ui::draw(greeter.clone(), &mut terminal).await?;
      },

      Some(Event::SetFrameRate(fps)) => events.set_frame_rate(fps),

      _ => {},
    }
  }

  Ok(())
}

async fn exit(greeter: &mut Greeter, status: AuthStatus) {
  tracing::info!("preparing exit with status {}", status);

  match status {
    AuthStatus::Success => {},
    AuthStatus::Cancel | AuthStatus::Failure => Ipc::cancel(greeter).await,
  }

  #[cfg(not(test))]
  clear_screen();

  let _ = execute!(io::stdout(), LeaveAlternateScreen, Hide);
  let _ = disable_raw_mode();

  greeter.exit = Some(status);
}

fn register_panic_handler() {
  let hook = std::panic::take_hook();

  std::panic::set_hook(Box::new(move |info| {
    #[cfg(not(test))]
    clear_screen();

    let _ = execute!(io::stdout(), LeaveAlternateScreen, Hide);
    let _ = disable_raw_mode();

    hook(info);
  }));
}

#[cfg(not(test))]
pub fn clear_screen() {
  // Emit cursor-hide and clear directly via crossterm rather than wrapping the
  // stdout in a `ratatui::Terminal`: the latter re-emits `\x1b[?25h` from its
  // `Drop` impl whenever `hidden_cursor` is true, which would un-hide the
  // cursor right after we hide it.
  let _ = execute!(io::stdout(), Hide, Clear(ClearType::All));
}

#[cfg(not(test))]
fn init_logger(
  greeter: &Greeter,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
  use std::fs::OpenOptions;

  use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    prelude::*,
  };

  let logfile = OpenOptions::new()
    .write(true)
    .create(true)
    .append(true)
    .clone();

  match (greeter.debug, logfile.open(&greeter.logfile)) {
    (true, Ok(file)) => {
      let (appender, guard) = tracing_appender::non_blocking(file);
      let target = Targets::new().with_target("tuigreet", LevelFilter::DEBUG);

      tracing_subscriber::registry()
        .with(
          tracing_subscriber::fmt::layer()
            .with_writer(appender)
            .with_line_number(true),
        )
        .with(target)
        .init();

      Some(guard)
    },

    _ => None,
  }
}

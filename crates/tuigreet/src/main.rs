#[macro_use]
mod macros;

mod action;
mod datetime;
mod event;
mod greeter;
mod info;
mod ipc;
mod keyboard;
mod power;
mod ui;
#[cfg(not(test))] mod watcher;

#[cfg(test)] mod integration;

#[cfg(not(test))] use std::mem;
use std::{
  env,
  error::Error,
  fmt::Display,
  io::{self, Write},
  panic,
  process::ExitCode,
  sync::Arc,
};

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
use tokio::{
  sync::{RwLock, mpsc},
  task,
};
#[cfg(not(test))]
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
#[cfg(not(test))] use tracing_subscriber::fmt;
use tui::{
  Terminal,
  backend::{Backend, CrosstermBackend},
};
use tuigreet_config::{Config, SessionEntry, cli::Cli, parser::load_config};
use tuigreet_types::AuthStatus;

pub use self::greeter::*;
#[cfg(not(test))] use self::watcher::ConfigWatcher;
use self::{event::Events, ipc::Ipc, ui::common::masked::MaskedString};

fn main() -> ExitCode {
  caress::handle(Cli::spec());
  run_main()
}

#[tokio::main]
async fn run_main() -> ExitCode {
  if standalone_version_requested() {
    let Some(()) = startup_value(print_version()) else {
      return ExitCode::FAILURE;
    };
    return ExitCode::SUCCESS;
  }
  let cli = Cli::parse();
  let Some(immediate_action) = startup_value(run_immediate_action(&cli)) else {
    return ExitCode::FAILURE;
  };
  if immediate_action {
    return ExitCode::SUCCESS;
  }

  let Some(loaded) = startup_value(load_config(&cli)) else {
    return ExitCode::FAILURE;
  };
  if cli.dump_config {
    let Some(()) = startup_value(write_effective_config(&loaded.config)) else {
      return ExitCode::FAILURE;
    };
    return ExitCode::SUCCESS;
  }
  if cli.show_all_sessions {
    let Some(()) = startup_value(write_session_template(&loaded.config)) else {
      return ExitCode::FAILURE;
    };
    return ExitCode::SUCCESS;
  }

  if !loaded.config.outputs.is_empty() || loaded.config.terminal.cols.is_some()
  {
    tuigreet_system::apply_output_config(
      &loaded.config.outputs,
      &loaded.config.terminal,
    );
  }

  let backend = CrosstermBackend::new(io::stdout());
  let events = Events::new();
  let greeter =
    Greeter::new(events.sender(), loaded.config, cli.mock_mode).await;
  for warning in loaded.warnings {
    tracing::warn!("Config warning: {warning}");
  }
  events.set_frame_rate(greeter.frame_rate());
  events.set_idle_timeout(greeter.idle_timeout());

  if let Err(error) = run(backend, greeter, events, cli).await {
    if matches!(
      error.downcast_ref::<AuthStatus>(),
      Some(AuthStatus::Success)
    ) {
      return ExitCode::SUCCESS;
    }
    return ExitCode::FAILURE;
  }
  ExitCode::SUCCESS
}

fn standalone_version_requested() -> bool {
  let mut args = env::args_os().skip(1);
  let Some(arg) = args.next() else {
    return false;
  };
  args.next().is_none()
    && matches!(arg.to_str(), Some("-v" | "-V" | "--version"))
}

fn run_immediate_action(cli: &Cli) -> Result<bool, Box<dyn Error>> {
  if cli.version {
    print_version()?;
    return Ok(true);
  }
  if cli.list_outputs {
    tuigreet_system::list_outputs()?;
    return Ok(true);
  }
  Ok(false)
}

fn write_effective_config(config: &Config) -> Result<(), Box<dyn Error>> {
  let config = toml::to_string_pretty(config)?;
  writeln!(io::stdout().lock(), "{config}")?;
  Ok(())
}

// Prints every discovered session as `[[session.entries]]` tables, to be used
// as a starting point for customizing the session menu.
fn write_session_template(config: &Config) -> Result<(), Box<dyn Error>> {
  let paths = info::configured_session_paths(&config.session);
  let entries: Vec<SessionEntry> = info::discover_sessions(&paths)
    .into_iter()
    .zip(0..)
    .filter_map(|(session, order)| {
      Some(SessionEntry {
        path: session.path?.to_string_lossy().into_owned(),
        name: Some(session.name),
        enabled: true,
        order,
      })
    })
    .collect();

  let mut stdout = io::stdout().lock();
  writeln!(
    stdout,
    "# enabled = true|false: whether the session is displayed"
  )?;
  writeln!(
    stdout,
    "# order = <number>: display order, lower values are displayed first"
  )?;
  writeln!(stdout)?;
  write!(stdout, "{}", SessionEntry::to_toml(entries)?)?;
  Ok(())
}

fn startup_value<T, E: Display>(result: Result<T, E>) -> Option<T> {
  match result {
    Ok(value) => Some(value),
    Err(error) => {
      report_startup_error(error);
      None
    },
  }
}

fn report_startup_error(error: impl Display) {
  if let Err(write_error) = writeln!(io::stderr().lock(), "{error}") {
    tracing::error!("Failed to write startup error: {write_error}");
  }
}

fn print_version() -> io::Result<()> {
  let mut stdout = io::stdout().lock();
  writeln!(stdout, "tuigreet {} ({})", env!("VERSION"), env!("TARGET"))?;
  writeln!(
    stdout,
    "Copyright (C) 2020 Antoine POPINEAU <https://github.com/apognu/tuigreet>."
  )?;
  writeln!(
    stdout,
    "Maintained by NotAShelf <https://github.com/NotAShelf/tuigreet>."
  )?;
  writeln!(
    stdout,
    "Licensed under GPLv3+ (GNU GPL version 3 or later)."
  )?;
  writeln!(stdout)?;
  writeln!(
    stdout,
    "This is free software, you are welcome to redistribute it under some \
     conditions."
  )?;
  writeln!(
    stdout,
    "There is NO WARRANTY, to the extent provided by law."
  )
}

async fn run<B>(
  backend: B,
  mut greeter: Greeter,
  mut events: Events,
  cli: Cli,
) -> Result<(), Box<dyn Error>>
where
  B: Backend,
  B::Error: 'static,
{
  tracing::info!("tuigreet started");
  register_panic_handler();
  if greeter.numlock {
    enable_numlock();
  }

  #[cfg(not(test))]
  {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
  }

  let terminal = Terminal::new(backend)?;
  #[cfg(not(test))]
  let terminal = {
    let mut terminal = terminal;
    terminal.clear()?;
    terminal
  };

  events.start();
  let ipc = Ipc::new();
  start_initial_session(&mut greeter, &ipc).await;

  #[cfg(test)]
  let _ = cli;
  let greeter = Arc::new(RwLock::new(greeter));

  #[cfg(not(test))]
  let _config_watcher =
    start_config_watcher(cli, Arc::clone(&greeter), events.sender());

  spawn_ipc_handler(Arc::clone(&greeter), ipc.clone(), events.sender());
  run_event_loop(terminal, greeter, ipc, &mut events).await
}

async fn start_initial_session(greeter: &mut Greeter, ipc: &Ipc) {
  if greeter.remember && !greeter.username.value.is_empty() {
    greeter.working = true;
    tracing::info!(
      "creating remembered session for user {}",
      greeter.username.value
    );
    send_create_session(ipc, greeter.username.value.clone()).await;
    return;
  }

  if greeter.user_menu
    && !greeter.working
    && greeter.users.options.len() == 1
    && let Some(user) = greeter.users.options.first().cloned()
  {
    tracing::info!("auto-selecting sole eligible user: {}", user.username);
    greeter.username = MaskedString::from(user.username, user.name);
    greeter.working = true;
    send_create_session(ipc, greeter.username.value.clone()).await;
  }
}

async fn send_create_session(ipc: &Ipc, username: String) {
  ipc.send(Request::CreateSession { username }).await;
}

#[cfg(not(test))]
fn start_config_watcher(
  cli: Cli,
  greeter: Arc<RwLock<Greeter>>,
  event_sender: mpsc::Sender<Event>,
) -> Option<ConfigWatcher> {
  if cli.no_config {
    return None;
  }
  match ConfigWatcher::new(cli, greeter, event_sender) {
    Ok(watcher) => Some(watcher),
    Err(error) => {
      tracing::warn!("Failed to initialize config watcher: {error}");
      None
    },
  }
}

fn spawn_ipc_handler(
  greeter: Arc<RwLock<Greeter>>,
  ipc: Ipc,
  event_tx: mpsc::Sender<Event>,
) {
  task::spawn(async move {
    loop {
      if ipc.handle(Arc::clone(&greeter)).await.is_err() {
        tracing::error!("greetd IPC handler failed");
        let _ = event_tx.send(Event::Exit(AuthStatus::Failure)).await;
        break;
      }
    }
  });
}

async fn run_event_loop<B>(
  mut terminal: Terminal<B>,
  greeter: Arc<RwLock<Greeter>>,
  ipc: Ipc,
  events: &mut Events,
) -> Result<(), Box<dyn Error>>
where
  B: Backend,
  B::Error: 'static,
{
  loop {
    let exit_status = greeter.read().await.exit;
    if let Some(status) = exit_status {
      tracing::info!("exiting main loop");

      // Skip `Terminal`'s `Drop`, it would re-emit `\x1b[?25h` whenever the
      // last frame ran with `hidden_cursor = true`, un-hiding the cursor right
      // after `exit` hid it. Leaking the buffers is harmless at process exit.
      #[cfg(not(test))]
      mem::forget(terminal);

      return Err(status.into());
    }

    if handle_event(events.next().await, &greeter, &ipc, events, &mut terminal)
      .await?
    {
      // Same rationale as the exit-status branch above.
      #[cfg(not(test))]
      mem::forget(terminal);
      return Ok(());
    }
  }
}

async fn handle_event<B>(
  event: Option<Event>,
  greeter: &Arc<RwLock<Greeter>>,
  ipc: &Ipc,
  events: &Events,
  terminal: &mut Terminal<B>,
) -> Result<bool, Box<dyn Error>>
where
  B: Backend,
  B::Error: 'static,
{
  match event {
    Some(Event::Render) => {
      ui::draw(Arc::clone(greeter), terminal).await?;
    },
    Some(Event::Key(key)) => {
      if greeter.write().await.wake_from_idle() {
        ui::draw(Arc::clone(greeter), terminal).await?;
      } else {
        keyboard::handle(Arc::clone(greeter), key, ipc.clone()).await?;
      }
    },
    Some(Event::Idle) => {
      greeter.write().await.enter_idle();
      ui::draw(Arc::clone(greeter), terminal).await?;
    },
    Some(Event::Exit(status)) => {
      crate::exit(&mut *greeter.write().await, status).await;
    },
    Some(Event::PowerCommand(command)) => {
      if matches!(
        power::run(greeter, command).await,
        PowerPostAction::ClearScreen
      ) {
        execute!(io::stdout(), LeaveAlternateScreen)?;
        terminal.set_cursor_position((1, 1))?;
        terminal.clear()?;
        execute!(io::stdout(), Hide)?;
        disable_raw_mode()?;
        return Ok(true);
      }
    },
    Some(Event::Refresh) => {
      let timeout = greeter.read().await.idle_timeout();
      events.set_idle_timeout(timeout);
      ui::draw(Arc::clone(greeter), terminal).await?;
    },
    Some(Event::SetFrameRate(fps)) => events.set_frame_rate(fps),
    unexpected => return Err(event_stream_error(unexpected)),
  }
  Ok(false)
}

fn event_stream_error(event: Option<Event>) -> Box<dyn Error> {
  match event {
    Some(Event::InputError(error)) => error.into(),
    None => {
      io::Error::new(io::ErrorKind::BrokenPipe, "event channel closed").into()
    },
    Some(_) => {
      io::Error::new(
        io::ErrorKind::InvalidData,
        "unexpected event reached the terminal error boundary",
      )
      .into()
    },
  }
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

use nix::{ioctl_read_bad, ioctl_write_int_bad};
ioctl_read_bad!(get_keyboard_flags, 0x4B64, u8);
ioctl_write_int_bad!(set_keyboard_flags, 0x4B65);
#[expect(
  unsafe_code,
  reason = "Linux keyboard ioctls require an unsafe FFI call"
)]
fn enable_numlock() {
  // SAFETY: KDGKBLED writes one byte into the live flags value. KDSKBLED
  // takes an integer value, not a pointer. fd 0 is only queried here; errors
  // (including stdin not being a Linux console) are handled without
  // dereferencing it.
  unsafe {
    let mut flags: u8 = 0;
    if get_keyboard_flags(0, &raw mut flags).is_ok() {
      flags |= 0x22;
      let _ = set_keyboard_flags(0, i32::from(flags));
    }
  }
}

fn register_panic_handler() {
  let hook = panic::take_hook();

  panic::set_hook(Box::new(move |info| {
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
fn init_logger(greeter: &Greeter) -> Option<WorkerGuard> {
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
      let (appender, guard) = non_blocking(file);
      let target = Targets::new().with_target("tuigreet", LevelFilter::DEBUG);

      tracing_subscriber::registry()
        .with(fmt::layer().with_writer(appender).with_line_number(true))
        .with(target)
        .init();

      Some(guard)
    },

    _ => None,
  }
}

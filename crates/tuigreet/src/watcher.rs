use std::{
  error::Error,
  path::{PathBuf, absolute},
  sync::Arc,
  time::Duration,
};

use notify::{
  Config as NotifyConfig,
  Event,
  EventKind,
  RecommendedWatcher,
  RecursiveMode,
  Watcher,
};
use tokio::{
  sync::{RwLock, mpsc},
  time::sleep,
};
use tracing::{debug, error, info, warn};
use tuigreet_config::{Config, cli::Cli, parser::load_config};

use crate::{Greeter, event::Event as GreeterEvent};

/// File watcher for hot-reloading configuration changes.
///
/// Keeps the watcher alive for the lifetime of the struct.
/// Dropping this struct will stop file watching.
pub struct ConfigWatcher {
  _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
  /// Create a watcher for the file selected by `cli`.
  ///
  /// # Errors
  /// Returns an error if the file watcher cannot be initialized.
  pub fn new(
    cli: Cli,
    greeter: Arc<RwLock<Greeter>>,
    event_sender: mpsc::Sender<GreeterEvent>,
  ) -> Result<Self, Box<dyn Error + Send + Sync>> {
    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>(100);
    let mut watcher = RecommendedWatcher::new(
      move |result| {
        if let Err(error) = tx.try_send(result) {
          error!("Failed to enqueue file watch event: {error}");
        }
      },
      NotifyConfig::default(),
    )?;

    let watch_paths = Self::config_paths(&cli);
    Self::watch_existing_files(&mut watcher, &watch_paths)?;
    Self::spawn_event_task(rx, cli, greeter, event_sender, watch_paths);

    Ok(Self { _watcher: watcher })
  }

  fn config_paths(cli: &Cli) -> Vec<PathBuf> {
    if let Some(path) = cli.config_path() {
      let path = absolute(path).unwrap_or_else(|_| path.to_path_buf());
      return vec![path];
    }

    let mut paths = vec![PathBuf::from("/etc/tuigreet/config.toml")];
    if let Some(path) = dirs::config_dir() {
      paths.push(path.join("tuigreet").join("config.toml"));
    }
    paths
  }

  fn watch_existing_files(
    watcher: &mut RecommendedWatcher,
    paths: &[PathBuf],
  ) -> Result<(), notify::Error> {
    let mut watched_parents = Vec::new();
    for path in paths {
      if !path.exists() {
        info!(
          "Config file does not exist, not watching: {}",
          path.display()
        );
        continue;
      }
      info!("Starting config file watcher for: {}", path.display());
      let Some(parent) = path.parent() else {
        continue;
      };
      if watched_parents.iter().any(|watched| watched == parent) {
        continue;
      }
      watcher.watch(parent, RecursiveMode::NonRecursive)?;
      watched_parents.push(parent.to_path_buf());
    }
    Ok(())
  }

  fn spawn_event_task(
    mut rx: mpsc::Receiver<Result<Event, notify::Error>>,
    cli: Cli,
    greeter: Arc<RwLock<Greeter>>,
    event_sender: mpsc::Sender<GreeterEvent>,
    watch_paths: Vec<PathBuf>,
  ) {
    tokio::spawn(async move {
      while let Some(result) = rx.recv().await {
        Self::handle_watch_result(
          result,
          &cli,
          &greeter,
          &event_sender,
          &watch_paths,
        )
        .await;
      }
    });
  }

  async fn handle_watch_result(
    result: Result<Event, notify::Error>,
    cli: &Cli,
    greeter: &Arc<RwLock<Greeter>>,
    event_sender: &mpsc::Sender<GreeterEvent>,
    watch_paths: &[PathBuf],
  ) {
    match result {
      Ok(event) if Self::is_config_event(&event, watch_paths) => {
        debug!("Config file change detected: {:?}", event);
        sleep(Duration::from_millis(100)).await;
        Self::reload_and_apply(cli, greeter, event_sender).await;
      },
      Ok(_) => {},
      Err(error) => error!("File watcher error: {error}"),
    }
  }

  async fn reload_and_apply(
    cli: &Cli,
    greeter: &Arc<RwLock<Greeter>>,
    event_sender: &mpsc::Sender<GreeterEvent>,
  ) {
    debug!("Reloading configuration");
    let loaded = match load_config(cli) {
      Ok(loaded) => loaded,
      Err(error) => {
        error!("Failed to reload config: {error}");
        return;
      },
    };
    for warning in loaded.warnings {
      warn!("Config warning after reload: {warning}");
    }

    Self::apply_config_to_greeter(greeter, loaded.config).await;
    info!("Config successfully reloaded");
    let frame_rate = greeter.read().await.frame_rate();
    if event_sender
      .send(GreeterEvent::SetFrameRate(frame_rate))
      .await
      .is_err()
      || event_sender.send(GreeterEvent::Refresh).await.is_err()
    {
      warn!("Failed to send configuration refresh events");
    }
  }

  fn is_config_event(event: &Event, config_paths: &[PathBuf]) -> bool {
    matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
      && event
        .paths
        .iter()
        .any(|event_path| config_paths.iter().any(|path| path == event_path))
  }

  async fn apply_config_to_greeter(
    greeter: &Arc<RwLock<Greeter>>,
    config: Config,
  ) {
    let mut greeter_guard = greeter.write().await;

    // A tracing subscriber is installed once at startup and cannot be safely
    // replaced here. Keep its settings stable until the next restart.
    let logger_settings = (greeter_guard.debug, greeter_guard.logfile.clone());
    if greeter_guard.loaded_config.as_ref().is_some_and(|old| {
      old.general.debug != config.general.debug
        || old.general.log_file != config.general.log_file
    }) {
      warn!("general.debug and general.log_file changes require a restart");
    }

    // Applying an already validated configuration replaces all
    // configuration-owned runtime state while holding this write lock.
    greeter_guard.apply_config(config);
    (greeter_guard.debug, greeter_guard.logfile) = logger_settings;
    greeter_guard.reload_sessions();
    drop(greeter_guard);

    info!("Config hot reload completed successfully");
  }
}

pub mod backend;
mod output;

use std::{
  io,
  panic,
  sync::{Arc, Mutex},
  time::Duration,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use libgreetd_stub::SessionOptions;
use tempfile::NamedTempFile;
use tokio::{
  sync::{
    RwLock,
    mpsc::{Sender, UnboundedReceiver},
  },
  task::{JoinError, JoinHandle, spawn},
  time::timeout,
};
use tui::buffer::Buffer;
use tuigreet_config::{Config, cli::Cli};
use tuigreet_types::AuthStatus;

use self::{
  backend::{TestBackend, output},
  output::Output,
};
use crate::{
  Greeter,
  event::{Event, Events},
  run,
  ui::sessions::SessionSource,
};

pub(super) struct IntegrationRunner(Arc<RwLock<RunnerState>>);

struct RunnerState {
  server: Option<JoinHandle<()>>,
  client: Option<JoinHandle<io::Result<()>>>,
  buffer: Arc<Mutex<Buffer>>,
  sender: Sender<Event>,
  tick:   UnboundedReceiver<bool>,
}

impl Clone for IntegrationRunner {
  fn clone(&self) -> Self {
    Self(Arc::clone(&self.0))
  }
}

impl IntegrationRunner {
  pub fn new(
    opts: SessionOptions,
    builder: Option<fn(&mut Greeter)>,
  ) -> io::Result<Self> {
    Self::new_with_size(opts, builder, (200, 40))
  }

  pub fn new_with_size(
    opts: SessionOptions,
    builder: Option<fn(&mut Greeter)>,
    size: (u16, u16),
  ) -> io::Result<Self> {
    let socket = NamedTempFile::new()?.into_temp_path().to_path_buf();
    let socket_name = socket
      .to_str()
      .ok_or_else(|| io::Error::other("socket path is not valid UTF-8"))?
      .to_string();

    let (backend, buffer, tick) = TestBackend::new(size.0, size.1);
    let events = Events::new();
    let sender = events.sender();

    let server = spawn(async move {
      libgreetd_stub::start(&socket, &opts).await;
    });

    let client = spawn(async move {
      let mut greeter =
        Greeter::new(events.sender(), Config::default(), false).await;
      greeter.session_source = SessionSource::Command("uname".to_string());

      if let Some(builder) = builder {
        builder(&mut greeter);
      }

      greeter.logfile = "/tmp/tuigreet.log".to_string();
      greeter.socket = socket_name;
      greeter.events = Some(events.sender());
      greeter.connect().await;

      let cli = Cli::try_parse_from(["--no-config"])
        .map_err(|error| io::Error::other(error.to_string()))?;
      match run(backend, greeter, events, cli).await {
        Ok(()) => Ok(()),
        // Authentication and explicit user exits are normal test completion.
        Err(error)
          if matches!(
            error.downcast_ref::<AuthStatus>(),
            Some(AuthStatus::Success | AuthStatus::Cancel)
          ) =>
        {
          Ok(())
        },
        Err(error) => Err(io::Error::other(error.to_string())),
      }
    });

    Ok(Self(Arc::new(RwLock::new(RunnerState {
      server: Some(server),
      client: Some(client),
      buffer,
      sender,
      tick,
    }))))
  }

  pub async fn join_until_client_exit(
    &self,
    mut events: JoinHandle<io::Result<()>>,
  ) -> io::Result<()> {
    let (mut server, mut client) = {
      let mut runner = self.0.write().await;
      (
        runner
          .server
          .take()
          .ok_or_else(|| io::Error::other("server already joined"))?,
        runner
          .client
          .take()
          .ok_or_else(|| io::Error::other("client already joined"))?,
      )
    };

    let result = timeout(Duration::from_secs(5), async {
      let mut server_done = false;
      let mut client_done = false;
      let mut events_done = false;
      while !client_done || !events_done {
        tokio::select! {
          ret = &mut server, if !server_done => {
            rethrow(ret)?;
            server_done = true;
          },
          ret = &mut client, if !client_done => {
            rethrow(ret)??;
            client_done = true;
          },
          ret = &mut events, if !events_done => {
            rethrow(ret)??;
            events_done = true;
          },
        }
      }
      Ok(())
    })
    .await;

    server.abort();
    client.abort();
    events.abort();
    result.map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))?
  }

  pub async fn join_until_end(
    &self,
    mut events: JoinHandle<io::Result<()>>,
  ) -> io::Result<()> {
    let (mut server, mut client) = {
      let mut runner = self.0.write().await;
      (
        runner
          .server
          .take()
          .ok_or_else(|| io::Error::other("server already joined"))?,
        runner
          .client
          .take()
          .ok_or_else(|| io::Error::other("client already joined"))?,
      )
    };

    let result = timeout(Duration::from_secs(5), async {
      tokio::select! {
        ret = &mut server => {
          rethrow(ret)?;
          Err(io::Error::other("server exited before test assertions completed"))
        },
        ret = &mut client => {
          rethrow(ret)??;
          Err(io::Error::other("client exited before test assertions completed"))
        },
        ret = &mut events => rethrow(ret)?,
      }
    }).await;

    server.abort();
    client.abort();
    events.abort();
    result.map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))?
  }

  pub async fn wait_until_buffer_contains(
    &self,
    needle: &str,
  ) -> io::Result<()> {
    loop {
      if self.output().await?.contains(needle) {
        return Ok(());
      }
      self.wait_for_render().await?;
    }
  }

  pub async fn send_key(&self, key: KeyCode) -> io::Result<()> {
    self.send_modified_key(key, KeyModifiers::empty()).await
  }

  pub async fn send_modified_key(
    &self,
    key: KeyCode,
    modifiers: KeyModifiers,
  ) -> io::Result<()> {
    let sender = self.0.read().await.sender.clone();
    sender
      .send(Event::Key(KeyEvent::new(key, modifiers)))
      .await
      .map_err(io::Error::other)
  }

  pub async fn send_text(&self, text: &str) -> io::Result<()> {
    for char in text.chars() {
      self.send_key(KeyCode::Char(char)).await?;
    }
    self.send_key(KeyCode::Enter).await
  }

  pub async fn wait_for_render(&self) -> io::Result<()> {
    let mut runner = self.0.write().await;
    // Only a fresh frame can acknowledge keys sent since the previous wait.
    while runner.tick.try_recv().is_ok() {}
    runner.tick.recv().await.ok_or_else(|| {
      io::Error::other("renderer stopped before expected frame")
    })?;
    drop(runner);
    Ok(())
  }

  pub async fn output(&self) -> io::Result<Output> {
    let buffer = Arc::clone(&self.0.read().await.buffer);
    Ok(Output(output(&buffer)?))
  }
}

fn rethrow<T>(result: Result<T, JoinError>) -> io::Result<T> {
  match result {
    Ok(value) => Ok(value),
    Err(err) if err.is_panic() => panic::resume_unwind(err.into_panic()),
    Err(err) => Err(io::Error::other(err)),
  }
}

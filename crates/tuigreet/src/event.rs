use std::{io, time::Duration};

#[cfg(not(test))] use crossterm::event::EventStream;
use crossterm::event::{Event as TermEvent, KeyEvent};
#[cfg(test)] use futures::stream;
use futures::{StreamExt, future::FutureExt};
use tokio::{
  process::Command,
  sync::{
    mpsc::{self, Sender},
    watch,
  },
  task,
  time::{Instant, MissedTickBehavior, interval, sleep_until},
};
use tuigreet_types::AuthStatus;

/// Default render tick.
const DEFAULT_FRAME_RATE: f64 = 2.0;

/// Upper clamp on the configurable frame rate.
const MAX_FRAME_RATE: f64 = 1_000.0;

/// Events that drive the UI event loop.
pub enum Event {
  /// Keyboard input event.
  Key(KeyEvent),
  /// Render frame event.
  Render,
  /// Configured inactivity timeout elapsed.
  Idle,
  /// Terminal input failed.
  InputError(io::Error),
  /// Power command to execute.
  PowerCommand(Command),
  /// Exit with authentication status.
  Exit(AuthStatus),
  /// UI refresh.
  Refresh,
  /// Update the render tick rate.
  SetFrameRate(f64),
}

/// Event channel for receiving terminal and internal events.
pub struct Events {
  rx:              mpsc::Receiver<Event>,
  tx:              mpsc::Sender<Event>,
  frame_rate_tx:   watch::Sender<f64>,
  idle_timeout_tx: watch::Sender<u64>,
}

fn frame_interval(fps: f64) -> Duration {
  Duration::from_secs_f64(1.0 / fps)
}

impl Events {
  /// Creates a new event stream with keyboard and render events.
  pub fn new() -> Self {
    let (tx, rx) = mpsc::channel(10);
    let (frame_rate_tx, _) = watch::channel(DEFAULT_FRAME_RATE);
    let (idle_timeout_tx, _) = watch::channel(0);
    Self {
      rx,
      tx,
      frame_rate_tx,
      idle_timeout_tx,
    }
  }

  /// Starts receiving terminal and render events.
  ///
  /// The terminal input stream must only be created after raw mode is enabled.
  /// Creating it earlier can make crossterm's reader panic before it has a
  /// source, leaving the UI without render events.
  pub fn start(&self) {
    let tx = self.tx.clone();
    let frame_rate_rx = self.frame_rate_tx.subscribe();
    let idle_timeout_rx = self.idle_timeout_tx.subscribe();
    task::spawn(run_events(tx, frame_rate_rx, idle_timeout_rx));
  }

  /// Updates the render tick rate.
  pub fn set_frame_rate(&self, fps: f64) {
    if fps <= 0.0 || !fps.is_finite() {
      return;
    }
    self.frame_rate_tx.send_replace(fps.min(MAX_FRAME_RATE));
  }

  /// Updates and rearms the inactivity timeout. Zero disables idle handling.
  pub fn set_idle_timeout(&self, seconds: u64) {
    self.idle_timeout_tx.send_replace(seconds);
  }

  /// Waits for the next event, or returns `None` after all senders close.
  pub async fn next(&mut self) -> Option<Event> {
    self.rx.recv().await
  }

  /// Returns a sender for producers of internal events.
  pub fn sender(&self) -> Sender<Event> {
    self.tx.clone()
  }
}

async fn run_events(
  tx: mpsc::Sender<Event>,
  mut frame_rate_rx: watch::Receiver<f64>,
  mut idle_timeout_rx: watch::Receiver<u64>,
) {
  #[cfg(not(test))]
  let mut stream = EventStream::new();
  #[cfg(test)]
  let mut stream = stream::pending::<io::Result<TermEvent>>();

  let mut render_interval = interval(frame_interval(*frame_rate_rx.borrow()));
  render_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
  let mut idle_timeout = *idle_timeout_rx.borrow();
  let mut last_activity = Instant::now();
  let mut idle_sent = false;

  loop {
    let idle_enabled = idle_timeout > 0 && !idle_sent;
    let idle_deadline = last_activity + Duration::from_secs(idle_timeout);

    tokio::select! {
      event = stream.next().fuse() => {
        if !forward_terminal_event(
          event,
          &tx,
          &mut last_activity,
          &mut idle_sent,
        ).await {
          break;
        }
      },
      _ = render_interval.tick() => {
        if !send_event(&tx, Event::Render).await {
          break;
        }
      },
      changed = frame_rate_rx.changed() => {
        if changed.is_err() {
          break;
        }
        render_interval = interval(frame_interval(*frame_rate_rx.borrow_and_update()));
        render_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
      },
      changed = idle_timeout_rx.changed() => {
        if changed.is_err() {
          break;
        }
        idle_timeout = *idle_timeout_rx.borrow_and_update();
        last_activity = Instant::now();
        idle_sent = false;
      },
      () = sleep_until(idle_deadline), if idle_enabled => {
        idle_sent = true;
        if !send_event(&tx, Event::Idle).await {
          break;
        }
      },
    }
  }
}

async fn forward_terminal_event(
  event: Option<io::Result<TermEvent>>,
  tx: &mpsc::Sender<Event>,
  last_activity: &mut Instant,
  idle_sent: &mut bool,
) -> bool {
  match event {
    Some(Ok(TermEvent::Key(key))) => {
      *last_activity = Instant::now();
      *idle_sent = false;
      send_event(tx, Event::Key(key)).await
        && send_event(tx, Event::Render).await
    },
    Some(Ok(_)) => true,
    Some(Err(error)) => {
      let _ = send_event(tx, Event::InputError(error)).await;
      false
    },
    None => {
      let error = io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "terminal input stream closed",
      );
      let _ = send_event(tx, Event::InputError(error)).await;
      false
    },
  }
}

async fn send_event(tx: &mpsc::Sender<Event>, event: Event) -> bool {
  tx.send(event).await.is_ok()
}

#[cfg(test)]
mod tests {
  use std::{io, time::Duration};

  use tokio::{
    sync::mpsc,
    time::{Instant, sleep, timeout},
  };

  use super::{Event, Events, forward_terminal_event};

  async fn wait_for_idle(events: &mut Events, limit: Duration) -> bool {
    timeout(limit, async {
      loop {
        if matches!(events.next().await, Some(Event::Idle)) {
          return;
        }
      }
    })
    .await
    .is_ok()
  }

  #[tokio::test]
  async fn idle_event_is_emitted_once_until_rearmed() {
    let mut events = Events::new();
    events.set_idle_timeout(1);
    events.start();

    assert!(wait_for_idle(&mut events, Duration::from_millis(1_500)).await);
    assert!(!wait_for_idle(&mut events, Duration::from_millis(300)).await);
  }

  #[tokio::test]
  async fn timeout_update_rearms_from_update_time() {
    let mut events = Events::new();
    events.set_idle_timeout(1);
    events.start();
    sleep(Duration::from_millis(700)).await;

    events.set_idle_timeout(1);
    assert!(!wait_for_idle(&mut events, Duration::from_millis(500)).await);
    assert!(wait_for_idle(&mut events, Duration::from_millis(700)).await);
  }

  #[tokio::test]
  async fn terminal_eof_is_forwarded_as_an_input_error() -> io::Result<()> {
    let (tx, mut rx) = mpsc::channel(1);
    let mut last_activity = Instant::now();
    let mut idle_sent = false;

    assert!(
      !forward_terminal_event(None, &tx, &mut last_activity, &mut idle_sent)
        .await
    );
    let event = rx.recv().await.ok_or_else(|| {
      io::Error::new(io::ErrorKind::BrokenPipe, "input error event was lost")
    })?;
    let Event::InputError(error) = event else {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "terminal EOF produced the wrong event",
      ));
    };
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    Ok(())
  }
}

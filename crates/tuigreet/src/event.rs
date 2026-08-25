use std::{
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

#[cfg(not(test))] use crossterm::event::EventStream;
use crossterm::event::{Event as TermEvent, KeyEvent};
use futures::{StreamExt, future::FutureExt};
use tokio::{
  process::Command,
  sync::mpsc::{self, Sender},
};
use tuigreet_types::AuthStatus;

/// Default render tick.
const DEFAULT_FRAME_RATE: f64 = 2.0;

/// Upper clamp on the configurable frame rate.
const MAX_FRAME_RATE: f64 = 1_000.0;

/// Events that drive the UI event loop
pub enum Event {
  /// Keyboard input event
  Key(KeyEvent),

  /// Render frame event
  Render,

  /// Power command to execute
  PowerCommand(Command),

  /// Exit with authentication status
  Exit(AuthStatus),

  /// UI refresh
  Refresh,

  /// Clear screen and refresh UI
  ClearScreen,

  /// Update the render tick rate
  SetFrameRate(f64),
}

/// Event channel for receiving terminal and internal events
pub struct Events {
  rx:         mpsc::Receiver<Event>,
  tx:         mpsc::Sender<Event>,
  /// Render rate in FPS, f64 stored as atomic u64
  frame_rate: Arc<AtomicU64>,
}

fn load_frame_rate(cell: &AtomicU64) -> f64 {
  f64::from_bits(cell.load(Ordering::Relaxed))
}

fn frame_interval(fps: f64) -> Duration {
  Duration::from_secs_f64(1.0 / fps)
}

impl Events {
  /// Create a new event stream with keyboard and render events
  pub async fn new() -> Self {
    let (tx, rx) = mpsc::channel(10);
    let frame_rate = Arc::new(AtomicU64::new(DEFAULT_FRAME_RATE.to_bits()));

    Self { rx, tx, frame_rate }
  }

  /// Start receiving terminal and render events.
  ///
  /// The terminal input stream must only be created after raw mode is enabled.
  /// Creating it earlier can make crossterm's reader panic before it has a
  /// source, leaving the UI without render events.
  pub fn start(&self) {
    tokio::task::spawn({
      let tx = self.tx.clone();
      let frame_rate = self.frame_rate.clone();

      async move {
        #[cfg(not(test))]
        let mut stream = EventStream::new();

        // Dummy stream for tests
        #[cfg(test)]
        let mut stream = futures::stream::pending::<Result<TermEvent, ()>>();

        let mut current_fps = load_frame_rate(&frame_rate);
        let mut render_interval =
          tokio::time::interval(frame_interval(current_fps));

        loop {
          // Pick up fps changes between iterations
          let target = load_frame_rate(&frame_rate);
          if target != current_fps {
            current_fps = target;
            render_interval =
              tokio::time::interval(frame_interval(current_fps));
            render_interval.tick().await;
          }

          // Re-creating the interval means an immediate tick, consume it so we
          // don't render twice
          let render = render_interval.tick();
          let event = stream.next().fuse();

          tokio::select! {
            event = event => {
              if let Some(Ok(TermEvent::Key(event))) = event {
                let _ = tx.send(Event::Key(event)).await;
                let _ = tx.send(Event::Render).await;
              }
            }

            _ = render => { let _ = tx.send(Event::Render).await; },
          }
        }
      }
    });
  }

  /// Update the render tick rate
  pub fn set_frame_rate(&self, fps: f64) {
    if fps <= 0.0 || !fps.is_finite() {
      return;
    }
    let clamped = fps.min(MAX_FRAME_RATE);
    self.frame_rate.store(clamped.to_bits(), Ordering::Relaxed);
  }

  pub async fn next(&mut self) -> Option<Event> {
    self.rx.recv().await
  }

  pub fn sender(&self) -> Sender<Event> {
    self.tx.clone()
  }
}

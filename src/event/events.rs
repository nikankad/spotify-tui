use crate::event::Key;
use crossterm::event;
use std::{sync::mpsc, thread, time::Duration};

#[derive(Debug, Clone, Copy)]
/// Configuration for event handling.
pub struct EventConfig {
  /// The tick rate at which the application will sent an tick event.
  pub tick_rate: Duration,
}

impl Default for EventConfig {
  fn default() -> EventConfig {
    EventConfig {
      tick_rate: Duration::from_millis(250),
    }
  }
}

/// An occurred event.
pub enum Event<I> {
  /// An input event occurred.
  Input(I),
  /// An tick event occurred.
  Tick,
}

/// A bounded input/tick queue fed by one Crossterm polling thread.
pub struct Events {
  rx: mpsc::Receiver<Event<Key>>,
}

impl Events {
  /// Constructs an new instance of `Events` with the default config.
  pub fn new(tick_rate: u64) -> Events {
    Events::with_config(EventConfig {
      tick_rate: Duration::from_millis(tick_rate),
    })
  }

  /// Constructs an new instance of `Events` from given config.
  pub fn with_config(config: EventConfig) -> Events {
    let (event_tx, rx) = mpsc::sync_channel(64);

    thread::spawn(move || {
      while let Ok(ready) = event::poll(config.tick_rate) {
        let input = if ready {
          match event::read() {
            Ok(input) => Some(input),
            Err(_) => break,
          }
        } else {
          None
        };
        if let Some(event::Event::Key(key)) = input {
          if key.kind == event::KeyEventKind::Release {
            continue;
          }
          let key = Key::from(key);

          if event_tx.send(Event::Input(key)).is_err() {
            break;
          }
        }

        match event_tx.try_send(Event::Tick) {
          Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
          Err(mpsc::TrySendError::Disconnected(_)) => break,
        }
      }
    });

    Events { rx }
  }

  /// Attempts to read an event.
  /// This function will block the current thread.
  pub fn next(&self) -> Result<Event<Key>, mpsc::RecvError> {
    self.rx.recv()
  }
}

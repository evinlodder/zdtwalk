use std::time::Duration;

use crossterm::event::EventStream;
use futures::StreamExt;
use tokio::sync::mpsc;

use super::app::Message;

/// Tick interval for UI refresh.
const TICK_RATE: Duration = Duration::from_millis(250);

/// Reads crossterm events on a background task and forwards them as Messages.
pub struct EventLoop {
    tx: mpsc::Sender<Message>,
}

impl EventLoop {
    /// Create a new event loop and its receiving channel.
    pub fn new() -> (Self, mpsc::Receiver<Message>) {
        let (tx, rx) = mpsc::channel(64);
        (Self { tx }, rx)
    }

    /// Run the event loop. This should be spawned on a tokio task.
    pub async fn run(self) {
        let mut reader = EventStream::new();
        let mut tick = tokio::time::interval(TICK_RATE);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if self.tx.send(Message::Tick).await.is_err() {
                        break;
                    }
                }
                event = reader.next() => {
                    match event {
                        Some(Ok(crossterm::event::Event::Key(key))) => {
                            if self.tx.send(Message::Key(key)).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(crossterm::event::Event::Resize(w, h))) => {
                            if self.tx.send(Message::Resize(w, h)).await.is_err() {
                                break;
                            }
                        }
                        Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
            }
        }
    }
}

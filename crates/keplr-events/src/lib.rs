//! Keplr's event bridge: a Rust host talking to a supervised LAML service.
//!
//! The split is deliberate. Terminals, files, and processes live in Rust, where
//! they can be tested and contained. LAML owns what it is good at: fan-out to
//! every window, rooms per pane kind, and timers, in a few lines of code.

pub mod protocol;
pub mod supervisor;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub use protocol::{encode_event, parse_command, Command, Event, TaskState, DEFAULT_PORT};
pub use supervisor::{State, Supervisor, SupervisorConfig};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A websocket connection to the event service, carrying typed frames.
pub struct Bridge {
    socket: Option<Socket>,
}

impl Bridge {
    pub fn new() -> Self {
        Self { socket: None }
    }

    /// Connects to the service, retrying until `timeout`.
    ///
    /// The child process needs a moment between exec and listening, so the first
    /// attempt failing is normal rather than an error.
    pub async fn connect(&mut self, port: u16, timeout: Duration) -> anyhow::Result<()> {
        let url = format!("ws://127.0.0.1:{port}/");
        let deadline = tokio::time::Instant::now() + timeout;
        let mut last = String::new();
        loop {
            match connect_async(url.as_str()).await {
                Ok((socket, _)) => {
                    self.socket = Some(socket);
                    return Ok(());
                }
                Err(error) => {
                    last = error.to_string();
                    if tokio::time::Instant::now() >= deadline {
                        return Err(anyhow::anyhow!(
                            "event service on port {port} never accepted: {last}"
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    /// Pushes one event to the service.
    pub async fn send(&mut self, event: &Event) -> anyhow::Result<()> {
        let socket = self
            .socket
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("not connected to the event service"))?;
        socket
            .send(Message::Text(encode_event(event).into()))
            .await
            .map_err(|error| anyhow::anyhow!("event send failed: {error}"))
    }

    /// Reads the next command, or `None` when the service closed the socket.
    pub async fn recv(&mut self) -> anyhow::Result<Option<Command>> {
        let socket = self
            .socket
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("not connected to the event service"))?;
        loop {
            match socket.next().await {
                Some(Ok(message)) => match Frame::from(message) {
                    Frame::Text(text) => return Ok(Some(parse_command(&text))),
                    Frame::Close => {
                        self.socket = None;
                        return Ok(None);
                    }
                    _ => continue,
                },
                Some(Err(error)) => {
                    self.socket = None;
                    return Err(anyhow::anyhow!("event read failed: {error}"));
                }
                None => {
                    self.socket = None;
                    return Ok(None);
                }
            }
        }
    }

    /// Forgets a broken connection so the next call reconnects.
    pub fn disconnect(&mut self) {
        self.socket = None;
    }
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

/// A WebSocket frame in either direction, so the host can log or test framing
/// without a live service.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

impl From<Message> for Frame {
    fn from(message: Message) -> Self {
        match message {
            Message::Text(text) => Frame::Text(text.to_string()),
            Message::Binary(bytes) => Frame::Binary(bytes.to_vec()),
            Message::Ping(bytes) => Frame::Ping(bytes.to_vec()),
            Message::Pong(bytes) => Frame::Pong(bytes.to_vec()),
            Message::Close(_) => Frame::Close,
            Message::Frame(_) => Frame::Binary(Vec::new()),
        }
    }
}

/// The command a frame carried, if it carried one.
pub fn command_in(frame: &Frame) -> Option<Command> {
    match frame {
        Frame::Text(text) => Some(parse_command(text)),
        _ => None,
    }
}

/// The event a frame carried, if it carried one.
pub fn event_in(frame: &Frame) -> Option<Event> {
    match frame {
        Frame::Text(text) => serde_json::from_str(text).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_frame_becomes_a_command() {
        let frame = Frame::Text(r#"{"cmd":"invalidate","reason":"task.update"}"#.into());
        assert_eq!(
            command_in(&frame),
            Some(Command::Invalidate {
                reason: "task.update".into()
            })
        );
    }

    #[test]
    fn a_binary_frame_is_not_a_command() {
        assert_eq!(command_in(&Frame::Binary(vec![1, 2, 3])), None);
    }

    #[test]
    fn an_event_frame_round_trips() {
        let event = Event::Watch {
            of: "terminal".into(),
        };
        let line = encode_event(&event);
        assert_eq!(event_in(&Frame::Text(line)), Some(event));
    }

    #[test]
    fn a_frame_with_the_wrong_shape_is_not_an_event() {
        assert_eq!(
            event_in(&Frame::Text(r#"{"cmd":"pong","id":1}"#.into())),
            None
        );
    }
}

#[cfg(test)]
mod integration {
    use super::*;

    /// The real LAML service, when the interpreter is installed.
    ///
    /// This is the one test that proves the two halves agree on the protocol. It
    /// skips itself when `laml` is not on the machine, so CI stays green without
    /// the toolchain while a developer with LAML installed runs the real thing.
    #[tokio::test]
    async fn the_laml_service_answers_a_ping() {
        let config = SupervisorConfig::discover();
        if !config.program.exists() && which(&config.program).is_none() {
            eprintln!(
                "skipping: no laml interpreter at {}",
                config.program.display()
            );
            return;
        }
        let mut supervisor = Supervisor::new(config);
        supervisor.poll().expect("laml starts");
        assert_eq!(supervisor.state(), State::Running);

        let mut bridge = Bridge::new();
        bridge
            .connect(DEFAULT_PORT, Duration::from_secs(5))
            .await
            .expect("service accepts a websocket");
        let hello = bridge
            .recv()
            .await
            .expect("hello arrives")
            .expect("a command");
        assert!(matches!(hello, Command::Hello { .. }), "{hello:?}");

        bridge
            .send(&Event::Ping { id: 7 })
            .await
            .expect("ping sends");
        let pong = bridge
            .recv()
            .await
            .expect("pong arrives")
            .expect("a command");
        assert_eq!(pong, Command::Pong { id: 7 });
        supervisor.stop();
    }
}

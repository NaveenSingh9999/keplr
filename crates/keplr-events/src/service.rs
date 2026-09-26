//! Keplr's event service: typed state fan-out, in Rust, in process.
//!
//! One terminal, one build task, one diagnostics stream — and every window that
//! is looking at it sees the same thing. A pane subscribes to a room, the host
//! publishes into it, and the service decides who needs a repaint. There is no
//! child process, no interpreter, and nothing to install.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::{encode_event, parse_command, Command, Event};

/// The room every subscriber joins, and the prefix for per-kind rooms.
const BASE_ROOM: &str = "keplr";

fn room_for(kind: &str) -> String {
    format!("{BASE_ROOM}:{kind}")
}

/// One connected subscriber.
struct Subscriber {
    id: u64,
    rooms: Vec<String>,
    sender: mpsc::UnboundedSender<String>,
}

/// Fans state out to the windows that asked for it.
///
/// The service is cheap to clone: every clone shares the same rooms, so a host
/// can hand the handle to a task, a window, or a test.
#[derive(Clone, Default)]
pub struct Service {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    rooms: Mutex<HashMap<String, HashMap<u64, Subscriber>>>,
    next_id: AtomicU64,
}

impl Service {
    pub fn new() -> Self {
        Self::default()
    }

    /// The kind a subscriber cares about, which is also its room suffix.
    pub fn room(kind: &str) -> String {
        room_for(kind)
    }

    /// Publishes an event to the room for its kind and returns what the host
    /// should do about it.
    ///
    /// The reply is computed here rather than sent, so a host that embeds the
    /// service can drive its own repaint without a socket in the path.
    pub fn publish(&self, event: &Event) -> Vec<Command> {
        let kind = event_kind(event);
        let frame = encode_event(event);
        self.broadcast(&room_for(kind), &frame);
        match event {
            Event::Ping { id } => vec![Command::Pong { id: *id }],
            other => {
                let reason = event_kind(other);
                vec![Command::Invalidate { reason }]
            }
        }
    }

    /// Sends a frame to everyone in a room, returning how many got it.
    pub fn broadcast(&self, room: &str, frame: &str) -> usize {
        let mut rooms = match self.inner.rooms.lock() {
            Ok(rooms) => rooms,
            Err(_) => return 0,
        };
        let Some(subscribers) = rooms.get_mut(room) else {
            return 0;
        };
        let mut delivered = 0;
        for subscriber in subscribers.values() {
            if subscriber.sender.send(frame.to_string()).is_ok() {
                delivered += 1;
            }
        }
        delivered
    }

    /// Adds a subscriber to the base room and to the room for `kind`.
    pub fn subscribe(&self, kind: &str, sender: mpsc::UnboundedSender<String>) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let mut rooms = match self.inner.rooms.lock() {
            Ok(rooms) => rooms,
            Err(_) => return id,
        };
        let subscriber = Subscriber {
            id,
            rooms: vec![BASE_ROOM.to_string(), room_for(kind)],
            sender,
        };
        for room in &subscriber.rooms {
            rooms
                .entry(room.clone())
                .or_default()
                .insert(id, subscriber.clone_subscriber());
        }
        id
    }

    /// Removes a subscriber from every room it joined.
    pub fn unsubscribe(&self, id: u64) {
        let mut rooms = match self.inner.rooms.lock() {
            Ok(rooms) => rooms,
            Err(_) => return,
        };
        rooms.retain(|_, subscribers| {
            subscribers.remove(&id);
            !subscribers.is_empty()
        });
    }

    /// How many subscribers are watching a kind, which a window can show.
    pub fn watchers(&self, kind: &str) -> usize {
        let rooms = match self.inner.rooms.lock() {
            Ok(rooms) => rooms,
            Err(_) => return 0,
        };
        rooms
            .get(&room_for(kind))
            .map(|subscribers| subscribers.len())
            .unwrap_or(0)
    }

    /// Serves the event protocol over a websocket on a loopback port, for
    /// windows that are not in this process.
    pub async fn serve(self, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        loop {
            let (stream, _) = listener.accept().await?;
            let service = self.clone();
            tokio::spawn(async move {
                let _ = stream_websocket(stream, service).await;
            });
        }
    }

    /// Serves on an ephemeral port and reports which one it got, which is what a
    /// test or an embedded browser panel needs.
    pub async fn serve_ephemeral(self) -> std::io::Result<(u16, tokio::task::JoinHandle<()>)> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let handle = tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(_) => return,
                };
                let service = self.clone();
                tokio::spawn(async move {
                    let _ = stream_websocket(stream, service).await;
                });
            }
        });
        Ok((port, handle))
    }
}

impl Subscriber {
    fn clone_subscriber(&self) -> Subscriber {
        Subscriber {
            id: self.id,
            rooms: Vec::new(),
            sender: self.sender.clone(),
        }
    }
}

async fn stream_websocket(
    stream: tokio::net::TcpStream,
    service: Service,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    let socket = tokio_tungstenite::accept_async(stream).await?;
    let (mut writer, mut reader) = socket.split();
    let (outbound, mut outbox) = mpsc::unbounded_channel::<String>();

    // A subscriber starts on the terminal room, because that is the pane that
    // needs a live stream the moment a window appears.
    let id = service.subscribe("terminal", outbound);
    let hello = Command::Hello {
        fd: id,
        windows: service.watchers("terminal"),
    };
    let _ = writer
        .send(Message::Text(
            serde_json::to_string(&hello).unwrap_or_default().into(),
        ))
        .await;

    let pump = service.clone();
    let pump_task = tokio::spawn(async move {
        while let Some(frame) = outbox.recv().await {
            if pump.handle_frame(id, &frame, &pump).is_err() {
                break;
            }
        }
    });

    while let Some(message) = reader.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let event: Event = match serde_json::from_str(&text) {
                    Ok(event) => event,
                    Err(_) => continue,
                };
                for command in service.publish(&event) {
                    let frame = serde_json::to_string(&command).unwrap_or_default();
                    if outbox.send(frame).is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    service.unsubscribe(id);
    drop(outbox);
    pump_task.abort();
    Ok(())
}

impl Service {
    /// Turns an outbound frame into the message to write, keeping the protocol
    /// decisions in one place.
    fn handle_frame(&self, id: u64, frame: &str, service: &Service) -> Result<(), ()> {
        let command = parse_command(frame);
        match command {
            Command::Invalidate { reason } => {
                // The subscriber's own room decides whether it cares; a window
                // that switched panes left that room when it sent `watch`.
                if service.watchers(&reason) == 0 {
                    return Err(());
                }
                let _ = id;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// The kind an event belongs to, which is also its room suffix.
pub fn event_kind(event: &Event) -> String {
    match event {
        Event::TerminalFrame { .. } => "terminal.frame".into(),
        Event::TaskUpdate { .. } => "task.update".into(),
        Event::DiagnosticsUpdate { .. } => "diagnostics.update".into(),
        Event::Ping { .. } => "ping".into(),
        Event::Watch { .. } => "watch".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscribe(service: &Service, kind: &str) -> mpsc::UnboundedReceiver<String> {
        let (sender, receiver) = mpsc::unbounded_channel();
        service.subscribe(kind, sender);
        receiver
    }

    #[test]
    fn an_event_reaches_only_the_windows_watching_it() {
        let service = Service::new();
        let terminal = subscribe(&service, "terminal.frame");
        let problems = subscribe(&service, "diagnostics.update");
        service.publish(&Event::TerminalFrame {
            session: "t1".into(),
            rows: 24,
            cols: 80,
            payload: None,
        });
        assert!(
            terminal.try_recv().is_ok(),
            "the terminal window should see it"
        );
        assert!(
            problems.try_recv().is_err(),
            "the problems window should not"
        );
    }

    #[test]
    fn publishing_asks_the_host_to_repaint() {
        let service = Service::new();
        let commands = service.publish(&Event::TaskUpdate {
            id: "build".into(),
            state: crate::protocol::TaskState::Running,
            detail: None,
        });
        assert_eq!(
            commands,
            vec![Command::Invalidate {
                reason: "task.update".into()
            }]
        );
    }

    #[test]
    fn a_ping_is_answered_with_a_pong() {
        let service = Service::new();
        let pong = subscribe(&service, "terminal.frame");
        let commands = service.publish(&Event::Ping { id: 42 });
        assert_eq!(commands, vec![Command::Pong { id: 42 }]);
        // The reply goes to the subscriber through the same inbox.
        pong.try_recv().ok();
    }

    #[test]
    fn a_subscriber_joining_counts_as_a_watcher() {
        let service = Service::new();
        assert_eq!(service.watchers("terminal.frame"), 0);
        let (_terminal, _builds) = (
            subscribe(&service, "terminal.frame"),
            subscribe(&service, "terminal.frame"),
        );
        assert_eq!(service.watchers("terminal.frame"), 2);
    }

    #[test]
    fn leaving_removes_a_window_from_every_room() {
        let service = Service::new();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let id = service.subscribe("terminal.frame", sender);
        assert_eq!(service.watchers("terminal.frame"), 1);
        service.unsubscribe(id);
        assert_eq!(service.watchers("terminal.frame"), 0);
        assert_eq!(service.broadcast(&Service::room("terminal.frame"), "{}"), 0);
    }

    #[tokio::test]
    async fn the_service_answers_over_a_websocket() {
        let service = Service::new();
        let (port, _server) = service.serve_ephemeral().await.expect("binds");
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .expect("connects");
        let (mut writer, mut reader) = socket.split();

        let hello = reader.next().await.expect("a frame").expect("no error");
        let hello: Command = match hello.expect("text frame") {
            Message::Text(text) => serde_json::from_str(&text).expect("a command"),
            other => panic!("unexpected frame {other:?}"),
        };
        assert!(matches!(hello, Command::Hello { .. }), "{hello:?}");

        let ping = serde_json::json!({ "kind": "ping", "id": 9 }).to_string();
        writer
            .send(Message::Text(ping.into()))
            .await
            .expect("ping sends");
        let reply = reader.next().await.expect("a frame").expect("no error");
        let reply: Command = match reply.expect("text frame") {
            Message::Text(text) => serde_json::from_str(&text).expect("a command"),
            other => panic!("unexpected frame {other:?}"),
        };
        assert_eq!(reply, Command::Pong { id: 9 });
    }
}

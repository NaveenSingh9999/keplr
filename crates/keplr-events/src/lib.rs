//! Keplr's event bus: typed state fan-out between the host and its windows.
//!
//! Terminals, files, and processes live in Rust, where they are testable and
//! contained, and so does the service that fans their state out. A pane
//! subscribes to a room, the host publishes into it, and the service decides who
//! needs a repaint. Windows in the same process talk to the [`Service`] handle
//! directly; a window in another process talks to it over a loopback websocket.

pub mod protocol;
pub mod service;

pub use protocol::{encode_event, parse_command, Command, Event, TaskState, DEFAULT_PORT};
pub use service::{event_kind, Service};

/// The channel a subscription hands back. Re-exported so a window can subscribe
/// without depending on the async runtime the service happens to use.
pub use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

/// Creates the channel a subscription delivers frames on: the service holds the
/// sender, the window drains the receiver.
pub fn channel() -> (UnboundedSender<String>, UnboundedReceiver<String>) {
    tokio::sync::mpsc::unbounded_channel()
}

/// Applies `apply` to every frame queued on a subscription, and reports whether
/// any of them changed something.
///
/// The loop lives here so a window never names the runtime's error type: it
/// drains frames and gets a yes or no.
pub fn drain(
    receiver: &mut UnboundedReceiver<String>,
    mut apply: impl FnMut(&str) -> bool,
) -> bool {
    let mut changed = false;
    loop {
        match receiver.try_recv() {
            Ok(frame) => changed |= apply(&frame),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drain_reports_whether_any_frame_changed_something() {
        let (mut sender, mut receiver) = channel();
        assert!(
            !drain(&mut receiver, |_| true),
            "an empty room changes nothing"
        );
        sender.send("one".to_string()).expect("queued");
        sender.send("\"two\"".to_string()).expect("queued");
        let mut seen = Vec::new();
        assert!(drain(&mut receiver, |frame| {
            seen.push(frame.to_string());
            true
        }));
        assert_eq!(seen, ["one", "\"two\""]);
        assert!(!drain(&mut receiver, |_| true), "drained twice is empty");
    }

    #[test]
    fn a_frame_the_window_ignores_does_not_count_as_a_change() {
        let (mut sender, mut receiver) = channel();
        sender
            .send(encode_event(&Event::Ping { id: 1 }))
            .expect("queued");
        assert!(!drain(&mut receiver, |_| false));
    }

    #[test]
    fn events_round_trip_through_json() {
        let event = Event::TerminalFrame {
            session: "t1".into(),
            rows: 24,
            cols: 80,
            payload: None,
        };
        let line = encode_event(&event);
        assert_eq!(
            serde_json::from_str::<Event>(&line).expect("decodes"),
            event
        );
    }

    #[test]
    fn a_command_uses_the_tag_the_service_speaks() {
        let line = r#"{"cmd":"invalidate","reason":"terminal.frame"}"#;
        assert_eq!(
            parse_command(line),
            Command::Invalidate {
                reason: "terminal.frame".into()
            }
        );
    }

    #[test]
    fn a_broken_frame_becomes_an_error_command() {
        assert!(matches!(parse_command("{not json"), Command::Error { .. }));
        assert!(matches!(
            parse_command(r#"{"cmd":"somethingNew"}"#),
            Command::Error { .. }
        ));
    }

    #[test]
    fn optional_fields_are_omitted_rather_than_null() {
        let line = encode_event(&Event::DiagnosticsUpdate {
            path: "src/lib.rs".into(),
            count: 3,
        });
        assert!(!line.contains("null"), "{line}");
    }

    #[test]
    fn every_event_maps_to_a_room() {
        let cases = [
            (
                Event::TaskUpdate {
                    id: "b".into(),
                    state: TaskState::Failed,
                    detail: None,
                },
                "task.update",
            ),
            (
                Event::Watch {
                    of: "terminal".into(),
                },
                "watch",
            ),
        ];
        for (event, kind) in cases {
            assert_eq!(event_kind(&event), kind);
            assert_eq!(Service::room(kind), format!("keplr:{kind}"));
        }
    }
}

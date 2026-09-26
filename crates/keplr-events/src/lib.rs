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

#[cfg(test)]
mod tests {
    use super::*;

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

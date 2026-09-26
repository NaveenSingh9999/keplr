//! The event protocol Keplr's host and windows speak.
//!
//! The host pushes [`Event`]s; the service answers with [`Command`]s. Both are
//! plain JSON with a tag, so a window in another process needs no schema, and a
//! new variant is a compile error in Rust rather than a silently dropped frame.

use serde::{Deserialize, Serialize};

/// What the host tells the event service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    /// A terminal frame, already shaped by the terminal crate.
    TerminalFrame {
        session: String,
        rows: u16,
        cols: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
    },
    /// A background task changed state.
    TaskUpdate {
        id: String,
        state: TaskState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Diagnostics for a document changed.
    DiagnosticsUpdate { path: String, count: usize },
    /// A liveness probe with a matching [`Command::Pong`].
    Ping { id: u64 },
    /// A window switched to a different pane.
    Watch { of: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Queued,
    Running,
    Passed,
    Failed,
    Canceled,
}

/// What the event service asks the host to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Command {
    /// Repaint: the service forwarded state this window cares about.
    Invalidate { reason: String },
    /// Answer to a [`Event::Ping`].
    Pong { id: u64 },
    /// The window is now watching this pane kind.
    Watching { of: String },
    /// The service does not handle this event kind.
    Ignored { kind: String },
    /// The service rejected the frame.
    Error { detail: String },
    /// Sent once when a window connects.
    Hello { fd: u64, windows: usize },
}

/// The port the service listens on, and the room every window joins.
pub const DEFAULT_PORT: u16 = 18787;

/// Parses one command frame, mapping a malformed frame to a [`Command::Error`]
/// rather than a panic, because the frame comes from another process.
pub fn parse_command(line: &str) -> Command {
    serde_json::from_str(line).unwrap_or_else(|error| Command::Error {
        detail: error.to_string(),
    })
}

/// Encodes an event as one line of JSON.
pub fn encode_event(event: &Event) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| "{\"kind\":\"ping\",\"id\":0}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_json() {
        let event = Event::TaskUpdate {
            id: "build".into(),
            state: TaskState::Running,
            detail: Some("linking".into()),
        };
        let line = encode_event(&event);
        let decoded: Event = serde_json::from_str(&line).expect("event decodes");
        assert_eq!(decoded, event);
    }

    #[test]
    fn a_task_state_is_spelled_in_camel_case() {
        let line = encode_event(&Event::TaskUpdate {
            id: "build".into(),
            state: TaskState::Failed,
            detail: None,
        });
        assert!(line.contains("\"state\":\"failed\""), "{line}");
    }

    #[test]
    fn an_optional_detail_is_left_out() {
        let line = encode_event(&Event::TaskUpdate {
            id: "build".into(),
            state: TaskState::Running,
            detail: None,
        });
        assert!(!line.contains("detail"), "{line}");
    }
}

//! IPC protocol between the daemon and clients (the Cinder UI, `diskgc --json` remote commands).
//!
//! Transport: newline-delimited JSON over a Unix socket (Unix) or a loopback TCP connection
//! authenticated by a token file (Windows). One request per line, one response per line carrying
//! the same `id`; events have no `id` and may arrive at any time after `subscribe`.
//! See `docs/PROTOCOL.md` for the wire format with examples.

use serde::{Deserialize, Serialize};

use crate::model::{Level, Plan, Progress, RunSummary};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub body: RequestBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RequestBody {
    /// Protocol version and daemon version.
    Hello,
    Status,
    /// `refresh: true` forces a new scan; otherwise a report younger than 15 minutes is reused.
    Report {
        #[serde(default)]
        refresh: bool,
    },
    /// Build a plan without executing; the plan is cached on the connection for `execute`.
    Plan {
        level: Option<Level>,
    },
    /// Execute the cached plan (all actions, or only `action_ids`).
    Execute {
        action_ids: Option<Vec<u32>>,
        dry_run: bool,
    },
    /// Plan and execute in one go.
    Clean {
        level: Option<Level>,
        dry_run: bool,
    },
    Stats,
    History {
        limit: usize,
    },
    Memory,
    KillOrphans,
    GetConfig,
    SetConfig {
        toml: String,
    },
    /// Receive progress / snapshot / log events on this connection.
    Subscribe,
    Ping,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", content = "data", rename_all = "snake_case")]
pub enum ResponseBody {
    Ok(serde_json::Value),
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Event {
    Progress(Progress),
    Snapshot {
        used: u64,
        available: u64,
        level: Level,
    },
    Log {
        level: String,
        message: String,
    },
    RunFinished(RunSummary),
    PlanReady(Plan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub daemon_version: String,
    pub pid: u32,
}

impl Response {
    pub fn ok<T: Serialize>(id: u64, value: &T) -> Self {
        Self {
            id,
            body: ResponseBody::Ok(serde_json::to_value(value).unwrap_or(serde_json::Value::Null)),
        }
    }

    pub fn error(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
            body: ResponseBody::Error {
                message: message.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shapes() {
        let r: Request =
            serde_json::from_str(r#"{"id":1,"method":"plan","params":{"level":"low"}}"#).unwrap();
        assert!(matches!(
            r.body,
            RequestBody::Plan {
                level: Some(Level::Low)
            }
        ));
        let r: Request = serde_json::from_str(r#"{"id":2,"method":"status"}"#).unwrap();
        assert!(matches!(r.body, RequestBody::Status));
        let s = serde_json::to_string(&Response::error(2, "nope")).unwrap();
        assert_eq!(s, r#"{"id":2,"result":"error","data":{"message":"nope"}}"#);
    }
}

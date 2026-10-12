//! Minimal synchronous MessagePack-RPC client for an embedded Neovim.
//!
//! A reader thread decodes everything Neovim writes and forwards it over a
//! channel, so the main thread can either block on one response with a
//! deadline or drain notifications without blocking — the same shape as the
//! forge `poll_*_events` pattern. No async runtime is involved.

use std::io::{self, BufReader, BufWriter, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rmpv::Value;

/// One decoded message from Neovim.
#[derive(Debug)]
pub enum Incoming {
    Response {
        id: u64,
        result: Result<Value, String>,
    },
    Notification {
        method: String,
        params: Vec<Value>,
    },
    /// Neovim closed its stdout: the process exited or crashed.
    Closed,
}

type SharedWriter = Arc<Mutex<BufWriter<ChildStdin>>>;

pub struct RpcClient {
    writer: SharedWriter,
    incoming: Receiver<Incoming>,
    next_id: u64,
    /// The reader thread is gone; `Closed` has been delivered once already.
    disconnected: bool,
}

impl RpcClient {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let writer: SharedWriter = Arc::new(Mutex::new(BufWriter::new(stdin)));
        let (tx, rx) = mpsc::channel();
        let reply_writer = Arc::clone(&writer);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(value) = rmpv::decode::read_value(&mut reader) {
                let Some(message) = parse_message(value, &reply_writer) else {
                    continue;
                };
                if tx.send(message).is_err() {
                    return;
                }
            }
            let _ = tx.send(Incoming::Closed);
        });
        Self {
            writer,
            incoming: rx,
            next_id: 0,
            disconnected: false,
        }
    }

    /// Send a request and return its id; the response arrives via `recv`.
    pub fn request(&mut self, method: &str, params: Vec<Value>) -> io::Result<u64> {
        self.next_id += 1;
        let id = self.next_id;
        write_message(
            &self.writer,
            &Value::Array(vec![
                0.into(),
                id.into(),
                method.into(),
                Value::Array(params),
            ]),
        )?;
        Ok(id)
    }

    pub fn notify(&mut self, method: &str, params: Vec<Value>) -> io::Result<()> {
        write_message(
            &self.writer,
            &Value::Array(vec![2.into(), method.into(), Value::Array(params)]),
        )
    }

    /// Next message, waiting until `deadline` at most; `None` on timeout or
    /// once the channel is closed.
    pub fn recv_until(&mut self, deadline: Instant) -> Option<Incoming> {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match self.incoming.recv_timeout(timeout) {
            Ok(message) => Some(message),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => self.disconnected(),
        }
    }

    /// Next already-queued message, without waiting.
    pub fn try_recv(&mut self) -> Option<Incoming> {
        match self.incoming.try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => self.disconnected(),
        }
    }

    /// The reader thread sends `Closed` before it exits; this covers it dying
    /// some other way, and reports the close only once so callers that drain
    /// until `None` terminate.
    fn disconnected(&mut self) -> Option<Incoming> {
        if std::mem::replace(&mut self.disconnected, true) {
            None
        } else {
            Some(Incoming::Closed)
        }
    }
}

fn write_message(writer: &SharedWriter, message: &Value) -> io::Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| io::Error::other("nvim writer lock poisoned"))?;
    rmpv::encode::write_value(&mut *writer, message).map_err(io::Error::other)?;
    writer.flush()
}

fn parse_message(value: Value, writer: &SharedWriter) -> Option<Incoming> {
    let Value::Array(mut parts) = value else {
        return None;
    };
    match parts.first().and_then(Value::as_u64)? {
        // [1, id, error, result]
        1 if parts.len() == 4 => {
            let result = parts.pop()?;
            let error = parts.pop()?;
            let id = parts.get(1).and_then(Value::as_u64)?;
            let result = if error.is_nil() {
                Ok(result)
            } else {
                Err(error_text(&error))
            };
            Some(Incoming::Response { id, result })
        }
        // [2, method, params]
        2 if parts.len() == 3 => {
            let params = match parts.pop()? {
                Value::Array(params) => params,
                _ => Vec::new(),
            };
            let method = parts.pop()?.as_str()?.to_string();
            Some(Incoming::Notification { method, params })
        }
        // [0, id, method, params]: tuicr serves no RPC methods. Reply anyway
        // so a user config that calls `rpcrequest()` on this channel does not
        // leave Neovim blocked forever.
        0 => {
            let id = parts.get(1).and_then(Value::as_u64)?;
            let _ = write_message(
                writer,
                &Value::Array(vec![
                    1.into(),
                    id.into(),
                    "tuicr does not handle RPC requests".into(),
                    Value::Nil,
                ]),
            );
            None
        }
        _ => None,
    }
}

/// Neovim reports errors as `[type, message]`.
fn error_text(error: &Value) -> String {
    match error {
        Value::Array(parts) => parts
            .get(1)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string()),
        Value::String(text) => text.as_str().unwrap_or_default().to_string(),
        other => other.to_string(),
    }
}

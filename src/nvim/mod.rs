//! Embedded Neovim (`nvim --embed --headless`) as the comment box's vim engine.
//!
//! Neovim owns editing — modes, operators, registers, macros, undo, ex
//! commands, and the user's own config and mappings. tuicr keeps rendering
//! the box: after every forwarded key it reads back the buffer text, cursor,
//! mode, visual selection, and command line (`get_state.lua`) and syncs them
//! into `App::comment_buffer`/`comment_cursor`, which stay canonical.
//!
//! The protocol is synchronous where Neovim allows it: `nvim_input` only
//! queues keys, and Neovim consumes queued input before it services the next
//! non-fast request, so one state request issued after the input observes its
//! effect. Neovim only services such requests from its top-level mode loops,
//! though: while a command waits for another key (`q`, `f`, `r`, a partial
//! mapping such as `jk`, a `:s///c` prompt) the request is deferred, and
//! `nvim_get_mode` reports `blocking`. Then the key returns after a short grace
//! period and the state lands later through `poll`, as does anything changing
//! on its own (a mapping resolving after 'timeoutlen'), which Neovim announces
//! with a `tuicr_dirty` notification.

mod keys;
mod rpc;

use std::io::{self, Read};
use std::ops::Range;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::KeyEvent;
use rmpv::Value;

pub use keys::to_nvim_input;
use rpc::{Incoming, RpcClient};

const SETUP_LUA: &str = include_str!("lua/setup.lua");
const BEGIN_LUA: &str = include_str!("lua/begin.lua");
const GET_STATE_LUA: &str = include_str!("lua/get_state.lua");

/// Startup includes sourcing the user's config.
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const CALL_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a forwarded key may block the UI waiting for the resulting state.
/// Past this the state is applied whenever it arrives.
const SYNC_TIMEOUT: Duration = Duration::from_millis(200);
/// The wait when Neovim reports it is blocked waiting for input. The key just
/// sent usually resolves that within a millisecond; when it does not (a
/// partial mapping), the state cannot come before the next key anyway.
const BLOCKED_GRACE: Duration = Duration::from_millis(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);
const STDERR_TAIL_BYTES: usize = 4000;

#[derive(Debug, thiserror::Error)]
pub enum NvimError {
    #[error("`{0}` was not found on PATH")]
    NotFound(String),
    #[error("failed to start Neovim: {0}")]
    Spawn(io::Error),
    #[error("Neovim exited{0}")]
    Exited(String),
    #[error("Neovim did not respond within {0:?}")]
    Timeout(Duration),
    #[error("{0}")]
    Rpc(String),
}

#[derive(Debug, Clone)]
pub struct NvimOptions {
    pub program: String,
    /// Extra arguments ahead of `--embed --headless` (tests pass `--clean`).
    pub args: Vec<String>,
    /// Spaces per indent level in the comment buffer.
    pub tab_width: usize,
}

impl Default for NvimOptions {
    fn default() -> Self {
        Self {
            program: "nvim".to_string(),
            args: Vec::new(),
            tab_width: 4,
        }
    }
}

/// Coarse mode, derived from the raw `nvim_get_mode().mode` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NvimMode {
    #[default]
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    Select,
    Command,
    OperatorPending,
    /// Hit-enter, more, or confirm prompt.
    Prompt,
    Terminal,
}

impl NvimMode {
    pub fn from_raw(raw: &str) -> Self {
        if raw.starts_with("no") {
            return Self::OperatorPending;
        }
        // `ni*`: one Normal command run from Insert mode via <C-o>.
        if let Some(rest) = raw.strip_prefix("ni") {
            return if rest == "I" {
                Self::Insert
            } else {
                Self::Replace
            };
        }
        match raw.chars().next() {
            Some('i') => Self::Insert,
            Some('R') => Self::Replace,
            Some('v') => Self::Visual,
            Some('V') => Self::VisualLine,
            Some('\u{16}') => Self::VisualBlock,
            Some('s' | 'S' | '\u{13}') => Self::Select,
            Some('c') => Self::Command,
            Some('r') => Self::Prompt,
            Some('t') => Self::Terminal,
            _ => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Replace => "REPLACE",
            Self::Visual => "VISUAL",
            Self::VisualLine => "V-LINE",
            Self::VisualBlock => "V-BLOCK",
            Self::Select => "SELECT",
            Self::Command => "COMMAND",
            Self::OperatorPending => "O-PENDING",
            Self::Prompt => "PROMPT",
            Self::Terminal => "TERMINAL",
        }
    }
}

/// The comment buffer as Neovim last reported it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NvimState {
    pub text: String,
    /// UTF-8 byte offset of the cursor into `text`.
    pub cursor: usize,
    pub mode: NvimMode,
    /// Normal mode with no count, operator, or partial mapping pending.
    pub plain_normal: bool,
    /// Command-line contents including its type character (`:`, `/`, `?`).
    pub cmdline: Option<String>,
    /// Visually selected byte ranges of `text`, one per line.
    pub selection: Vec<Range<usize>>,
    /// Register being recorded into (`q{reg}`).
    pub recording: Option<char>,
}

impl NvimState {
    /// Parse the map returned by `get_state.lua`.
    fn from_value(value: &Value) -> Option<Self> {
        let map = value.as_map()?;
        let field = |name: &str| {
            map.iter()
                .find(|(key, _)| key.as_str() == Some(name))
                .map(|(_, value)| value)
        };
        let lines: Vec<&str> = field("lines")?
            .as_array()?
            .iter()
            .map(|line| line.as_str().unwrap_or_default())
            .collect();
        let row = field("row").and_then(Value::as_u64).unwrap_or(0) as usize;
        let col = field("col").and_then(Value::as_u64).unwrap_or(0) as usize;
        let selection = field("selection")
            .and_then(Value::as_array)
            .map(|spans| {
                spans
                    .iter()
                    .filter_map(|span| {
                        let span = span.as_array()?;
                        let at = |i: usize| span.get(i).and_then(Value::as_i64);
                        Some((at(0)?, at(1)?, at(2)?))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(Self {
            text: lines.join("\n"),
            cursor: line_col_to_byte(&lines, row, col),
            mode: NvimMode::from_raw(field("mode").and_then(Value::as_str).unwrap_or("n")),
            plain_normal: field("plain_normal")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            cmdline: field("cmdline").and_then(Value::as_str).map(str::to_string),
            selection: selection_ranges(&lines, &selection),
            recording: field("recording")
                .and_then(Value::as_str)
                .and_then(|reg| reg.chars().next()),
        })
    }
}

/// Something the app has to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvimEvent {
    /// `state()` changed.
    State,
    /// `:w`: save the comment with this text.
    Save(String),
    /// Neovim is exiting (`:q`, `:wq`, `ZZ`, ...): cancel the comment.
    Quit,
    /// A message Neovim would have shown on its command line.
    Message { kind: String, text: String },
    /// The process is gone. `expected` after a `Quit`; otherwise a crash.
    Exited { expected: bool, detail: String },
}

impl NvimEvent {
    fn message(kind: &str, text: impl Into<String>) -> Self {
        Self::Message {
            kind: kind.to_string(),
            text: text.into(),
        }
    }
}

/// One `nvim --embed --headless` process and its comment buffer.
pub struct NvimHost {
    child: Child,
    rpc: RpcClient,
    tab_width: usize,
    ready: bool,
    state: NvimState,
    /// In-flight `get_state` request. At most one is outstanding: a change
    /// Neovim announces before answering it is already in the answer.
    state_request: Option<u64>,
    /// Keys were sent while `state_request` was in flight, so its answer may
    /// predate them; ask again once it arrives.
    state_stale: bool,
    events: Vec<NvimEvent>,
    quitting: bool,
    exited: bool,
    stderr_tail: Arc<Mutex<String>>,
}

impl NvimHost {
    /// Start Neovim without waiting for it: `--headless` lets it source the
    /// user's config in the background while tuicr keeps running.
    pub fn spawn(options: &NvimOptions) -> Result<Self, NvimError> {
        let mut child = Command::new(&options.program)
            .args(&options.args)
            .args(["--embed", "--headless"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                if err.kind() == io::ErrorKind::NotFound {
                    NvimError::NotFound(options.program.clone())
                } else {
                    NvimError::Spawn(err)
                }
            })?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            // Keep the pipe drained (a full pipe would stall Neovim) and the
            // tail around for reporting startup failures.
            let tail = Arc::clone(&stderr_tail);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 1024];
                while let Ok(read) = stderr.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_str(&String::from_utf8_lossy(&chunk[..read]));
                        if tail.len() > STDERR_TAIL_BYTES {
                            let mut cut = tail.len() - STDERR_TAIL_BYTES;
                            while !tail.is_char_boundary(cut) {
                                cut += 1;
                            }
                            tail.drain(..cut);
                        }
                    }
                }
            });
        }
        Ok(Self {
            child,
            rpc: RpcClient::new(stdin, stdout),
            tab_width: options.tab_width,
            ready: false,
            state: NvimState::default(),
            state_request: None,
            state_stale: false,
            events: Vec::new(),
            quitting: false,
            exited: false,
            stderr_tail,
        })
    }

    pub fn is_alive(&self) -> bool {
        !self.exited
    }

    pub fn state(&self) -> &NvimState {
        &self.state
    }

    pub fn take_events(&mut self) -> Vec<NvimEvent> {
        std::mem::take(&mut self.events)
    }

    /// A state request is outstanding: `state()` may not reflect every key
    /// sent yet.
    pub fn is_syncing(&self) -> bool {
        self.state_request.is_some()
    }

    /// Start editing `text` in Insert mode with the cursor at byte `cursor`.
    /// Blocks until Neovim is ready, which on first use includes its startup.
    pub fn begin(&mut self, text: &str, cursor: usize) -> Result<(), NvimError> {
        self.ensure_ready()?;
        // Leave whatever mode the previous comment ended in.
        self.input(r"<C-\><C-n>")?;
        let (lines, row, col) = byte_to_line_col(text, cursor);
        let lines = Value::Array(lines.into_iter().map(Value::from).collect());
        let append = self.call(
            "nvim_exec_lua",
            vec![
                BEGIN_LUA.into(),
                Value::Array(vec![lines, (row as u64).into(), (col as u64).into()]),
            ],
            CALL_TIMEOUT,
        )?;
        // A save or quit raised before the new text was in place belongs to
        // the previous comment (an autosave plugin reacting to the mode
        // change above, say) and must not act on this one.
        self.events
            .retain(|event| !matches!(event, NvimEvent::Save(_) | NvimEvent::Quit));
        // `<Cmd>` runs the command without consulting the user's mappings of
        // `i`/`A`, and as input it is ordered before the state request below.
        self.input(if append.as_bool() == Some(true) {
            "<Cmd>startinsert!<CR>"
        } else {
            "<Cmd>startinsert<CR>"
        })?;
        // Starting a comment may wait longer than a keystroke: the box should
        // open showing Insert mode, not catch up a tick later.
        self.sync(CALL_TIMEOUT);
        Ok(())
    }

    /// Forward a key and wait (briefly) for the state it produces.
    pub fn send_key(&mut self, key: KeyEvent) {
        if let Some(keys) = to_nvim_input(key) {
            self.send_keys(&keys);
        }
    }

    /// Forward keys in `nvim_input()` notation, then sync.
    pub fn send_keys(&mut self, keys: &str) {
        if self.input(keys).is_ok() {
            self.sync(SYNC_TIMEOUT);
        }
    }

    /// Paste text the way a terminal paste would (`vim.paste`).
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let params = vec![text.into(), true.into(), (-1).into()];
        if self.call("nvim_paste", params, CALL_TIMEOUT).is_ok() {
            self.sync(SYNC_TIMEOUT);
        }
    }

    /// Process whatever Neovim sent since the last call, without blocking.
    pub fn poll(&mut self) {
        while let Some(message) = self.rpc.try_recv() {
            self.handle(message);
        }
    }

    fn ensure_ready(&mut self) -> Result<(), NvimError> {
        if self.exited {
            return Err(NvimError::Exited(self.exit_detail()));
        }
        if self.ready {
            return Ok(());
        }
        let info = self.call("nvim_get_api_info", Vec::new(), READY_TIMEOUT)?;
        let channel = info
            .as_array()
            .and_then(|info| info.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| NvimError::Rpc("nvim_get_api_info returned no channel".into()))?;
        self.call(
            "nvim_exec_lua",
            vec![
                SETUP_LUA.into(),
                Value::Array(vec![channel.into(), (self.tab_width as u64).into()]),
            ],
            READY_TIMEOUT,
        )?;
        self.ready = true;
        Ok(())
    }

    fn input(&mut self, keys: &str) -> Result<(), NvimError> {
        // A notification: `nvim_input` is fast and only queues the keys, so
        // there is nothing worth waiting for.
        self.rpc
            .notify("nvim_input", vec![keys.into()])
            .map_err(|err| self.write_failed(err))
    }

    /// Bring `state` up to date with everything sent so far, waiting at most
    /// `timeout` (or `BLOCKED_GRACE` once Neovim reports it is blocked).
    fn sync(&mut self, timeout: Duration) {
        if !self.ready || self.exited {
            return;
        }
        self.request_state(true);
        // Sent right behind the state request: when Neovim is blocked it
        // answers at once, otherwise only after the state request.
        let mode_request = self.rpc.request("nvim_get_mode", Vec::new()).ok();
        let mut deadline = Instant::now() + timeout;
        while self.state_request.is_some() {
            let Some(message) = self.rpc.recv_until(deadline) else {
                break;
            };
            match message {
                Incoming::Response { id, result } if Some(id) == mode_request => {
                    if result.ok().as_ref().is_some_and(is_blocking_mode) {
                        deadline = deadline.min(Instant::now() + BLOCKED_GRACE);
                    }
                }
                other => self.handle(other),
            }
        }
    }

    /// Ask for a fresh state unless one is already in flight. `after_input`
    /// marks the in-flight answer as possibly predating keys just sent.
    fn request_state(&mut self, after_input: bool) {
        if self.state_request.is_some() {
            self.state_stale |= after_input;
            return;
        }
        if !self.ready || self.exited {
            return;
        }
        self.state_stale = false;
        let params = vec![GET_STATE_LUA.into(), Value::Array(Vec::new())];
        match self.rpc.request("nvim_exec_lua", params) {
            Ok(id) => self.state_request = Some(id),
            Err(err) => {
                let _ = self.write_failed(err);
            }
        }
    }

    /// Blocking request; other traffic that arrives meanwhile is handled.
    fn call(
        &mut self,
        method: &str,
        params: Vec<Value>,
        timeout: Duration,
    ) -> Result<Value, NvimError> {
        if self.exited {
            return Err(NvimError::Exited(self.exit_detail()));
        }
        let id = self
            .rpc
            .request(method, params)
            .map_err(|err| self.write_failed(err))?;
        let deadline = Instant::now() + timeout;
        loop {
            let Some(message) = self.rpc.recv_until(deadline) else {
                return Err(NvimError::Timeout(timeout));
            };
            match message {
                Incoming::Response { id: got, result } if got == id => {
                    return result.map_err(NvimError::Rpc);
                }
                other => {
                    self.handle(other);
                    if self.exited {
                        return Err(NvimError::Exited(self.exit_detail()));
                    }
                }
            }
        }
    }

    fn handle(&mut self, message: Incoming) {
        match message {
            Incoming::Response { id, result } => {
                if self.state_request != Some(id) {
                    return; // late answer to a call that already timed out
                }
                self.state_request = None;
                match result {
                    Ok(value) => {
                        if let Some(state) = NvimState::from_value(&value)
                            && state != self.state
                        {
                            self.state = state;
                            self.events.push(NvimEvent::State);
                        }
                    }
                    Err(err) => self.events.push(NvimEvent::message("emsg", err)),
                }
                if self.state_stale {
                    self.request_state(false);
                }
            }
            Incoming::Notification { method, params } => match method.as_str() {
                "tuicr_dirty" => self.request_state(false),
                "tuicr_save" => {
                    let lines: Vec<&str> = params
                        .first()
                        .and_then(Value::as_array)
                        .map(|lines| lines.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default();
                    self.events.push(NvimEvent::Save(lines.join("\n")));
                }
                "tuicr_quit" => {
                    self.quitting = true;
                    self.events.push(NvimEvent::Quit);
                }
                "tuicr_message" => {
                    let text = |i: usize| {
                        params
                            .get(i)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    self.events.push(NvimEvent::message(&text(0), text(1)));
                }
                "nvim_error_event" => {
                    let text = params
                        .get(1)
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                        .to_string();
                    self.events.push(NvimEvent::message("emsg", text));
                }
                _ => {}
            },
            Incoming::Closed => {
                if !self.exited {
                    self.exited = true;
                    self.state_request = None;
                    let _ = self.child.try_wait();
                    self.events.push(NvimEvent::Exited {
                        expected: self.quitting,
                        detail: self.exit_detail(),
                    });
                }
            }
        }
    }

    fn write_failed(&mut self, err: io::Error) -> NvimError {
        // A broken pipe means the process is gone; the reader thread reports
        // the exit itself, so pick that up now.
        while let Some(message) = self.rpc.try_recv() {
            self.handle(message);
        }
        NvimError::Rpc(format!("failed to write to Neovim: {err}"))
    }

    fn exit_detail(&self) -> String {
        let tail = self
            .stderr_tail
            .lock()
            .map(|tail| tail.trim().to_string())
            .unwrap_or_default();
        if tail.is_empty() {
            String::new()
        } else {
            format!(": {tail}")
        }
    }
}

impl Drop for NvimHost {
    fn drop(&mut self) {
        if !self.exited {
            // `qa!` rather than a kill so Neovim still writes its ShaDa
            // (registers, history) like any other session.
            let _ = self.rpc.notify("nvim_command", vec!["qa!".into()]);
        }
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn is_blocking_mode(mode: &Value) -> bool {
    mode.as_map()
        .and_then(|map| {
            map.iter()
                .find(|(key, _)| key.as_str() == Some("blocking"))
                .and_then(|(_, value)| value.as_bool())
        })
        .unwrap_or(false)
}

/// Split `text` into lines and locate byte offset `cursor` as (row, byte col).
fn byte_to_line_col(text: &str, cursor: usize) -> (Vec<&str>, usize, usize) {
    let cursor = cursor.min(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    let mut start = 0;
    for (row, line) in lines.iter().enumerate() {
        let end = start + line.len();
        if cursor <= end {
            return (lines, row, cursor - start);
        }
        start = end + 1;
    }
    let last = lines.len() - 1;
    let col = lines[last].len();
    (lines, last, col)
}

/// Byte offset of (row, byte col) in the newline-joined `lines`, clamped.
fn line_col_to_byte(lines: &[&str], row: usize, col: usize) -> usize {
    let row = row.min(lines.len().saturating_sub(1));
    let start: usize = lines[..row].iter().map(|line| line.len() + 1).sum();
    let line = lines.get(row).copied().unwrap_or_default();
    let mut col = col.min(line.len());
    while !line.is_char_boundary(col) {
        col -= 1;
    }
    start + col
}

/// Turn `(row, start_col, end_col)` spans into byte ranges of the joined text,
/// clamped to each line (the newline itself is never highlighted) and widened
/// to char boundaries so the renderer can slice on them.
fn selection_ranges(lines: &[&str], spans: &[(i64, i64, i64)]) -> Vec<Range<usize>> {
    spans
        .iter()
        .filter_map(|&(row, start, end)| {
            let row = usize::try_from(row).ok()?;
            let line = lines.get(row)?;
            let line_start: usize = lines[..row].iter().map(|line| line.len() + 1).sum();
            let clamp = |col: i64| (col.max(0) as usize).min(line.len());
            let (mut start, mut end) = (clamp(start), clamp(end));
            while !line.is_char_boundary(start) {
                start -= 1;
            }
            while !line.is_char_boundary(end) {
                end += 1;
            }
            (start < end).then(|| line_start + start..line_start + end)
        })
        .collect()
}

#[cfg(test)]
mod tests;

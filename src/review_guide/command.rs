//! Runs the configured review guide command.
//!
//! The request JSON goes to the child's stdin and its stdout is the response.
//! Nothing here knows about a model, a provider, or a harness.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Environment variable holding the same text as the request's `instructions`.
pub const PROMPT_ENV: &str = "TUICR_REVIEW_GUIDE_PROMPT";

const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// How long the output pipes may stay open after the command exits.
/// A grandchild that outlives the command can hold them open indefinitely.
const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// A guide is a few kilobytes; anything near this is not one.
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ERROR_DETAIL_CHARS: usize = 300;

pub(super) const CANCELLED: &str = "cancelled";

/// Run `argv` with `stdin` piped in and return its stdout.
/// Kills the command when `timeout` passes or `cancel` is set.
pub(super) fn run(
    argv: &[String],
    stdin: String,
    prompt: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let (program, args) = argv.split_first().ok_or("review_guide.command is empty")?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env(PROMPT_ENV, prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own process group, so a wrapper script's children die with it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("command not found: {program}"),
        _ => format!("failed to start {program}: {e}"),
    })?;

    // A command that never reads stdin must not block the wait loop,
    // and one that exits early closes the pipe: both are its business.
    if let Some(mut pipe) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = pipe.write_all(stdin.as_bytes());
        });
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                kill(&mut child);
                return Err(format!("failed to wait for {program}: {e}"));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            kill(&mut child);
            return Err(CANCELLED.to_string());
        }
        if Instant::now() >= deadline {
            kill(&mut child);
            return Err(format!(
                "{program} did not answer within {}s",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let stdout = collect(&stdout);
    let stderr = collect(&stderr);
    if !status.success() {
        let detail = error_detail(&stderr)
            .or_else(|| error_detail(&stdout))
            .map(|detail| format!(": {detail}"))
            .unwrap_or_default();
        return Err(format!("{program} failed ({status}){detail}"));
    }
    if stdout.trim().is_empty() {
        return Err(format!("{program} printed nothing"));
    }
    Ok(stdout)
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(pipe) = pipe {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.take(MAX_OUTPUT_BYTES).read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        });
    }
    rx
}

fn collect(rx: &Receiver<Vec<u8>>) -> String {
    match rx.recv_timeout(DRAIN_GRACE) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => String::new(),
    }
}

fn kill(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: plain syscall; `pgid` is the group `run` created for this child.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The last non-empty line of a failed command's output, shortened.
fn error_detail(output: &str) -> Option<String> {
    let line = output.lines().rev().find(|line| !line.trim().is_empty())?;
    let line = line.trim();
    Some(if line.chars().count() > MAX_ERROR_DETAIL_CHARS {
        let head: String = line.chars().take(MAX_ERROR_DETAIL_CHARS).collect();
        format!("{head}…")
    } else {
        line.to_string()
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), script.to_string()]
    }

    fn run_sh(script: &str, stdin: &str, timeout: Duration) -> Result<String, String> {
        run(
            &sh(script),
            stdin.to_string(),
            "the prompt",
            timeout,
            &AtomicBool::new(false),
        )
    }

    #[test]
    fn should_pipe_stdin_through_and_return_stdout() {
        let out = run_sh("cat", "request body", Duration::from_secs(10)).unwrap();
        assert_eq!(out, "request body");
    }

    #[test]
    fn should_export_the_prompt_to_the_command() {
        let out = run_sh(
            "printf %s \"$TUICR_REVIEW_GUIDE_PROMPT\"",
            "",
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out, "the prompt");
    }

    #[test]
    fn should_report_the_last_stderr_line_when_the_command_fails() {
        let err = run_sh(
            "echo noise >&2; echo 'no credentials' >&2; exit 3",
            "",
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert!(err.contains("no credentials"), "{err}");
        assert!(!err.contains("noise"), "{err}");
        assert!(err.contains('3'), "{err}");
    }

    #[test]
    fn should_report_a_missing_command() {
        let err = run(
            &["tuicr-no-such-command".to_string()],
            String::new(),
            "",
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(err, "command not found: tuicr-no-such-command");
    }

    #[test]
    fn should_kill_a_command_that_outlives_the_timeout() {
        let started = Instant::now();
        let err = run_sh("sleep 30", "", Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("did not answer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn should_stop_a_cancelled_command() {
        let started = Instant::now();
        let err = run(
            &sh("sleep 30"),
            String::new(),
            "",
            Duration::from_secs(30),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(err, CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn should_not_block_on_a_command_that_ignores_a_large_stdin() {
        let big = "x".repeat(1024 * 1024);
        let out = run_sh("echo done", &big, Duration::from_secs(10)).unwrap();
        assert_eq!(out.trim(), "done");
    }

    #[test]
    fn should_reject_empty_output() {
        let err = run_sh("true", "", Duration::from_secs(10)).unwrap_err();
        assert!(err.contains("printed nothing"), "{err}");
    }
}

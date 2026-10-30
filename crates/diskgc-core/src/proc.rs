//! External commands with a deadline. Nothing the daemon runs may stall a run: `uv cache clean`
//! blocks for as long as another uv process holds the cache lock, and the `docker` CLI waits
//! indefinitely for a VM that is starting, stuck or asleep.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// After the child exits, how long to wait for the rest of its output. A grandchild that outlives
/// it (a helper the CLI started in the background) can keep the pipes open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Run `argv`, capturing stdout and stderr; kill the child and return `TimedOut` after
/// `timeout`. Both pipes are drained on their own threads, so a chatty child cannot block on a
/// full pipe while we wait for it to exit.
pub fn output(argv: &[&str], timeout: Duration) -> std::io::Result<Output> {
    let (cmd, args) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty command"))?;
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            let _ = tx.send(buf);
        });
        rx
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "`{}` timed out after {} s",
                    argv.join(" "),
                    timeout.as_secs()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let deadline = Instant::now() + DRAIN_GRACE;
    let rest = |rx: mpsc::Receiver<Vec<u8>>| {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_default()
    };
    Ok(Output {
        status,
        stdout: rest(stdout),
        stderr: rest(stderr),
    })
}

/// stdout of a successful run, `None` on failure or timeout.
pub fn stdout(argv: &[&str], timeout: Duration) -> Option<String> {
    let out = output(argv, timeout).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output() {
        let out = stdout(
            &["sh", "-c", "echo hi; echo err >&2"],
            Duration::from_secs(5),
        );
        assert_eq!(out.as_deref(), Some("hi\n"));
    }

    #[test]
    fn kills_a_child_that_overruns() {
        let started = Instant::now();
        let err = output(&["sleep", "5"], Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_grandchild_holding_the_pipe_does_not_hang_the_caller() {
        let started = Instant::now();
        let out = output(
            &["sh", "-c", "sleep 30 & echo done"],
            Duration::from_secs(20),
        )
        .unwrap();
        assert!(out.status.success());
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn large_output_does_not_deadlock() {
        // 1 MB on stdout: more than a pipe buffer holds.
        let out = stdout(
            &["sh", "-c", "head -c 1048576 /dev/zero | tr '\\0' x"],
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out.len(), 1_048_576);
    }
}

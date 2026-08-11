//! Bounded subprocess execution with captured output and group-level cleanup.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
#[cfg(unix)]
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::sync::{
    Mutex, MutexGuard,
    mpsc::{self, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
// Each stream is retained only up to this limit; readers keep draining after it
// so a verbose child cannot block on a full pipe.
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;

#[cfg(unix)]
static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
#[cfg(unix)]
static DEFAULT_ENABLED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
#[cfg(unix)]
static SIGNAL_HANDLERS: OnceLock<Result<(), String>> = OnceLock::new();
static SUPERVISION_LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
fn install_interrupt_handlers() -> Result<(), Error> {
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};

    let interrupted = INTERRUPTED.get_or_init(|| Arc::new(AtomicBool::new(false)));
    let default_enabled = DEFAULT_ENABLED.get_or_init(|| Arc::new(AtomicBool::new(true)));
    let installed = SIGNAL_HANDLERS.get_or_init(|| {
        for signal in [SIGHUP, SIGINT, SIGTERM] {
            signal_hook::flag::register_conditional_default(signal, Arc::clone(default_enabled))
                .map_err(|error| format!("could not install default signal handler: {error}"))?;
            signal_hook::flag::register(signal, Arc::clone(interrupted))
                .map_err(|error| format!("could not install subprocess signal handler: {error}"))?;
        }
        Ok(())
    });
    installed.clone().map_err(Error::msg)
}

#[cfg(not(unix))]
fn install_interrupt_handlers() -> Result<(), Error> {
    Ok(())
}

#[cfg(unix)]
fn interrupted() -> bool {
    INTERRUPTED
        .get()
        .is_some_and(|flag| flag.load(AtomicOrdering::SeqCst))
}

#[cfg(not(unix))]
fn interrupted() -> bool {
    false
}

struct SupervisionGuard {
    _lock: MutexGuard<'static, ()>,
}

impl Drop for SupervisionGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(default_enabled) = DEFAULT_ENABLED.get() {
            default_enabled.store(true, AtomicOrdering::SeqCst);
        }
    }
}

fn begin_supervision() -> Result<SupervisionGuard, Error> {
    let lock = SUPERVISION_LOCK
        .lock()
        .map_err(|_| Error::msg("subprocess supervision lock is poisoned"))?;
    install_interrupt_handlers()?;
    if interrupted() {
        return Err(Error::msg("subprocess execution interrupted"));
    }
    #[cfg(unix)]
    {
        let Some(default_enabled) = DEFAULT_ENABLED.get() else {
            return Err(Error::msg(
                "subprocess signal handlers were not initialized",
            ));
        };
        default_enabled.store(false, AtomicOrdering::SeqCst);
    }
    Ok(SupervisionGuard { _lock: lock })
}

pub(crate) fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    context: &str,
    missing_message: Option<&str>,
) -> Result<Output, Error> {
    let _supervision = begin_supervision()?;
    if interrupted() {
        return Err(Error::msg(format!("{context} interrupted")));
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::msg(
                missing_message
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("could not start {context}: {error}")),
            )
        } else {
            Error::msg(format!("could not start {context}: {error}"))
        }
    })?;
    let Some(stdout) = child.stdout.take() else {
        return abort(
            &mut child,
            Error::msg(format!("could not capture {context} stdout")),
        );
    };
    let Some(stderr) = child.stderr.take() else {
        return abort(
            &mut child,
            Error::msg(format!("could not capture {context} stderr")),
        );
    };
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = stdout_tx.send(drain_capped(stdout));
    });
    thread::spawn(move || {
        let _ = stderr_tx.send(drain_capped(stderr));
    });

    let started = Instant::now();
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    loop {
        if interrupted() {
            return abort(&mut child, Error::msg(format!("{context} interrupted")));
        }
        if status.is_none() {
            status = match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    return abort(
                        &mut child,
                        Error::msg(format!("could not wait for {context}: {error}")),
                    );
                }
            };
        }
        if stdout.is_none() {
            stdout = match receive_pipe(&stdout_rx, context, "stdout") {
                Ok(output) => output,
                Err(error) => return abort(&mut child, error),
            };
        }
        if stderr.is_none() {
            stderr = match receive_pipe(&stderr_rx, context, "stderr") {
                Ok(output) => output,
                Err(error) => return abort(&mut child, error),
            };
        }
        if status.is_some() && stdout.is_some() && stderr.is_some() {
            match (status.take(), stdout.take(), stderr.take()) {
                (Some(status), Some(stdout), Some(stderr)) => {
                    if interrupted() {
                        return abort(&mut child, Error::msg(format!("{context} interrupted")));
                    }
                    if stdout.exceeded {
                        return abort(&mut child, capture_limit_error(context, "stdout"));
                    }
                    if stderr.exceeded {
                        return abort(&mut child, capture_limit_error(context, "stderr"));
                    }
                    // A direct child can exit while descendants with redirected pipes remain.
                    terminate_process_tree(&mut child);
                    return Ok(Output {
                        status,
                        stdout: stdout.bytes,
                        stderr: stderr.bytes,
                    });
                }
                _ => unreachable!("all subprocess output fields were checked above"),
            }
        }
        if started.elapsed() >= timeout {
            return abort(&mut child, Error::msg(format!("{context} timed out")));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

struct CapturedStream {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn drain_capped(mut reader: impl Read) -> std::io::Result<CapturedStream> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut exceeded = false;
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let retained = count.min(MAX_CAPTURE_BYTES.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..retained]);
        exceeded |= retained < count;
    }
    Ok(CapturedStream { bytes, exceeded })
}

fn capture_limit_error(context: &str, stream: &str) -> Error {
    Error::msg(format!(
        "{context} {stream} exceeded the {} MiB capture limit",
        MAX_CAPTURE_BYTES / (1024 * 1024)
    ))
}

fn abort(child: &mut Child, error: Error) -> Result<Output, Error> {
    terminate_process_tree(child);
    let _ = child.wait();
    Err(error)
}

fn receive_pipe(
    receiver: &mpsc::Receiver<std::io::Result<CapturedStream>>,
    context: &str,
    stream: &str,
) -> Result<Option<CapturedStream>, Error> {
    match receiver.try_recv() {
        Ok(Ok(bytes)) => Ok(Some(bytes)),
        Ok(Err(error)) => Err(Error::msg(format!(
            "could not read {context} {stream}: {error}"
        ))),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(Error::msg(format!(
            "could not read {context} {stream}: reader stopped"
        ))),
    }
}

fn terminate_process_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }

        if let Ok(process_group) = i32::try_from(child.id()) {
            // SAFETY: `kill` has the POSIX signature on supported macOS/Linux
            // targets. `process_group(0)` made the child's PID its group ID;
            // a negative PID targets that group without dereferencing memory.
            let _ = unsafe { kill(-process_group, 9) };
        }
    }
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    const HELPER_ENV: &str = "TINK_PROCESS_SIGNAL_HELPER";

    #[test]
    fn signal_helper() {
        let Some(root) = std::env::var_os(HELPER_ENV) else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let output = run_bounded(
            Command::new("sh").args(["-c", "exit 0"]),
            Duration::from_secs(5),
            "signal helper",
            None,
        )
        .expect("complete supervised helper");
        assert!(output.status.success());
        fs::write(root.join("ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(3));
        fs::write(root.join("survived"), b"survived").unwrap();
    }

    #[test]
    fn termination_after_supervision_uses_the_default_signal_action() {
        let temp = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process::tests::signal_helper"])
            .env(HELPER_ENV, temp.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !temp.path().join("ready").exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "signal helper exited before synchronization"
            );
            assert!(started.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(10));
        }

        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        let pid = i32::try_from(child.id()).unwrap();
        // SAFETY: POSIX kill takes an integer PID and signal; the spawned child
        // is live and PID conversion was checked.
        assert_eq!(unsafe { kill(pid, 15) }, 0);
        let status = child.wait().unwrap();
        assert!(!status.success());
        thread::sleep(Duration::from_millis(3200));
        assert!(!temp.path().join("survived").exists());
    }

    #[test]
    fn output_over_limit_is_drained_and_refused() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("drained");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "dd if=/dev/zero bs=1024 count=\"$1\" 2>/dev/null; printf drained > \"$2\"",
            "sh",
            &(MAX_CAPTURE_BYTES / 1024 + 1).to_string(),
            marker.to_str().unwrap(),
        ]);

        let error = run_bounded(
            &mut command,
            Duration::from_secs(10),
            "output-limit helper",
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            format!(
                "output-limit helper stdout exceeded the {} MiB capture limit",
                MAX_CAPTURE_BYTES / (1024 * 1024)
            )
        );
        assert!(marker.exists(), "the overflowing stream must be drained");
    }
}

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{E2eError, E2eResult};

/// A running UCI engine process with line-based I/O and timeout support.
pub struct EngineProcess {
    name: String,
    child: Child,
    writer: BufWriter<std::process::ChildStdin>,
    line_rx: mpsc::Receiver<String>,
    _reader_thread: thread::JoinHandle<()>,
    default_timeout: Duration,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    _stderr_thread: thread::JoinHandle<()>,
}

impl EngineProcess {
    /// Spawn an engine process from the given binary path.
    ///
    /// `working_dir` sets the cwd so the engine can find its NNUE model.
    pub fn spawn(
        name: &str,
        binary: &Path,
        working_dir: &Path,
        default_timeout: Duration,
    ) -> E2eResult<Self> {
        Self::spawn_with_model(name, binary, working_dir, default_timeout, None)
    }

    /// Spawn with an explicit model path; legacy engines still use their cwd.
    pub fn spawn_with_model(
        name: &str,
        binary: &Path,
        working_dir: &Path,
        default_timeout: Duration,
        model_path: Option<&Path>,
    ) -> E2eResult<Self> {
        // Explicit model selection for current PikaRust; older baselines and
        // official Pikafish ignore this variable and use the verified cwd file.
        let model = model_path
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("PIKARUST_NNUE_MODEL").map(std::path::PathBuf::from))
            .unwrap_or_else(|| {
                let models = working_dir.join("models/pikafish.nnue");
                if models.is_file() {
                    models
                } else {
                    working_dir.join("pikafish.nnue")
                }
            });
        let mut child = Command::new(binary)
            .env("PIKARUST_NNUE_FILE", model)
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| E2eError::Engine {
                engine: name.to_owned(),
                message: format!("failed to spawn: {e}"),
            })?;

        let stdin = child.stdin.take().ok_or_else(|| E2eError::Engine {
            engine: name.to_owned(),
            message: "failed to capture stdin".to_owned(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| E2eError::Engine {
            engine: name.to_owned(),
            message: "failed to capture stdout".to_owned(),
        })?;

        let stderr = child.stderr.take().ok_or_else(|| E2eError::Engine {
            engine: name.to_owned(),
            message: "failed to capture stderr".to_owned(),
        })?;
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_buffer = Arc::clone(&stderr_tail);
        let stderr_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Ok(mut tail) = stderr_buffer.lock() {
                    if tail.len() == 32 {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
        });
        let (tx, rx) = mpsc::sync_channel(4096);
        let reader_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            name: name.to_owned(),
            child,
            writer: BufWriter::new(stdin),
            line_rx: rx,
            _reader_thread: reader_thread,
            default_timeout,
            stderr_tail,
            _stderr_thread: stderr_thread,
        })
    }

    /// Send a line to the engine's stdin.
    pub fn send(&mut self, line: &str) -> E2eResult<()> {
        log::debug!("[{}] >> {}", self.name, line);
        writeln!(self.writer, "{line}").map_err(|e| E2eError::Engine {
            engine: self.name.clone(),
            message: format!("write failed: {e}; stderr: {}", self.stderr_diagnostics()),
        })?;
        self.writer.flush().map_err(|e| E2eError::Engine {
            engine: self.name.clone(),
            message: format!("flush failed: {e}; stderr: {}", self.stderr_diagnostics()),
        })?;
        Ok(())
    }

    /// Read one line from stdout, with timeout.
    pub fn read_line(&self, timeout: Duration) -> E2eResult<String> {
        self.line_rx
            .recv_timeout(timeout)
            .map(|line| {
                log::debug!("[{}] << {}", self.name, line);
                line
            })
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => E2eError::Timeout {
                    engine: self.name.clone(),
                    timeout_ms: timeout.as_millis() as u64,
                    context: "read_line".to_owned(),
                },
                mpsc::RecvTimeoutError::Disconnected => E2eError::Engine {
                    engine: self.name.clone(),
                    message: format!(
                        "stdout closed before the expected protocol response; stderr: {}",
                        self.stderr_diagnostics()
                    ),
                },
            })
    }

    /// Read lines until a predicate matches, returning all lines read.
    pub fn read_until(
        &self,
        predicate: impl Fn(&str) -> bool,
        timeout: Duration,
    ) -> E2eResult<Vec<String>> {
        let mut lines = Vec::new();
        let deadline = std::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(E2eError::Timeout {
                    engine: self.name.clone(),
                    timeout_ms: timeout.as_millis() as u64,
                    context: "read_until".to_owned(),
                });
            }

            let line = self.read_line(remaining)?;
            let matched = predicate(&line);
            let diagnostic = line.to_ascii_lowercase();
            if !matched
                && (diagnostic.starts_with("info string error:")
                    || diagnostic.starts_with("no such option:"))
            {
                return Err(E2eError::Protocol {
                    engine: self.name.clone(),
                    expected: "successful command response".to_owned(),
                    actual: line,
                });
            }
            lines.push(line);
            if matched {
                return Ok(lines);
            }
        }
    }

    /// Send "quit" and wait for the process to exit.
    pub fn quit(&mut self) -> E2eResult<()> {
        self.send("quit")?;
        let deadline = Instant::now() + self.default_timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(E2eError::Engine {
                        engine: self.name.clone(),
                        message: format!(
                            "exit status {status}; stderr: {}",
                            self.stderr_diagnostics()
                        ),
                    })
                };
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return Err(E2eError::Timeout {
                    engine: self.name.clone(),
                    timeout_ms: self.default_timeout.as_millis() as u64,
                    context: "quit".to_owned(),
                });
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn stderr_diagnostics(&self) -> String {
        self.stderr_tail.lock().map_or_else(
            |_| "unavailable".to_owned(),
            |tail| tail.iter().cloned().collect::<Vec<_>>().join("\n"),
        )
    }

    /// The engine's display name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = writeln!(self.writer, "quit");
        let _ = self.writer.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn closed_stdout_is_an_engine_failure_not_a_timeout() {
        let mut process = EngineProcess::spawn(
            "fixture",
            Path::new("/bin/sh"),
            Path::new("/tmp"),
            Duration::from_secs(1),
        )
        .unwrap();
        process.send("echo fixture-diagnostic >&2; exit 7").unwrap();
        let error = process.read_line(Duration::from_secs(1)).unwrap_err();
        assert!(matches!(error, E2eError::Engine { .. }));
    }

    #[test]
    fn quit_has_a_deadline_for_an_unresponsive_engine() {
        let mut process = EngineProcess::spawn(
            "fixture",
            Path::new("/bin/cat"),
            Path::new("/tmp"),
            Duration::from_millis(30),
        )
        .unwrap();
        let start = Instant::now();
        assert!(matches!(process.quit(), Err(E2eError::Timeout { .. })));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}

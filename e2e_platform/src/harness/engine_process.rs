use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::error::{E2eError, E2eResult};

/// A running UCI engine process with line-based I/O and timeout support.
pub struct EngineProcess {
    name: String,
    child: Child,
    writer: BufWriter<std::process::ChildStdin>,
    line_rx: mpsc::Receiver<String>,
    _reader_thread: thread::JoinHandle<()>,
}

impl EngineProcess {
    /// Spawn an engine process from the given binary path.
    ///
    /// `working_dir` sets the cwd so the engine can find its NNUE model.
    pub fn spawn(
        name: &str,
        binary: &Path,
        working_dir: &Path,
        _default_timeout: Duration,
    ) -> E2eResult<Self> {
        let mut child = Command::new(binary)
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
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

        let (tx, rx) = mpsc::channel();
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
        })
    }

    /// Send a line to the engine's stdin.
    pub fn send(&mut self, line: &str) -> E2eResult<()> {
        log::debug!("[{}] >> {}", self.name, line);
        writeln!(self.writer, "{line}").map_err(|e| E2eError::Engine {
            engine: self.name.clone(),
            message: format!("write failed: {e}"),
        })?;
        self.writer.flush().map_err(|e| E2eError::Engine {
            engine: self.name.clone(),
            message: format!("flush failed: {e}"),
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
            .map_err(|_| E2eError::Timeout {
                engine: self.name.clone(),
                timeout_ms: timeout.as_millis() as u64,
                context: "read_line".to_owned(),
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
            lines.push(line);
            if matched {
                return Ok(lines);
            }
        }
    }

    /// Send "quit" and wait for the process to exit.
    pub fn quit(&mut self) -> E2eResult<()> {
        let _ = self.send("quit");
        let _ = self.child.wait();
        Ok(())
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

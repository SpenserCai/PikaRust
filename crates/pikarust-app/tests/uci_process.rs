//! Process-level protocol regressions; no NNUE download is needed.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct UciProcess {
    child: Child,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
}

impl UciProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pikarust"))
            .arg("--material-only")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start UCI engine");
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            reader: Some(reader),
        }
    }

    fn send(&mut self, command: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
    }

    fn until(&self, prefix: &str) -> String {
        // Debug builds initialize attack tables before the handshake. Search
        // deadlines remain short once initialization has completed.
        let timeout = if prefix == "uciok" { 60 } else { 5 };
        let deadline = Instant::now() + Duration::from_secs(timeout);
        loop {
            let line = self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| panic!("missing {prefix:?} before deadline: {error}"));
            if line.starts_with(prefix) {
                return line;
            }
        }
    }
}

impl Drop for UciProcess {
    fn drop(&mut self) {
        // A failed deadline assertion must never leave an infinite search alive.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn zero_and_missing_side_clocks_return_bestmove_without_stop() {
    let mut engine = UciProcess::spawn();
    engine.send("uci");
    engine.until("uciok");
    for (position, go) in [
        ("position startpos", "go wtime 0 btime 0"),
        ("position startpos", "go wtime 0"),
        ("position startpos", "go btime 5000"),
        (
            "position fen rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR b - - 0 1",
            "go wtime 5000",
        ),
    ] {
        engine.send(position);
        engine.send(go);
        let bestmove = engine.until("bestmove ");
        assert_ne!(bestmove, "bestmove 0000", "{go}");
        engine.send("isready");
        engine.until("readyok");
    }
    engine.send("quit");
}

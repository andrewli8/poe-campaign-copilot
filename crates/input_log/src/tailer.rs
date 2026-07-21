use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::poller::FilePoller;

pub struct TailerHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl TailerHandle {
    /// Signals the tailer thread to stop and joins it. This is the explicit,
    /// preferred way to shut a tailer down: it blocks until the thread has
    /// actually exited.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for TailerHandle {
    /// Ensures a dropped handle (e.g. one that went out of scope without an
    /// explicit `stop()` call, such as on an early return or panic) doesn't
    /// leave the polling loop spinning forever in the background. This does
    /// NOT join the thread -- only `stop(self)` does that -- it just flips
    /// the flag so the thread notices on its next wakeup and exits.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn spawn_tailer(
    mut poller: FilePoller,
    poll_interval: Duration,
    sender: Sender<String>,
) -> TailerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let join = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::SeqCst) {
            match poller.poll() {
                Ok(lines) => {
                    for line in lines {
                        if sender.send(line).is_err() {
                            return; // receiver gone
                        }
                    }
                }
                Err(_) => { /* transient I/O error: retry next tick */ }
            }
            std::thread::sleep(poll_interval);
        }
    });
    TailerHandle {
        stop,
        join: Some(join),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// RAII guard that removes the temp file even if the test panics.
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn tails_appends_until_stopped() {
        let path =
            std::env::temp_dir().join(format!("poe-copilot-tailer-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "").unwrap();

        let poller = FilePoller::new(path.clone(), false).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = spawn_tailer(poller, Duration::from_millis(10), tx);

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "alpha").unwrap();
        writeln!(f, "beta").unwrap();
        f.flush().unwrap();

        let a = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("alpha", "beta"));

        handle.stop();
    }

    #[test]
    fn dropped_handle_signals_stop_without_joining() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-tailer-drop-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "").unwrap();

        let poller = FilePoller::new(path.clone(), false).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let handle = spawn_tailer(poller, Duration::from_millis(5), tx);
        let stop_flag = Arc::clone(&handle.stop);

        assert!(!stop_flag.load(Ordering::SeqCst));
        drop(handle); // no explicit stop(): Drop must still raise the flag
        assert!(
            stop_flag.load(Ordering::SeqCst),
            "dropping TailerHandle should signal the loop to stop"
        );
    }
}

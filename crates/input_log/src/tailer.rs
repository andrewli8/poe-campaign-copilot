use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::poller::FilePoller;

pub struct TailerHandle {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

impl TailerHandle {
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.join.join();
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
    TailerHandle { stop, join }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tails_appends_until_stopped() {
        let path =
            std::env::temp_dir().join(format!("poe-copilot-tailer-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
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
        let _ = std::fs::remove_file(&path);
    }
}

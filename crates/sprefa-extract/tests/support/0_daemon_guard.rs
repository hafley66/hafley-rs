use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Own the daemon started under one isolated cache directory. The detached
/// process records its PID after binding; Drop escalates if shutdown stalls.
pub struct DaemonGuard {
    socket: PathBuf,
    pid_file: PathBuf,
    stopped: bool,
}

impl DaemonGuard {
    pub fn new(cache: &Path) -> Self {
        Self {
            socket: ryi_proto::daemon_auto::socket_path_at(&cache.join("ryi")),
            pid_file: cache.join("ryi/ryi.pid"),
            stopped: false,
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn pid(&self) -> Option<i32> {
        std::fs::read_to_string(&self.pid_file)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    pub fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    fn wait_for_exit(pid: i32, deadline: Duration) -> bool {
        let until = Instant::now() + deadline;
        while Self::alive(pid) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(25));
        }
        !Self::alive(pid)
    }

    pub fn stop(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        let pid = self.pid();
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&self.socket) {
            let _ = stream.write_all(
                b"POST /__shutdown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
            );
        }
        if let Some(pid) = pid {
            if !Self::wait_for_exit(pid, Duration::from_secs(2)) {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
            if !Self::wait_for_exit(pid, Duration::from_secs(2)) {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            let exited = Self::wait_for_exit(pid, Duration::from_secs(2));
            if exited {
                let _ = std::fs::remove_file(&self.socket);
                let _ = std::fs::remove_file(&self.pid_file);
            }
            self.stopped = exited;
            return exited;
        }
        self.stopped = !self.socket.exists();
        self.stopped
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

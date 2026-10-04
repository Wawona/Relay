//! Bounded, per-machine boot evidence. Console markers are never readiness.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const LIMIT: usize = 2 * 1024 * 1024;

pub(crate) struct BootTrace {
    output: BufWriter<File>,
    remaining: usize,
    copied: usize,
    started: Instant,
    sampled: Instant,
}

impl BootTrace {
    pub(crate) fn open(path: &Path, memory_bytes: u64, disk_gib: u32) -> Option<Self> {
        let file = match File::create(path) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("Relay boot trace unavailable: {error}");
                return None;
            }
        };
        let now = Instant::now();
        let mut trace = Self {
            output: BufWriter::with_capacity(64 * 1024, file),
            remaining: LIMIT,
            copied: 0,
            started: now,
            sampled: now,
        };
        trace.append(
            format!("Relay boot trace: memory_bytes={memory_bytes} disk_gib={disk_gib}\n")
                .as_bytes(),
        );
        Some(trace)
    }

    fn append(&mut self, bytes: &[u8]) {
        let length = bytes.len().min(self.remaining);
        if self.output.write_all(&bytes[..length]).is_err() {
            self.remaining = 0;
        } else {
            self.remaining -= length;
        }
    }

    pub(crate) fn observe(&mut self, console: &[u8], pc: u64, instructions: u64) {
        // The CPU owns an append-only console. A reset must not panic this observer.
        if self.copied > console.len() {
            self.copied = 0;
        }
        self.append(&console[self.copied..]);
        self.copied = console.len();
        if self.sampled.elapsed() >= Duration::from_secs(5) {
            self.append(
                format!(
                    "\nRelay sample: elapsed_ms={} instructions={instructions} pc={pc:#x}\n",
                    self.started.elapsed().as_millis()
                )
                .as_bytes(),
            );
            let _ = self.output.flush();
            self.sampled = Instant::now();
        }
    }

    pub(crate) fn finish(&mut self, console: &[u8], pc: u64, instructions: u64, result: &str) {
        self.observe(console, pc, instructions);
        self.append(format!("\nRelay exit: elapsed_ms={} instructions={instructions} pc={pc:#x} result={result}\n", self.started.elapsed().as_millis()).as_bytes());
        let _ = self.output.flush();
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    #[test]
    fn trace_caps_disk_usage_and_survives_console_reset() {
        let directory = std::env::temp_dir().join(format!(
            "relay-boot-trace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("console.log");
        let mut trace = BootTrace::open(&path, 768 << 20, 9).unwrap();
        trace.observe(b"linux pid1\n", 0x80000, 4096);
        trace.observe(b"reset\n", 0x90000, 8192);
        trace.finish(b"reset\n", 0x90000, 8192, "stopped");
        trace.append(&vec![b'x'; LIMIT + 1024]);
        drop(trace);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        assert_eq!(bytes.len(), LIMIT);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with(
            "Relay boot trace: memory_bytes=805306368 disk_gib=9\nlinux pid1\nreset\n"
        ));
        assert!(text.contains("pc=0x90000 result=stopped"));
    }
}

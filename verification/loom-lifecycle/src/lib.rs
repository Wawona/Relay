//! Loom model of two host threads storing into a shared RAM span.
//! Product code: relay_vm::aot::SharedRam. This crate stays outside the app graph.

#[cfg(loom)]
use loom::sync::{Arc, Mutex};
#[cfg(not(loom))]
use std::sync::{Arc, Mutex};

fn ram_span_ok(offset: usize, len: usize, ram: usize) -> bool {
    offset.checked_add(len).map(|end| end <= ram).unwrap_or(false)
}

struct SharedRam {
    bytes: Mutex<Vec<u8>>,
}

impl SharedRam {
    fn new(len: usize) -> Self {
        Self {
            bytes: Mutex::new(vec![0; len]),
        }
    }

    fn store(&self, offset: usize, data: &[u8]) -> bool {
        let mut guard = self.bytes.lock().unwrap();
        if !ram_span_ok(offset, data.len(), guard.len()) {
            return false;
        }
        guard[offset..offset + data.len()].copy_from_slice(data);
        true
    }

    fn load(&self, offset: usize, out: &mut [u8]) -> bool {
        let guard = self.bytes.lock().unwrap();
        if !ram_span_ok(offset, out.len(), guard.len()) {
            return false;
        }
        out.copy_from_slice(&guard[offset..offset + out.len()]);
        true
    }
}

#[cfg(all(test, loom))]
mod tests {
    use super::*;

    #[test]
    fn two_threads_store_disjoint_spans() {
        loom::model(|| {
            let ram = Arc::new(SharedRam::new(8));
            let left = Arc::clone(&ram);
            let right = Arc::clone(&ram);
            let t1 = loom::thread::spawn(move || left.store(0, &[1, 2]));
            let t2 = loom::thread::spawn(move || right.store(4, &[3, 4]));
            assert!(t1.join().unwrap());
            assert!(t2.join().unwrap());
            let mut buf = [0u8; 8];
            assert!(ram.load(0, &mut buf));
            assert_eq!(&buf[0..2], &[1, 2]);
            assert_eq!(&buf[4..6], &[3, 4]);
        });
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn span_rejects_overflow() {
        assert!(!ram_span_ok(usize::MAX, 1, 8));
        assert!(ram_span_ok(0, 8, 8));
    }
}

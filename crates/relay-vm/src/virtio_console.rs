//! Bounded virtio-console buffers for guest boot logs and interactive input.
use std::collections::VecDeque;
pub(crate) struct Console {
    input: VecDeque<u8>,
    output: Vec<u8>,
    limit: usize,
}
impl Console {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            input: VecDeque::new(),
            output: Vec::new(),
            limit,
        }
    }
    pub(crate) fn push_input(&mut self, bytes: &[u8]) {
        for b in bytes
            .iter()
            .copied()
            .take(self.limit.saturating_sub(self.input.len()))
        {
            self.input.push_back(b);
        }
    }
    pub(crate) fn read_input(&mut self, out: &mut [u8]) -> usize {
        let n = out.len().min(self.input.len());
        for byte in out.iter_mut().take(n) {
            *byte = self.input.pop_front().unwrap();
        }
        n
    }
    pub(crate) fn write_output(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
        if self.output.len() > self.limit {
            self.output.drain(..self.output.len() - self.limit);
        }
    }
    pub(crate) fn output(&self) -> &[u8] {
        &self.output
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_is_bounded() {
        let mut c = Console::new(4);
        c.write_output(b"hello");
        assert_eq!(c.output(), b"ello");
        c.push_input(b"abcde");
        let mut out = [0; 4];
        assert_eq!(c.read_input(&mut out), 4);
        assert_eq!(&out, b"abcd");
    }
}

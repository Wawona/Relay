//! Bounded virtio-console buffers for guest boot logs and interactive input.
use crate::{guest::GuestMemory, virtio_block::parse_chain, virtio_mmio::Transport};
use relay_core::RelayError;
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
        if bytes.len() >= self.limit {
            self.output.clear();
            self.output
                .extend_from_slice(&bytes[bytes.len() - self.limit..]);
        } else {
            let discard = self.output.len().saturating_sub(self.limit - bytes.len());
            self.output.drain(..discard);
            self.output.extend_from_slice(bytes);
        }
    }

    pub(crate) fn output(&self) -> &[u8] {
        &self.output
    }

    /// Queue 0 is host-to-guest input and queue 1 is guest-to-host output for
    /// port zero. This is the standard non-multiport virtio-console layout.
    pub(crate) fn process_notified_queues(
        &mut self,
        transport: &mut Transport,
        memory: &mut GuestMemory,
    ) -> Result<usize, RelayError> {
        let mut completed = 0;
        // A waiting receive queue must neither lose its kick nor block output.
        for queue in 0..2 {
            if !transport.queue_notified(queue) {
                continue;
            }
            loop {
                if queue == 0 && self.input.is_empty() {
                    break;
                }
                let Some(head) = transport.pop_available(queue, memory)? else {
                    transport.clear_notification(queue);
                    break;
                };
                let chain = parse_chain(
                    memory,
                    transport.descriptor_table(queue)?,
                    head,
                    transport.queue_size(queue)?,
                )?;
                // Validate the whole chain before consuming host input or
                // changing guest RAM/output. Never allocate a guest-sized buffer.
                let mut capacity = 0u32;
                for descriptor in &chain {
                    if descriptor.writable != (queue == 0) {
                        return Err(RelayError::Failed(
                            "virtio console descriptor direction invalid".into(),
                        ));
                    }
                    memory.check_range(descriptor.address, descriptor.length as usize)?;
                    capacity = capacity.checked_add(descriptor.length).ok_or_else(|| {
                        RelayError::Failed("virtio console chain length overflow".into())
                    })?;
                }
                let mut transferred = 0u32;
                let mut scratch = [0u8; 4096];
                for descriptor in &chain {
                    let mut offset = 0u32;
                    while offset < descriptor.length {
                        let count = ((descriptor.length - offset) as usize).min(scratch.len());
                        let address = descriptor.address + u64::from(offset);
                        let count = if queue == 0 {
                            let count = self.read_input(&mut scratch[..count]);
                            if count == 0 {
                                break;
                            }
                            memory.write(address, &scratch[..count])?;
                            count
                        } else {
                            memory.read(address, &mut scratch[..count])?;
                            self.write_output(&scratch[..count]);
                            count
                        };
                        offset += count as u32;
                        transferred += count as u32;
                    }
                }
                // Used length counts device-written bytes, not transmitted bytes.
                transport.complete(
                    queue,
                    memory,
                    head,
                    if queue == 0 { transferred } else { 0 },
                )?;
                completed += 1;
            }
        }
        Ok(completed)
    }

    pub(crate) fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::GuestPageSize;
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

    #[test]
    fn empty_input_keeps_receive_buffers_owned_by_device() {
        let mut memory = GuestMemory::allocate_on_host(
            GuestPageSize::FOUR_KIB,
            relay_core::HostPageSize::SIXTEEN_KIB,
            4096,
        )
        .unwrap();
        let mut transport = Transport::new(3, 1 << 32, 8, 2);
        transport.write(0x024, 1).unwrap();
        transport.write(0x020, 1).unwrap();
        transport.write(0x038, 8).unwrap();
        transport.write(0x080, 0x100).unwrap();
        transport.write(0x090, 0x200).unwrap();
        transport.write(0x0a0, 0x300).unwrap();
        transport.write(0x044, 1).unwrap();
        for status in [1, 3, 11, 15] {
            transport.write(0x070, status).unwrap();
        }
        transport.write(0x050, 0).unwrap();

        let mut console = Console::new(1024);
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            0
        );
        assert_eq!(transport.pending_notification(), Some(0));
        assert_eq!(transport.read(0x060).unwrap(), 0);
    }
    fn device() -> (Console, Transport, GuestMemory) {
        let memory = GuestMemory::allocate_on_host(
            GuestPageSize::FOUR_KIB,
            relay_core::HostPageSize::SIXTEEN_KIB,
            16384,
        )
        .unwrap();
        let mut transport = Transport::new(3, 1 << 32, 8, 2);
        transport.write(0x024, 1).unwrap();
        transport.write(0x020, 1).unwrap();
        for queue in 0..2 {
            transport.write(0x030, queue).unwrap();
            transport.write(0x038, 8).unwrap();
            for (register, address) in [(0x080, 0x100), (0x090, 0x200), (0x0a0, 0x300)] {
                transport.write(register, address + queue * 0x400).unwrap();
            }
            transport.write(0x044, 1).unwrap();
        }
        for status in [1, 3, 11, 15] {
            transport.write(0x070, status).unwrap();
        }
        (Console::new(8), transport, memory)
    }

    fn descriptor(
        memory: &mut GuestMemory,
        table: u64,
        index: u16,
        address: u64,
        length: u32,
        flags: u16,
        next: u16,
    ) {
        let mut raw = [0; 16];
        raw[..8].copy_from_slice(&address.to_le_bytes());
        raw[8..12].copy_from_slice(&length.to_le_bytes());
        raw[12..14].copy_from_slice(&flags.to_le_bytes());
        raw[14..].copy_from_slice(&next.to_le_bytes());
        memory.write(table + u64::from(index) * 16, &raw).unwrap();
    }

    fn used(memory: &GuestMemory, base: u64) -> (u16, u32) {
        let mut raw = [0; 12];
        memory.read(base, &mut raw).unwrap();
        (
            u16::from_le_bytes(raw[2..4].try_into().unwrap()),
            u32::from_le_bytes(raw[8..12].try_into().unwrap()),
        )
    }

    #[test]
    fn receive_wait_survives_transmit_and_never_completes_empty_buffers() {
        let (mut console, mut transport, mut memory) = device();
        descriptor(&mut memory, 0x100, 0, 0x800, 4, 2, 0);
        descriptor(&mut memory, 0x100, 1, 0x804, 4, 2, 0);
        memory.write(0x202, &[2, 0, 0, 0, 1, 0]).unwrap();
        descriptor(&mut memory, 0x500, 0, 0x900, 5, 0, 0);
        memory.write(0x602, &[1, 0, 0, 0]).unwrap();
        memory.write(0x900, b"hello").unwrap();
        transport.write(0x050, 0).unwrap();
        transport.write(0x050, 1).unwrap();
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            1
        );
        assert_eq!(console.output(), b"hello");
        assert_eq!(used(&memory, 0x700), (1, 0));
        assert_eq!(used(&memory, 0x300), (0, 0));
        assert_eq!(transport.pending_notification(), Some(0));
        console.push_input(b"ab");
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            1
        );
        assert_eq!(used(&memory, 0x300), (1, 2));
        assert!(transport.queue_notified(0));
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            0
        );
        console.push_input(b"cdef");
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            1
        );
        let mut input = [0; 8];
        memory.read(0x800, &mut input).unwrap();
        assert_eq!(&input, b"ab\0\0cdef");
        transport.write(0x070, 0).unwrap();
        assert_eq!(transport.pending_notification(), None);
    }

    #[test]
    fn malformed_console_chain_has_no_partial_payload_effects() {
        for queue in 0..2u32 {
            for invalid_length in [false, true] {
                let (mut console, mut transport, mut memory) = device();
                let base = 0x100 + u64::from(queue) * 0x400;
                let flags = if queue == 0 { 2 } else { 0 };
                descriptor(&mut memory, base, 0, 0x800, 2, flags | 1, 1);
                descriptor(
                    &mut memory,
                    base,
                    1,
                    0x802,
                    if invalid_length { u32::MAX } else { 2 },
                    if invalid_length { flags } else { flags ^ 2 },
                    0,
                );
                memory.write(base + 0x102, &[1, 0, 0, 0]).unwrap();
                memory.write(0x800, b"keep").unwrap();
                console.push_input(b"abcd");
                transport.write(0x050, queue).unwrap();
                assert!(console
                    .process_notified_queues(&mut transport, &mut memory)
                    .is_err());
                let mut bytes = [0; 4];
                memory.read(0x800, &mut bytes).unwrap();
                assert_eq!(&bytes, b"keep");
                assert!(console.output().is_empty());
                assert_eq!(console.read_input(&mut bytes), 4);
                assert_eq!(&bytes, b"abcd");
                assert_eq!(used(&memory, base + 0x200), (0, 0));
            }
        }
    }

    #[test]
    fn transmit_chunks_keep_only_bounded_tail() {
        let (mut console, mut transport, mut memory) = device();
        let payload: Vec<u8> = (0..8193).map(|i| (i % 251) as u8).collect();
        memory.write(0x800, &payload).unwrap();
        descriptor(&mut memory, 0x500, 0, 0x800, payload.len() as u32, 0, 0);
        memory.write(0x602, &[1, 0, 0, 0]).unwrap();
        transport.write(0x050, 1).unwrap();
        assert_eq!(
            console
                .process_notified_queues(&mut transport, &mut memory)
                .unwrap(),
            1
        );
        assert_eq!(console.output(), &payload[payload.len() - 8..]);
        assert_eq!(used(&memory, 0x700), (1, 0));
        let mut zero = Console::new(0);
        zero.write_output(&payload);
        assert!(zero.output().is_empty());
    }
}

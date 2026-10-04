//! Virtio 1.2 vsock RX/TX split queues. Shared MMIO and descriptor validation.
use crate::{
    guest::GuestMemory, virtio_block::parse_chain, virtio_mmio::Transport, vsock::Vsock, vsock_wire,
};
use relay_core::RelayError;

pub(crate) fn service(
    device: &mut Vsock,
    transport: &mut Transport,
    memory: &mut GuestMemory,
) -> Result<usize, RelayError> {
    device.poll()?;
    let mut completed = 0;
    // Drain RX before accepting TX, but lack of RX buffers must not prevent
    // bounded TX progress. Retain RX kicks until a real packet is available.
    for queue in 0..2 {
        if !transport.queue_notified(queue) {
            continue;
        }
        for _ in 0..256 {
            if (queue == 0 && device.front().is_none())
                || (queue == 1 && !device.can_accept_packet())
            {
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
            let mut capacity = 0usize;
            for descriptor in &chain {
                if descriptor.writable != (queue == 0) {
                    return Err(RelayError::Failed(
                        "virtio vsock descriptor direction invalid".into(),
                    ));
                }
                memory.check_range(descriptor.address, descriptor.length as usize)?;
                capacity = capacity
                    .checked_add(descriptor.length as usize)
                    .ok_or_else(|| {
                        RelayError::Failed("virtio vsock descriptor capacity overflow".into())
                    })?;
            }
            let used = if queue == 0 {
                let packet = device.front().unwrap();
                if capacity < packet.len() {
                    return Err(RelayError::Failed(
                        "virtio vsock receive buffer too small".into(),
                    ));
                }
                let mut copied = 0;
                for descriptor in &chain {
                    let n = (descriptor.length as usize).min(packet.len() - copied);
                    memory.write(descriptor.address, &packet[copied..copied + n])?;
                    copied += n;
                    if copied == packet.len() {
                        break;
                    }
                }
                let used = packet.len() as u32;
                device.pop();
                used
            } else {
                if !(vsock_wire::HEADER_BYTES..=vsock_wire::HEADER_BYTES + vsock_wire::MAX_DATA)
                    .contains(&capacity)
                {
                    return Err(RelayError::Failed(
                        "virtio vsock transmit size invalid".into(),
                    ));
                }
                let mut packet = vec![0; capacity];
                let mut copied = 0;
                for descriptor in &chain {
                    let n = descriptor.length as usize;
                    memory.read(descriptor.address, &mut packet[copied..copied + n])?;
                    copied += n;
                }
                device.receive(&packet)?;
                0 // Device does not write into TX buffers.
            };
            transport.complete(queue, memory, head, used)?;
            completed += 1;
        }
    }
    // Event queue 2 is reserved for a live transport reset/CID change. Resetting
    // the device through status=0 drops flows instead; never fabricate events.
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vsock_wire::Header;
    use relay_core::{GuestPageSize, HostPageSize};
    fn fixture() -> (Vsock, Transport, GuestMemory) {
        let mut t = Transport::new(19, 1 << 32, 8, 3);
        t.write(0x070, 1).unwrap();
        t.write(0x070, 3).unwrap();
        t.write(0x024, 1).unwrap();
        t.write(0x020, 1).unwrap();
        t.write(0x070, 11).unwrap();
        for queue in 0..2 {
            let base = 0x100 + queue * 0x400;
            t.write(0x030, queue).unwrap();
            t.write(0x038, 8).unwrap();
            t.write(0x080, base).unwrap();
            t.write(0x090, base + 0x100).unwrap();
            t.write(0x0a0, base + 0x200).unwrap();
            t.write(0x044, 1).unwrap();
        }
        t.write(0x070, 15).unwrap();
        (
            Vsock::default(),
            t,
            GuestMemory::allocate_on_host(GuestPageSize::FOUR_KIB, HostPageSize::SIXTEEN_KIB, 4096)
                .unwrap(),
        )
    }
    fn descriptor(
        m: &mut GuestMemory,
        table: u64,
        index: u64,
        address: u64,
        len: u32,
        flags: u16,
        next: u16,
    ) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&address.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&next.to_le_bytes());
        m.write(table + index * 16, &bytes).unwrap();
    }
    fn request() -> Vec<u8> {
        Header {
            src_cid: 3,
            dst_cid: 2,
            src_port: 5000,
            dst_port: 1024,
            len: 0,
            kind: 1,
            op: 1,
            flags: 0,
            buf_alloc: 65536,
            fwd_cnt: 0,
        }
        .encode(&[])
        .unwrap()
    }
    #[test]
    #[cfg(not(miri))]
    fn split_header_and_retained_receive_kick_use_real_connection() {
        let (mut device, mut t, mut m) = fixture();
        let listener = device.listen(1024).unwrap();
        descriptor(&mut m, 0x100, 0, 0x800, 20, 3, 1);
        descriptor(&mut m, 0x100, 1, 0x820, 24, 2, 0);
        m.write(0x202, &[1, 0, 0, 0]).unwrap();
        t.write(0x050, 0).unwrap();
        assert_eq!(service(&mut device, &mut t, &mut m).unwrap(), 0);
        assert!(t.queue_notified(0));
        let packet = request();
        m.write(0x900, &packet[..13]).unwrap();
        m.write(0x930, &packet[13..]).unwrap();
        descriptor(&mut m, 0x500, 0, 0x900, 13, 1, 1);
        descriptor(&mut m, 0x500, 1, 0x930, 31, 0, 0);
        m.write(0x602, &[1, 0, 0, 0]).unwrap();
        t.write(0x050, 1).unwrap();
        assert_eq!(service(&mut device, &mut t, &mut m).unwrap(), 1);
        let host = listener.try_recv().unwrap();
        assert_eq!((host.guest_port, host.host_port), (5000, 1024));
        assert_eq!(service(&mut device, &mut t, &mut m).unwrap(), 1);
        let mut response = [0; 44];
        m.read(0x800, &mut response[..20]).unwrap();
        m.read(0x820, &mut response[20..]).unwrap();
        assert_eq!(
            Header::decode(&response).unwrap().0.op,
            vsock_wire::RESPONSE
        );
        assert_eq!(t.queue_activity(0), (1, 1));
        assert_eq!(t.queue_activity(1), (1, 1));
        let mut used = [0; 8];
        m.read(0x304, &mut used).unwrap();
        assert_eq!(u32::from_le_bytes(used[4..].try_into().unwrap()), 44);
    }
    #[test]
    fn entire_chain_preflight_prevents_partial_receive_effects() {
        for invalid_direction in [false, true] {
            let (mut device, mut t, mut m) = fixture();
            device.receive(&request()).unwrap(); // No listener: real RST.
            descriptor(&mut m, 0x100, 0, 0x800, 20, 3, 1);
            descriptor(
                &mut m,
                0x100,
                1,
                if invalid_direction { 0x820 } else { 4090 },
                24,
                if invalid_direction { 0 } else { 2 },
                0,
            );
            m.write(0x800, b"unchanged").unwrap();
            m.write(0x202, &[1, 0, 0, 0]).unwrap();
            t.write(0x050, 0).unwrap();
            assert!(service(&mut device, &mut t, &mut m).is_err());
            let mut bytes = [0; 9];
            m.read(0x800, &mut bytes).unwrap();
            assert_eq!(&bytes, b"unchanged");
            assert!(device.front().is_some());
            assert_eq!(t.queue_activity(0).1, 0);
        }
    }
}

//! Virtio 1.0 MMIO transport and split-ring ownership.

#![allow(dead_code)] // The static CPU bus will expose these registers next.

use crate::guest::GuestMemory;
use relay_core::RelayError;

pub(crate) const MAGIC: u32 = 0x7472_6976;
const VERSION: u32 = 2;
const VENDOR_RELAY: u32 = 0x5757;
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;
const INTERRUPT_USED_BUFFER: u32 = 1;

#[derive(Debug, Clone, Copy, Default)]
struct Queue {
    size: u16,
    ready: bool,
    descriptor_table: u64,
    available_ring: u64,
    used_ring: u64,
    last_available: u16,
    notified: bool,
}

pub(crate) struct Transport {
    device_id: u32,
    features: u64,
    status: u32,
    device_feature_select: u32,
    driver_features: u64,
    driver_feature_select: u32,
    queue_select: u32,
    queue_num_max: u16,
    queues: Vec<Queue>,
    interrupt_status: u32,
    notification_counts: Vec<u64>,
    completion_counts: Vec<u64>,
}

impl Transport {
    pub(crate) fn new(device_id: u32, features: u64, queue_num_max: u32, queue_count: u32) -> Self {
        Self {
            device_id,
            features,
            status: 0,
            device_feature_select: 0,
            driver_features: 0,
            driver_feature_select: 0,
            queue_select: 0,
            queue_num_max: queue_num_max.min(u16::MAX as u32) as u16,
            queues: vec![Queue::default(); queue_count.max(1) as usize],
            interrupt_status: 0,
            notification_counts: vec![0; queue_count.max(1) as usize],
            completion_counts: vec![0; queue_count.max(1) as usize],
        }
    }

    pub(crate) fn read(&self, offset: u64) -> Result<u32, RelayError> {
        Ok(match offset {
            0x000 => MAGIC,
            0x004 => VERSION,
            0x008 => self.device_id,
            0x00c => VENDOR_RELAY,
            0x010 if self.device_feature_select < 2 => {
                (self.features >> (self.device_feature_select * 32)) as u32
            }
            0x010 => 0,
            0x034 if self.queue(self.queue_select).is_ok() => self.queue_num_max as u32,
            0x034 => 0,
            0x044 if self.queue(self.queue_select).is_ok() => {
                u32::from(self.queue(self.queue_select).unwrap().ready)
            }
            0x044 => 0,
            0x060 => self.interrupt_status,
            0x070 => self.status,
            _ => {
                return Err(RelayError::Failed(format!(
                    "virtio-mmio read unsupported {offset:#x}"
                )))
            }
        })
    }

    pub(crate) fn write(&mut self, offset: u64, value: u32) -> Result<(), RelayError> {
        match offset {
            0x014 => self.device_feature_select = value,
            0x020 if self.driver_feature_select < 2 => {
                let shift = self.driver_feature_select * 32;
                self.driver_features =
                    (self.driver_features & !(0xffff_ffffu64 << shift)) | ((value as u64) << shift);
            }
            0x020 => return Err(RelayError::Failed("virtio feature selector invalid".into())),
            0x024 => self.driver_feature_select = value,
            0x030 => self.queue_select = value,
            0x038 if self.queue(self.queue_select).is_ok() => {
                if value == 0 || value > self.queue_num_max as u32 || !value.is_power_of_two() {
                    return Err(RelayError::Failed("virtio queue size invalid".into()));
                }
                self.queue_mut()?.size = value as u16;
            }
            0x038 => return Err(RelayError::Failed("virtio queue selector invalid".into())),
            0x044 if self.queue(self.queue_select).is_ok() => {
                if value > 1 {
                    return Err(RelayError::Failed(
                        "virtio queue-ready value invalid".into(),
                    ));
                }
                if value == 1 {
                    self.validate_queue_configuration()?;
                }
                self.queue_mut()?.ready = value == 1;
            }
            0x044 => return Err(RelayError::Failed("virtio queue selector invalid".into())),
            0x050 => {
                if self.queue(value).is_err()
                    || !self.queue(value).unwrap().ready
                    || self.status & STATUS_DRIVER_OK == 0
                {
                    return Err(RelayError::Failed(
                        "virtio notification for unavailable queue".into(),
                    ));
                }
                self.notification_counts[value as usize] += 1;
                self.queue_mut_at(value)?.notified = true;
            }
            0x064 => self.interrupt_status &= !value,
            0x070 => self.set_status(value)?,
            0x080 if self.queue(self.queue_select).is_ok() => {
                set_low(&mut self.queue_mut()?.descriptor_table, value)
            }
            0x084 if self.queue(self.queue_select).is_ok() => {
                set_high(&mut self.queue_mut()?.descriptor_table, value)
            }
            0x090 if self.queue(self.queue_select).is_ok() => {
                set_low(&mut self.queue_mut()?.available_ring, value)
            }
            0x094 if self.queue(self.queue_select).is_ok() => {
                set_high(&mut self.queue_mut()?.available_ring, value)
            }
            0x0a0 if self.queue(self.queue_select).is_ok() => {
                set_low(&mut self.queue_mut()?.used_ring, value)
            }
            0x0a4 if self.queue(self.queue_select).is_ok() => {
                set_high(&mut self.queue_mut()?.used_ring, value)
            }
            0x080 | 0x084 | 0x090 | 0x094 | 0x0a0 | 0x0a4 => {
                return Err(RelayError::Failed("virtio queue selector invalid".into()))
            }
            _ => {
                return Err(RelayError::Failed(format!(
                    "virtio-mmio write unsupported {offset:#x}"
                )))
            }
        }
        Ok(())
    }

    pub(crate) fn take_notification(&mut self) -> Option<u32> {
        let index = self.pending_notification()?;
        self.clear_notification(index);
        Some(index)
    }

    pub(crate) fn pending_notification(&self) -> Option<u32> {
        self.queues
            .iter()
            .position(|q| q.notified)
            .map(|i| i as u32)
    }

    pub(crate) fn queue_notified(&self, queue: u32) -> bool {
        self.queues.get(queue as usize).is_some_and(|q| q.notified)
    }

    pub(crate) fn clear_notification(&mut self, queue: u32) {
        if let Some(q) = self.queues.get_mut(queue as usize) {
            q.notified = false;
        }
    }

    pub(crate) fn descriptor_table(&self, queue: u32) -> Result<u64, RelayError> {
        Ok(self.queue(queue)?.descriptor_table)
    }

    pub(crate) fn queue_size(&self, queue: u32) -> Result<u16, RelayError> {
        Ok(self.queue(queue)?.size)
    }

    pub(crate) fn pop_available(
        &mut self,
        queue: u32,
        memory: &GuestMemory,
    ) -> Result<Option<u16>, RelayError> {
        self.require_operational(queue)?;
        let q = self.queue(queue)?;
        let available_index = read_u16(memory, q.available_ring + 2)?;
        if available_index.wrapping_sub(q.last_available) > q.size {
            return Err(RelayError::Failed(
                "virtio available ring advanced beyond queue size".into(),
            ));
        }
        if q.last_available == available_index {
            return Ok(None);
        }
        let slot = q.last_available % q.size;
        let head = read_u16(memory, q.available_ring + 4 + u64::from(slot) * 2)?;
        if head >= q.size {
            return Err(RelayError::Failed(
                "virtio available head is outside descriptor table".into(),
            ));
        }
        let q = self.queue_mut_at(queue)?;
        q.last_available = q.last_available.wrapping_add(1);
        Ok(Some(head))
    }

    pub(crate) fn complete(
        &mut self,
        queue: u32,
        memory: &mut GuestMemory,
        head: u16,
        bytes_written: u32,
    ) -> Result<(), RelayError> {
        self.require_operational(queue)?;
        let q = self.queue(queue)?;
        if head >= q.size {
            return Err(RelayError::Failed(
                "virtio used head is outside descriptor table".into(),
            ));
        }
        let used_index = read_u16(memory, q.used_ring + 2)?;
        let slot = used_index % q.size;
        let element = q.used_ring + 4 + u64::from(slot) * 8;
        memory.write(element, &u32::from(head).to_le_bytes())?;
        memory.write(element + 4, &bytes_written.to_le_bytes())?;
        memory.write(q.used_ring + 2, &used_index.wrapping_add(1).to_le_bytes())?;
        self.interrupt_status |= INTERRUPT_USED_BUFFER;
        self.completion_counts[queue as usize] += 1;
        Ok(())
    }

    fn set_status(&mut self, value: u32) -> Result<(), RelayError> {
        if value == 0 {
            self.status = 0;
            self.driver_features = 0;
            self.queues.fill(Queue::default());
            self.interrupt_status = 0;
            self.notification_counts.fill(0);
            self.completion_counts.fill(0);
            return Ok(());
        }
        if value
            & !(STATUS_ACKNOWLEDGE
                | STATUS_DRIVER
                | STATUS_DRIVER_OK
                | STATUS_FEATURES_OK
                | STATUS_FAILED)
            != 0
        {
            return Err(RelayError::Failed(
                "virtio status contains reserved bits".into(),
            ));
        }
        if value | self.status != value {
            return Err(RelayError::Failed(
                "virtio status bits cannot be cleared without reset".into(),
            ));
        }
        if value & STATUS_FEATURES_OK != 0 && self.driver_features & !self.features != 0 {
            return Err(RelayError::Failed(
                "virtio driver accepted unsupported features".into(),
            ));
        }
        if value & STATUS_DRIVER_OK != 0 {
            let required = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
            if value & required != required {
                return Err(RelayError::Failed(
                    "virtio driver-ready sequence invalid".into(),
                ));
            }
            if !self.queues.iter().any(|queue| queue.ready) {
                return Err(RelayError::Failed(
                    "virtio driver-ready without queue".into(),
                ));
            }
        }
        self.status = value;
        Ok(())
    }

    fn validate_queue_configuration(&self) -> Result<(), RelayError> {
        let queue = self.queue(self.queue_select)?;
        if queue.size == 0
            || queue.descriptor_table % 16 != 0
            || queue.available_ring % 2 != 0
            || queue.used_ring % 4 != 0
        {
            return Err(RelayError::Failed("virtio queue layout invalid".into()));
        }
        Ok(())
    }

    fn require_operational(&self, index: u32) -> Result<(), RelayError> {
        if !self.queue(index)?.ready || self.status & STATUS_DRIVER_OK == 0 {
            return Err(RelayError::Failed("virtio queue is not operational".into()));
        }
        Ok(())
    }

    fn queue(&self, index: u32) -> Result<&Queue, RelayError> {
        self.queues
            .get(index as usize)
            .ok_or_else(|| RelayError::Failed("virtio queue selector invalid".into()))
    }

    fn queue_mut_at(&mut self, index: u32) -> Result<&mut Queue, RelayError> {
        self.queues
            .get_mut(index as usize)
            .ok_or_else(|| RelayError::Failed("virtio queue selector invalid".into()))
    }

    fn queue_mut(&mut self) -> Result<&mut Queue, RelayError> {
        self.queue_mut_at(self.queue_select)
    }

    pub(crate) fn queue_activity(&self, queue: u32) -> (u64, u64) {
        let index = queue as usize;
        (
            self.notification_counts.get(index).copied().unwrap_or(0),
            self.completion_counts.get(index).copied().unwrap_or(0),
        )
    }
}

fn set_low(target: &mut u64, value: u32) {
    *target = (*target & 0xffff_ffff_0000_0000) | u64::from(value);
}

fn set_high(target: &mut u64, value: u32) {
    *target = (*target & 0x0000_0000_ffff_ffff) | (u64::from(value) << 32);
}

#[cfg(kani)]
mod kani_proofs {
    use super::{set_high, set_low};

    #[kani::proof]
    fn queue_address_halves_reconstruct_every_u64() {
        let address: u64 = kani::any();
        let initial: u64 = kani::any();
        let mut reconstructed = initial;

        set_low(&mut reconstructed, address as u32);
        assert_eq!(reconstructed >> 32, initial >> 32);
        set_high(&mut reconstructed, (address >> 32) as u32);

        assert_eq!(reconstructed, address);
    }
}

fn read_u16(memory: &GuestMemory, address: u64) -> Result<u16, RelayError> {
    let mut raw = [0; 2];
    memory.read(address, &mut raw)?;
    Ok(u16::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::GuestPageSize;

    fn configured_transport() -> Transport {
        let mut transport = Transport::new(2, 1 << 32, 128, 1);
        transport.write(0x014, 1).unwrap();
        assert_eq!(transport.read(0x010).unwrap(), 1);
        transport.write(0x024, 1).unwrap();
        transport.write(0x020, 1).unwrap();
        transport.write(0x038, 8).unwrap();
        transport.write(0x080, 0x100).unwrap();
        transport.write(0x090, 0x200).unwrap();
        transport.write(0x0a0, 0x300).unwrap();
        transport.write(0x044, 1).unwrap();
        transport.write(0x070, STATUS_ACKNOWLEDGE).unwrap();
        transport
            .write(0x070, STATUS_ACKNOWLEDGE | STATUS_DRIVER)
            .unwrap();
        transport
            .write(
                0x070,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
            )
            .unwrap();
        transport
            .write(
                0x070,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
            )
            .unwrap();
        transport
    }

    #[test]
    fn negotiates_modern_queue_before_driver_ready() {
        let mut transport = configured_transport();
        transport.write(0x050, 0).unwrap();
        assert_eq!(transport.take_notification(), Some(0));
        assert!(transport.write(0x038, 129).is_err());
    }

    #[test]
    fn consumes_available_and_publishes_used_element() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0x202, &1u16.to_le_bytes()).unwrap();
        memory.write(0x204, &3u16.to_le_bytes()).unwrap();
        let mut transport = configured_transport();
        assert_eq!(transport.pop_available(0, &memory).unwrap(), Some(3));
        assert_eq!(transport.pop_available(0, &memory).unwrap(), None);
        transport.complete(0, &mut memory, 3, 512).unwrap();
        let mut used = [0; 10];
        memory.read(0x302, &mut used).unwrap();
        assert_eq!(u16::from_le_bytes(used[..2].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(used[2..6].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(used[6..10].try_into().unwrap()), 512);
        assert_eq!(transport.read(0x060).unwrap(), INTERRUPT_USED_BUFFER);
        transport.write(0x064, INTERRUPT_USED_BUFFER).unwrap();
        assert_eq!(transport.read(0x060).unwrap(), 0);
    }

    #[test]
    fn rejects_bad_feature_and_status_sequences() {
        let mut transport = Transport::new(2, 0, 8, 1);
        transport.write(0x024, 1).unwrap();
        transport.write(0x020, 1).unwrap();
        assert!(transport
            .write(
                0x070,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK
            )
            .is_err());
        assert!(transport.write(0x070, STATUS_DRIVER_OK).is_err());
    }
}

//! Minimal, standards-shaped Flattened Device Tree builder for Relay Linux.

use relay_core::RelayError;

const FDT_MAGIC: u32 = 0xd00d_feed;
const BEGIN_NODE: u32 = 1;
const END_NODE: u32 = 2;
const PROP: u32 = 3;
const END: u32 = 9;
const GIC_PHANDLE: u32 = 1;

pub fn build(
    memory_bytes: u64,
    bootargs: &str,
    initrd: Option<(u64, u64)>,
) -> Result<Vec<u8>, RelayError> {
    if bootargs.contains('\0') {
        return Err(RelayError::Failed("DTB bootargs contains NUL".into()));
    }
    let mut strings = Vec::new();
    let mut st = Vec::new();
    node(&mut st, "");
    prop_str(&mut st, &mut strings, "compatible", "wawona,relay-virt");
    prop_str(&mut st, &mut strings, "model", "Wawona Relay");
    prop_u32(&mut st, &mut strings, "#address-cells", 2);
    prop_u32(&mut st, &mut strings, "#size-cells", 2);
    prop_u32(&mut st, &mut strings, "interrupt-parent", GIC_PHANDLE);

    node(&mut st, "cpus");
    prop_u32(&mut st, &mut strings, "#address-cells", 1);
    prop_u32(&mut st, &mut strings, "#size-cells", 0);
    node(&mut st, "cpu@0");
    prop_str(&mut st, &mut strings, "device_type", "cpu");
    prop_str(&mut st, &mut strings, "compatible", "arm,arm-v8");
    prop_str(&mut st, &mut strings, "enable-method", "psci");
    prop_u32(&mut st, &mut strings, "reg", 0);
    end_node(&mut st);
    end_node(&mut st);

    node(&mut st, "memory@0");
    prop_str(&mut st, &mut strings, "device_type", "memory");
    prop_cells(
        &mut st,
        &mut strings,
        "reg",
        &[0, 0, (memory_bytes >> 32) as u32, memory_bytes as u32],
    );
    end_node(&mut st);

    node(&mut st, "chosen");
    prop_str(&mut st, &mut strings, "bootargs", bootargs);
    if let Some((start, end)) = initrd {
        if start >= end || end > memory_bytes {
            return Err(RelayError::Failed("DTB initrd range is invalid".into()));
        }
        prop_u64(&mut st, &mut strings, "linux,initrd-start", start);
        prop_u64(&mut st, &mut strings, "linux,initrd-end", end);
    }
    end_node(&mut st);

    node(&mut st, "psci");
    prop_str(&mut st, &mut strings, "compatible", "arm,psci-0.2");
    prop_str(&mut st, &mut strings, "method", "hvc");
    end_node(&mut st);

    node(&mut st, "intc@8000000");
    prop_str(&mut st, &mut strings, "compatible", "arm,gic-v3");
    prop_u32(&mut st, &mut strings, "#interrupt-cells", 3);
    prop_empty(&mut st, &mut strings, "interrupt-controller");
    prop_u32(&mut st, &mut strings, "phandle", GIC_PHANDLE);
    prop_cells(
        &mut st,
        &mut strings,
        "reg",
        &[
            0,
            0x0800_0000,
            0,
            0x0001_0000,
            0,
            0x080a_0000,
            0,
            0x0020_0000,
        ],
    );
    end_node(&mut st);

    node(&mut st, "timer");
    prop_str(&mut st, &mut strings, "compatible", "arm,armv8-timer");
    prop_cells(
        &mut st,
        &mut strings,
        "interrupts",
        &[1, 13, 4, 1, 14, 4, 1, 11, 4, 1, 10, 4],
    );
    prop_empty(&mut st, &mut strings, "always-on");
    end_node(&mut st);

    // Early printk before virtio-console is available.  The StaticCpu bus
    // exposes this fixed PL011 window as required by the boot arguments.
    node(&mut st, "pl011@9000000");
    prop_str(&mut st, &mut strings, "compatible", "arm,pl011");
    prop_cells(&mut st, &mut strings, "reg", &[0, 0x0900_0000, 0, 0x1000]);
    prop_cells(&mut st, &mut strings, "interrupts", &[0, 33, 4]);
    prop_u32(&mut st, &mut strings, "clock-frequency", 24_000_000);
    end_node(&mut st);

    node(&mut st, "virtio_mmio@a000000");
    prop_str(&mut st, &mut strings, "compatible", "virtio,mmio");
    prop_cells(&mut st, &mut strings, "reg", &[0, 0x0a00_0000, 0, 0x1000]);
    prop_cells(&mut st, &mut strings, "interrupts", &[0, 32, 4]);
    end_node(&mut st);

    end_node(&mut st);
    be(&mut st, END);
    let off_struct = 56u32;
    let off_strings = off_struct + st.len() as u32;
    let total = off_strings + strings.len() as u32;
    let mut out = Vec::with_capacity(total as usize);
    for value in [
        FDT_MAGIC,
        total,
        off_struct,
        off_strings,
        40,
        17,
        16,
        0,
        strings.len() as u32,
        st.len() as u32,
    ] {
        be(&mut out, value);
    }
    out.extend_from_slice(&[0; 16]); // empty reserve map terminator
    out.extend_from_slice(&st);
    out.extend_from_slice(&strings);
    Ok(out)
}

fn be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn pad(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn string_offset(strings: &mut Vec<u8>, name: &str) -> u32 {
    let o = strings.len() as u32;
    strings.extend_from_slice(name.as_bytes());
    strings.push(0);
    o
}
fn node(out: &mut Vec<u8>, name: &str) {
    be(out, BEGIN_NODE);
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    pad(out);
}
fn end_node(out: &mut Vec<u8>) {
    be(out, END_NODE);
}
fn prop(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str, value: &[u8]) {
    be(out, PROP);
    be(out, value.len() as u32);
    be(out, string_offset(strings, name));
    out.extend_from_slice(value);
    pad(out);
}
fn prop_u32(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str, value: u32) {
    prop(out, strings, name, &value.to_be_bytes());
}
fn prop_u64(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str, value: u64) {
    prop_cells(out, strings, name, &[(value >> 32) as u32, value as u32]);
}
fn prop_empty(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str) {
    prop(out, strings, name, &[]);
}
fn prop_cells(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str, cells: &[u32]) {
    let mut value = Vec::new();
    for cell in cells {
        value.extend_from_slice(&cell.to_be_bytes());
    }
    prop(out, strings, name, &value);
}
fn prop_str(out: &mut Vec<u8>, strings: &mut Vec<u8>, name: &str, value: &str) {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    prop(out, strings, name, &bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtb_has_linux_boot_cpu_interrupt_and_virtio_nodes() {
        let dtb = build(0x20_0000, "console=hvc0", Some((0x10_0000, 0x18_0000))).unwrap();
        assert_eq!(&dtb[..4], &FDT_MAGIC.to_be_bytes());
        assert_eq!(
            u32::from_be_bytes(dtb[4..8].try_into().unwrap()) as usize,
            dtb.len()
        );
        for expected in [
            b"console=hvc0".as_slice(),
            b"arm,arm-v8",
            b"arm,gic-v3",
            b"arm,armv8-timer",
            b"arm,pl011",
            b"virtio,mmio",
            b"linux,initrd-start",
        ] {
            assert!(dtb.windows(expected.len()).any(|window| window == expected));
        }
    }

    #[test]
    fn dtb_rejects_initrd_outside_guest_memory() {
        assert!(build(0x20_0000, "", Some((0x10_0000, 0x30_0000))).is_err());
    }
}

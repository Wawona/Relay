//! Fixed, bounded micro-guests shared by hardware and interpreter runners.
//! This file constructs inputs only. It never computes expected CPU results.
pub const RAM_BYTES: usize = 0x40000;
pub const CODE: u64 = 0x20000;
pub const VECTOR: u64 = 0x28000;
pub const TARGET: u64 = 0x30000;
pub const HVC: u32 = 0xd4000002;
#[derive(Clone, Copy)]
pub enum Access {
    Fetch,
    Read,
    Write,
    Svc,
    Irq,
    Eret(u8),
    MaskedTimer,
}
#[derive(Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub access: Access,
    pub el: u8,
    pub sp0: bool,
    pub leaf: u64,
    pub table: u64,
    pub wxn: bool,
}
pub fn cases() -> Vec<Case> {
    use Access::*;
    let af = (1 << 10) | 3;
    let ro_user = af | (3 << 6);
    vec![
        Case {
            name: "eret_el1_sp0",
            access: Eret(4),
            el: 1,
            sp0: false,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
        Case {
            name: "eret_el1_spx",
            access: Eret(5),
            el: 1,
            sp0: true,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
        Case {
            name: "eret_el0",
            access: Eret(0),
            el: 1,
            sp0: false,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
        Case {
            name: "masked_timer_status",
            access: MaskedTimer,
            el: 1,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el0_irq",
            access: Irq,
            el: 0,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_irq_sp0",
            access: Irq,
            el: 1,
            sp0: true,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_irq_spx",
            access: Irq,
            el: 1,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_table_pxn",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af,
            table: 1 << 59,
            wxn: false,
        },
        Case {
            name: "el0_table_uxn",
            access: Fetch,
            el: 0,
            sp0: false,
            leaf: ro_user,
            table: 1 << 60,
            wxn: false,
        },
        Case {
            name: "el0_table_no_user",
            access: Read,
            el: 0,
            sp0: false,
            leaf: ro_user,
            table: 1 << 61,
            wxn: false,
        },
        Case {
            name: "el1_table_readonly",
            access: Write,
            el: 1,
            sp0: false,
            leaf: af,
            table: 1 << 62,
            wxn: false,
        },
        Case {
            name: "el1_table_no_user_exec",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af | (1 << 6),
            table: 1 << 61,
            wxn: false,
        },
        Case {
            name: "el1_table_readonly_exec",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af | (1 << 6),
            table: 1 << 62,
            wxn: false,
        },
        Case {
            name: "el1_invalid_page_type",
            access: Read,
            el: 1,
            sp0: false,
            leaf: af & !2,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_svc_sp0",
            access: Svc,
            el: 1,
            sp0: true,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_svc_spx",
            access: Svc,
            el: 1,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_pxn",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af | (1 << 53),
            table: 0,
            wxn: false,
        },
        Case {
            name: "el0_uxn",
            access: Fetch,
            el: 0,
            sp0: false,
            leaf: ro_user | (1 << 54),
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_user_writable",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af | (1 << 6),
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_wxn",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: true,
        },
        Case {
            name: "el1_fetch_af",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: 3,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el0_fetch_af",
            access: Fetch,
            el: 0,
            sp0: false,
            leaf: (3 << 6) | 3,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_read_af",
            access: Read,
            el: 1,
            sp0: false,
            leaf: 3,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_write_af_before_ap",
            access: Write,
            el: 1,
            sp0: false,
            leaf: (3 << 6) | 3,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_write_readonly",
            access: Write,
            el: 1,
            sp0: false,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el0_read_privileged",
            access: Read,
            el: 0,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el0_read_allowed",
            access: Read,
            el: 0,
            sp0: false,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_write_allowed",
            access: Write,
            el: 1,
            sp0: false,
            leaf: af,
            table: 0,
            wxn: false,
        },
        Case {
            name: "el1_fetch_allowed",
            access: Fetch,
            el: 1,
            sp0: false,
            leaf: ro_user,
            table: 0,
            wxn: false,
        },
    ]
}
impl Case {
    pub fn saved_pstate(self) -> u64 {
        if let Access::Eret(mode) = self.access {
            0x90000340 | u64::from(mode)
        } else {
            0
        }
    }
    pub fn return_pc(self) -> u64 {
        if matches!(self.access, Access::Eret(_)) {
            self.target()
        } else {
            0
        }
    }
    pub fn target(self) -> u64 {
        if self.table == 0 {
            TARGET
        } else {
            0x2000000
        }
    }

    pub fn pc(self) -> u64 {
        if matches!(self.access, Access::Fetch) {
            self.target()
        } else {
            CODE
        }
    }
    pub fn pstate(self) -> u64 {
        (if matches!(self.access, Access::Irq) {
            0
        } else {
            0x3c0
        }) | if self.el == 0 {
            0
        } else if self.sp0 {
            4
        } else {
            5
        }
    }
    pub fn sctlr(self) -> u64 {
        0x30d00801 | (u64::from(self.wxn) << 19)
    }
    pub fn tcr(self, page: u32) -> u64 {
        16 | (16 << 16) | (1 << 23) | if page == 16384 { 2 << 14 } else { 0 }
    }
    pub fn memory(self, page: u32) -> Vec<u8> {
        let mut ram = vec![0; RAM_BYTES];
        let size = u64::from(page);
        for level in 0..3u64 {
            let descriptor = (level + 1) * size | 3;
            put(&mut ram, level * size, &descriptor.to_le_bytes());
        }
        let shift = if page == 4096 { 12 } else { 14 };
        for address in [CODE, VECTOR, TARGET] {
            let attributes = if address == TARGET {
                self.leaf
            } else {
                (1 << 10) | (3 << 6) | 3
            };
            let entry = 3 * size + ((address >> shift) & (size / 8 - 1)) * 8;
            put(&mut ram, entry, &(address | attributes).to_le_bytes());
        }
        if self.table != 0 {
            let block_shift = if page == 4096 { 21 } else { 25 };
            let index = (self.target() >> block_shift) & (size / 8 - 1);
            put(
                &mut ram,
                2 * size + index * 8,
                &(4 * size | 3 | self.table).to_le_bytes(),
            );
            let index = (self.target() >> shift) & (size / 8 - 1);
            put(
                &mut ram,
                4 * size + index * 8,
                &(TARGET | self.leaf).to_le_bytes(),
            );
        }
        let instruction: u32 = match self.access {
            Access::Svc => 0xd4024681, // SVC #0x1234
            Access::Irq => 0x14000000, // Fixed loop if delivery fails; runner has a timeout.
            Access::Eret(_) => 0xd69f03e0,
            Access::MaskedTimer => 0xd53be320, // MRS X0,CNTV_CTL_EL0.
            Access::Read => 0xf9400020,
            Access::Write => 0xf9000020,
            Access::Fetch => 0xd503201f,
        };
        put(&mut ram, CODE, &instruction.to_le_bytes());
        let stop = if self.el == 0 { 0xd4000001u32 } else { HVC };
        put(&mut ram, CODE + 4, &stop.to_le_bytes());
        put(&mut ram, TARGET, &0x8877665544332211u64.to_le_bytes());
        if matches!(self.access, Access::Fetch | Access::Eret(_)) {
            put(&mut ram, TARGET, &0xd2800540u32.to_le_bytes()); // MOV X0,#42
            let stop = if matches!(self.access, Access::Eret(0)) {
                0xd4000001u32
            } else {
                HVC
            };
            put(&mut ram, TARGET + 4, &stop.to_le_bytes());
        }
        for offset in [0, 0x80, 0x200, 0x280, 0x400, 0x480] {
            put(&mut ram, VECTOR + offset, &HVC.to_le_bytes());
        }
        ram
    }
}
fn put(ram: &mut [u8], address: u64, bytes: &[u8]) {
    ram[address as usize..address as usize + bytes.len()].copy_from_slice(bytes);
}

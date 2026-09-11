//! In-process App Store-class interpreter references.
//!
//! These are **not** iSH.app or UTM SE.app. They model the same *class* of
//! work (threaded / opcode-loop interpreters) so CI always has comparable
//! bars. External CLIs override when present in PATH.

use std::hint::black_box;
use std::time::Instant;

use crate::suites::Sample;

const ITERS: u32 = 1_000_000;

/// Same recurrence as `import/runtime` dispatch_bench: wrap-add / wrap-mul.
fn expected(mut value: u64, count: u64) -> u64 {
    for _ in 0..count {
        value = value.wrapping_add(7).wrapping_mul(3);
    }
    value
}

/// Native host baseline (not an interpreter). Charts may show it as a floor.
pub fn native_recurrence() -> Sample {
    let count = u64::from(ITERS);
    let start = Instant::now();
    let result = expected(black_box(5), black_box(count));
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ITERS);
    assert_eq!(result, expected(5, count));
    Sample {
        name: "ref-native".into(),
        status: "ok".into(),
        ms_per_op: Some(ms),
        note: "host native recurrence floor".into(),
    }
}

/// Asbestos-class: switch-threaded interpreter over a tiny opcode stream.
pub fn asbestos_class_recurrence() -> Sample {
    #[derive(Clone, Copy)]
    enum Op {
        Add7,
        Mul3,
        Dec,
        Ret,
    }
    let program = [Op::Add7, Op::Mul3, Op::Dec, Op::Ret];
    let count = u64::from(ITERS);
    let want = expected(5, count);
    let start = Instant::now();
    let mut value = 5u64;
    let mut left = count;
    let mut pc = 0usize;
    loop {
        match program[pc] {
            Op::Add7 => {
                value = value.wrapping_add(7);
                pc += 1;
            }
            Op::Mul3 => {
                value = value.wrapping_mul(3);
                pc += 1;
            }
            Op::Dec => {
                left = left.saturating_sub(1);
                pc = if left == 0 { 3 } else { 0 };
            }
            Op::Ret => break,
        }
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ITERS);
    black_box(value);
    assert_eq!(value, want);
    Sample {
        name: "ref-asbestos-class".into(),
        status: "ok".into(),
        ms_per_op: Some(ms),
        note: "in-process threaded interpreter (iSH asbestos class)".into(),
    }
}

/// Unicorn-class: heavier per-op bookkeeping (fake CPU context / mem touch).
pub fn unicorn_class_recurrence() -> Sample {
    struct Ctx {
        x0: u64,
        left: u64,
        scratch: [u8; 64],
    }
    let count = u64::from(ITERS);
    let want = expected(5, count);
    let mut ctx = Ctx {
        x0: 5,
        left: count,
        scratch: [0; 64],
    };
    let start = Instant::now();
    while ctx.left > 0 {
        ctx.x0 = ctx.x0.wrapping_add(7).wrapping_mul(3);
        ctx.left -= 1;
        // Unicorn-like: touch a faux MMIO / TB slot each iteration.
        let slot = (ctx.left as usize) % ctx.scratch.len();
        ctx.scratch[slot] = ctx.scratch[slot].wrapping_add(1);
        black_box(ctx.scratch[slot]);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ITERS);
    black_box(ctx.x0);
    assert_eq!(ctx.x0, want);
    Sample {
        name: "ref-unicorn-class".into(),
        status: "ok".into(),
        ms_per_op: Some(ms),
        note: "in-process heavy emu bookkeeping (Unicorn class)".into(),
    }
}

/// TCTI-class: interpreter with an extra decoding layer (bytecode → op).
pub fn tcti_class_recurrence() -> Sample {
    // Bytecode: 0=add7, 1=mul3, 2=loop_or_end
    let code: &[u8] = &[0, 1, 2];
    let count = u64::from(ITERS);
    let want = expected(5, count);
    let mut value = 5u64;
    let mut left = count;
    let mut pc = 0usize;
    let start = Instant::now();
    loop {
        let op = code[pc];
        match op {
            0 => {
                value = value.wrapping_add(7);
                pc += 1;
            }
            1 => {
                value = value.wrapping_mul(3);
                pc += 1;
            }
            2 => {
                left = left.saturating_sub(1);
                if left == 0 {
                    break;
                }
                pc = 0;
            }
            _ => unreachable!(),
        }
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ITERS);
    black_box(value);
    assert_eq!(value, want);
    Sample {
        name: "ref-tcti-class".into(),
        status: "ok".into(),
        ms_per_op: Some(ms),
        note: "in-process bytecode decode loop (UTM-SE TCTI class)".into(),
    }
}

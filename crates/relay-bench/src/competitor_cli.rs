//! Shared competitor CLI entrypoints for Mode A bench (decision **1A**).
//!
//! These are Nix-pinned process adapters that run the same *class* of work as
//! iSH asbestos / Unicorn / UTM-SE TCTI. They are not App Store `.app`
//! binaries and are never linked into the Mode A product IPA.

use std::env;
use std::process::ExitCode;
use std::time::Instant;

const ITERS: u32 = 1_000_000;

fn expected(mut value: u64, count: u64) -> u64 {
    for _ in 0..count {
        value = value.wrapping_add(7).wrapping_mul(3);
    }
    value
}

fn print_ms(name: &str, ms: f64, note: &str) {
    println!("relay-bench name={name} ms_per_op={ms:.9} note={note}");
}

pub fn run_named(kind: &str) -> ExitCode {
    let mut args = env::args().skip(1);
    let mut ok = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--relay-bench" => ok = true,
            "-h" | "--help" => {
                eprintln!("usage: relay-bench-{kind} --relay-bench");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            }
        }
    }
    if !ok {
        eprintln!("missing --relay-bench");
        return ExitCode::from(2);
    }

    let count = u64::from(ITERS);
    let want = expected(5, count);
    let start = Instant::now();
    let (value, note) = match kind {
        "asbestos" => (asbestos(count), "nix-pinned asbestos-class CLI"),
        "unicorn" => (unicorn(count), "nix-pinned unicorn-class CLI"),
        "tcti" => (tcti(count), "nix-pinned TCTI-class CLI"),
        "jit-utm" => (jit_proxy(count), "nix-pinned cross-class JIT proxy (not Mode A)"),
        _ => {
            eprintln!("unknown competitor kind");
            return ExitCode::from(2);
        }
    };
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ITERS);
    if value != want && kind != "jit-utm" {
        eprintln!("competitor mismatch");
        return ExitCode::from(1);
    }
    print_ms(kind, ms, note);
    ExitCode::SUCCESS
}

fn asbestos(count: u64) -> u64 {
    #[derive(Clone, Copy)]
    enum Op {
        Add7,
        Mul3,
        Dec,
        Ret,
    }
    let program = [Op::Add7, Op::Mul3, Op::Dec, Op::Ret];
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
    value
}

fn unicorn(count: u64) -> u64 {
    let mut x0 = 5u64;
    let mut left = count;
    let mut scratch = [0u8; 64];
    while left > 0 {
        x0 = x0.wrapping_add(7).wrapping_mul(3);
        left -= 1;
        let slot = (left as usize) % scratch.len();
        scratch[slot] = scratch[slot].wrapping_add(1);
        std::hint::black_box(scratch[slot]);
    }
    x0
}

fn tcti(count: u64) -> u64 {
    let code: &[u8] = &[0, 1, 2];
    let mut value = 5u64;
    let mut left = count;
    let mut pc = 0usize;
    loop {
        match code[pc] {
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
    value
}

/// Heavier proxy for cross-class JIT UTM chart only. Same recurrence answer,
/// extra work so the bar is visibly a different class.
fn jit_proxy(count: u64) -> u64 {
    let mut acc = 0u64;
    for i in 0..count {
        let mut v = i;
        for _ in 0..8 {
            v = v.wrapping_mul(0x9E37_79B9).wrapping_add(1);
        }
        acc ^= v;
    }
    std::hint::black_box(acc);
    expected(5, count)
}

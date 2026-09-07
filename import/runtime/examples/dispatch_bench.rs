//! Dispatch microbenchmark, not a UTM or WASI benchmark.
use std::{hint::black_box, time::Instant};
use wwn_runtime::{Op::*, Optimization, Program};

fn native(mut value: u64, count: u64) -> u64 {
    for _ in 0..count {
        value = value.wrapping_add(7).wrapping_mul(3);
    }
    value
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = 1_000_000;
    let ops = [
        Add {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Mul {
            dst: 0,
            lhs: 0,
            rhs: 2,
        },
        Add {
            dst: 3,
            lhs: 3,
            rhs: 4,
        },
        JumpIf {
            condition: 3,
            target: 0,
        },
        Return { src: 0 },
    ];
    let programs = [
        Program::compile(&ops, 5, false)?,
        Program::compile(&ops, 5, true)?,
        Program::compile_optimized(&ops, 5, Optimization::Semantic)?,
    ];
    let args = [5, 7, 3, iterations, u64::MAX];
    let fuel = 4 * iterations + 1;
    let expected = native(args[0], iterations);
    for p in &programs {
        assert_eq!(p.run(&args, &mut [], fuel)?.value, expected);
    }
    let mut samples = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for round in 0..12 {
        // Alternate execution order to reduce thermal/order bias; discard warmup.
        for offset in 0..4 {
            let engine = (round + offset) % 4;
            let start = Instant::now();
            let result = if engine < 3 {
                programs[engine].run(black_box(&args), &mut [], fuel)?.value
            } else {
                native(black_box(args[0]), black_box(iterations))
            };
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(black_box(result), expected);
            if round > 0 {
                samples[engine].push(elapsed);
            }
        }
    }
    for (engine, values) in [
        "static-unfused",
        "static-fused",
        "static-semantic",
        "native-reference",
    ]
    .iter()
    .zip(samples)
    {
        println!("{{\"benchmark\":\"wrapping-recurrence-v1\",\"engine\":\"{engine}\",\"host_os\":\"{}\",\"host_arch\":\"{}\",\"iterations\":{iterations},\"result\":{expected},\"elapsed_ns\":{values:?}}}", std::env::consts::OS, std::env::consts::ARCH);
    }
    Ok(())
}

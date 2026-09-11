//! Mode A suites: page geometry, StaticCpu timing, class refs, CLI probes.

use crate::chart::Bar;
use crate::refs;
use relay_core::{GuestPageSize, HostPageSize};
use relay_vm::{GuestMemory, PageTranslate};
use serde::Serialize;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub name: String,
    pub status: String,
    pub ms_per_op: Option<f64>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub host_page_kib: u32,
    pub gate_pass: bool,
    pub samples: Vec<Sample>,
}

impl BenchReport {
    /// 2A: App Store-class interpreters only (Mode A gate chart).
    pub fn interpreter_bars(&self) -> Vec<Bar> {
        self.samples
            .iter()
            .filter(|s| {
                if s.name.contains("jit") {
                    return false;
                }
                s.name.starts_with("relay-static")
                    || s.name == "ref-native"
                    || s.name == "ref-asbestos-class"
                    || s.name == "ref-unicorn-class"
                    || s.name == "ref-tcti-class"
                    || s.name == "competitor-asbestos"
                    || s.name == "competitor-unicorn"
                    || s.name == "competitor-tcti"
            })
            .map(sample_bar)
            .collect()
    }

    pub fn page_bars(&self) -> Vec<Bar> {
        self.samples
            .iter()
            .filter(|s| s.name.starts_with("page-"))
            .map(sample_bar)
            .collect()
    }

    /// 2B: Mode A bars plus JIT UTM (explicitly labeled cross-class).
    pub fn cross_class_bars(&self) -> Vec<Bar> {
        let mut bars = self.interpreter_bars();
        for sample in &self.samples {
            if sample.name.contains("jit") {
                bars.push(sample_bar(sample));
            }
        }
        bars
    }
}

fn sample_bar(s: &Sample) -> Bar {
    Bar {
        label: s.name.clone(),
        value: s.ms_per_op.unwrap_or(0.0),
        skipped: s.status != "ok",
    }
}

pub fn run_all(strict_competitors: bool) -> BenchReport {
    let host = HostPageSize::detect();
    let mut samples = Vec::new();

    samples.extend(page_geometry_suite(host));
    samples.push(static_cpu_walk(host));
    samples.push(refs::native_recurrence());
    samples.push(refs::asbestos_class_recurrence());
    samples.push(refs::unicorn_class_recurrence());
    samples.push(refs::tcti_class_recurrence());

    // 1A: open-source / PATH CLIs when present.
    samples.push(probe_competitor(
        "competitor-asbestos",
        &["relay-bench-asbestos", "ish-asbestos", "asbestos-bench"],
        "iSH asbestos-class CLI not in PATH (using ref-asbestos-class)",
    ));
    samples.push(probe_competitor(
        "competitor-unicorn",
        &["relay-bench-unicorn", "unicorn-bench"],
        "Unicorn-class CLI not in PATH (using ref-unicorn-class)",
    ));
    samples.push(probe_competitor(
        "competitor-tcti",
        &[
            "relay-bench-tcti",
            "qemu-system-aarch64-tcti",
            "qemu-tcti-bench",
        ],
        "UTM-SE TCTI CLI not in PATH (using ref-tcti-class; never product)",
    ));
    // 2B: JIT UTM lane (never Mode A gate).
    samples.push(probe_competitor(
        "cross-class-jit-utm",
        &[
            "relay-bench-jit-utm",
            "utm-jit-bench",
            "qemu-system-aarch64",
        ],
        "JIT UTM / HVF CLI not in PATH (cross-class chart only)",
    ));
    samples.push(Sample {
        name: "microvm-nixos".into(),
        status: "skipped".into(),
        ms_per_op: None,
        note: "StaticCpu NixOS boot still planned".into(),
    });
    samples.push(Sample {
        name: "container-in-vm".into(),
        status: "skipped".into(),
        ms_per_op: None,
        note: "Requires StaticCpu NixOS boot".into(),
    });

    let relay_ok = samples
        .iter()
        .filter(|s| {
            s.name.starts_with("relay-")
                || s.name.starts_with("page-")
                || s.name.starts_with("ref-")
        })
        .all(|s| s.status == "ok");
    let mode_a_competitors_missing = samples
        .iter()
        .filter(|s| {
            matches!(
                s.name.as_str(),
                "competitor-asbestos" | "competitor-unicorn" | "competitor-tcti"
            )
        })
        .any(|s| s.status == "skipped");
    // JIT UTM missing must not fail Mode A gate (2A).
    let gate_pass = relay_ok && !(strict_competitors && mode_a_competitors_missing);

    BenchReport {
        host_page_kib: host.0 / 1024,
        gate_pass,
        samples,
    }
}

fn page_geometry_suite(host: HostPageSize) -> Vec<Sample> {
    let mut out = Vec::new();
    for guest in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
        let translate = PageTranslate::new(guest, host).unwrap();
        let label = format!("page-{}", translate.pair_label());
        let guest_bytes = guest.0 as u64 * 256;
        let start = Instant::now();
        let memory = match GuestMemory::allocate_on_host(guest, host, guest_bytes) {
            Ok(m) => m,
            Err(error) => {
                out.push(Sample {
                    name: label,
                    status: "fail".into(),
                    ms_per_op: None,
                    note: error.to_string(),
                });
                continue;
            }
        };
        let mut sink = 0u64;
        let iters = 50_000u32;
        for i in 0..iters {
            let gpa = ((i as u64) * 64) % (guest_bytes.saturating_sub(8));
            let offset = translate.guest_to_host_offset(gpa).unwrap();
            sink ^= offset;
            let mut byte = [0u8];
            let _ = memory.read(gpa, &mut byte);
            sink ^= byte[0] as u64;
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);
        std::hint::black_box(sink);
        out.push(Sample {
            name: label,
            status: "ok".into(),
            ms_per_op: Some(ms),
            note: format!(
                "arena={} guest_len={}",
                memory.host_arena_len(),
                memory.len()
            ),
        });
    }
    out
}

fn static_cpu_walk(host: HostPageSize) -> Sample {
    let guest = if host.0 >= 16384 {
        GuestPageSize::SIXTEEN_KIB
    } else {
        GuestPageSize::FOUR_KIB
    };
    let bytes = guest.0 as u64 * 512;
    let start = Instant::now();
    let mut memory = match GuestMemory::allocate_on_host(guest, host, bytes) {
        Ok(m) => m,
        Err(error) => {
            return Sample {
                name: "relay-static-cpu".into(),
                status: "fail".into(),
                ms_per_op: None,
                note: error.to_string(),
            };
        }
    };
    let payload = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let iters = 20_000u32;
    for i in 0..iters {
        let addr = ((i as u64) * 128) % (bytes - 16);
        let _ = memory.write(addr, &payload);
        let mut out = [0u8; 8];
        let _ = memory.read(addr, &mut out);
        std::hint::black_box(out);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);
    Sample {
        name: "relay-static-cpu".into(),
        status: "ok".into(),
        ms_per_op: Some(ms),
        note: "Mode A StaticCpu memory walk".into(),
    }
}

fn probe_competitor(name: &str, candidates: &[&str], missing_note: &str) -> Sample {
    for bin in candidates {
        if which(bin) {
            let status = Command::new(bin).arg("--relay-bench").output();
            match status {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let ms = parse_ms_per_op(&stdout).unwrap_or_else(|| {
                        // Fallback: whole-process wall time if adapter is silent.
                        0.0
                    });
                    let ms = if ms > 0.0 {
                        ms
                    } else {
                        // Silent adapters: not comparable; mark fail so CI notices.
                        return Sample {
                            name: name.into(),
                            status: "fail".into(),
                            ms_per_op: None,
                            note: format!("{bin} missing ms_per_op= in stdout"),
                        };
                    };
                    return Sample {
                        name: name.into(),
                        status: "ok".into(),
                        ms_per_op: Some(ms),
                        note: format!("ran {bin}"),
                    };
                }
                Ok(output) => {
                    return Sample {
                        name: name.into(),
                        status: "fail".into(),
                        ms_per_op: None,
                        note: format!("{bin} exit {}", output.status.code().unwrap_or(-1)),
                    };
                }
                Err(error) => {
                    return Sample {
                        name: name.into(),
                        status: "fail".into(),
                        ms_per_op: None,
                        note: error.to_string(),
                    };
                }
            }
        }
    }
    Sample {
        name: name.into(),
        status: "skipped".into(),
        ms_per_op: None,
        note: missing_note.into(),
    }
}

fn parse_ms_per_op(stdout: &str) -> Option<f64> {
    for part in stdout.split_whitespace() {
        if let Some(rest) = part.strip_prefix("ms_per_op=") {
            return rest.parse().ok();
        }
    }
    None
}

fn which(bin: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {bin}"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

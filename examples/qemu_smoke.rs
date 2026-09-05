//! Real x86 guest arithmetic through QEMU; no guest kernel or OCI claim.
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn boot_sector(iterations: u32) -> Vec<u8> {
    assert!(iterations > 0);
    // Real-mode guest code is data interpreted by QEMU, never host code.
    let mut b = vec![0xfa, 0x31, 0xc0, 0x8e, 0xd8, 0x8e, 0xd0, 0xbc, 0, 0x7c];
    b.extend([0x66, 0xb9]); // mov ecx, iterations
    b.extend(iterations.to_le_bytes());
    b.extend([0x66, 0xb8, 5, 0, 0, 0]); // mov eax, 5
    b.extend([
        0x66, 0x83, 0xc0, 7, 0x66, 0x6b, 0xc0, 3, 0x66, 0x49, 0x75, 0xf4,
    ]);
    let mut expected = 5u32;
    for _ in 0..iterations {
        expected = expected.wrapping_add(7).wrapping_mul(3);
    }
    b.extend([0x66, 0x3d]); // cmp eax, expected
    b.extend(expected.to_le_bytes());
    b.extend([0x75, 0]); // jne failure
    let branch = b.len() - 1;
    b.extend([0xba, 0xe9, 0]); // debug console port
    for byte in b"WWN_QEMU_ARITHMETIC_OK\n" {
        b.extend([0xb0, *byte, 0xee]);
    }
    b.extend([0xba, 0xf4, 0, 0xb0, 0x10, 0xee]); // isa-debug-exit -> status 33
    b[branch] = u8::try_from(b.len() - branch - 1).unwrap();
    b.extend([0xfa, 0xf4, 0xeb, 0xfd]); // failure: halt forever, host timeout
    b.resize(510, 0);
    b.extend([0x55, 0xaa]);
    b
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qemu = std::env::args_os()
        .nth(1)
        .ok_or("usage: qemu_smoke QEMU_SYSTEM_X86_64")?;
    let dir = std::env::temp_dir().join(format!("wwn-qemu-smoke-{}", std::process::id()));
    fs::create_dir(&dir)?;
    let result = run(qemu, &dir);
    let _ = fs::remove_dir_all(&dir);
    result
}

fn run(qemu: std::ffi::OsString, dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let disk = dir.join("boot.img");
    let log = dir.join("guest.log");
    fs::write(&disk, boot_sector(1_000_000))?;
    let mut samples = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let mut child = Command::new(&qemu)
            .args([
                "-machine",
                "pc,accel=tcg",
                "-m",
                "32",
                "-display",
                "none",
                "-monitor",
                "none",
                "-serial",
                "none",
                "-no-reboot",
            ])
            .args(["-drive", &format!("format=raw,file={}", disk.display())])
            .args([
                "-debugcon",
                &format!("file:{}", log.display()),
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() > Duration::from_secs(180) {
                child.kill()?;
                child.wait()?;
                return Err("guest timeout (arithmetic failure or boot failure)".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if status.code() != Some(33) || fs::read_to_string(&log)? != "WWN_QEMU_ARITHMETIC_OK\n" {
            return Err(format!("guest verification failed: {status}").into());
        }
        samples.push(start.elapsed().as_nanos());
    }
    println!("{{\"benchmark\":\"qemu-x86-cold-boot-arithmetic-v1\",\"iterations\":1000000,\"elapsed_ns\":{samples:?},\"correctness\":\"passed\"}}");
    Ok(())
}

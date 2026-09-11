//! Mode A App Store-safe Relay microbenches.
//!
//! Decisions locked: **1A** (CI = open-source engine CLIs + in-process class
//! refs; device apps later) and **2A+2B** (Mode A gate = interpreter class
//! only; a second chart shows JIT UTM labeled as a different class).

mod chart;
mod refs;
mod suites;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use suites::{run_all, BenchReport};

/// Consecutive Mode A gate passes required before marketing eligibility.
const MARKETING_STREAK_N: u32 = 5;

fn main() -> ExitCode {
    let mut out = PathBuf::from("bench-out");
    let mut strict = false;
    let mut streak_in: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                out = PathBuf::from(args.next().expect("--out needs a path"));
            }
            "--streak-in" => {
                streak_in = Some(PathBuf::from(
                    args.next().expect("--streak-in needs a path"),
                ));
            }
            "--strict-competitors" => strict = true,
            "-h" | "--help" => {
                eprintln!(
                    "usage: relay-mode-a-bench [--out DIR] [--streak-in FILE] [--strict-competitors]\n\
                     Mode A StaticCpu + page-geometry vs App Store-class interpreters.\n\
                     Cross-class chart includes JIT UTM (labeled; not Mode A gate).\n\
                     Marketing eligibility requires {MARKETING_STREAK_N} consecutive gate passes."
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            }
        }
    }

    if let Err(error) = fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {error}", out.display());
        return ExitCode::from(1);
    }

    let prev_streak = streak_in
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("consecutivePass").and_then(|x| x.as_u64()))
        .unwrap_or(0) as u32;

    let report = run_all(strict);
    let consecutive = if report.gate_pass {
        prev_streak.saturating_add(1)
    } else {
        0
    };
    let marketing_eligible = report.gate_pass && consecutive >= MARKETING_STREAK_N;

    if let Err(error) = write_report(&out, &report, consecutive, marketing_eligible) {
        eprintln!("write failed: {error}");
        return ExitCode::from(1);
    }

    println!(
        "relay-mode-a-bench host_page={}KiB samples={} gate={} streak={}/{} marketing={}",
        report.host_page_kib,
        report.samples.len(),
        if report.gate_pass { "PASS" } else { "FAIL" },
        consecutive,
        MARKETING_STREAK_N,
        if marketing_eligible {
            "eligible"
        } else {
            "no"
        }
    );
    for sample in &report.samples {
        println!(
            "  {} status={} ms={:?} note={}",
            sample.name, sample.status, sample.ms_per_op, sample.note
        );
    }

    if report.gate_pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn write_report(
    out: &Path,
    report: &BenchReport,
    consecutive: u32,
    marketing_eligible: bool,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    fs::write(out.join("results.json"), json).map_err(|e| e.to_string())?;

    fs::write(
        out.join("mode-a-interpreters.svg"),
        chart::bar_chart(
            "Mode A vs App Store-class interpreters (lower is better)",
            &report.interpreter_bars(),
        ),
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        out.join("mode-a-page-geom.svg"),
        chart::bar_chart(
            "Mode A page geometry map-walk (lower is better)",
            &report.page_bars(),
        ),
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        out.join("cross-class-incl-jit-utm.svg"),
        chart::bar_chart(
            "Cross-class (Mode A + JIT UTM labeled; not Mode A gate)",
            &report.cross_class_bars(),
        ),
    )
    .map_err(|e| e.to_string())?;

    let gate_msg = if report.gate_pass {
        format!("pass ({consecutive}/{MARKETING_STREAK_N})")
    } else {
        "fail".into()
    };
    let gate = serde_json::json!({
        "schemaVersion": 1,
        "label": "Mode A bench",
        "message": gate_msg,
        "color": if report.gate_pass { "brightgreen" } else { "red" },
    });
    fs::write(
        out.join("gate.json"),
        serde_json::to_string_pretty(&gate).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let streak = serde_json::json!({
        "schemaVersion": 1,
        "consecutivePass": consecutive,
        "requiredForMarketing": MARKETING_STREAK_N,
        "marketingEligible": marketing_eligible,
        "gatePass": report.gate_pass,
    });
    fs::write(
        out.join("streak.json"),
        serde_json::to_string_pretty(&streak).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Shields endpoint: never say "world's fastest" until streak threshold.
    let marketing = if marketing_eligible {
        serde_json::json!({
            "schemaVersion": 1,
            "label": "Mode A interpreters",
            "message": "fastest-eligible",
            "color": "blue",
        })
    } else {
        serde_json::json!({
            "schemaVersion": 1,
            "label": "Mode A interpreters",
            "message": format!("streak {consecutive}/{MARKETING_STREAK_N}"),
            "color": "lightgrey",
        })
    };
    fs::write(
        out.join("marketing.json"),
        serde_json::to_string_pretty(&marketing).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let mut summary = String::from("# Mode A bench summary\n\n");
    summary.push_str(&format!(
        "Host page: {} KiB. Mode A gate: **{}**. Streak: **{}/{}**. Marketing: **{}**.\n\n",
        report.host_page_kib,
        if report.gate_pass { "PASS" } else { "FAIL" },
        consecutive,
        MARKETING_STREAK_N,
        if marketing_eligible {
            "eligible (still no unmeasured world's-fastest copy)"
        } else {
            "not eligible"
        }
    ));
    summary.push_str(
        "Decisions: **1A** (engine CLIs + class refs). **2A+2B** (interpreter gate + labeled JIT chart).\n\n",
    );
    summary.push_str("| Sample | Status | ms/op | Note |\n|---|---|---|---|\n");
    for sample in &report.samples {
        summary.push_str(&format!(
            "| {} | {} | {:?} | {} |\n",
            sample.name, sample.status, sample.ms_per_op, sample.note
        ));
    }
    summary.push_str(
        "\nMethodology: `docs/mode-a-bench.md`. Mode A gate ignores JIT UTM.\n",
    );
    fs::write(out.join("SUMMARY.md"), summary).map_err(|e| e.to_string())?;
    Ok(())
}

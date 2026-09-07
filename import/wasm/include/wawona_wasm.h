#ifndef WAWONA_WASM_H
#define WAWONA_WASM_H

#ifdef __cplusplus
extern "C" {
#endif

/*
 * In-process WASI P1/P2 runner. Linked from wawona-dispatch.c (weak).
 * .wasm files are user documents — not Mach-O, not Apple-signed.
 *
 * Apple mobile builds must use the Pulley interpreter (no Cranelift native,
 * no MAP_JIT). See Wawona/docs/wasm-wasi.md and milestone
 * "Support WASI P1 P2 WASM!".
 */

/* 1 if path is a readable WASM core module or component (\0asm). */
int wawona_wasm_can_run(const char *path);

/*
 * Run argv[0] (or argv[1] when argv[0] is "wasm") as a WASI program.
 * Stdio is the current fds (PTY slave). FS preopen is the sandbox root.
 * Returns the guest exit code, or 127 on host error.
 */
int wawona_wasm_run(int argc, char **argv);

/* Terminal raw/cooked bit owned by the current WASM process (0 cooked, 1 raw). */
int wawona_terminal_raw_enabled(void);

#ifdef __cplusplus
}
#endif

#endif /* WAWONA_WASM_H */

# Runtime binary hang investigation

Date: 2026-06-06

## Summary

Runtime validation commands that execute local binaries hung before user code
ran. Compilation still worked.

This was an environment/local-executable launch issue, not an `fxl` checker
regression. Rebooting macOS cleared the hang, and runtime validation works
again.

## Evidence

Passing compile-only commands:

```sh
cargo check
cargo test --no-run
```

Commands that time out in this shell:

```sh
cargo test -p fx-cli --bin fx -- --list
cargo test -p fx-core --test phase1 -- --list
target/debug/fx
target/debug/fx check examples/phase-1/lambda.fx
```

The same behavior also happens for temporary non-repo executables:

```sh
cc /private/tmp/fxl-rust-runtime-probe/probe.c -o /private/tmp/fxl-rust-runtime-probe/probe-c
/private/tmp/fxl-rust-runtime-probe/probe-c
```

The failure is broader than Rust or Mach-O binaries. Directly executing files
created or copied in writable Codex paths also hangs:

```sh
cp /usr/bin/true /private/tmp/fxl-true-probe
/private/tmp/fxl-true-probe

printf '#!/bin/sh\necho script-ok\n' > /private/tmp/fxl-script-probe/run.sh
chmod +x /private/tmp/fxl-script-probe/run.sh
/private/tmp/fxl-script-probe/run.sh
```

Invoking the script through an existing system interpreter works:

```sh
/bin/sh /private/tmp/fxl-script-probe/run.sh
```

Sampling both the temporary C binary and `target/debug/fx` shows the process
stuck before program entry:

```text
_dyld_start  (in dyld)
```

Removing `com.apple.provenance`, ad-hoc re-signing, and launching through
`arch -arm64` did not change the behavior. Attempts to remove
`com.apple.provenance` from copied/local executables in this context do not
stick; the attribute remains present.

Running `target/debug/fx --help` outside the managed filesystem sandbox still
hangs from this thread, so the issue is tied to the Codex-launched process
context rather than only workspace sandbox permissions.

## Resolution

Rebooting macOS restored local executable launch. After reboot:

```sh
cargo fmt --all --check
cargo test
cargo run -q -p fx-cli -- check examples/phase-2/rows.fx
```

all complete normally.

If the hang returns, retry the tiny executable probe first:

```sh
mkdir -p /private/tmp/rust-exec-probe
cd /private/tmp/rust-exec-probe
cat > main.rs <<'EOF'
fn main() {
    println!("hello");
}
EOF
rustc main.rs -o hello
./hello
```

If `./hello` hangs, reboot before assuming a repository or toolchain
regression.

## Revisit when

- Local executable launch hangs again after a reboot.
- Runtime validation fails for a repository-specific reason.

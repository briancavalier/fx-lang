# Runtime binary hang investigation

Date: 2026-06-06

## Summary

Runtime validation commands that execute local binaries hang in the current
Codex shell before user code runs. Compilation still works.

This is an environment/local-executable launch issue, not an `fxl` checker
regression.

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

## Current validation workaround

Until local executable launch works in this shell, use compile-only validation
inside Codex:

```sh
cargo fmt --all --check
cargo check
cargo test --no-run
```

Run runtime validation such as `cargo test` and `fx check ...` in an external
terminal, or in a Codex environment where local executables can launch.

Do not rely on direct execution of any newly built or copied executable file
from this Codex thread. Shell scripts may be inspected by running them through
an existing interpreter such as `/bin/sh script.sh`, but Rust test binaries and
CLI binaries still require an external terminal or CI runtime.

## Revisit when

- Local executable launch behavior changes in Codex.
- Runtime validation is needed before committing a feature slice.
- A future shell can run `target/debug/fx` and test binaries normally.

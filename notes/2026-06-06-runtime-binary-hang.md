# Runtime binary hang investigation

Date: 2026-06-06

## Summary

Runtime validation commands that execute local binaries hang in the current
Codex shell before user code runs. Compilation still works.

This appears to be an environment/local-executable launch issue, not an `fxl`
checker regression.

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

Sampling both the temporary C binary and `target/debug/fx` shows the process
stuck before program entry:

```text
_dyld_start  (in dyld)
```

Removing `com.apple.provenance`, ad-hoc re-signing, and launching through
`arch -arm64` did not change the behavior.

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

## Revisit when

- Local executable launch behavior changes in Codex.
- Runtime validation is needed before committing a feature slice.
- A future shell can run `target/debug/fx` and test binaries normally.

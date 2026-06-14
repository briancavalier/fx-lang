# Phase 6B learnings

Date: 2026-06-14

## Context

Phase 6B turns the Phase 6A branch-lifetime contract into executable
resourceful context boundaries.

## Learnings

- `finally(cleanup)` and `given each(init, cleanup)` need explicit checked and
  lowered nodes. Treating them as ordinary calls would hide lifecycle ownership
  from the runtime.
- The existing contextual insertion machinery can still be reused: `given each`
  lowers provider fields as projections from a per-entry provider record.
- Cleanup on ignored resumes requires the runtime finalizer boundary to wrap
  operation requests. Cleanup cannot be implemented only as a JavaScript
  `finally` around the generated body.
- Initial escape checking is intentionally conservative. Phase 6B rejects direct
  return of a branch-local contextual name and leaves full region inference for
  later scope work.

## Validation

- `cargo test -p fx-syntax phase_6b`
- `cargo test -p fx-core phase_6b`
- `cargo test -p fx-compiler phase_6b`
- `cargo run -q -p fx-cli -- check examples/phase-6b`
- `cargo run -q -p fx-cli -- fmt --check examples`

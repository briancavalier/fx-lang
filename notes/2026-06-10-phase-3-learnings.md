# Phase 3 learnings

Date: 2026-06-10

## Purpose

This note records implementation learnings and decisions from Phase 3 of the
implementation plan in `notes/2026-05-23-implementation-phases.md`.

Use it for decisions, surprises, rejected approaches, and implications for
later phases. Keep entries short, concrete, and tied to the implementation
evidence that produced them.

## Entry template

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

## Entries

- Decision: Start Phase 3 with goal-target effect examples before parser and
  checker support.
- Context: `examples/phase-3/` documents the intended ordinary effect surface:
  `effect` declarations, operation calls as ordinary function calls, result
  effect rows using `with <...>`, function types with effect rows, handler
  clauses, explicit `resume`, imported operations, and prefix trailing-action
  handler use. These examples started as goal targets and are promoted to
  `fx check` fixtures as each Phase 3 checker slice lands.
- Applies to phase(s): Phase 3.
- Future implication: Promote focused pieces of the Phase 3 fixtures into
  parser, checker, diagnostics, hover, go-to-definition, and completion tests
  as support lands. Keep JavaScript execution expectations in Phase 3.1.
- Revisit when: Effect declarations, effect-row type syntax, operation
  resolution, and handler checking are implemented.

- Decision: Accept ordinary `effect` declarations in the surface AST before
  checker support.
- Context: The parser now accepts exported and non-exported `effect`
  declarations, effect type parameters, and indented operation signatures.
  `fx-core` indexes effect declarations and operation signatures as distinct
  symbol kinds, while checker/runtime semantics remain deferred.
- Applies to phase(s): Phase 3.
- Future implication: Operation resolution and checker typing can consume
  parsed `EffectDecl` and `OperationDecl` nodes instead of reconstructing
  operation signatures from value declarations.
- Revisit when: Operation symbols need typed spans, hover display, or
  declaration-specific checker diagnostics.

- Decision: Parse result effect rows as explicit type syntax before effect-row
  checking.
- Context: Type syntax now accepts forms such as `a with <Raise>`,
  `a with <Raise, ...e>`, and `() -> a with <Raise, ...e>`. The AST stores an
  effect row separately from record and variant rows.
- Applies to phase(s): Phase 3.
- Future implication: The checker lowers this syntax to effectful result
  types. During Phase 3, `fx compile --target js` rejected effectful programs
  until Phase 3.1 added runtime support.
- Revisit when: Internal effect-row representation and unhandled-effect
  diagnostics are implemented.

- Decision: Track performed effects by splitting expression results into value
  type plus effect row at evaluation sites.
- Context: Calls to operations or effectful functions return effectful result
  types. The checker now peels those rows from evaluated subexpressions in
  blocks, `if`, `match`, records, lists, record updates, calls, and binary
  operators, then reattaches the merged row to the enclosing expression.
  Lambda and trailing-action thunk construction keep body effects on the
  function result instead of leaking them to the construction site.
- Applies to phase(s): Phase 3.
- Future implication: Handler checking can consume the same value/effect split
  to remove handled effects and add clause effects without changing the
  surface AST.
- Revisit when: Handler effect removal and resume typing are implemented.

- Decision: Surface effect metadata through the same typed-span and indexed
  symbol paths as earlier language features.
- Context: Effect declarations and operation declarations now receive checker
  typed spans for hover, and the LSP maps indexed effect and operation symbols
  into document symbols, workspace symbols, and completion items. Completion is
  intentionally index-based in this phase; more precise lexical filtering can
  be added later without changing effect metadata.
- Applies to phase(s): Phase 3.
- Future implication: Phase 4 scoped effects and Phase 5 coeffects should add
  metadata to these same analysis paths instead of inventing parallel LSP
  plumbing.
- Revisit when: Completion becomes scope-sensitive or needs imported-module
  qualification edits.

- Decision: Treat Phase 3 as complete when effect syntax, checking, handler
  diagnostics, LSP-adjacent metadata, CLI check, and JavaScript runtime gating
  all pass together.
- Context: The final sweep passed `cargo fmt --all --check`, `cargo test
  --no-run`, `cargo check`, `cargo test -p fx-syntax`, `cargo test -p
  fx-core`, `cargo test -p fx-compiler`, `cargo test -p fx-cli`, `cargo test
  -p fx-lsp`, `cargo run -q -p fx-cli -- check examples`, and `cargo run -q
  -p fx-cli -- compile --target js examples/phase-2.6`. At Phase 3 completion,
  `cargo run -q -p fx-cli -- compile --target js examples/phase-3` rejected
  with `unsupported-effect-runtime`, as required before Phase 3.1.
- Applies to phase(s): Phase 3 and Phase 3.1.
- Future implication: Phase 3.1 can start from checked handler/effect programs
  and focus on executable JavaScript lowering/runtime support.
- Revisit when: Phase 3.1 adds JavaScript runtime support for the same
  examples.

- Decision: Parse handler expressions before implementing handler typing.
- Context: The parser now accepts `handle action()` expressions with operation
  clauses, explicit resume parameters, and a `return` clause. Resolver and
  local-definition analysis walk handler action and clause bodies. The checker
  uses operation signatures to type handler clause payload parameters, explicit
  `resume` parameters, and return clauses, and removes handled operation
  effects from the action row. The JavaScript compiler reports
  `unsupported-effect-runtime` for handlers until Phase 3.1.
- Applies to phase(s): Phase 3 and Phase 3.1.
- Future implication: Handler checking can stay source-level while Phase 3.1
  lowers the same action, clause, return, and resume shape into executable
  JavaScript runtime protocol operations.
- Revisit when: Missing, duplicate, or unknown handler clause diagnostics need
  effect-specific wording, or when Phase 3.1 JavaScript runtime lowering is
  implemented.

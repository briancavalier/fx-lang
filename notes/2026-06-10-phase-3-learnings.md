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
  handler use. These examples are not expected-passing fixtures yet.
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
  effect row separately from record and variant rows, but checker lowering
  still erases it to the underlying result type until the effect-row
  representation lands.
- Applies to phase(s): Phase 3.
- Future implication: The checker should replace this temporary erasure with a
  real effect-row-aware type representation, and `fx compile --target js`
  should continue rejecting effectful programs until Phase 3.1.
- Revisit when: Internal effect-row representation and unhandled-effect
  diagnostics are implemented.

- Decision: Parse handler expressions before implementing handler typing.
- Context: The parser now accepts `handle action()` expressions with operation
  clauses, explicit resume parameters, and a `return` clause. Resolver and
  local-definition analysis walk handler action and clause bodies, while the
  checker currently treats handler expressions as `Unknown` until effect-row
  tracking and handler removal semantics land. The JavaScript compiler reports
  `unsupported-effect-runtime` for handlers until Phase 3.1.
- Applies to phase(s): Phase 3 and Phase 3.1.
- Future implication: Handler typing should replace the temporary `Unknown`
  behavior with effect removal, resume typing, clause result checking, and
  handler-added effect propagation without changing the parsed surface shape.
- Revisit when: Handler checker semantics and Phase 3.1 JavaScript runtime
  lowering are implemented.

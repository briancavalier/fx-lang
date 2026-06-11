# Phase 4 learnings

Date: 2026-06-11

## Purpose

This note records implementation learnings and decisions from Phase 4 of the
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

- Decision: Start Phase 4 with goal-target scoped-effect examples and a
  work-plan note before parser and checker support.
- Context: `examples/phase-4/` documents the intended named-scope surface:
  `scoped effect`, `scope Name with <...>`, explicit scoped operation calls,
  `within` operation elision, scope-polymorphic rows, exact scoped handler
  removal, and diagnostic boundaries. These examples are not expected to pass
  until Phase 4 support lands.
- Applies to phase(s): Phase 4.
- Future implication: Promote focused pieces of the Phase 4 fixtures into
  parser, checker, diagnostics, hover, go-to-definition, and compiler-gating
  tests as each slice lands.
- Revisit when: Scoped effect declarations, scope declarations, scoped effect
  rows, `within`, and scoped operation resolution are implemented.

- Decision: Treat `scope` as a reserved keyword, so reusable scope-polymorphic
  examples should not use `scope` as a value parameter name.
- Context: Parser support for `scope` declarations and `within` expressions
  made examples such as `increment(scope: s)` invalid. The Phase 4 examples and
  notes now use names such as `cell`, `current`, or `region` for value-level
  scope parameters. `cargo test -p fx-syntax` passes with the Phase 4 parser
  fixtures.
- Applies to phase(s): Phase 4.
- Future implication: Diagnostics and docs should distinguish the `scope`
  keyword from ordinary value parameters that carry scope-polymorphic
  identities.
- Revisit when: The language decides whether keywords can be escaped or used as
  identifiers in limited positions.

- Decision: Keep scoped effect-row validation in the checker, not the parser.
- Context: The parser preserves effect-row members as ordinary type-expression
  applications so `State Counter Int`, `State s Int`, and ordinary Phase 3 rows
  share one syntax path. The checker now resolves effect families and enforces
  Phase 4 rules: `scope Name with <...>` must name one scoped effect family,
  scope protocol rows omit the scope parameter, and value-level effect rows
  include the scope parameter. `cargo test -p fx-core` covers ordinary effects
  in scope protocol rows, missing and extra protocol arguments, and the v0
  single-protocol limit.
- Applies to phase(s): Phase 4.
- Future implication: Multiple protocols can be added by relaxing checker
  validation without changing the accepted row syntax.
- Revisit when: Phase 4.1 or later adds multiple protocol-row entries per
  scope declaration.

- Decision: `within` stores the checked scope type, not only the source name.
- Context: Concrete `within Counter` can use a top-level scope symbol, but
  polymorphic `within cell` must elaborate elided calls with the type variable
  carried by the value parameter. The checker now records the resolved scope
  type when entering `within`, and elided calls report an ambiguity diagnostic
  when more than one active default scope can satisfy the operation scope slot.
- Applies to phase(s): Phase 4.
- Future implication: Scope-polymorphic helpers can use `within` without
  inventing nominal scope constants for parameter names.
- Revisit when: The language adds explicit scope masking or locally generated
  fresh scopes.

- Decision: Scoped handler clauses do not expose the implicit scope slot as a
  payload parameter.
- Context: Scoped operation calls pass a scope first, but handler clauses for
  `State s a` use payload-only forms such as `get(resume)` and
  `put(value, resume)`. The checker matches scoped operation effects against
  the original action row to infer the handled scope, removes only the matching
  family member from the remaining row, and treats multiple clauses for the
  same effect family as valid after the first removal.
- Applies to phase(s): Phase 4.
- Future implication: Handler syntax remains stable if scoped runtime support
  later elaborates scope identity into explicit interpreter state.
- Revisit when: Runtime lowering for scoped effects starts and handler clauses
  need executable scope dispatch.

## Verification

- `cargo test -p fx-syntax`
- `cargo test -p fx-core`
- `cargo test -p fx-compiler`
- `cargo check`
- `cargo run -p fx-cli -- check examples/phase-4/state.fx`
- `cargo run -p fx-cli -- check examples/phase-4/polymorphic.fx`
- `cargo run -p fx-cli -- check examples/phase-4/diagnostics.fx`
- `cargo run -p fx-cli -- compile --target js examples/phase-4/state.fx`
- `cargo run -p fx-cli -- compile --target js examples/phase-4/polymorphic.fx`

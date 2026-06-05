# Phase 2 learnings

Date: 2026-06-04

## Purpose

This note records implementation learnings and decisions from Phase 2 of the
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

- Decision: Start Phase 2 with goal-target row examples before parser and
  checker support.
- Context: `examples/phase-2/rows.fx` documents the intended open-record and
  open-variant behavior plus future diagnostic boundaries. The current parser
  still reports Phase 2 row syntax as unsupported, so these examples are not
  expected-passing fixtures yet.
- Applies to phase(s): Phase 2.
- Future implication: Promote focused pieces of the Phase 2 fixture into
  parser, checker, diagnostics, hover, and go-to-definition tests as row
  support lands instead of weakening the goal target to match current support.
- Revisit when: Open row parsing, row-aware checker types, row unification, and
  row diagnostics are implemented.

- Decision: Accept open row tails in the surface AST before checker support.
- Context: The parser now accepts final-only row tails in record and variant
  type syntax, such as `{ name: Text, ...r }` and
  `< Ok(a), Error(Text), ...r >`. The AST stores the full `...r` span plus the
  row variable name span, and closed rows remain `tail: None`.
- Applies to phase(s): Phase 2.
- Future implication: Later checker work should consume the parsed tail instead
  of reinterpreting spread syntax, and should inherit the final-only spread
  contract from the parser.
- Revisit when: Row variable scoping and row-kind-aware lowering are
  implemented.

- Decision: Allow a trailing comma after a final row tail.
- Context: Existing delimited record and variant type syntax already tolerates
  trailing commas, and accepting `{ name: Text, ...r, }` keeps row tails
  consistent with those delimiters.
- Applies to phase(s): Phase 2.
- Future implication: Formatter and pretty-printer work should preserve the
  same delimiter style rather than special-casing row tails.
- Revisit when: A formatter or syntax normalizer is implemented.

- Decision: Use a shared internal row carrier for records and variants while
  keeping row kinds distinct.
- Context: `fx-core::check::Type` now represents records and variants as
  `Type::Row(RowType)`, with `RowKind::Record` and `RowKind::Variant`
  separating the two cases. Existing Phase 1 behavior still constructs closed
  rows with no tail.
- Applies to phase(s): Phase 2.
- Future implication: Lowering, duplicate-label diagnostics, and row
  unification can build on one row representation without allowing record rows
  and variant rows to unify accidentally.
- Revisit when: Parsed row tails are lowered into checker types and row
  variables are scoped.

- Decision: Lower parsed row tails into checker row types before row
  unification.
- Context: Type lowering now carries parsed record and variant row tails into
  `RowTailType::Var`, so hovers and diagnostics can show open rows such as
  `{ name: Text, ...r }` and `< Ok(a), Error(Text), ...r >`.
- Applies to phase(s): Phase 2.
- Future implication: Row unification should consume the existing lowered tail
  instead of reparsing or reconstructing tail syntax from source AST nodes.
- Revisit when: Width-polymorphic row compatibility is implemented.

- Decision: Report duplicate row labels from checker-owned lowering.
- Context: Duplicate record fields and duplicate variant cases are now detected
  while lowering type aliases and value annotations into checker row types.
  These diagnostics are owned by the module being checked.
- Applies to phase(s): Phase 2.
- Future implication: Parser support can stay recovery-focused, while semantic
  row validation remains attached to the same module-owned diagnostics path as
  other checker errors.
- Revisit when: Duplicate labels need diagnostic codes or fix suggestions.

- Decision: Use row unification for width-polymorphic compatibility while
  keeping closed rows exact.
- Context: Expected open records and variants now accept found rows that
  contain the required members, and bind the expected row tail to the remaining
  fields or cases. Closed rows still require an exact closed match, and record
  rows and variant rows produce explicit row-kind mismatch diagnostics.
- Applies to phase(s): Phase 2.
- Future implication: Later row-variable scoping and generalization can build
  on the same row-tail substitution path instead of adding a separate
  compatibility mechanism.
- Revisit when: Open rows need declaration-level scoping, generalized row
  variables, or solving between two independent open row tails.

- Decision: Keep ordinary type substitutions and row-tail substitutions in
  separate maps.
- Context: Function calls now substitute ordinary type variables from one map
  and expand row-tail variables from a row map. Substituting a bound row tail
  appends the remainder entries and preserves any found open tail.
- Applies to phase(s): Phase 2.
- Future implication: Row-variable names can remain plain strings for this
  slice, but later scoping work should avoid conflating type variables and row
  variables with the same textual name.
- Revisit when: Row variable binders are represented explicitly.

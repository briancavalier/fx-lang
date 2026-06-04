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

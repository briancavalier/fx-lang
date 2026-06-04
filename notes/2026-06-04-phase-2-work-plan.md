# Phase 2 work plan

Date: 2026-06-04

## Purpose

This note breaks Phase 2 from `notes/2026-05-23-implementation-phases.md`
into implementation units. Phase 1 is complete, so this plan starts from the
current Rust syntax/core/LSP stack and focuses on adding shared row machinery
before effects depend on it.

Phase 2 should make records and variants meaningfully row-polymorphic while
keeping effect rows, context rows, handlers, named scopes, and coeffects out of
scope.

## Scope

Include:

- row kinds for records and variants;
- open record types such as `{ name: Text, ...r }`;
- open variant types such as `< Ok(a), Error(Text), ...r >`;
- final-only row spreads;
- row variable scoping and generalization;
- row unification;
- record field access and update against open rows;
- variant construction and matching against open rows;
- user-facing row diagnostics;
- compact hover display for open-row types.

Defer:

- effect rows;
- context rows;
- tuple rows unless a concrete Phase 2 example requires them;
- variadic tuple packs;
- advanced exhaustiveness and redundancy checking;
- effects, handlers, named scopes, coeffects, refs, and mutation.

## Work units

### 1. Phase 2 setup and goal fixture

Create `examples/phase-2/rows.fx` with the target examples from the phase plan
plus compact negative examples for missing fields, unknown cases, duplicate row
labels, and row-kind mismatches.

Create `notes/2026-06-04-phase-2-learnings.md` using the same entry shape as
the Phase 1 learning log:

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

Acceptance:

- Phase 2 examples are either documented as goal targets or wired into focused
  parser/checker tests as support lands.
- The learnings note exists before implementation decisions start accumulating.

### 2. Surface AST for open rows

Extend the syntax AST for row tails in record and variant type expressions.
Closed rows should remain representable without a tail, and open rows should
carry the final spread name and span.

Parser behavior:

- parse `{ name: Text, ...r }`;
- parse `< Ok(a), Error(Text), ...r >`;
- keep spreads final-only;
- diagnose non-final spreads and duplicate row spreads;
- keep existing recovery-friendly behavior around delimiters and layout.

Acceptance:

- Parser tests cover open record rows, open variant rows, final-only spreads,
  and useful diagnostics for malformed row tails.
- Existing Phase 1 fixtures continue to parse unchanged.

### 3. Internal row representation

Replace closed-list-only `Type::Record` and `Type::Variant` internals with a
row-aware representation: labels plus an optional tail variable plus an explicit
row kind.

The key design constraint is that record rows and variant rows must not unify
with each other. Effects and context rows are still deferred, but the internal
shape should leave room for them later.

Acceptance:

- Closed Phase 1 record and variant types still display and check the same way.
- Internal APIs can distinguish closed rows from open rows and record rows from
  variant rows.

### 4. Lowering and duplicate-label diagnostics

Lower row-aware `TypeExpr` values into the new internal row representation.
Add duplicate row label checks for records and variants, including aliases.

This should land before row unification so later errors are not polluted by
malformed row types.

Acceptance:

- Duplicate record fields produce a record-specific diagnostic.
- Duplicate variant cases produce a variant-specific diagnostic.
- Type aliases preserve enough information for later row-polymorphic checking
  while still supporting named-type hovers.

### 5. Row unification core

Implement row unification and row-tail substitution separately from ordinary
type-variable substitution.

The first useful target is width-polymorphic compatibility:

```fx
greet(person: { name: Text, ...r }) : Text
  person.name
```

A closed record such as `{ name: Text, age: Int }` should satisfy the expected
open row, binding `r` to the remaining row.

Acceptance:

- Open record expected types accept closed records with at least the required
  fields.
- Open variant expected types accept variants with at least the required cases.
- Row-kind mismatches produce direct diagnostics instead of falling through to
  generic type mismatch messages.

### 6. Row variable scoping and generalization

Decide and implement how row variables in annotations are scoped alongside
existing type parameters.

Examples:

```fx
greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

message(result: < Ok(a), Error(Text), ...r >) : Text
  match result
    Ok(_) -> "ok"
    Error(text) -> text
    other -> "unknown"
```

The checker should treat `r` as a row variable and `a` as an ordinary type
variable. They should not share one undifferentiated variable namespace inside
unification.

Acceptance:

- Row variables are scoped to the annotated declaration that mentions them.
- Ordinary type variables and row variables cannot be accidentally substituted
  for each other.
- The learnings note records any syntax or scoping decision that affects later
  effect and context rows.

### 7. Open-record field access

Update field access so required fields are available from open records.

Acceptance:

- `person.name` checks when `person` has `{ name: Text, ...r }`.
- Hovers on field names show the field type.
- Missing-field diagnostics name the specific missing field and point at the
  field access.

### 8. Open-record update

Extend the existing spread-first update syntax `{ ...record, field: value }` to
open rows.

Start conservatively: updates to known existing fields should be valid. Adding
new fields through an unconstrained row tail should be a deliberate sub-decision
and recorded in the Phase 2 learning note before implementation.

Acceptance:

- Updating a known field on an open record checks the value against the field
  type.
- Unknown-field diagnostics remain clear.
- Existing closed-record update behavior remains unchanged.

### 9. Open-variant construction

Make constructors type-check against open variant result types, including
aliases such as `< Ok(a), Error(Text), ...r >`.

Acceptance:

- Known constructors can produce values accepted by open variant annotations.
- Constructor payload types continue to be checked.
- Go-to-definition from constructor use to constructor declaration continues to
  work across modules.

### 10. Open-variant matching

Allow matching known cases from an open variant and support a fallback binder
such as `other -> "unknown"` from the Phase 2 target.

Keep exhaustiveness conservative. Phase 2 needs row membership checking, not a
complete exhaustiveness or redundancy checker.

Acceptance:

- Matching `Ok`, `Missing`, and `Error` against an open variant checks payloads.
- Unknown cases produce diagnostics that name the missing case.
- A fallback binder can handle the remaining row without forcing advanced
  exhaustiveness analysis.

### 11. Display, hover, and diagnostics polish

Update type display, typed spans, and analysis hover output so open rows render
compactly.

Target display examples:

```fx
{ name: Text, ...r }
< Ok(a), Error(Text), ...r >
```

Acceptance:

- Hover shows compact open-row types for declarations, locals, fields, and
  constructors.
- Row diagnostics mention user-facing labels and row kinds rather than internal
  unifier details.
- CLI diagnostics remain concise and source-spanned.

### 12. Phase 2 acceptance sweep

Wire `examples/phase-2` into parser, checker, CLI, and LSP-oriented tests as
features become supported.

Final acceptance commands:

```sh
cargo fmt --all --check
cargo test
cargo clippy --all-targets -- -D warnings
npm run compile
cargo run -q -p fx-cli -- check examples/phase-2
```

Run `npm run compile` from `editors/vscode-fx`.

Acceptance:

- All Phase 1 fixtures still pass.
- Phase 2 fixtures pass parser and checker validation.
- Hovers and go-to-definition remain routed through shared `fx-core` analysis.

## Suggested implementation order

1. Create the Phase 2 examples and learning log.
2. Parse open record and variant rows.
3. Introduce row-aware internal types behind existing closed behavior.
4. Add lowering and duplicate-label diagnostics.
5. Implement row unification and row-tail substitution.
6. Add row variable scoping and generalization.
7. Enable open-record field access.
8. Enable open-record update.
9. Enable open-variant construction.
10. Enable open-variant matching.
11. Polish display, hover, and diagnostics.
12. Run the full Phase 2 acceptance sweep.

## Notes for future phases

Keep row kinds distinct internally even while the surface syntax is visually
uniform. Effects and contexts should reuse the row machinery from this phase
without allowing record rows, variant rows, effect rows, and context rows to
unify across kinds.

Do not use tuple rows as the proof case unless records and variants are already
stable. Tuple rows and variadic tuple packs can wait for Phase 6 unless a small
Phase 2 fixture exposes an implementation need.

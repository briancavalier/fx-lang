# Phase 3 work plan

Date: 2026-06-10

## Purpose

This note breaks Phase 3 from
`notes/2026-05-23-implementation-phases.md` into implementation units. Phase 3
adds ordinary algebraic effect declarations, effect rows, operation calls, and
basic handlers after the pure language, row machinery, JavaScript backend, and
prefix `with` sugar are in place.

Phase 3 should introduce effect requirements without scoped effect families,
named scopes, coeffects, effect masking, handler literals, or advanced
multi-shot restrictions.

## Scope

Include:

- `effect` declarations;
- operation symbols exported by effect declarations;
- operation call sites using ordinary function-call syntax;
- function result effects, such as `a with <Console>`;
- open effect rows, such as `a with <Console, ...e>`;
- function types with effects, such as `() -> a with <Console, ...e>`;
- operation resolution by imported name, namespace, and expected effect row;
- basic handlers as function-like values;
- explicit `resume` in handler clauses;
- existing prefix trailing-action sugar for handler functions;
- diagnostics for unhandled effects, unknown operations, ambiguous operations,
  invalid `with` usage, and handler return mismatches;
- hover, go-to-definition, completion, and CLI diagnostics for effectful
  programs.

Defer:

- scoped effect declarations and named scopes;
- coeffects and `given`;
- effect masking;
- syntactic distinction between resumable and non-resumable operations;
- handler literals unless required as an implementation target;
- advanced laws or restrictions around multi-shot resumptions;
- richer standard-library effect design beyond fixtures needed for this phase;
- JavaScript runtime support for effectful programs, which belongs to Phase
  3.1.

## Surface rules

Use `with <effects>` after the return type for effect rows:

```fx
safe-divide(x: Int, y: Int) : Int with <Raise>
```

Open effect rows reuse final-only row tails:

```fx
catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
```

Function types attach effects to the function result:

```fx
() -> a with <Console, ...e>
```

The pure form omits `with <>`. The older `! <effects>` spelling remains
abandoned unless the design is explicitly reopened.

Handlers are function-like values in Phase 3 examples. Prefix `with` stays the
general trailing-action sugar from Phase 2.6; it is not special effect syntax.

## Work units

### 1. Phase 3 setup and goal fixtures

Create `examples/phase-3/effects.fx` with the target `Raise`/`safe-divide`/
`catch` example from the phase plan plus compact examples for imported effects,
effect-polymorphic helpers, and handler clauses that resume. These examples are
checking and LSP fixtures during Phase 3; executable JavaScript behavior is a
Phase 3.1 acceptance target.

Create `notes/2026-06-10-phase-3-learnings.md` using the established learning
log shape:

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

Acceptance:

- Phase 3 examples are documented as goal targets before syntax/checker support
  lands.
- The learning log exists before effect representation and handler decisions
  start accumulating.
- Boundary examples avoid scoped effects, coeffects, masking, and handler
  literals.

### 2. Syntax AST for effect declarations

Extend the parser and AST for top-level `effect` declarations with operation
signatures.

Parser behavior:

- parse `export effect Raise`;
- parse effect type parameters such as `effect Emit a`;
- parse indented operation signatures;
- keep operation signatures distinct from value declarations;
- recover cleanly from malformed operation parameter lists and result types.

Acceptance:

- Parser tests cover exported and non-exported effects, parameterized effects,
  multiple operations, and useful recovery diagnostics.
- Existing value/type/module syntax continues to parse unchanged.
- The AST preserves spans for the effect name, operation names, parameters, and
  result types for diagnostics and go-to-definition.

### 3. Syntax AST for effect rows in types

Extend type syntax for `with <...>` effect rows on value result types and
function result types.

Parser behavior:

- parse `a with <Console>`;
- parse `a with <Console, ...e>`;
- parse `() -> a with <Console, ...e>`;
- keep row tails final-only;
- reject or recover from the abandoned `! <...>` spelling with a useful message
  if it appears.

Acceptance:

- Parser tests cover closed effect rows, open effect rows, empty rows if
  accepted, function types with result effects, and malformed row tails.
- `with f(args) body` from Phase 2.6 remains parsed as trailing-action sugar,
  not confused with type-level `with <...>`.

### 4. Effect symbols and module exports

Add effect declarations and operation declarations to the module symbol model.
An exported effect exports its operation symbols as addressable declarations.

Acceptance:

- Duplicate effect names and duplicate operation names produce diagnostics at
  the declaring module.
- Imports can bring effects and operations into scope.
- Fully qualified operation names resolve through imported module namespaces.
- Go-to-definition has stable declaration targets for effects and operations.

### 5. Internal effect-row representation

Extend the checker type model with a distinct effect-row kind that reuses row
machinery without unifying accidentally with record, variant, tuple, or future
context rows.

The first representation should support:

- closed effect rows;
- open effect row tails;
- parameterized effect entries, such as `Emit a`;
- a pure/no-effect result as the default.

Acceptance:

- Effect rows display compactly as `with <Raise>` and `with <Raise, ...e>`.
- Record rows, variant rows, and effect rows cannot unify across kinds.
- Ordinary type variables and effect-row variables remain separate in
  substitution.

### 6. Lower annotations into effectful types

Lower parsed effect rows into checker types for value annotations, function
declarations, operation signatures, and function type parameters.

Acceptance:

- Top-level annotated declarations can carry explicit effect rows.
- Function parameters can be annotated with effectful function types.
- Effect row variables are declaration-local, kinded, and generalized using the
  same discipline established for Phase 2 row variables.
- Hovers on declarations show value types plus effect rows.

### 7. Track effects through expression checking

Change expression checking from producing only a value type to producing a value
type plus performed effect row.

Initial effect sources:

- operation calls;
- calls to functions whose result type has effects;
- branch bodies, match arms, local bindings, lambdas, and nested calls.

Acceptance:

- Calling an effectful function contributes its effect row to the caller.
- `if` branches and `match` arms combine performed effects conservatively.
- Lambdas keep their body effects on the resulting function type instead of
  leaking them to the lambda construction site.
- Pure annotations reject unhandled performed effects with the call site that
  introduced the effect.

### 8. Effect row compatibility and polymorphism

Implement effect-row compatibility using the existing row unification approach.

Acceptance:

- A function annotated `a with <Console, ...e>` can call operations/functions
  that require `Console` plus effects represented by `e`.
- Closed effect rows reject extra unhandled effects.
- Open effect rows bind tails without conflating them with record or variant
  tails.
- Diagnostics name the missing effect and point at the introducing call site
  when possible.

### 9. Operation resolution

Resolve operation calls using ordinary names first, then effect operation
metadata and the expected or allowed effect row.

Acceptance:

- Imported operation names call the corresponding operation.
- Qualified names such as `module.raise(...)` resolve to imported operation
  declarations.
- Ambiguous unqualified operation names produce a diagnostic that lists the
  conflicting candidates.
- Unknown operations are reported separately from ordinary unknown value names
  where the context makes that distinction useful.
- Go-to-definition from an operation call reaches the operation signature.

### 10. Handler surface and checker model

Add `handle action()` expressions with operation clauses and a `return` clause.
Start with the Phase 3 target shape, where handlers are written inside
function-like helper values.

Checker behavior:

- the handled action must be a thunk or function call whose result performs the
  handled effect;
- each operation clause receives operation payload parameters plus an explicit
  resume parameter;
- `resume(value)` type-checks against the operation result and continuation
  result;
- the `return` clause receives the action result;
- all clauses agree on the handler result type.

Acceptance:

- `catch(default, action)` removes `Raise` from `action` and returns `a with
  <...e>`.
- Handler clauses may perform additional effects that are added to the handler
  result.
- Handler return mismatches point at the mismatching clause.
- Missing, duplicate, or unknown handler clauses produce effect-specific
  diagnostics.

### 11. LSP diagnostics, hover, definition, and completion

Wire effect metadata into analysis output used by the language server.

Acceptance:

- Hover on effect declarations and operations shows their signatures.
- Hover on effectful declarations shows value type plus effect row.
- Go-to-definition works from operation calls to operation declarations across
  modules.
- Diagnostics point at the operation or function call that introduces an
  unhandled effect.
- Completion can suggest imported operations from effect declarations.

### 12. CLI and validation-path polish

Preserve existing `fx check` and `fx compile --target js` contracts while adding
Phase 3 behavior.

Acceptance:

- `fx check examples/phase-3` reports no diagnostics once the feature lands.
- `fx compile --target js` continues to compile pure programs from earlier
  phases.
- `fx compile --target js examples/phase-3` rejects effectful programs with a
  clear unsupported-effect-runtime compiler diagnostic before emission.
- Invalid targets, parser errors, checker errors, and compiler unsupported
  diagnostics remain distinct.
- Existing examples from earlier phases still check and compile where they did
  before Phase 3.

### 13. Phase 3 acceptance sweep

Run a final effect-focused sweep and record the results in the Phase 3 learning
log.

Final acceptance commands:

```sh
cargo fmt --all --check
cargo test --no-run
cargo check
cargo test -p fx-syntax
cargo test -p fx-core
cargo test -p fx-compiler
cargo test -p fx-cli
cargo run -q -p fx-cli -- check examples
cargo run -q -p fx-cli -- compile --target js examples/phase-2.6
```

Acceptance:

- The Phase 3 example target is accepted by parser, checker, LSP-adjacent
  analysis, and CLI check.
- `fx compile --target js` preserves pure-program behavior and rejects
  effectful Phase 3 programs with a clear unsupported-effect-runtime diagnostic
  until Phase 3.1 lands.
- The learning log documents durable decisions that Phase 4 named scopes and
  Phase 5 coeffects should inherit.
- Any intentionally deferred behavior is called out explicitly instead of left
  ambiguous.

## Phase 3.1 handoff

Phase 3.1 owns executable JavaScript support for effects after Phase 3 checker
and handler semantics are stable.

Phase 3.1 should include:

- compiler IR and lowering support for operation calls, handlers, `return`
  clauses, and explicit `resume`;
- a small JavaScript runtime protocol for operation requests, handler dispatch,
  resumptions, and ordinary returns;
- JavaScript emission for the supported Phase 3 effect subset;
- Node smoke tests for handled `Raise` and at least one resumptive handler;
- compiler diagnostics for accepted-but-not-yet-executable effect constructs.

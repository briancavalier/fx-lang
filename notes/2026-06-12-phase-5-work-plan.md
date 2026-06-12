# Phase 5 work plan

Date: 2026-06-12

## Purpose

This note breaks Phase 5 from
`notes/2026-05-23-implementation-phases.md` into implementation units. Phase 5
adds coeffects and `given`: contextual value requirements, context rows,
lexical context providers, contextual insertion, diagnostics, LSP-adjacent
metadata, and JavaScript execution for the supported subset.

The research context is in `notes/2026-06-12-phase-5-research.md`. The settled
surface direction is to keep `given` in source syntax while allowing the typed
core, lowering, and generated JavaScript ABI to represent coeffects as an
initial hidden context-row parameter.

## Scope

Include:

- context rows such as `a given { clock: Clock }`;
- open context rows such as `a given { clock: Clock, ...c }`;
- combined effects and context such as
  `a with <Console, ...e> given { clock: Clock, ...c }`;
- `given { clock = system-clock() } body`;
- contextual values visible by name inside the provider body;
- contextual values visible by name inside a function body whose signature
  declares those contextual requirements;
- named provider functions using existing trailing-action `with` sugar;
- lexical provider stacks with strict name shadowing;
- name-and-type contextual resolution;
- checked contextual insertion at call sites;
- diagnostics for missing contextual values, ambiguous contextual values,
  duplicate provider fields, provider type mismatches, malformed context rows,
  and invalid provider expressions;
- LSP-adjacent hover/go-to-definition metadata;
- JavaScript lowering and runtime smoke tests for the supported subset.

Defer:

- `given each`;
- resourceful branch-local context;
- module-level contextual exports;
- final `given { ... }` call arguments;
- ordinary named arguments for contextual requirements;
- qualified contextual names;
- typeclass-style instance search;
- runtime dynamic context lookup;
- context import rules beyond ordinary imported function/type names.

## Design target

Source keeps `given` as a requirement annotation:

```fx
timestamp(message: Text) : Text given { clock: Clock }
  clock.now() ++ " " ++ message
```

`given` establishes lexical context:

```fx
main() : Text
  given { clock = system-clock() }
    timestamp("started")
```

The checker resolves `clock: Clock` at the call to `timestamp` and records the
inserted contextual value. Lowering may represent this as:

```txt
timestamp({ clock: resolvedClock }, "started")
```

Plain `given` is shared across handler resumptions. Fresh context per resumed
branch is a later `given each` feature.

## Work units

### 1. Phase 5 setup, learning log, and goal fixtures

Create the phase execution scaffolding before parser and checker edits.

Acceptance:

- `notes/2026-06-12-phase-5-learnings.md` exists with the established
  learning-log shape:
  - Decision:
  - Context:
  - Applies to phase(s):
  - Future implication:
  - Revisit when:
- The learnings note links back to this work plan and the Phase 5 research
  note.
- `examples/phase-5/` exists with goal-target examples for:
  - basic `Clock` context;
  - effects plus context;
  - a context-polymorphic provider function;
  - a higher-order function carrying context rows;
  - diagnostics for missing, duplicate, and mismatched context.
- Goal fixtures stay within the Phase 5 boundary and do not use `given each`,
  module-level contextual exports, final call-site `given`, named arguments, or
  qualified contextual names.

### 2. Parse context-row type syntax

Extend type syntax for contextual requirements.

Parser behavior:

- parse `a given { clock: Clock }`;
- parse `a given { clock: Clock, ...c }`;
- parse `a with <Console, ...e> given { clock: Clock, ...c }`;
- parse function result context rows such as
  `() -> a given { clock: Clock, ...c }`;
- preserve spans for `given`, row braces, field names, field types, spreads,
  and the full contextual result type;
- recover from malformed context rows with parser diagnostics.

Acceptance:

- Parser tests cover closed rows, open rows, combined effects/context, function
  result context, missing braces, duplicate spreads, non-final spreads, and
  malformed field entries.
- Existing record, variant, effect-row, and function-type parsing remains
  unchanged.
- Formatting can round-trip parsed context-row type syntax, even if formatter
  polish lands in a later work unit.

### 3. Parse `given` providers

Extend expression syntax for lexical context providers:

```fx
given { clock = system-clock(), config = read-config() }
  body
```

Parser behavior:

- parse `given { ... } body` as a distinct expression that preserves provider
  entries structurally;
- preserve spans for each provider field name and value expression;
- reject or recover from duplicate-looking fields, missing `=`, missing braces,
  and empty/malformed provider entries;
- keep ordinary trailing-action `with f(args) body` and `within Scope body`
  parsing unchanged.

Acceptance:

- Parser tests cover one provider field, multiple fields, nested providers,
  provider values with calls/records/lambdas, malformed entries, and recovery.
- The AST distinguishes provider fields from ordinary record literals.
- `given each { ... }` remains unsupported with a clear parser or checker
  diagnostic, not silently accepted.

### 4. Add internal context-row types

Add a distinct internal representation for context rows.

Expected model:

- a `ContextRowType` carrying named fields and an optional tail;
- context row substitution independent from record rows and effect rows;
- a qualified result representation that can carry both effects and context,
  avoiding order-sensitive nesting of effectful and contextual result types.

Acceptance:

- Record rows and context rows cannot accidentally unify.
- Effect rows and context rows have separate substitution paths.
- Display renders compact source-like signatures:
  - `a given { clock: Clock }`;
  - `a with <Console, ...e> given { clock: Clock, ...c }`.
- Existing Phase 1 through Phase 4 type display and compatibility tests keep
  passing.

### 5. Lower context-row syntax into checker types

Teach the checker to lower parsed context rows into the internal context-row
representation.

Checker behavior:

- detect duplicate context row fields;
- detect duplicate context row spreads;
- enforce final-only context row spreads;
- bind context-row tails distinctly from record and effect row tails;
- preserve context fields on function result types.

Acceptance:

- Checker tests cover closed and open context rows.
- Checker tests prove context tails do not unify with record, variant, or effect
  tails.
- Hover display for declarations includes context rows in source order.
- Existing effect-row checking remains unchanged.

### 6. Track contextual requirements through expressions

Extend expression inference/checking so contextual requirements propagate like a
second requirement row alongside effects.

Checker behavior:

- expression results carry value type, effect row, and context row;
- calls to contextual functions add required context fields unless they are
  resolved by an active provider;
- blocks, `if`, `match`, records, lists, record updates, calls, pipelines,
  binary expressions, handlers, and `within` merge contextual requirements;
- lambda construction keeps body context requirements on the function result,
  not on the lambda construction site;
- handler clauses and return clauses track context requirements like ordinary
  expressions.

Acceptance:

- Checker tests cover direct calls, nested expressions, lambdas, higher-order
  parameters, branches with different context rows, and combined
  effect/context propagation.
- Existing effect propagation behavior remains unchanged.
- Missing context is reported only at boundaries that require pure/no-context
  results, or when a contextual call has no provider and cannot pass the
  requirement outward.

### 7. Bind declared context names inside function bodies

Make a function's own contextual requirements available by name in its body.

Example:

```fx
timestamp(message: Text) : Text given { clock: Clock }
  clock.now() ++ " " ++ message
```

Checker behavior:

- `clock` is available as a local contextual value in the function body;
- contextual body bindings use the declared field type;
- ordinary parameters or locals with the same name shadow contextual fields
  according to the usual local-name rules, or are rejected if shadowing is
  disallowed by the existing checker style.

Acceptance:

- Hover on `clock` in the body shows `Clock`.
- Go-to-definition from `clock` in the body targets the context-row field in
  the signature or a synthetic contextual parameter span.
- Unknown contextual names still report ordinary unknown-name diagnostics.
- Contextual body names are not exported as ordinary module values.

### 8. Implement lexical provider checking

Check `given { ... } body` provider expressions.

Checker behavior:

- infer each provider value expression;
- reject duplicate provider fields in the same provider;
- push provider fields on a lexical provider stack for the body;
- make provider fields available by name inside the provider body;
- use strict name shadowing: an inner provider field named `clock` blocks outer
  `clock` fields, even if its type is incompatible;
- provider value expressions are checked before the provider fields they define
  are in scope, avoiding self-reference unless later explicitly supported.

Acceptance:

- Checker tests cover successful provision, nested provider shadowing,
  duplicate fields, type mismatch, unknown provider values, and provider fields
  visible by name in the body.
- Diagnostics for provider type mismatches point to both the provider field and
  the requiring call when possible.
- Provider bodies may still perform effects and require context according to
  ordinary expression rules.

### 9. Resolve and record contextual insertion at call sites

Implement name-and-type contextual insertion.

Resolution behavior:

- for each required context field, search lexical providers from innermost to
  outermost;
- stop at the first provider depth containing that field name;
- accept exactly one type-compatible field at that depth;
- report a type mismatch if the same-name field exists but is incompatible;
- report ambiguity if more than one compatible candidate is possible at the
  same depth;
- report missing context if no provider depth contains the field name and the
  requirement cannot be propagated outward.

Acceptance:

- Checker tests cover successful insertion, missing context, strict
  same-name/type-mismatch shadowing, nested shadowing, duplicate/ambiguous
  candidates, and open context tails.
- Inserted contextual arguments are recorded in checker output or analysis data
  for lowering and future tooling.
- Diagnostics use source-level wording such as
  `missing given { clock: Clock }`, not hidden-parameter wording.

### 10. Check context-polymorphic provider functions

Support reusable provider functions that remove one context requirement from an
action and pass the remaining context outward.

Example:

```fx
with-clock(
  clock: Clock,
  action: () -> a given { clock: Clock, ...c }
) : a given { ...c }
  given { clock = clock }
    action()
```

Acceptance:

- Checker tests cover the `with-clock` shape.
- Context-row tail subtraction/removal is sound for the handled field.
- Remaining context tails propagate outward.
- Existing trailing-action `with with-clock(system-clock()) body` works after
  parser/checker support for the provider function type lands.

### 11. Add diagnostics and recovery polish

Make Phase 5 diagnostics source-facing and actionable.

Acceptance:

- Missing context points to the call requiring it.
- Ambiguous context lists candidate provider fields when practical.
- Provider type mismatch reports the provider field type and expected required
  type.
- Duplicate provider fields point to the duplicate entry.
- Context-row kind mismatches distinguish context rows from record, variant, and
  effect rows.
- Unsupported deferred features report explicit messages:
  - `given each is not supported in Phase 5`;
  - `final given arguments are not supported in Phase 5`;
  - `module-level contextual exports are not supported in Phase 5`.

### 12. Add LSP-adjacent metadata

Expose context information through the existing analysis/typed-span path.

Acceptance:

- Hover shows context rows in declaration signatures.
- Hover on contextual provider fields and contextual body names shows their
  value type.
- Go-to-definition from a body contextual name reaches the context-row field or
  provider field.
- Go-to-definition from an implicitly resolved contextual use can reach the
  provider field when the inserted-use metadata is available.
- Document symbols and workspace symbols remain stable for existing constructs.

### 13. Add formatter support

Format Phase 5 syntax after parser/checker support has settled.

Acceptance:

- Context rows format consistently with record rows while preserving their
  `given` requirement syntax.
- `given { ... } body` formats consistently with trailing-action `with` and
  `within`.
- Formatter idempotence tests include context rows, combined effect/context
  signatures, provider blocks, and nested providers.
- `fx fmt --check examples` succeeds once Phase 5 examples are promoted.

### 14. Extend core IR for context parameters and inserted context

Represent context rows and contextual insertion explicitly in compiler IR.

Expected IR additions:

- function declarations with optional hidden context-record parameter metadata;
- calls with inserted context-record arguments;
- provider expressions lowered to ordinary bindings plus scoped insertion
  metadata;
- debug-tree output that makes context insertion visible.

Acceptance:

- Pure, ordinary-effect, and scoped-effect programs produce equivalent IR to
  before, except for deliberate shared qualified-result representation changes.
- Contextual functions preserve context field names and types in IR metadata.
- Calls to contextual functions show inserted context records in compiler tests.
- Context metadata is not reconstructed in the JavaScript emitter from source
  syntax.

### 15. Lower contextual functions and calls to JavaScript

Emit executable JavaScript for the supported Phase 5 subset.

Lowering behavior:

- contextual functions accept a hidden context record before ordinary
  parameters;
- ordinary source calls receive inserted context-record arguments;
- declared contextual names in a function body lower to context-record field
  access or synthetic locals;
- provider fields lower to ordinary values captured in inserted context records;
- exported contextual functions use a stable generated ABI.

Acceptance:

- Generated JavaScript for a basic `Clock` example runs under Node.
- Generated JavaScript for a context-polymorphic `with-clock` example runs under
  Node.
- Existing Phase 1 through Phase 4.1 executable examples continue to compile
  and run unchanged.
- Compiler unsupported diagnostics remain for accepted-but-not-yet-executable
  Phase 5 constructs until the relevant lowering is covered by tests.

### 16. Smoke-test effects, handlers, and shared context

Verify the interaction between context and the existing effect runtime.

Acceptance:

- A `Unit with <Console> given { clock: Clock }` example compiles and runs.
- A handler clause body can use contextual values.
- A smoke test proves plain `given` is shared across multiple resumptions.
- Resuming a continuation uses the context captured at the operation site, not
  a provider visible only at the resume site.
- Ordinary Phase 3 and scoped Phase 4.1 handler behavior remains unchanged.

### 17. Promote examples and run full validation

Move goal fixtures from expected-failing to accepted examples as support lands.

Acceptance:

- `cargo fmt --all --check`
- `cargo check`
- `cargo test -p fx-syntax`
- `cargo test -p fx-core`
- `cargo test -p fx-compiler`
- `cargo test -p fx-cli`
- `cargo test -p fx-lsp`
- `cargo test`
- `cargo run -q -p fx-cli -- check examples/phase-5`
- `cargo run -q -p fx-cli -- compile --target js examples/phase-5/basic.fx`
- `cargo run -q -p fx-cli -- compile --target js examples/phase-5/with-clock.fx`
- generated Node smoke tests for accepted Phase 5 executable examples.

## Suggested implementation order

1. Work units 1-3: examples and syntax.
2. Work units 4-6: internal types and requirement propagation.
3. Work units 7-10: contextual names, providers, insertion, and
   context-polymorphic helpers.
4. Work units 11-13: diagnostics, LSP metadata, and formatter.
5. Work units 14-16: IR, JavaScript lowering, and effect/runtime interaction.
6. Work unit 17: promotion and full validation.

This order keeps the checker semantics visible before runtime work starts and
preserves the Phase 3/Phase 4 pattern of goal fixtures becoming acceptance
tests as each slice lands.

## Open decisions before implementation

- Whether the internal qualified-result representation should replace
  `Type::Effectful` outright or extend it with context.
- Whether context provider fields need a distinct `TypedSpanKind` or can reuse
  existing local/parameter spans with labels.
- Whether ordinary locals are allowed to shadow contextual names in function
  bodies, or whether duplicate names should be rejected.
- Whether formatter support should land immediately after parser support or
  after checker support confirms the final AST shape.

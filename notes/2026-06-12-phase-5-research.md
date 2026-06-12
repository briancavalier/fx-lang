# Phase 5 coeffects research

Date: 2026-06-12

## Purpose

This note records research for Phase 5 of
`notes/2026-05-23-implementation-phases.md`: coeffects and `given`.

Phase 5 adds contextual requirements after ordinary effects, named scopes, and
the scoped JavaScript runtime are stable. The research goal is to choose a
small, explicit design that can be implemented in parser, checker, LSP metadata,
lowering, and JavaScript emission without committing early to later features
such as `given each`, module-level contextual exports, final `given` call
arguments, ordinary named arguments, or resourceful branch-local context.

## Current phase boundary

Phase 5 should include:

- context rows such as `a given { clock: Clock }`;
- open context rows such as `a given { clock: Clock, ...c }`;
- combined effects and context such as
  `a with <Console, ...e> given { clock: Clock, ...c }`;
- scoped providers: `given { clock = system-clock() } body`;
- name-and-type contextual resolution;
- contextual values visible by name inside the provider body;
- named provider functions using trailing-action sugar;
- diagnostics for missing contextual values, ambiguity, duplicates, and invalid
  provider expressions.

Phase 5 should still defer:

- `given each`;
- resourceful branch-local context;
- module-level contextual exports;
- final `given { ... }` call arguments;
- ordinary named arguments for contextual requirements;
- qualified contextual names.

This boundary matters because coeffects are not just syntax on calls. They add a
second requirement system alongside effect rows. The implementation should make
that system visible in types and lowering while keeping runtime behavior
predictable.

## Prior art

### Koka

Koka is the closest syntax and effects reference for this language. Current
Koka documentation describes effect typing, handlers, named/scoped handlers,
implicit arguments, inlay hints for implicit arguments, and `initially` /
`finally` handler clauses. Koka's 2026 documentation also notes improved
implicit and overloading support, plus unique implicit solutions that take
scoping into account.

Implication for fxl:

- Treat implicit context as something the compiler can show to tooling.
- Prefer a unique-solution rule, not best-effort overload choice.
- Keep `initially` as an internal semantic anchor only. The existing fxl syntax
  note already demotes it from public v0 surface syntax.
- Do not conflate shared context with branch-local/resourceful context. Koka's
  lifecycle machinery is useful prior art, but Phase 5 should only implement
  shared `given`.

Source: https://koka-lang.github.io/koka/doc/book.html

### Scala 3

Scala 3 separates context parameters (`using`) from given instances. Context
arguments can be supplied explicitly or synthesized by the compiler. Context
function types elaborate expressions under an expected context-function type by
making contextual values available inside the body. Scala also deliberately
separates ordinary wildcard imports from given imports, because invisible
implicit imports hurt clarity and tooling.

Implication for fxl:

- The most implementation-friendly model is elaboration to ordinary arguments,
  not runtime dynamic lookup.
- Contextual insertion should be represented in typed/lowered artifacts so
  tooling can eventually show the inserted values.
- Module-level contextual exports should remain deferred. Import behavior is a
  separate design problem, not a small extension to scoped providers.
- Fresh wrapper/opaque types are a known way to avoid accidental contextual
  matches. In fxl, Phase 5 should use name plus type to reduce accidental
  matches instead of type-only resolution.

Sources:

- https://docs.scala-lang.org/scala3/reference/contextual/givens.html
- https://docs.scala-lang.org/scala3/reference/contextual/using-clauses.html
- https://docs.scala-lang.org/scala3/reference/contextual/given-imports.html
- https://docs.scala-lang.org/scala3/reference/contextual/context-functions.html

### GHC implicit parameters

GHC implicit parameters are dynamically scoped constraints identified by names
such as `?x`. GHC guarantees that the most nested implicit parameter binding is
chosen. Its documentation also shows surprising behavior where adding a type
signature changes whether an implicit parameter is captured or re-resolved.

Implication for fxl:

- Nearest-provider shadowing is easy to explain, but dynamic rebinding plus
  inference can surprise users.
- fxl should avoid a model where adding a local annotation changes whether a
  context value is captured or re-resolved.
- Because fxl is already annotation-driven at top level, contextual insertion
  should happen at checked call sites and be preserved by lowering rather than
  discovered dynamically at runtime.

Source:
https://downloads.haskell.org/ghc/latest/docs/users_guide/exts/implicit_parameters.html

### Modular implicits

The OCaml modular implicits design uses type-directed implicit module
parameters and elaborates them into ordinary module/functor constructs. It is a
useful warning that powerful implicit systems quickly become systems for
ad-hoc polymorphism, associated types, and instance search.

Implication for fxl:

- Do not turn Phase 5 context into a typeclass or instance search system.
- Keep providers lexical and named in v0.
- Keep module-level defaults and imported implicit candidates out of Phase 5.

Source: https://arxiv.org/abs/1512.01895

## Semantic model

Recommendation: model Phase 5 `given` requirements as compiler-elaborated
context parameters.

The source-level type:

```fx
timestamp(message: Text) : Text given { clock: Clock }
```

should be represented internally as a function whose result type carries a
context row. At a call site:

```fx
given { clock = system-clock() }
  timestamp("started")
```

the checker resolves the required `clock: Clock` from the lexical provider and
records an inserted contextual argument. Lowering should turn the call into an
explicit core call with the context value or context record supplied.

Do not model Phase 5 as runtime dynamic lookup. Runtime context would make
ordinary function calls depend on an ambient interpreter state, obscure module
boundaries, and conflict with the Phase 4.1 direction where `within` is
compile-time elaboration and scoped metadata is explicit in compiler IR.

Recommended v0 laws:

- `given { ... }` is lexical and shared.
- Inner providers shadow outer providers by field name.
- A contextual field is visible by ordinary name inside the provider body.
- Contextual insertion happens at checked call sites.
- The inserted value should be recoverable for tooling and lowering.
- Context does not authorize effects or scoped effects; it only supplies values.
- Plain `given` shares the same value across handler resumptions.
- Branch-local context and cleanup belong to later `given each`.

## Resolution model

Use name-and-type resolution, but make the search deliberately small.

For each required context field `name: Type` at a call site:

1. Search the lexical context-provider stack from innermost to outermost.
2. At the first provider depth containing a field named `name`, collect entries
   at that depth whose type is compatible with the required type.
3. If there is exactly one compatible entry, insert it.
4. If there is no compatible entry at that provider depth, report a type
   mismatch for that contextual field instead of silently skipping to an outer
   same-name provider.
5. If more than one compatible entry exists at the same provider depth, report
   ambiguity.
6. If no provider depth has that field name, report a missing contextual value.

This makes shadowing name-based and easy to explain. It also avoids a surprising
case where an inner `clock` with the wrong type is ignored in favor of an outer
`clock`.

Examples:

```fx
given { clock = system-clock() }
  timestamp("started")
```

Accepted if `system-clock()` has type `Clock`.

```fx
given { clock = fake-clock() }
  given { clock = test-clock() }
    timestamp("started")
```

Accepted. The inner `clock` is inserted.

```fx
given { clock = "not a clock" }
  timestamp("started")
```

Rejected at the call to `timestamp`, with a note pointing to the provider field.

```fx
given { show = int-show(), show = hex-show() }
  format(42)
```

Rejected as a duplicate provider field before insertion. If the parser/checker
later permits multiple entries with the same field name, this should become an
ambiguity diagnostic, but v0 should reject duplicate provider fields directly.

Open question for design review:

- Should provider shadowing be strict by name as recommended here, or should a
  same-name provider with an incompatible type be skipped so an outer provider
  can satisfy the requirement?

I recommend strict name shadowing because it is simpler for users and better for
diagnostics.

## Context row typing

Recommendation: add a distinct context-row representation instead of reusing
record rows directly.

Surface context rows look like record rows:

```fx
a given { clock: Clock, ...c }
```

but they are not value records. They represent implicit requirements on a
computation. Internally they need their own row kind or row type so that record
rows and context rows cannot accidentally unify.

Current checker state:

- `Type::Effectful { result, effects }` carries effect requirements.
- `EffectRowType` is separate from `RowType`.
- `RowKind` currently has `Record` and `Variant`.
- `Substitution` has separate maps for record/variant rows and effect rows.

Recommended internal shape:

```txt
Type::Qualified {
  result: Type,
  effects: EffectRowType,
  context: ContextRowType,
}
```

or, as a smaller migration:

```txt
Type::Effectful {
  result: Type,
  effects: EffectRowType,
  context: ContextRowType,
}
```

The first shape is cleaner because `Effectful` becomes a misnomer once context
is also tracked. The second shape is a smaller patch. Either way, avoid nesting
`Type::Effectful` and a new `Type::Contextual` because combined effect/context
signatures would become order-sensitive internally.

Recommended `ContextRowType`:

```txt
ContextRowType {
  fields: Vec<ContextField>,
  tail: Option<RowTailType>
}

ContextField {
  name: String,
  ty: Type
}
```

This parallels records structurally while remaining a separate requirement kind.
It also gives diagnostics access to field names.

Typing rules:

- A pure result has empty effects and empty context.
- `a with <Console>` has effects and empty context.
- `a given { clock: Clock }` has empty effects and context.
- `a with <Console, ...e> given { clock: Clock, ...c }` has both.
- Function result types carry both effects and context.
- Lambda construction does not perform effects or require context at
  construction time; its body requirements remain on the function result.
- Calling a function consumes ordinary arguments, then attempts contextual
  insertion for the function result context row.
- Unresolved context requirements remain on the enclosing expression, just as
  unhandled effects remain on enclosing expressions.

## Core lowering

Recommendation: lower context insertion as explicit arguments in core IR.

There are two plausible encodings.

Option A: one hidden context record argument per call.

```txt
timestamp("started")
=> timestamp({ clock: resolvedClock }, "started")
```

Option B: one hidden argument per required field.

```txt
timestamp("started")
=> timestamp(resolvedClock, "started")
```

Prefer Option A for Phase 5. A context record tracks the source row shape,
preserves field names for generated debug output, and makes open context rows
natural. The generated JavaScript can still optimize obvious cases later.

Core consequences:

- Function declarations with context rows should lower to functions that accept
  a context record plus ordinary parameters.
- Calls to contextual functions should include an inserted context record.
- Provider expressions should lower as ordinary let-bound values plus a lexical
  context stack used during lowering.
- A field reference to a contextual value inside a provider body should lower to
  a local binding or a field access on the active context record.
- Exported contextual functions should have a stable generated ABI. If the
  source function is exported, the generated JS function can accept the hidden
  context record first and ordinary arguments afterward.

Open question for design review:

- Should the hidden context record be first or last in the generated function
  ABI?

I recommend first because it mirrors environment-passing and makes partial
application of ordinary arguments easier to reason about later. If fxl strongly
prefers preserving ordinary argument order in generated output, choose last and
document it early.

## Effects, handlers, and resumptions

Plain `given` should be shared across resumptions.

Example:

```fx
given { request = request-context }
  with choose-first()
    render-page()
```

If a handler resumes the continuation more than once, every resumed branch sees
the same `request` value. This follows from lowering `given` to an explicit
context argument captured by the continuation.

Do not allocate fresh context for each resume in Phase 5. That is the purpose of
later `given each`.

Important checks:

- A handler clause body may itself require context. Those requirements should be
  tracked like any other expression requirements.
- Resuming a continuation should supply the context captured at the operation
  site, not whatever provider happens to be visible at the resume call site.
- A handler function with an action parameter such as
  `() -> a with <Choose, ...e> given { clock: Clock, ...c }` should preserve the
  action's context row until the handler body calls the action under a provider
  or passes the requirement outward.

This is another reason to avoid runtime dynamic lookup. Dynamic lookup would
make resume-site context accidentally affect operation-site context.

## Tooling and diagnostics

Phase 5 should extend existing typed-span and indexed-symbol paths.

Recommended typed metadata:

- Add a typed span kind for contextual provider fields, or reuse `Local` with a
  label that marks it as contextual.
- Record implicit context uses at call sites so tooling can eventually show
  inserted contextual arguments.
- Hover for functions should print combined effects and context in source order:
  `a with <Console, ...e> given { clock: Clock, ...c }`.
- Go-to-definition from an implicitly resolved context use should jump to the
  provider field.

Diagnostics matrix:

| Case | Primary span | Message shape |
| --- | --- | --- |
| Missing contextual field | Call requiring it | `missing given { clock: Clock }` |
| Ambiguous contextual field | Call requiring it | `ambiguous given { show: Int -> Text }` |
| Provider duplicate field | Duplicate field | `duplicate given field clock` |
| Provider type mismatch | Provider field and call | `given field clock has type Text, expected Clock` |
| Invalid provider expression | Provider expression | ordinary type/effect diagnostic |
| Context row kind mismatch | Signature row | `expected context row, found record row` |

Diagnostics should point to the call that requires missing context, with
secondary provider spans where available. Phase 5 does not need rich code
actions yet, but the internal data should make these possible:

- wrap expression in `given { ... }`;
- add a missing field to an existing provider;
- show inserted contextual arguments.

## Example targets

Create goal-target examples before implementation, then promote focused pieces
to parser/checker/LSP/compiler tests.

### Basic provider

```fx
export type Instant = Text

export type Clock = {
  now: () -> Instant
}

timestamp(message: Text) : Text given { clock: Clock }
  clock.now() ++ " " ++ message

main() : Text
  given { clock = system-clock() }
    timestamp("started")
```

### Effects plus context

```fx
log-time(message: Text) : Unit with <Console> given { clock: Clock }
  print-line(clock.now() ++ " " ++ message)
```

### Context-polymorphic provider function

```fx
with-clock(
  clock: Clock,
  action: () -> a given { clock: Clock, ...c }
) : a given { ...c }
  given { clock = clock }
    action()
```

### Higher-order combined requirements

```fx
map-log(
  f: a -> b with <Console, ...e> given { clock: Clock, ...c },
  values: List a
) : List b with <Console, ...e> given { clock: Clock, ...c }
  map(f, values)
```

### Diagnostics

```fx
bad-missing() : Text
  timestamp("started")

bad-provider-type() : Text
  given { clock = "not a clock" }
    timestamp("started")

bad-duplicate() : Text
  given { clock = system-clock(), clock = test-clock() }
    timestamp("started")
```

## Implementation slices

1. Add goal-target examples under `examples/phase-5/`.
2. Parse context type syntax:
   - `a given { clock: Clock }`;
   - `a given { clock: Clock, ...c }`;
   - `a with <Console, ...e> given { clock: Clock, ...c }`;
   - function result context rows.
3. Parse `given { name = expr } body` as a distinct provider expression that
   preserves provider entries in AST.
4. Add internal `ContextRowType` and qualified result representation.
5. Extend compatibility/unification for context rows and context tails.
6. Add checker environment support for lexical provider stacks.
7. Implement contextual insertion at call sites and unresolved-context
   propagation on enclosing expressions.
8. Add typed spans for context rows, provider fields, and inserted contextual
   uses.
9. Add LSP hover/go-to-definition support using the existing typed-span and
   symbol metadata path.
10. Lower context insertion into core IR as explicit context-record arguments.
11. Emit JavaScript for contextual functions and calls.
12. Add compiler gating for any accepted context constructs not yet executable.
13. Promote examples and diagnostics into tests.

## Validation targets

Static:

- parser fixtures for context rows and `given`;
- checker tests for closed/open context rows;
- checker tests for combined effects and context;
- checker tests for higher-order action parameters with context rows;
- diagnostics for missing, ambiguous, duplicate, and mismatched context;
- hover tests for combined signatures;
- go-to-definition from implicit context uses to provider fields.

Runtime:

- compile and run a basic `Clock` example;
- compile and run a context-polymorphic `with-clock` provider function;
- compile and run effects plus context once ordinary effect runtime and context
  lowering interact;
- compile and run a handler/resumption smoke test proving plain `given` is
  shared across resumptions.

Commands should follow the repo's existing phase validation pattern:

```txt
cargo test -p fx-syntax
cargo test -p fx-core
cargo test -p fx-compiler
cargo test -p fx-cli
cargo test -p fx-lsp
cargo test
cargo run -q -p fx-cli -- check examples/phase-5
cargo run -q -p fx-cli -- compile --target js examples/phase-5/basic.fx
```

## Design recommendations

- Use compiler-elaborated context parameters, not runtime dynamic lookup.
- Represent context rows separately from record rows and effect rows.
- Use lexical provider stacks with strict name shadowing.
- Resolve by field name plus compatible type.
- Lower inserted context as an explicit context record argument.
- Preserve inserted context in core/lowering metadata for tooling.
- Treat plain `given` as shared across resumptions.
- Leave branch-local/resourceful context to `given each`.
- Keep module-level contextual exports and final call-site `given` arguments out
  of Phase 5.
- Start implementation with goal examples and a work-plan note, following the
  Phase 3 and Phase 4 pattern.

## Open questions

1. Should same-name provider shadowing be strict even when the inner field has
   an incompatible type?
2. Should generated JavaScript place the hidden context record before or after
   ordinary arguments?
3. Should contextual provider fields get a distinct typed-span kind, or reuse
   local/parameter spans with contextual metadata?
4. Should v0 allow referencing contextual values by name only inside provider
   bodies, or should a function body with a context row also bind those names
   automatically?

My recommended answers are: strict shadowing, context record first, distinct
typed-span kind if the LSP surface benefits, and automatic binding of context
field names inside any body whose signature declares those requirements.

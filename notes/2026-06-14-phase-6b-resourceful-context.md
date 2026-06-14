# Phase 6B resourceful context plan

Date: 2026-06-14

## Purpose

Phase 6B makes the Phase 6A branch-lifetime model executable for resourceful
context and lifecycle cleanup.

The goal is to add the smallest surface that can safely express values that are
fresh per resumed branch and cleaned up when that branch exits:

```fx
given each(
  () -> { trace = start-trace() },
  context -> end-trace(context.trace)
)
  action()
```

Phase 6B also adds a generic `finally(cleanup)` lifecycle boundary for cleanup
that does not produce contextual values.

This phase should not add fresh scope creation, non-escape checking, scope
masking, structured concurrency, handler literals, or static one-shot/linear
resumption checking. Those features depend on the same lifetime model but need
separate design work.

## Local baseline

Phase 5 implemented shared lexical `given` context:

- context rows such as `a given { clock: Clock }`;
- providers such as `given { clock = system-clock() } body`;
- contextual insertion at checked call sites;
- operation-site context preservation across resumptions.

Phase 6A documented and validated the handler/lifetime contract:

- handlers are deep by default;
- resumptions are multi-resumption-capable;
- plain `given` context is shared across resumed branches;
- branch-local setup must run once for each branch that enters a resourceful
  boundary;
- branch-local cleanup must run once for each branch whose setup ran;
- cleanup behavior must be defined for normal return, non-resume, unhandled
  escape, cancellation, and any later explicit discontinuation operation.

Phase 6B starts from those rules and turns the deferred `given each` and
`finally` behavior into parser, checker, lowering, runtime, diagnostics, and
examples.

## Research summary

### Koka `initially` and `finally`

Koka's lifecycle machinery is the closest semantic reference. Its book points
out that ordinary handlers may never resume, resume once, or resume more than
once, and that resource cleanup must account for all three cases. Koka provides
`finally` for cleanup on normal return or non-resuming unwind, and `initially`
for setup that runs on first entry and again when a resumption is entered more
than once.

The useful lesson for fxl is not to expose `initially` directly. Use it as the
semantic model behind `given each`: setup is branch-local, cleanup belongs to
the same branch, and plain `given` remains shared.

Source:

- <https://koka-lang.github.io/koka/doc/book.html>

### OCaml one-shot continuations and discontinuation

OCaml 5 uses one-shot continuations and exposes `discontinue` to resume a
captured continuation with an exception so cleanup can run. Its manual warns
that continuations which are neither continued nor discontinued can retain
resources, and that multi-shot continuations make linear resource discipline
hard to debug.

fxl deliberately keeps multi-resumption-capable handlers for now, but Phase 6B
should copy the resource-safety lesson: a resourceful boundary must have an
explicit branch exit path. If a future feature stores or abandons resumptions,
it must either finalize the branch or be gated as raw control.

Source:

- <https://ocaml.org/manual/5.0/effects.html>

### ZIO `Scope`

ZIO models a scope as a lifetime that owns resources and finalizers. Resources
created with `acquireRelease` register finalizers with the current scope, and
`ZIO.scoped` closes the scope when the workflow finishes. ZIO also makes exit
values available to finalizers and releases resources in reverse acquisition
order.

The useful lesson for fxl is to treat cleanup as owned by a dynamic lifetime
boundary. Cleanup failure ownership should belong to that boundary, and nested
resourceful boundaries should clean up in innermost-first order.

Source:

- <https://zio.dev/reference/resource/scope/>

### Cats Effect `Resource`

Cats Effect `Resource` packages acquisition and finalization, composes nested
resources without bracket pyramids, and releases resources in reverse
acquisition order. It also highlights a key safety expectation: finalization
happens as soon as the `use` block finishes, so resource values must not escape
their lifetime.

The useful lesson for fxl is that ergonomic resource composition needs a
structured boundary, not ad hoc cleanup calls. Phase 6B should prevent
branch-local context from escaping as an ordinary value when the checker can
see the escape.

Source:

- <https://typelevel.org/cats-effect/docs/std/resource>

### Prior `fx` finalizer and handler-capture work

The TypeScript `fx` work settled two constraints that remain useful here:

- handler capture for delayed runnable work should be scope-delimited and
  recursive through the owning scope;
- cleanup failure aggregation belongs to the scope release boundary, not to
  registration-site recovery flow.

Phase 6B should not directly port those APIs, but it should preserve the
separation: registration/setup effects are different from cleanup effects, and
cleanup failures are owned by the lifetime boundary that runs cleanup.

Relevant local reference:

- `/Users/brian/dev/@briancavalier/fx/notes/2026-06-03-scoped-handler-capture-design.md`

## Selected direction

Phase 6B should support two surface forms:

```fx
finally(cleanup)
  body

given each(init)
  body

given each(init, cleanup)
  body
```

where:

- `finally(cleanup)` runs `cleanup` when the body branch exits;
- `given each(init)` runs `init` for each branch that enters the body and
  installs the returned context row for that branch;
- `given each(init, cleanup)` additionally runs `cleanup(context)` when that
  branch exits;
- omitted cleanup is equivalent to `_ -> unit`;
- `init` is zero-argument in Phase 6B;
- branch identity is not exposed in Phase 6B;
- setup and cleanup are effectful computations;
- cleanup runs in LIFO order for nested resourceful boundaries;
- cleanup failures are reported by the owning lifetime boundary.

The implementation should treat `given each` as a resourceful context provider,
not as runtime dynamic lookup. Contextual insertion inside the body still uses
checked, explicit provider metadata. The difference from plain `given` is that
the provider record is created per branch instead of once for the shared
operation-site context.

## Include

- parser support for `given each(init) body` and
  `given each(init, cleanup) body`;
- parser/checker/compiler support for `finally(cleanup) body` as a function-like
  lifecycle boundary;
- internal representation for resourceful boundaries in checked/lowered
  expressions;
- branch-local provider frames in the JavaScript runtime;
- cleanup on normal return, non-resume, unhandled escape, and handler-thrown
  failure where the current runtime can observe it;
- LIFO cleanup ordering for nested resourceful boundaries;
- diagnostics for invalid initializer result rows, cleanup arity/type
  mismatches, unsupported branch exits, and escaping branch-local context;
- examples under `examples/phase-6b/`;
- updates to `examples/phase-6a/resourceful-boundary.fx.txt` once the syntax is
  supported.

## Defer

- fresh scope creation;
- non-escape checking for locally generated scopes;
- scope masking;
- structured concurrency and cancellation APIs;
- final call-site `given` arguments;
- module-level contextual exports;
- ordinary named arguments;
- exposing branch identity to `given each` initializers;
- linear or one-shot operation annotations;
- raw control operations that can store resumptions without automatic cleanup;
- parallel finalizer execution;
- user-defined cleanup aggregation policies.

## Semantic model

### Branch entry

A resourceful boundary is entered when control reaches the boundary body for a
specific branch.

For `given each(init, cleanup)`:

1. run `init()`;
2. require the result to be a context-provider record;
3. install the provider record for the branch body;
4. run the body using ordinary contextual insertion;
5. run `cleanup(provider)` exactly once when the branch exits.

For `given each(init)`, use `_ -> unit` as cleanup.

For `finally(cleanup)`:

1. install a cleanup frame;
2. run the body;
3. run `cleanup()` exactly once when the branch exits.

### Branch exit

Phase 6B should classify branch exits as:

- **return**: the body branch returns normally;
- **non-resume**: a handler abandons the branch by not resuming;
- **unhandled escape**: an operation escapes all matching handlers and reaches
  runtime failure;
- **thrown runtime failure**: generated JavaScript or runtime support throws;
- **future cancellation/discontinuation**: not implemented yet, but reserved by
  the model.

Cleanup must run for return, non-resume, unhandled escape, and thrown runtime
failure to the extent those exits are represented in the generated computation
protocol. Cancellation and explicit discontinuation remain future work, but the
runtime shape should leave a slot for them.

### Multi-resumption

If a handler resumes a continuation more than once and the suspended
continuation enters a resourceful boundary, each resumed branch gets its own
setup and cleanup.

If a resourceful boundary was entered before the operation site, both resumed
branches share that already-entered boundary. This matches the Phase 6A rule:
values computed outside the suspended continuation are shared, while values
computed inside the suspended continuation are branch-local.

### Handler and context visibility

Initializer expressions run at branch entry under the handlers and context
visible at that branch entry.

Cleanup expressions run at branch exit under the same captured branch-local
handler/context environment used for the resourceful boundary, not under an
unrelated resume-site provider.

This preserves the Phase 5 and Phase 6A operation-site context rule: a provider
visible only at the resume site does not rewrite contextual insertions that
belong to the suspended branch.

### Cleanup failures

Phase 6B should use the simplest failure rule that matches the current runtime:

- if the body exits successfully and cleanup fails, the boundary fails with the
  cleanup failure;
- if the body fails and cleanup succeeds, the boundary preserves the body
  failure;
- if both body and cleanup fail, the boundary reports an aggregate cleanup
  failure with the body failure as primary;
- multiple cleanup failures are accumulated in LIFO cleanup order.

The exact aggregate value can initially be runtime-internal. A user-facing
failure type or effect-specific cleanup error can wait for a later error-model
phase.

## Work units

### 1. Phase 6B setup, learning log, and goal fixtures

Create the phase scaffolding before implementation edits.

Acceptance:

- `notes/2026-06-14-phase-6b-learnings.md` exists with the standard learning
  log shape.
- `examples/phase-6b/` exists with goal-target examples for:
  - plain `finally`;
  - `given each(init)`;
  - `given each(init, cleanup)`;
  - repeated resume creating fresh context per branch;
  - a resourceful provider that cleans up after non-resume;
  - nested resourceful boundaries proving LIFO cleanup order;
  - diagnostics for bad init result, bad cleanup parameter, and escaping
    branch-local context.
- Examples avoid fresh scopes, scope masking, structured concurrency, exposed
  branch IDs, and raw control.

### 2. Settle the exact surface syntax

Confirm the surface before parser work.

Recommended syntax:

```fx
finally(cleanup)
  body

given each(init)
  body

given each(init, cleanup)
  body
```

where `cleanup` is a value-level expression:

- `finally(() -> close(file))`;
- `given each(() -> { trace = start-trace() })`;
- `given each(() -> { trace = start-trace() }, context -> end-trace(context.trace))`.

Acceptance:

- The chosen syntax composes with existing layout and trailing-action style.
- `initially` remains absent from public syntax.
- `given each { ... }` continues to produce a helpful diagnostic or migrates to
  a precise syntax error that suggests `given each(init)`.
- The formatter has a clear expected layout for nested forms.

### 3. Parse `finally` lifecycle expressions

Add parser support for lifecycle cleanup boundaries.

Parser behavior:

- parse `finally(cleanup) body` as a distinct expression;
- preserve spans for `finally`, cleanup expression, and body;
- recover from missing parentheses, missing cleanup expression, and missing
  body;
- avoid changing ordinary function calls named `finally` if the parser requires
  disambiguation.

Acceptance:

- Parser tests cover `finally(() -> unit) body`, nested `finally`, malformed
  cleanup, malformed body, and formatting round-trip.
- Existing `given`, `within`, handler, and call parsing remains unchanged.

### 4. Parse `given each`

Replace the Phase 5 unsupported parser diagnostic with structural parser
support.

Parser behavior:

- parse `given each(init) body`;
- parse `given each(init, cleanup) body`;
- preserve spans for `given`, `each`, initializer, optional cleanup, and body;
- reject extra arguments clearly;
- recover from missing parentheses, missing initializer, malformed cleanup, and
  missing body.

Acceptance:

- Parser tests cover one-argument, two-argument, nested, and malformed forms.
- The AST distinguishes shared `given` providers from resourceful `given each`.
- Existing Phase 5 shared provider syntax remains unchanged.

### 5. Add checked resourceful-boundary nodes

Represent `finally` and `given each` explicitly in checked expressions and
lowered core.

Expected internal data:

- cleanup expression;
- initializer expression for `given each`;
- provider fields produced by the initializer;
- branch-local provider frame identity;
- body expression;
- spans for diagnostics and LSP metadata.

Acceptance:

- The checker can distinguish shared provider frames from branch-local provider
  frames.
- Lowering can see resourceful boundaries without re-parsing source shape.
- Existing checked representations for plain `given`, `within`, handlers, and
  calls remain stable.

### 6. Check `finally`

Type-check lifecycle cleanup boundaries.

Checker behavior:

- `cleanup` must be callable with no arguments and return `Unit`;
- cleanup may have effects and context requirements;
- body type, effect row, and context row remain the result surface of the
  `finally` expression;
- cleanup effects are either merged into the expression effect row or gated
  behind an explicit cleanup-runtime diagnostic until the compiler/runtime can
  execute them.

Acceptance:

- Checker tests cover valid cleanup, wrong arity, wrong return type, cleanup
  needing context, cleanup with effects, and nested cleanup.
- Diagnostics point to the cleanup expression and explain the expected shape.

### 7. Check `given each`

Type-check resourceful contextual providers.

Checker behavior:

- `init` must be callable with no arguments;
- `init` result must be a record-like provider whose fields can satisfy context
  requirements inside the body;
- `cleanup`, when present, must accept exactly the provider record type and
  return `Unit`;
- branch-local provider fields are visible by ordinary contextual name inside
  the body;
- contextual insertion inside the body uses branch-local providers before outer
  shared providers;
- provider fields are not ordinary values that can be returned directly unless
  they were also produced by ordinary source expressions with safe lifetime.

Acceptance:

- Checker tests cover fresh context per branch, context tail propagation,
  provider type mismatch, cleanup parameter mismatch, cleanup return mismatch,
  provider shadowing, and missing context.
- Shared `given` behavior from Phase 5 remains unchanged.

### 8. Define branch-local escape diagnostics

Prevent obvious escapes of branch-local context.

Initial conservative rule:

- contextual names introduced only by `given each` may be used to satisfy
  contextual requirements inside the body;
- they may not be returned as ordinary values, stored in top-level bindings, or
  captured by lambdas that escape the resourceful boundary when the checker can
  detect it;
- passing them to functions that consume them inside the body is allowed.

Acceptance:

- Diagnostics identify the branch-local context field and the resourceful
  boundary that owns it.
- The rule is conservative and may reject some safe programs.
- The rule does not attempt full region inference or fresh-scope non-escape
  checking.

### 9. Extend the JavaScript runtime with cleanup frames

Add runtime support for resourceful boundaries in the computation protocol.

Expected runtime helpers:

- enter a cleanup frame;
- run body computation;
- run cleanup on return and failure;
- aggregate body and cleanup failures;
- preserve handler dispatch through cleanup where supported;
- support repeated resume by creating fresh frames for boundary entries inside
  the resumed continuation.

Acceptance:

- Runtime unit tests cover cleanup on success, body failure, cleanup failure,
  body-plus-cleanup failure, nested LIFO cleanup, and repeated resume.
- Existing Phase 3.1, Phase 4.1, Phase 5, and Phase 6A runtime tests keep
  passing.

### 10. Lower `finally`

Compile `finally(cleanup) body` to the cleanup-frame runtime.

Acceptance:

- Generated JavaScript runs cleanup after normal return.
- Generated JavaScript runs cleanup when body fails through the runtime
  computation protocol.
- Nested `finally` cleans up in LIFO order.
- Cleanup can use contextual values visible at the boundary.

### 11. Lower `given each`

Compile `given each` to branch-local provider frames.

Lowering behavior:

- lower `init` as a computation at branch entry;
- install the returned provider in the branch-local context frame;
- lower the body with contextual insertions pointing at that branch-local
  provider;
- lower cleanup with access to the provider record;
- remove the provider frame after cleanup.

Acceptance:

- `given each(init)` creates a fresh provider for each resumed branch.
- `given each(init, cleanup)` cleans up each provider exactly once.
- Resume-site providers do not affect the branch-local provider created at the
  operation site.
- Generated output remains deterministic for examples.

### 12. Handle non-resume and unhandled escape

Ensure abandoned branches clean up when the runtime observes abandonment.

Acceptance:

- A handler that ignores `resume` triggers cleanup for boundaries entered in
  the abandoned branch.
- An unhandled operation escaping through a resourceful boundary triggers
  cleanup before the runtime reports the unhandled operation.
- Unsupported raw-control-like patterns remain gated if the runtime cannot
  observe their exit.

### 13. Update diagnostics and unsupported gates

Replace old Phase 5/6A unsupported messages with Phase 6B-specific diagnostics.

Acceptance:

- `given each` no longer reports "not supported in Phase 5" once parser/checker
  support lands.
- Unsupported branch exits identify the missing runtime capability.
- Error messages distinguish shared `given` from branch-local `given each`.
- Cleanup diagnostics mention whether the expected cleanup shape is
  `() -> Unit` or `Context -> Unit`.

### 14. Update formatter and examples

Teach the formatter the accepted layout and promote the Phase 6A goal target.

Acceptance:

- Formatter tests cover `finally` and `given each`;
- `examples/phase-6a/resourceful-boundary.fx.txt` is either moved/promoted to
  `examples/phase-6b/` or left as a historical pointer;
- `examples/README.md` documents Phase 6B examples and validation commands;
- `fx fmt --check examples` remains a useful broad gate.

### 15. LSP metadata

Expose enough metadata for tooling to explain resourceful context.

Acceptance:

- Hover on branch-local context fields shows the field type and indicates
  branch-local provider ownership.
- Go-to-definition from a contextual use inside `given each` reaches the
  provider field returned by the initializer or a synthetic provider span.
- Diagnostics can point from missing/mismatched context to the nearest
  branch-local provider.

### 16. Validation

Expected validation commands:

- `cargo test -p fx-syntax`;
- `cargo test -p fx-core`;
- `cargo test -p fx-compiler`;
- `cargo run -p fx-cli -- check examples/phase-6b`;
- `cargo run -p fx-cli -- compile --target js examples/phase-6b/<fixture>.fx`
  for executable fixtures;
- `cargo run -p fx-cli -- fmt --check examples`.

## Success criteria

Phase 6B is complete when:

- `finally` and `given each` parse, check, format, compile, and run for the
  supported subset;
- branch-local context is fresh for each resumed branch;
- cleanup runs exactly once for each successful setup;
- cleanup runs on normal return, non-resume, and observable failure exits;
- nested cleanup uses LIFO ordering;
- cleanup failure ownership is documented and tested;
- obvious branch-local context escapes are diagnosed;
- Phase 3 through Phase 6A examples and runtime behavior remain unchanged.

## Open questions

1. Should cleanup effects be merged into the enclosing effect row immediately,
   or should cleanup have a distinct visible requirement form later?
2. Should cleanup failures use a standard runtime aggregate value first, or
   wait for an explicit error-effect design?
3. Should branch-local context escape diagnostics be purely syntactic in Phase
   6B, or should the checker add a small region marker for provider fields?
4. Should `finally` be syntax, a prelude function with compiler-recognized
   semantics, or both?
5. Should future structured concurrency reuse the same cleanup-frame runtime
   directly, or wrap it in a separate nursery lifetime protocol?

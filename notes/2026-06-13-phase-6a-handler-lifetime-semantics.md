# Phase 6A handler and lifetime semantics plan

Date: 2026-06-13

## Purpose

Phase 6A turns the current effect-handler runtime behavior into an explicit
semantic contract before Phase 6 adds resourceful context, fresh scopes,
structured concurrency, or richer handler forms.

The narrow goal is not to add every advanced feature. It is to make the
interaction between handlers, resumptions, operation-site context, and lifetime
boundaries precise enough that later Phase 6 tracks can build on it without
retroactively changing basic handler behavior.

## Local baseline

Phase 3.1 introduced the JavaScript effect runtime protocol:

- operations compile to request objects with operation identity, payload, and a
  `resume` continuation;
- handlers dispatch by operation key;
- `resume(value)` re-enters the suspended continuation under the same handler
  boundary;
- the protocol deliberately did not settle full laws for storing, duplicating,
  repeatedly calling, or abandoning resumptions.

Phase 4.1 extended the same protocol to scoped effects:

- scoped requests carry family and scope identity metadata;
- scoped handlers dispatch by exact family plus scope;
- scoped handler clauses still expose only payload parameters plus `resume`;
- scoped effects did not change the Phase 3.1 resumption law.

Phase 5 added coeffects and contextual insertion:

- plain `given` context is shared across resumptions;
- resumed continuations use the operation-site contextual insertion, not a
  provider visible only at the resume site;
- `given each` and resourceful branch-local context remain deferred.

Phase 6A starts from that baseline. It should not surprise existing Phase 3,
Phase 4.1, or Phase 5 programs.

## Research summary

### Deep handlers and explicit continuations

Pretnar's tutorial on algebraic effects and handlers models operation calls as
branching points with explicit continuations, and explains deep handlers as
handlers that continue to wrap the resumed computation. This matches the
current fxl runtime: a resumed continuation remains under the same handler.

Source:

- <https://www.eff-lang.org/handlers-tutorial.pdf>

### Practical multi-resumption behavior

Flix documents algebraic effect handlers with dynamic scope, deep handlers, and
multiple resumptions. Its ambiguity example calls a continuation twice to
explore both outcomes, and it shows that handler nesting order matters.

Source:

- <https://doc.flix.dev/effects-and-handlers.html>

### One-shot continuations and resource safety

OCaml 5 chooses one-shot delimited continuations. Its manual gives two useful
warnings for fxl:

- multi-shot continuations are more expensive and can break linear resources
  such as sockets and file descriptors;
- failing to resume or discontinue a captured continuation can leak the
  suspended computation and resources it owns.

OCaml's choice is not a direct fit for fxl because fxl already has examples and
tests that rely on multi-resumption-style behavior, but it is a strong warning
that resourceful Phase 6 features need explicit lifetime semantics.

Source:

- <https://ocaml.org/manual/5.0/effects.html>

### Koka lifecycle lessons

Koka distinguishes ordinary resumption, resuming more than once, finalization,
raw control, linear effects, and named/scoped handlers. The important lesson for
fxl is the separation of concerns:

- normal handlers can support multiple resumption;
- resource finalization must define what happens when an operation never
  resumes, resumes once, or resumes more than once;
- `initially` can model per-branch initialization under repeated resumptions;
- raw control shifts responsibility for eventual resume/finalize back to the
  programmer.

For fxl, `initially` should remain an explanatory semantic primitive. The
surface direction from earlier notes remains `finally` and later `given each`.

Source:

- <https://koka-lang.github.io/koka/doc/book.html>

### Prior fx finalizer and handler-capture work

The TypeScript `fx` design work already found two durable constraints:

- handler capture for delayed runnable work should be scope-delimited and
  recursive through the owning scope;
- cleanup failure aggregation belongs to the scope release boundary, not to
  registration-site recovery flow.

Phase 6A should carry these lessons into fxl as design constraints, not as a
direct port of TypeScript APIs.

Relevant local reference:

- `/Users/brian/dev/@briancavalier/fx/notes/2026-06-03-scoped-handler-capture-design.md`

## Selected direction

Phase 6A should document and test the current deep, multi-resumption-capable
handler model, while making resourceful boundaries explicit enough for later
features.

Rules to establish:

- Handlers are deep by default.
- Handler nesting order is observable and remains part of the semantics.
- A handler clause may ignore `resume`, call it once, or call it more than once.
- Resumed continuations re-enter the operation-site handler and context
  environment.
- Resume-site providers do not rewrite operation-site contextual insertion.
- Non-resumption is allowed for abortive control, but any resourceful boundary
  crossed by the abandoned continuation must have a defined cleanup rule before
  resourceful surface features are accepted.
- Phase 6A does not add static linearity checking. Linear or one-shot effects
  can be a later optimization and safety track.

This keeps fxl expressive enough for backtracking, search, generators, and
testable contextual control, while acknowledging that resources need a stronger
branch-lifetime model.

## Include

- a written handler/resumption/lifetime semantic contract;
- goal examples under `examples/phase-6a/`;
- checker/compiler/runtime tests that lock in current deep-handler behavior;
- diagnostics or unsupported-runtime gates for resourceful constructs whose
  cleanup semantics are not implemented yet;
- a small internal model of branch lifetime terminology that later `finally`,
  `given each`, fresh-scope, and structured-concurrency work can reuse.

## Defer

- `given each` syntax and implementation;
- resourceful branch-local context implementation;
- fresh scopes and non-escape checking;
- scope masking;
- first-class handler names;
- handler literals as values;
- structured concurrency APIs;
- static one-shot or linear continuation checking;
- final call-site `given` arguments;
- module-level contextual exports.

## Semantic terms

Phase 6A should standardize these terms across notes, diagnostics, and tests:

- **operation site**: the source expression that performs an operation request.
- **handler boundary**: the dynamic handler that receives a matching request.
- **resumption**: the continuation from the operation site to the handler
  boundary.
- **resume site**: the call to the resumption inside a handler clause.
- **branch**: one execution of a suspended continuation, including the original
  action path and every resumed path.
- **abandoned branch**: a continuation path that is not resumed.
- **resourceful boundary**: a dynamic boundary that owns setup/cleanup work for
  a branch.
- **operation-site context**: contextual values inserted for the computation
  that performed the operation.
- **resume-site context**: contextual values visible where the handler calls
  `resume`.

## Work units

### 1. Create Phase 6A fixtures and learning log

Create `examples/phase-6a/` before changing semantics.

Fixtures should cover:

- a handler that resumes once;
- a handler that resumes twice;
- a handler that does not resume;
- nested handlers where order changes the result;
- a scoped handler that resumes and re-enters the same scope;
- a contextual operation proving operation-site context wins over resume-site
  context;
- an expected-failing resourceful example that documents the future cleanup
  rule without accepting unsupported syntax.

Acceptance:

- `examples/phase-6a/README.md` explains which fixtures should compile now and
  which are goal targets.
- `notes/2026-06-13-phase-6a-learnings.md` exists with the standard learning
  log shape.
- Existing Phase 3, Phase 4.1, and Phase 5 examples remain unchanged.

### 2. Write the semantic contract

Add a focused contract note, or expand this plan after implementation starts,
covering:

- deep versus shallow handlers;
- handler nesting order;
- unhandled request forwarding;
- `return` clause behavior;
- resumption cardinality;
- operation-site context preservation;
- abandoned branch behavior;
- runtime responsibility for unsupported resourceful boundaries.

Acceptance:

- The contract explicitly says fxl handlers are deep.
- The contract explicitly says current fxl resumptions are
  multi-resumption-capable.
- The contract explains that Phase 6A does not guarantee resource safety for
  arbitrary multi-shot resumptions until a resourceful boundary protocol lands.
- The contract names the later decision points for linear/one-shot effects.

### 3. Lock in deep-handler runtime behavior

Add compiler/runtime tests that prove resumed continuations remain under the
same handler boundary.

Acceptance:

- A resumed continuation performs the same operation again and is handled by
  the same handler.
- A nested handler can override an outer handler for the resumed path.
- A request not handled by the nearest handler still forwards outward.
- Existing Phase 3.1 runtime tests continue passing.

### 4. Lock in multi-resumption behavior

Add executable tests for calling `resume` more than once.

Acceptance:

- A `choose`-style handler can call `resume(left)` and `resume(right)` and
  combine both results.
- Handler-local immutable values are shared by both resumed branches.
- Operation-site plain `given` context is shared by both resumed branches.
- Resume-site context does not affect contextual values already inserted for
  the operation-site continuation.

### 5. Specify non-resumption

Clarify what happens when a handler ignores `resume`.

Acceptance:

- Abortive operations whose result type is uninhabited can be handled without
  resume.
- Resumable operations may also be handled without resume, producing a handler
  result instead.
- The checker continues to type the explicit `resume` parameter even when it is
  unused.
- Diagnostics explain unreachable continuation behavior only when the type
  system has enough information to do so clearly.

### 6. Classify operation result shapes

Introduce documentation and tests around operation result types:

- uninhabited result: naturally abortive;
- inhabited result: resumable by default;
- future linear operation: must resume exactly once;
- future raw control operation: may escape ordinary finalization discipline.

Acceptance:

- No new surface syntax is required in Phase 6A.
- Notes avoid claiming that inhabited operations must resume.
- Later `linear effect` or operation annotations have a clear design slot.

### 7. Define branch lifetime invariants

Write the branch-lifetime model that later resource features must satisfy.

Required invariants:

- each resumed branch has a distinct branch lifetime;
- branch-local setup runs once per branch;
- branch-local cleanup runs once for each branch whose setup ran;
- shared context installed outside the branch remains shared;
- abandoned branches do not run branch setup that never occurred;
- resourceful boundaries must define whether cleanup happens at branch return,
  branch abort, unhandled escape, cancellation, or explicit discontinuation.

Acceptance:

- The model explains why plain `given` is shared across resumptions.
- The model explains why `given each` must be branch-local.
- The model explains why `finally` is simpler than `given each` but still
  depends on abandoned-branch behavior.

### 8. Add unsupported gates for resourceful syntax

If parser support for future syntax appears before runtime semantics, keep it
behind clear diagnostics.

Acceptance:

- `given each` remains unsupported until branch-local initialization and
  cleanup semantics are executable.
- Any `finally` surface form remains unsupported until cleanup behavior is
  tested for return, non-resume, multi-resume, and unhandled escape.
- Unsupported diagnostics point at the syntax whose semantics are incomplete.

### 9. Prepare an internal lifetime-boundary design

Design the internal runtime shape for a future resourceful boundary without
committing to public syntax.

The design should answer:

- how setup and cleanup computations are represented;
- whether cleanup can perform ordinary effects;
- which handlers are visible to cleanup;
- how cleanup failures are aggregated;
- whether cleanup sees operation-site context, registration-site context, or
  release-site context;
- how repeated resumptions create repeated branch-local setup/cleanup pairs.

Acceptance:

- The design keeps cleanup failure ownership at the lifetime boundary.
- The design distinguishes registration-time effects from cleanup-time effects.
- The design does not require scope masking or fresh scopes to exist yet.

### 10. Document the later `given each` mapping

Add a short follow-on section showing how `given each` would use the Phase 6A
branch-lifetime model.

Expected mapping:

```fx
with given each(
  () -> { trace: start-trace() },
  context -> end-trace(context.trace)
)
  action()
```

Means:

```txt
for each branch that enters action:
  create context
  run the branch with that context
  clean up that context when the branch exits according to the lifetime rules
```

Acceptance:

- `given each` remains a Phase 6B feature.
- The mapping is precise enough to drive parser/checker/runtime work later.

### 11. Update examples and implementation plan

Update the implementation plan and examples README so Phase 6A is discoverable.

Acceptance:

- `notes/2026-05-23-implementation-phases.md` has a short Phase 6A section
  pointing to this plan.
- `examples/README.md` lists `phase-6a` once fixtures exist.
- Phase 6 still lists the broader advanced-surface candidates.

### 12. Validate

Run the focused validation gates after implementation work begins.

Expected commands:

- `cargo test -p fx-syntax`;
- `cargo test -p fx-core`;
- `cargo test -p fx-compiler`;
- `cargo run -p fx-cli -- check examples/phase-6a`;
- `cargo run -p fx-cli -- compile --target js examples/phase-6a/<fixture>.fx`
  for executable fixtures.

Documentation-only edits should at least run formatting checks if the phase
touches examples.

## Phase Artifacts

- Semantic contract:
  `notes/2026-06-13-phase-6a-semantic-contract.md`
- Learning log:
  `notes/2026-06-13-phase-6a-learnings.md`
- Goal examples:
  `examples/phase-6a/`

## Success criteria

Phase 6A is complete when:

- fxl has a documented deep-handler, multi-resumption-capable semantic
  contract;
- tests prove nested handlers, repeated resume, non-resume, scoped resume, and
  operation-site context behavior;
- resourceful syntax remains rejected or gated until branch-lifetime semantics
  are executable;
- later `finally`, `given each`, fresh scopes, and structured concurrency work
  can cite a single branch-lifetime model instead of re-deciding resumption
  behavior feature by feature.

## Open questions

1. Should fxl eventually add explicit `linear effect` or operation-level
   cardinality annotations, or should one-shot behavior stay an optimization of
   selected library interpreters?
2. Should escaping/storing `resume` be statically restricted before resourceful
   boundaries land, or documented as advanced behavior until a later safety
   phase?
3. Should cleanup effects be captured at registration time, release time, or by
   an explicit handler-capture API?
4. Should cleanup failures aggregate through a standard `AggregateError`-like
   value, an effect-specific error type, or a scoped resource protocol?
5. Should Phase 6A introduce internal branch identifiers for tests and tooling,
   or keep branch identity purely semantic until `given each` needs it?

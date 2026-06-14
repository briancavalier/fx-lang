# Phase 6A handler and lifetime semantic contract

Date: 2026-06-13

## Status

This contract describes the behavior validated by Phase 6A. It is intentionally
narrow: it covers existing handlers, resumptions, scoped handlers, and plain
context. It does not add resourceful context, fresh scopes, scope masking,
structured concurrency, first-class handlers, or static one-shot checking.

## Handler Boundaries

fxl handlers are deep by default. When a handler clause calls `resume`, the
resumed continuation runs under the same handler boundary that received the
operation request. If the resumed continuation performs the same operation
again, the same boundary can handle it again.

Handler nesting order is observable. The nearest matching handler receives an
operation first. If that handler has no matching clause for a request, the
request forwards outward to the next handler boundary.

Handler `return` clauses run when the handled action returns normally. They do
not run for a request that is forwarded beyond the handler and remains
unhandled.

## Resumptions

A resumption is the continuation from the operation site to the handler
boundary. Current fxl resumptions are multi-resumption-capable: a handler
clause may ignore `resume`, call it once, or call it more than once.

Ignoring `resume` is ordinary abortive control. This is natural for operations
with an uninhabited result type, but it is not limited to them: an inhabited
operation may also be handled without resuming when the handler produces the
overall result.

Calling `resume` more than once creates multiple executions of the suspended
continuation. Handler-local immutable values outside the suspended continuation
are shared by those executions. Values computed inside the suspended
continuation belong to the resumed branch that computed them.

Phase 6A does not guarantee resource safety for arbitrary multi-shot
resumptions. Resourceful boundaries need an explicit setup and cleanup protocol
before resource-owning computations can be safely duplicated, abandoned, or
escaped.

## Context

Operation-site context is the contextual insertion visible to the computation
that performed the operation. Resume-site context is the contextual provider
visible where the handler clause calls `resume`.

Resumed continuations preserve operation-site context. A provider introduced
only at the resume site does not rewrite contextual values already inserted for
the suspended continuation. Plain `given` providers are therefore shared across
resumed branches unless a later branch-local construct says otherwise.

## Scoped Handlers

Scoped effect requests carry operation identity, effect-family identity, and
scope identity. Scoped handler dispatch matches the exact family and scope. A
scoped resumption re-enters the same handler boundary and preserves the same
scope identity used at the operation site.

`within` only supplies the default scope for operation elision. It does not
change the deep-handler or resumption rules.

## Branch Lifetimes

A branch is one execution path through a suspended continuation, including the
original action path and every resumed path. Each resumed branch has a distinct
branch lifetime.

The branch-lifetime invariants for future resourceful boundaries are:

- branch-local setup runs once for each branch that enters the boundary;
- branch-local cleanup runs once for each branch whose setup ran;
- shared context installed outside the branch remains shared;
- abandoned branches do not run setup that never occurred;
- resourceful boundaries must define cleanup on normal return, non-resume,
  unhandled escape, cancellation, and any explicit discontinuation operation.

This explains the later split between plain `given` and `given each`. Plain
`given` installs shared operation-site context. `given each` must install
branch-local context by running setup for each branch that enters its body and
cleanup when that branch exits.

`finally` is simpler than `given each` because it does not need to produce
contextual values, but it still depends on the same abandoned-branch and
multi-resumption cleanup rules.

## Operation Result Shapes

Operation result shapes classify expected use, not current resumption
cardinality:

- an uninhabited result is naturally abortive;
- an inhabited result is resumable by default;
- a future linear operation must resume exactly once;
- a future raw-control operation may escape ordinary finalization discipline.

No Phase 6A syntax is required for this classification. Later `linear effect`
or operation-level annotations have a clear design slot here.

## Future Resourceful Boundary Shape

A future internal lifetime boundary should represent setup and cleanup as
separate computations owned by the boundary. Cleanup failure ownership belongs
to the lifetime boundary, not to registration-site recovery flow.

Open decisions for the resourceful phase:

- whether cleanup can perform ordinary effects;
- which handlers are visible to cleanup;
- whether cleanup sees operation-site, registration-site, release-site, or
  explicitly captured context;
- how cleanup failures aggregate;
- how repeated resumptions create repeated setup and cleanup pairs.

The expected `given each` mapping is:

```fx
with given each(
  () -> { trace = start-trace() },
  context -> end-trace(context.trace)
)
  action()
```

Meaning:

```txt
for each branch that enters action:
  create context
  run the branch with that context
  clean up that context when the branch exits according to the lifetime rules
```

`given each` remains a Phase 6B feature until this behavior is executable and
tested.

# Phase 6A learnings

Date: 2026-06-13

## Scope

Phase 6A is a semantics checkpoint for handlers, resumptions, operation-site
context, non-resumption, and branch lifetimes. It should keep existing Phase 3,
Phase 4.1, and Phase 5 behavior stable while making later resourceful features
depend on one written contract.

## Learnings

- The current JavaScript runtime is already deep: `resume` re-enters the
  suspended continuation using the same local handler boundary.
- The current runtime is multi-resumption-capable: calling `resume` more than
  once replays the continuation once per call and can combine the returned
  branch values.
- Handler-local immutable values are shared by resumed branches because they
  are outside the branch continuation.
- Plain `given` context is operation-site context. A provider visible only
  where a handler calls `resume` does not rewrite contextual insertion that was
  already compiled into the suspended continuation.
- Scoped handlers preserve exact family and scope identity through resumption.
  This keeps `within` elision consistent with explicit scoped operation calls.
- Resourceful features needed a separate branch-lifetime protocol before they
  could be accepted. Phase 6A documents the protocol shape; Phase 6B promotes
  `given each` and `finally` while fresh scopes and static one-shot checking
  remain later work.

## Validation Notes

- The former `resourceful-boundary.fx.txt` goal target now lives under
  `examples/phase-6b/resourceful-boundary.fx`.
- If future changes alter the runtime handler protocol, re-run the Phase 6A
  compiler smoke tests before changing the semantic contract.

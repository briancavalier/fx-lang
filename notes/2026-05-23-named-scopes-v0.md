# Named scopes in v0

Date: 2026-05-23

## Context

The effect syntax direction so far is:

- operation call sites use ordinary function syntax, such as `ask()`,
  `abort()`, and `yield(value)`;
- handlers are functions, or at least function-like values;
- effect requirements appear in row syntax;
- the language should stay lightweight, layout-sensitive, and sparse in
  punctuation.

Named scopes are the mechanism for distinguishing separate instances of the
same scoped effect family. They are useful when an operation belongs to a
resource region, control boundary, state cell, session, or data-flow channel.

The TypeScript `fx` library already uses this model. A scoped effect is an
ordinary effect request plus a scope identity, and handlers interpret requests
only when both the effect type and scope match. Same-effect requests from other
scopes remain unhandled.

## Research anchors

- Scoped algebraic effects separate operations that happen inside a delimited
  region from ordinary one-shot operation requests. This matters for resources,
  local state, generators, transactions, and other constructs whose behavior is
  tied to a nested action.
- Named handlers address the multiple-instance problem: a program may need more
  than one instance of the same effect, such as two state cells or two output
  channels.
- First-class fresh names are powerful but require non-escape rules, commonly
  modeled with rank-2 typing or a similar restriction. That is a larger feature
  than we want to take on in v0.
- Koka supports named/scoped handlers and masking, but the full surface is too
  large for v0. The useful lesson is the separation between ordinary effects,
  scoped effects, and handler-local naming.

Sources:

- A Calculus for Scoped Effects & Handlers: https://arxiv.org/abs/2304.09697
- Structured Handling of Scoped Effects: https://link.springer.com/chapter/10.1007/978-3-030-99336-8_17
- First-Class Names for Effect Handlers: https://xnning.github.io/papers/oopsla22namedh.pdf
- Koka book: https://koka-lang.github.io/koka/doc/book.html

## Option 1: scopes as explicit values only

In this option, v0 would not have dedicated scoped effect declarations. A scope
would be a normal value passed to ordinary effect operations:

```fx
scope SubmitOrder

abort(region: s) : never ! <Abort s>
using-managed(region: s, acquire: () -> Managed a) : a ! <Finally s, ...e>

submit-order() : OrderResult ! <Abort SubmitOrder, Finally SubmitOrder, ...e>
  session = using-managed(SubmitOrder, open-order-session)
  if token-expired() then abort(SubmitOrder) else submit(session)
```

Strengths:

- Smallest compiler feature.
- Closest to the current `fx` library API shape.
- Keeps scope passing explicit at every call site.

Weaknesses:

- Makes scoped operations feel bolted on rather than part of the effect system.
- Adds repetitive scope arguments in code that is already inside a known
  control or resource region.
- Does not give the language a direct way to say that an effect is inherently
  scope-indexed.

This option is viable, but it underuses the language design opportunity.

## Option 2: scoped effect declarations

In this option, v0 has a distinct declaration form for effect families indexed
by a scope parameter:

```fx
export scoped effect Abort s
  abort() : never

export scoped effect Finally s
  finally(finalizer: Exit s -> Unit) : Unit

export scoped effect State s a
  get() : a
  put(value: a) : Unit

export scoped effect Yield s out in
  yield(value: out) : in
```

Concrete scope declarations introduce named scope identities:

```fx
scope SubmitOrder
scope SessionState with <State Session>
scope AgentEvents with <Yield AgentEvent Unit>
```

The protocol row on a `scope` declaration describes the scoped protocol shape
supported by that scope. It omits the scope parameter itself. For example,
`scope Counter with <State Int>` means `Counter` can be used in rows as
`State Counter Int`.

Effect rows mention both the scoped effect family and the scope:

```fx
submit-order() : OrderResult ! <Abort SubmitOrder, Finally SubmitOrder, ...e>
  within SubmitOrder
    session = using-managed(open-order-session)
    if token-expired() then abort()
    submit(session)
```

Inside `within SubmitOrder`, unqualified scoped operations can resolve to
`SubmitOrder` when the operation and scope are unambiguous. The body above
elaborates as though `abort()` and `using-managed(...)` were called with
`SubmitOrder`.

Reusable code can remain scope-polymorphic:

```fx
submit-order(current: s) : OrderResult ! <Abort s, Finally s, ...e>
  within current
    session = using-managed(open-order-session)
    if token-expired() then abort()
    submit(session)
```

Strengths:

- Makes scoped effects first-class in the surface language.
- Preserves ordinary function-call operation syntax.
- Keeps handlers precise: they remove only `Effect Scope`, not every instance of
  `Effect`.
- Supports library abstractions through scope polymorphism.
- Matches the important semantic lesson from `fx` without copying its TypeScript
  encoding.

Weaknesses:

- Requires the type checker to track scope parameters in effect rows.
- Requires an elaboration rule for scope argument elision inside `within`.
- Needs clear ambiguity errors when more than one active scope could satisfy an
  operation.

This is the selected v0 direction.

## Option 3: first-class fresh scopes

This option would add a construct for creating fresh local scopes:

```fx
with new-scope Request
  ...
```

The body could use `Request`, but the scope name could not escape. This is the
language-level analogue of first-class names for handlers.

Strengths:

- Supports dynamically allocated effect instances.
- Prevents accidental interference between same-named or same-shaped regions.
- Gives a strong foundation for local resources, local state, and dynamic
  sessions.

Weaknesses:

- Requires non-escape checking for scope names.
- Likely requires rank-2 typing or an equivalent type-system device.
- Adds meaningful complexity before v0 has proved the basic effect system.

This option is valuable, but it is deferred until after v0.

## Selected v0 design

v0 should support scoped effect declarations and scope-polymorphic use.

Core forms:

```fx
scope Name
scope Name with <ScopeProtocol>

export scoped effect Effect s ...
  operation(...) : Result

within Scope
  body
```

Effect rows:

```fx
Result ! <Effect Scope, ...e>
Result ! <Effect s, ...e>
```

Rules:

- A scoped effect declaration introduces an effect family indexed by a scope
  parameter.
- A scope declaration introduces a concrete typed scope identity.
- Scope identity is part of an effect requirement.
- A handler for `Effect S` handles only requests for `Effect S`.
- Requests for `Effect T` remain unhandled when `T` is a different scope.
- Scope variables are ordinary type variables and may appear in effect rows.
- Scope polymorphism abstracts over a caller-supplied scope. It does not create
  fresh scopes.
- `within s` establishes a default scope for scoped operations in its body.
- Scope argument elision is allowed only when the operation's scoped effect
  family and active scope are unambiguous.
- Public and reusable APIs should usually expose a scope parameter rather than
  hard-code a concrete application scope.
- Fresh scope creation, scope masking, and first-class named handlers are not in
  v0.

## Handler shape

Handlers should stay function-like:

```fx
with-state(cell: s, initial: a, action: () -> r ! <State s a, ...e>) : r ! <...e>
  handle action()
    get(resume) ->
      ...

    put(value, resume) ->
      ...

    return(value) ->
      value
```

Use-site syntax should compose with the general trailing-action `with` form:

```fx
result =
  with with-state(SessionState, initial-session)
    n = get()
    put({ n | requests: n.requests + 1 })
    n
```

This keeps handlers as ordinary functions from the user's perspective. The
special behavior is in the function's implementation and type, not in a
separate handler declaration syntax.

## Examples

Scoped abort and finalization:

```fx
scope SubmitOrder

submit-order() : OrderResult ! <Abort SubmitOrder, Finally SubmitOrder, Console>
  within SubmitOrder
    session = using-managed(open-order-session)

    if token-expired()
    then
      refresh-token()
      abort()
    else
      submit(session)
```

Scope-polymorphic helper:

```fx
with-restart(region: s, action: () -> a ! <Abort s, Finally s, ...e>) : a ! <Abort s, Finally s, ...e>
  with restart-on-abort(region, restarts: 1)
    action()
```

Scoped state:

```fx
scope Counter with <State Int>

increment() : Int ! <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n
```

Scoped yield:

```fx
scope AgentEvents with <Yield AgentEvent Unit>

publish(event: AgentEvent) : Unit ! <Yield AgentEvents AgentEvent Unit>
  within AgentEvents
    yield(event)
```

## Open questions

- Should `scope Name with <State Int>` remain limited to one protocol-row
  entry in v0, or should a scope be allowed to declare several supported
  scoped protocols?
- Should operation names from different active scoped effects be resolved by
  expected type, or should ambiguity always require an explicit scope argument?
- Should explicit scoped calls use `abort(SubmitOrder)` or a distinct syntax
  such as `SubmitOrder.abort()` when elision is unavailable?
- How much of `Finally`, `Abort`, and `Yield` should be built into the standard
  library versus treated as normal user-definable scoped effects?

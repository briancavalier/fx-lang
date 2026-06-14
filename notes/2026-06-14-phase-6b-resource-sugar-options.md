# Phase 6B resource sugar options

Date: 2026-06-14

## Purpose

The current Phase 6B surface is intentionally direct:

```fx
finally(cleanup)
  body

given each(init)
  body

given each(init, cleanup)
  body
```

This works, but the function-call shape is visually heavy for common cleanup
and branch-local context cases. This note surveys sugar options and recommends
which ones are worth trying without changing the underlying checked/lowered
nodes.

## External references

### Koka: `with finally`

Koka keeps `finally` as an ordinary function but uses its `with` statement and
trailing-lambda sugar:

```koka
with finally
  hclose(h)
with fun fread()
  hreadline(h)
action()
```

Earlier in the Koka book, the compact form is shown as:

```koka
with finally{ println("exiting..") }
println("entering..")
throw("oops") + 42
```

The important lesson is that Koka does not need a special `defer` statement for
this. It keeps the semantic primitive as a function and lets block-oriented
application syntax carry the ergonomics.

Reference: <https://koka-lang.github.io/koka/doc/book.html>

### Go: function-scoped `defer`

Go uses:

```go
defer unlock(l)
```

The deferred call runs when the surrounding function returns. Arguments are
evaluated when the `defer` statement executes, and multiple defers run in
reverse order.

This is concise but function-scoped. fxl's Phase 6B boundaries are expression
and branch-lifetime scoped, not just function-return scoped, so copying Go
directly would be misleading unless `defer` were explicitly scoped to the
current block or resourceful boundary.

Reference: <https://go.dev/ref/spec#Defer_statements>

### Zig: block-scoped `defer` and `errdefer`

Zig uses:

```zig
defer close(file);
errdefer cleanupPartial(resource);
```

`defer` runs at scope exit and multiple defers run in reverse order. `errdefer`
runs only on error exits.

This is a closer fit than Go because it is block-scoped. The caveat is that it
registers cleanup in the current lexical block, while fxl also needs
multi-resumption branch entry. `defer` is good sugar for `finally`; it is not
enough for `given each` because the cleanup needs the per-branch provider value.

Reference: <https://ziglang.org/documentation/master/#defer>

### Python: `with expression as target`

Python wraps a block with a context manager:

```python
with open(path) as file:
    use(file)
```

The context manager's enter method produces a value, and its exit method is
guaranteed after a successful enter. Multiple context managers nest.

This is a good model for a future packaged resource abstraction, but it hides
the initializer/finalizer pair behind a value. Phase 6B still benefits from an
explicit pair because the branch-local contextual record is not meant to escape
as an ordinary value.

Reference: <https://docs.python.org/3/reference/compound_stmts.html#the-with-statement>

### Rust RAII

Rust ties cleanup to ownership and scope: when a value goes out of scope, its
`drop` function runs. This is excellent when resources are ordinary owned
values, but it depends on an ownership system and deterministic destructors.

fxl does not have that surface yet. RAII is a later design direction for owned
resources or fresh scopes, not a good immediate replacement for Phase 6B
resourceful context.

Reference: <https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html>

### Cats Effect `Resource` and ZIO `Scope`

Cats Effect's `Resource.make(acquire)(release).use(...)` and ZIO's
`acquireRelease` / `scoped` model resources as composable values owned by a
dynamic lifetime. Both emphasize reverse-order release and cleanup as soon as
the use/scoped block finishes.

These are strong references for later library design. For syntax, they argue
for keeping the underlying construct structured and composable rather than
making cleanup an ad hoc statement.

References:

- <https://typelevel.org/cats-effect/docs/std/resource>
- <https://zio.dev/reference/resource/scope/>

### Higher-order and scoped effects papers

Recent work on scoped and higher-order effects treats constructs like bracketing
and scoped effects as operations over computations, not as ordinary first-order
operations. This supports keeping `finally` and `given each` as explicit
boundary nodes internally even if the surface grows lighter sugar.

References:

- <https://arxiv.org/abs/2302.01415>
- <https://arxiv.org/abs/2402.03103>

## Sugar candidates

### Option A: Koka-style `with finally`

Surface:

```fx
with finally
  close(file)
body
```

or, preserving current trailing-action indentation:

```fx
with finally
  close(file)
  body
```

The first form is visually nice but ambiguous with the current `with target`
body rule: does `body` belong to `with finally`, or is it the next statement?
The second form is less ambiguous but makes cleanup and protected body siblings
inside the same block, which is not how existing `with` works.

Verdict: attractive, but it probably wants a broader `with` grammar redesign
before adoption.

### Option B: Prefix keyword `defer`

Surface:

```fx
defer close(file)
body
```

Desugaring:

```fx
finally(() -> close(file))
  body
```

This is the lightest sugar for plain cleanup. It should be block-scoped, not
function-scoped. The hard part is layout: in expression-position fxl needs a
clear protected body. A bare statement-like `defer` inside any block would
require changing block semantics to collect finalizers, which is larger than
Phase 6B.

Verdict: good later sugar for `finally`, but only if fxl gets statement-level
finalizer registration. Not enough for `given each`.

### Option C: `finally` as a block header

Surface:

```fx
finally
  close(file)
body
```

Desugaring:

```fx
finally(() -> close(file))
  body
```

This removes `()` and `->` from the common case while preserving the existing
resourceful-boundary node. It still has the same two-block layout question as
Koka-style `with finally`: where does cleanup end and body begin?

A safer version uses an explicit separator:

```fx
finally
  close(file)
then
  body
```

Verdict: possible, but `then` is new grammar and visually heavier than the
problem it solves.

### Option D: `given each` block clauses

Surface:

```fx
given each
  init
    { trace: start-trace() }
  cleanup context
    end-trace(context.trace)
  body
    action()
```

This is explicit and scales to multi-line setup/cleanup. It is also too much
ceremony for the common case and introduces sub-block labels that no other fxl
construct uses today.

Verdict: too large for v0 sugar.

### Option E: `given each` with `finally` clause

Surface:

```fx
given each { trace: start-trace() }
finally context -> end-trace(context.trace)
  action()
```

or:

```fx
given each { trace: start-trace() } finally context ->
  end-trace(context.trace)
  action()
```

This reads well for resourceful context, but it conflates a provider expression
with its initializer computation. The current zero-arg initializer is explicit
about effects and branch entry:

```fx
() ->
  emit("setup")
  { trace: start-trace() }
```

Verdict: good for pure provider records only, weak for effectful setup.

### Option F: `given each` record body plus optional `after`

Surface:

```fx
given each
  trace = start-trace()
after context
  end-trace(context.trace)
  action()
```

This is concise but reuses `=` provider-field spelling, while initializer
results are ordinary records that use `:`. That would reintroduce the exact
shared-`given` versus record-field distinction Phase 6B just clarified.

Verdict: avoid.

### Option G: Keep function syntax, add lambda elision

Surface:

```fx
finally(close(file))
  body

given each({ trace: start-trace() }, context -> end-trace(context.trace))
  action()
```

Desugaring:

```fx
finally(() -> close(file))
  body

given each(() -> { trace: start-trace() }, context -> end-trace(context.trace))
  action()
```

This is a small local sugar: if `finally` receives a non-lambda expression,
treat it as the zero-argument cleanup body; if `given each` receives a record
expression, treat it as the zero-argument initializer result.

It keeps the current syntax shape, avoids new block grammar, and solves the
most obvious heaviness in examples.

Risks:

- `finally(close(file))` looks like `close(file)` is evaluated immediately,
  while the semantics should delay it until exit.
- `given each({ ... })` hides that setup is re-run per branch.

Verdict: lowest implementation cost, but the delayed-evaluation surprise may
not be worth it.

### Option H: Bare block lambdas for the existing function syntax

Surface:

```fx
finally
  close(file)
do
  body

given each
  { trace: start-trace() }
cleanup context
  end-trace(context.trace)
do
  action()
```

This is familiar from languages with `do` notation or block arguments, and it
gives unambiguous boundaries. But it introduces a new general block-lambda
mechanism and several reserved words.

Verdict: useful if fxl wants general block arguments later; too broad as a
small Phase 6B cleanup.

## Recommendation

Do not immediately replace the implemented function syntax.

The best near-term sugar is a narrow Koka-inspired extension to existing
trailing-action syntax:

```fx
with finally(() -> close(file))
  body

with given each(() -> { trace: start-trace() }, context -> end-trace(context.trace))
  action()
```

This does not remove the function call, but it lets resourceful boundaries read
like other block-oriented control forms and preserves the current checked/lowered
nodes. It also aligns with the older examples that already used `with given
each(...)` as the aspirational surface.

If we want genuinely lighter syntax, prefer adding only one new sugar first:

```fx
defer close(file)
  body
```

as an expression-level alias for:

```fx
finally(() -> close(file))
  body
```

Do not add a matching `each` shorthand yet. `given each` carries too much
semantic weight: branch-local setup, contextual provider insertion, cleanup
argument binding, and non-escape restrictions. The explicit function syntax is
still buying clarity there.

## Candidate next experiment

Try this formatter/parser-only surface experiment behind tests:

```fx
with finally(() -> emit(":cleanup"))
  "body"

with given each(
  () ->
    emit("setup:")
    { clock: { now: "branch-" } }
  ,
  context -> emit(":cleanup")
)
  stamp(choice)
```

Acceptance:

- It desugars to the existing `Expr::Finally` / `Expr::GivenEach` nodes.
- It does not revive old shared-provider spelling like `given each { x = y }`.
- It does not make `finally(close(file))` imply delayed evaluation.
- `fx fmt --check examples` remains stable for multi-line setup and cleanup.

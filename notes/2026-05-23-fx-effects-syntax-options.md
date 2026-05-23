# fx algebraic effects syntax options

Date: 2026-05-23

## Research anchors

- Koka declares effects as operation signatures, and operation calls look like ordinary function calls. A handler can be a value/function that scopes over an action. Koka's `with f(args)` form desugars to passing the following block as a final function argument, which is a strong fit for fx's "handlers as functions" goal.
- Koka distinguishes ordinary effect operations from control operations (`fun` versus `ctl`) and exposes resumptions explicitly in handler clauses. It also supports named/scoped handlers and effect masking, but those are probably beyond v0 syntax.
- Unison calls effects "abilities". Ability requirements appear in function types, operations are value-level constructors, and handling is expressed as `handle e with h`, where the handler is a function over `Request`.
- Flix has dedicated `eff` declarations and `run { ... } with handler Effect { ... }` syntax. Its examples are useful because they show non-resumable effects, resumable effects, multiple handlers, and multiple resumptions clearly.
- OCaml 5 exposes effect handlers as lower-level runtime/control primitives rather than a fully typed source-level effect syntax. That is a useful warning: source syntax and type tracking should be designed together.

Sources:

- Koka book, effect handlers: https://koka-lang.github.io/koka/doc/book.html
- Koka qualifier sample: https://github.com/koka-lang/koka/blob/dev/samples/learn/qualifiers.kk
- Unison abilities reference: https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/
- Unison writing abilities: https://www.unison-lang.org/docs/fundamentals/abilities/writing-abilities/
- Flix effects and handlers: https://doc.flix.dev/effects-and-handlers.html
- OCaml effect handlers: https://ocaml.org/manual/5.0/effects.html

## Design constraints from current fx syntax

- Operation call sites should use ordinary function syntax: `ask()`, `raise("bad")`, `emit(value)`.
- Handlers should be functions, or at least function-like values.
- Function declarations are `name(params) : Return` followed by an indented body.
- Function calls always use parenthesized comma-separated arguments.
- Rows use final-only spreads: `{ ...r }`, `< ...r >`, and `(...tail)`.
- Visual and syntactic coherence are primary design constraints.

## Option A: effect suffix on result types

Effect declarations:

```fx
export effect Raise
  raise(message: Text) : a

export effect Emit a
  emit(value: a) : Unit
```

Function types put effects after the result type:

```fx
safe-divide(x: Int, y: Int) : Int ! <Raise>
  if y == 0 then raise("divide by zero") else x / y

hello() : Unit ! <Emit Text>
  emit("hello")
  emit("world")
```

Effect polymorphism uses the same final spread idea:

```fx
map-emit(f: a -> b ! <...e>, values: List a) : List b ! <Emit b, ...e>
  map(value -> 
    next = f(value)
    emit(next)
    next
  , values)
```

Handlers are ordinary functions that take an action:

```fx
catch(default: a, action: () -> a ! <Raise, ...e>) : a ! <...e>
  handle action()
    raise(message, resume) ->
      default

    return(value) ->
      value
```

Direct use is just a function call:

```fx
value = catch(0, () -> safe-divide(10, 0))
```

Add `with` only as trailing-action sugar:

```fx
value =
  with catch(0)
    safe-divide(10, 0)
```

Desugars to:

```fx
value = catch(0, () -> safe-divide(10, 0))
```

Strengths:

- Operation call sites are exactly normal function calls.
- Handlers are normal functions.
- `with` is not special to effects; it is a general trailing-action call form.
- The `! <...>` effect row is visually separate from value rows and variant rows while still reusing row syntax.
- Pure functions stay clean: no suffix.

Weaknesses:

- `!` is new punctuation.
- Function types need a precise precedence rule: `() -> a ! <E>` means a function returning `a` while performing `E`.
- `handle` is still special syntax inside handler bodies.

## Option B: Koka-like result prefix

Effect declarations are the same:

```fx
export effect Raise
  raise(message: Text) : a
```

Function result types put effects before the return type:

```fx
safe-divide(x: Int, y: Int) : <Raise> Int
  if y == 0 then raise("divide by zero") else x / y

hello() : <Emit Text> Unit
  emit("hello")
```

Effect-polymorphic handlers:

```fx
catch(default: a, action: () -> <Raise, ...e> a) : <...e> a
  handle action()
    raise(message, resume) -> default
    return(value) -> value
```

Strengths:

- Closest to Koka's mental model where an effect row qualifies the result.
- Avoids `!`.
- Reads left-to-right as "effectful result".

Weaknesses:

- `<Raise> Int` is visually close to type application and variant rows.
- It makes the return type less immediately visible.
- It is less familiar to programmers coming from typed languages with suffix annotations.

## Option C: Unison-style ability arrows

Effect declarations:

```fx
export ability Store a
  get() : a
  put(value: a) : Unit
```

Function types attach abilities to arrows:

```fx
get-name() : Unit ->{Store Text} Text
  get()
```

For fx declarations, this could be adapted as:

```fx
get-name() : Text with <Store Text>
  get()
```

Handlers are request functions:

```fx
store(initial: a, request: Request (Store a) result) : result
  match request
    get(resume) ->
      handle resume(initial) with store(initial)

    put(value, resume) ->
      handle resume(unit) with store(value)

    return(value) ->
      value
```

Strengths:

- Makes the handler-as-function model explicit.
- Gives a direct formal core: handlers consume requests.
- Good for teaching continuations and advanced handlers.

Weaknesses:

- Exposes `Request` too early.
- The surface syntax gets heavier than Koka.
- Less visually aligned with fx's current function declaration shape.

## Option D: handler literals as function values

Effect declarations:

```fx
export effect Emit a
  emit(value: a) : Unit
```

Handler literals are values:

```fx
emit-console = handler
  emit(value, resume) ->
    print-line(value)
    resume(unit)

  return(value) ->
    value
```

Use as a function:

```fx
emit-console(() -> hello())
```

Or with trailing-action sugar:

```fx
with emit-console()
  hello()
```

Parameterized handler factories remain normal functions:

```fx
emit-collect(values: Ref (List a)) : Handler <Emit a> a a
  handler
    emit(value, resume) ->
      values := [value, ...!values]
      resume(unit)

    return(value) ->
      value
```

Strengths:

- Handlers are visibly values.
- Reusable handlers are easy to name and pass around.
- Works well as a desugaring target for inline handler functions.

Weaknesses:

- Requires a `Handler` type constructor or equivalent.
- Handler type signatures can get dense.
- Does not by itself answer how effect rows appear in function types.

## Combined effects and coeffects

If fx adds coeffect implicit parameters, effects and coeffects should be designed together. They are dual-looking requirements:

- Effects are operations a computation may perform outward.
- Coeffects are contextual values a computation requires inward.

Koka's experimental implicit-parameter syntax uses `?name` parameters that can usually be resolved by the compiler and can still be passed explicitly. The important semantic idea for fx is not the exact `?` spelling, but that implicit parameters are still parameters: they have names, types, lexical scope, explicit override syntax, and tooling can show inserted arguments.

### Combined option 1: `with` effects, `given` context

```fx
sort(values: List a) : List a given { compare: (a, a) -> Order }
  ...

read-config() : Config given { env: Env }
  env.config

load-user(id: UserId) : User with <Http, Raise> given { config: Config, clock: Clock }
  ...
```

Open coeffect rows use record-row syntax:

```fx
log-time(message: Text) : Unit with <Console, ...e> given { clock: Clock, ...c }
  print-line(clock.now() ++ " " ++ message)
```

Effect and coeffect polymorphism stay visually parallel:

```fx
map-effect(f: a -> b with <...e> given { ...c }, values: List a) : List b with <...e> given { ...c }
  map(f, values)
```

Context can be supplied by the scoped `given` provider:

```fx
with given { compare = compare-user-name }
  sort(users)
```

Local contextual values use the same provider form:

```fx
main() : IO Unit
  with given { config = read-config-file("app.toml"), clock = system-clock() }
    load-user(UserId(42))
```

Strengths:

- `with <...>` and `given { ... }` read naturally: "returns `A` with effects, given context".
- Coeffects reuse record rows because implicit parameters are named contextual fields.
- Effects reuse variant-like rows because operations are open alternatives handled by handlers.
- Context providers are functions with trailing-action sugar, the same shape as handlers and lifecycle functions.

Weaknesses:

- `with` has two roles if also used for trailing-action sugar.
- `given` is a new keyword.
- Long signatures can become wide.

Effect operations are ordinary lowercase namespace symbols. Operation call sites use normal function-call syntax and normal namespace/type-directed disambiguation.

V0 effect declarations do not distinguish resumable and non-resumable operations syntactically. Handlers decide whether to resume.

V0 handler clauses receive `resume` explicitly. Resumption is never implicit in handler clause return values.

Effect rows list effect names/capabilities only, such as `<Raise>` or `<Emit a>`. Operation signatures live in `effect` declarations, not in effect rows.

`with` is general trailing-action sugar for any function whose final parameter is an action/thunk. The type checker rejects `with` when the target function's final parameter is not an action.

V0 context rows use unique plain field names, aligned with record rows. Functions that need two contextual values with the same conceptual role should give them distinct names, such as `eq-a` and `eq-b`; qualified context names are deferred.

### Combined option 2: one `requires` clause

```fx
load-user(id: UserId) : User
  requires effects <Http, Raise>, context { config: Config, clock: Clock }
  ...
```

or more compact:

```fx
load-user(id: UserId) : User requires <Http, Raise> { config: Config, clock: Clock }
  ...
```

Strengths:

- One place for all non-ordinary requirements.
- Avoids overloading `with`.

Weaknesses:

- Heavier and less expression-like.
- Less consistent with Koka/Unison effect notation.
- The compact form is visually too dense.

### Combined option 3: parameter-list implicits

```fx
sort(values: List a, ?compare: (a, a) -> Order) : List a
  ...

load-user(id: UserId, ?config: Config, ?clock: Clock) : User with <Http, Raise>
  ...
```

Explicit call:

```fx
sort(users, ?compare = compare-user-name)
```

Strengths:

- Closest to Koka's current implicit-parameter experiment.
- Makes "implicit parameters are still parameters" very concrete.
- No separate coeffect row syntax required.

Weaknesses:

- Coeffects are mixed into ordinary value parameters, which weakens the signature-level duality with effects.
- Higher-order function types get harder to scan.
- It is less obvious which requirements are passed by callers versus resolved from context.

### Combined option 4: context row before the arrow

```fx
sort : { compare: (a, a) -> Order } => List a -> List a
load-user : { config: Config, clock: Clock } => UserId -> User with <Http, Raise>
```

Declaration form:

```fx
sort(values: List a) given { compare: (a, a) -> Order } : List a
  ...
```

Strengths:

- Familiar to readers of type-class/context-constraint syntax.
- Keeps context requirements visually before the computation.

Weaknesses:

- Splits declaration syntax from function type syntax unless we redesign headers.
- `=>` introduces another arrow-like form.
- Less aligned with the current `name(params) : Return` shape.

## Recommended combined v0 direction

Use `with <effects>` and `given { context }` after the return type:

Core syntax:

```fx
export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x / y

export type Clock = {
  now: () -> Instant
}

timestamp(message: Text) : Text given { clock: Clock }
  std.instant.to-text(clock.now()) ++ " " ++ message

catch(default: a, action: () -> a with <Raise, ...e> given { ...c }) : a with <...e> given { ...c }
  handle action()
    raise(message, resume) ->
      default

    return(value) ->
      value

main() : IO Unit
  with given { clock = system-clock() }
    value =
      with catch(0)
        safe-divide(10, 0)

    print-line(timestamp(std.int.to-text(value)))
```

Effect rows:

```fx
Pure result:      a
Closed effects:   a with <Console, Raise>
Open effects:     a with <Console, ...e>
No effects:       a with <>
Context:          a given { config: Config, clock: Clock }
Open context:     a given { config: Config, ...c }
Both:             a with <Console, ...e> given { config: Config, ...c }
```

Function types:

```fx
() -> a with <Console>
(a, b) -> c with <Emit c, ...e> given { config: Config, ...c }
```

The pure form omits `with <>` and `given {}`:

```fx
(a, b) -> c
```

## Satisfying `given` requirements

The main design question for coeffects is how a required context row is satisfied at call sites. The most coherent answer is to make `given` itself a context-provider function:

```fx
given(context, action)
```

with trailing-action sugar:

```fx
with given { clock = system-clock() }
  timestamp("started")
```

Desugars to:

```fx
given({ clock: system-clock() }, () -> timestamp("started"))
```

```fx
timestamp(message: Text) : Text given { clock: Clock }
  std.instant.to-text(clock.now()) ++ " " ++ message
```

The source expression:

```fx
timestamp("started")
```

is valid only if the compiler can resolve a contextual value named `clock` with type `Clock`.

### Option 1: scoped `given` provider

Provide contextual values for a lexical action:

```fx
main() : IO Unit
  with given { clock = system-clock(), config = read-config("app.toml") }
    print-line(timestamp("started"))
```

Rules:

- `given { name = expr }` introduces contextual entries for the trailing action.
- Inner `given` providers shadow outer ones by name.
- Context entries are available for implicit resolution and can also be referenced by name inside the action.
- Tooling should show inserted contextual arguments at call sites.

Strengths:

- Very readable.
- Same shape as effect handlers and lifecycle functions.
- Keeps the context supply near the code that needs it.
- Avoids a separate `given name = expr` declaration form.

Weaknesses:

- One-call overrides need a scoped block.
- Nested providers can get visually heavy if overused.

### Option 2: explicit final `given` call argument

Allow a final `given { ... }` argument group in ordinary function calls:

```fx
timestamp("started", given { clock = test-clock })

sort(users, given { compare = compare-user-name })
```

This fits existing call syntax because it is still inside the parenthesized argument list. It is not a second call form.

Rules:

- The `given { ... }` group must be the final argument.
- Entries are named assignments, not positional arguments.
- Explicit entries override lexical contextual lookup for that call.
- Missing contextual entries are still resolved lexically.

Partial override:

```fx
render-page(user, given { locale = Locale("en-US") })
```

If `render-page` requires:

```fx
Html given { theme: Theme, locale: Locale, clock: Clock }
```

then `locale` is supplied explicitly, while `theme` and `clock` are resolved from context.

Strengths:

- Coherent with parenthesized calls.
- Makes one-off overrides explicit.
- Avoids Koka's `?name = value` syntax, which would introduce a second named-argument style.

Weaknesses:

- `given` appears both in signatures and calls.
- The call grammar needs a special final argument form.

### Option 3: scoped provider function

Use ordinary functions to establish context for an action:

```fx
with-clock(clock: Clock, action: () -> a given { clock: Clock, ...c }) : a given { ...c }
  with given { clock = clock }
    action()

main() : IO Unit
  with with-clock(system-clock())
    print-line(timestamp("started"))
```

Desugars to a normal call with an action:

```fx
with-clock(system-clock(), () -> print-line(timestamp("started")))
```

Strengths:

- Same shape as effect handlers.
- Context providers are just functions.
- Works for dynamic/scoped configuration such as clocks, loggers, locales, allocators, tracing spans, and request context.

Weaknesses:

- Requires higher-order function types involving `given`.
- If overused, provider nesting can get visually heavy.

### Option 4: module-level contextual exports

Allow exported values to be considered for implicit resolution after importing a namespace:

```fx
export given clock: Clock = system-clock()

import std.time

timestamp("started")
```

Strengths:

- Convenient for standard defaults.
- Similar to typeclass instance imports or Koka's global qualified implicit candidates.

Weaknesses:

- Risks invisible global behavior.
- Makes imports affect more than name resolution.
- Can create surprising ambiguity across imported namespaces.

Recommendation for v0: do not support module-level `export given` defaults. Prefer scoped `given` providers and named provider functions. Add library-level defaults later only if the resolver and tooling are strong enough.

### Option 5: ordinary named arguments

Use normal named arguments instead of a `given` group:

```fx
timestamp("started", clock = test-clock)
sort(users, compare = compare-user-name)
```

Strengths:

- Minimal syntax.
- Very familiar.

Weaknesses:

- fx does not currently have ordinary named arguments.
- It blurs ordinary optional/named parameters with contextual requirements.
- It makes it harder to see which arguments are part of the stable function arity.

Recommendation for v0: avoid this unless the language also commits to named arguments generally.

### Resolution model

Use name-and-type resolution:

```fx
format(value: a) : Text given { show: a -> Text }
  show(value)
```

At a call to `format(42)`, the compiler looks for a contextual value named `show` whose type can instantiate to `Int -> Text`.

If there is exactly one match, insert it:

```fx
with given { show = int.show }
  format(42)
```

If there are multiple matches, require an explicit scoped provider:

```fx
with given { show = hex.show }
  format(42)
```

If there is no match, report the missing contextual requirement:

```txt
missing given { show: Int -> Text }
```

This mirrors the existing namespace-disambiguation direction: type information can resolve convenience, but explicit qualification or explicit arguments remain the stable core.

### Recommended satisfaction syntax

Use these mechanisms in v0:

1. Scoped `given` providers:

```fx
with given { clock = system-clock() }
  timestamp("started")
```

2. Named provider functions with trailing-action sugar:

```fx
with with-clock(test-clock)
  timestamp("started")
```

Do not include lexical `given name = expr` declarations, final `given` call arguments, module-level implicit defaults, or ordinary named-argument satisfaction in v0.

## Lifecycle, `given`, and multi-shot resumptions

Koka's `initially` and `finally` blocks are valuable research anchors, but fx should not expose `initially` as v0 surface syntax. With `given` and `given each`, the common user-facing setup cases are clearer:

- `given { ... }` for shared contextual values.
- `given each(init)` for fresh contextual values per resumed branch.
- `given each(init, cleanup)` for fresh contextual values with per-branch cleanup.
- `finally(cleanup)` for generic cleanup that is not about contextual values.

`given` installs shared contextual values for an action:

```fx
with given { clock = clock }
  action()
```

`finally` remains a generic lifecycle function:

```fx
with finally(() -> close(file))
  action()
```

Both desugar to ordinary function calls with a trailing action.

The difference between shared and branch-local context matters for multi-shot handlers. If a handler resumes the same continuation more than once, the continuation re-enters the dynamic scope around the operation. A `given` value captured outside the resumption is shared by all resumed branches; a value produced by `given each` is fresh for each branch.

Shared context:

```fx
with given { request = request-context }
  with choose()
    render-page()
```

Every resumed branch sees the same `request`.

For branch-local context, `given` needs a resourceful form. Manual composition with `initially` and `finally` is awkward because the cleanup must see the contextual values created for exactly that branch.

Resourceful branch-local context:

```fx
with given each(
  () -> { trace = start-trace() },
  context -> end-trace(context.trace)
)
  with choose()
    render-page()
```

This means:

```txt
for the original action and each resumed branch:
  create context
  install that context for the branch
  run the branch
  clean up that branch's context when it returns, aborts, or does not resume
```

For non-resourceful branch-local values, cleanup can be omitted:

```fx
with given each(() -> { trace-id = new-trace-id() })
  with choose()
    render-page()
```

The omitted cleanup is equivalent to:

```fx
_ -> unit
```

Recommended v0 rule:

- `given { ... }` installs shared context for the action and all resumptions inside its dynamic extent.
- `given each(init)` creates fresh context for the original action and every resumed branch.
- `given each(init, cleanup)` creates fresh context per branch and cleans it up per branch.
- `given each` initializers are zero-argument in v0. Branch identity is not exposed.
- `finally(cleanup)` is the generic cleanup primitive for non-contextual resources.
- `initially` remains an internal semantic primitive used to explain `given each`, but is not exposed as v0 surface syntax.
- Resourceful `given each(init, cleanup)` is the preferred surface for branch-local contextual resources.

Example provider:

```fx
with-trace(action: () -> a given { trace: Trace, ...c }) : a given { ...c }
  with given each(
    () -> { trace = start-trace() },
    context -> end-trace(context.trace)
  )
    action()
```

This keeps the two common cases visually distinct:

```fx
with given { request = request-context }
  action()

with given each(() -> { trace = start-trace() }, context -> end-trace(context.trace))
  action()
```

Use plain `given` for shared context. Use `given each` for context that must be fresh, finalized, or otherwise branch-local under multi-shot resumptions.

Resourceful provider:

```fx
with-config(path: Path, action: () -> a given { config: Config, ...c }) : a with <File, ...e> given { ...c }
  config = read-config(path)

  with finally(() -> config.close())
    with given { config = config }
      action()
```

Branch-local resourceful provider:

```fx
with-trace(action: () -> a given { trace: Trace, ...c }) : a given { ...c }
  with given each(
    () -> { trace = start-trace() },
    context -> end-trace(context.trace)
  )
    action()
```

V0 lifecycle surface:

```fx
with finally(cleanup)
  body

with given { ... }
  body

with given each(init)
  body

with given each(init, cleanup)
  body
```

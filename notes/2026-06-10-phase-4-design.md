# Phase 4 named scopes v0 design

Date: 2026-06-10

## Purpose

This note freezes a v0 implementation target for Phase 4 named scopes. It is
based on:

- `notes/2026-05-23-implementation-phases.md`;
- `notes/2026-05-23-named-scopes-v0.md`;
- `notes/2026-06-10-phase-4-research.md`;
- Phase 3's settled `with <...>` effect-row syntax.

## Scope

Phase 4 implements named, statically declared scope identities for scoped
effect families.

Include:

- `scoped effect` declarations;
- `scope` declarations;
- one optional single-entry protocol row per `scope`;
- scoped effect-row members;
- scope-polymorphic effect-row members;
- explicit scoped operation calls;
- `within` default-scope elision;
- handler typing that removes only one scoped family/scope instance;
- parser, checker, LSP-adjacent metadata, and compile-time runtime gating.

Defer:

- fresh scope creation;
- scope masking;
- first-class handler names;
- non-escape checking;
- multiple protocol-row entries on one scope;
- coeffects and `given`;
- JavaScript execution for scoped effects unless Phase 4 explicitly grows a
  runtime slice.

## Surface syntax

Scoped effect declarations:

```fx
export scoped effect State s a
  get() : a
  put(value: a) : Unit
```

The first parameter of a scoped effect declaration is the scope parameter. It
has scope kind. Remaining parameters are ordinary type parameters unless a
later phase adds explicit kind annotations.

Scope declarations:

```fx
scope Counter with <State Int>
scope Session
```

`scope Counter with <State Int>` means:

```text
Counter : Scope
State Counter Int : Effect
```

The protocol row omits the scope parameter because the declared scope supplies
it. In v0 the row contains at most one entry.

Effect rows:

```fx
Int with <State Counter Int>
Int with <State s Int, ...e>
() -> Int with <State Counter Int, ...e>
```

Explicit scoped operation calls:

```fx
get(Counter)
put(Counter, n + 1)
```

Default scope elision:

```fx
within Counter
  n = get()
  put(n + 1)
  n
```

This elaborates as:

```fx
n = get(Counter)
put(Counter, n + 1)
n
```

## Internal model

Add an internal scope kind:

```text
Kind::Scope
```

A scoped effect family has a scope parameter plus ordinary type parameters:

```text
State : Scope -> Type -> Effect
```

A concrete scoped effect row member is structured:

```text
ScopedEffectMember {
  family: EffectId(State),
  scope: ScopeRef(Counter),
  args: [Type::Int],
}
```

A scope-polymorphic member uses a scoped variable:

```text
ScopedEffectMember {
  family: EffectId(State),
  scope: ScopeRef::Var(s),
  args: [Type::Int],
}
```

Do not encode scoped members as formatted strings. The checker needs the
separate fields for row unification, handler removal, hovers, diagnostics, and
future fresh-scope features.

## Name resolution

`scoped effect State s a` introduces:

- an effect-family symbol `State`;
- operation symbols `State.get` and `State.put`;
- operation signatures whose first implicit protocol parameter is a scope.

`scope Counter with <State Int>` introduces:

- a scope symbol `Counter`;
- a protocol-row entry tying `Counter` to `State Counter Int`.

An explicit scoped call such as `get(Counter)` resolves in this order:

1. Resolve `Counter` as a scope identity.
2. Find scoped operations named `get`.
3. Keep candidates whose family can be applied to `Counter`.
4. Use expected effect row and protocol row to select a unique family
   and ordinary type arguments.
5. Report ambiguity if more than one candidate remains.

An elided scoped call such as `get()` inside `within Counter` resolves in
this order:

1. Collect active default scopes from nested `within` expressions.
2. Find scoped operations named `get`.
3. Keep candidates whose family and arguments are compatible with an active
   scope's protocol row or the expected effect row.
4. Select only if exactly one family and one active scope remain.
5. Elaborate to the explicit scoped call.

Do not use return type alone to make an otherwise ambiguous elided call legal.
Ambiguity should be resolved by the scoped effect family plus active scope.

## Type checking

### Scoped effect declarations

Check that:

- the first parameter is reserved as the scope parameter;
- operation signatures do not mention the scope parameter as an ordinary value;
- operation result types are well formed;
- operation declarations have unique names within the scoped effect family.

### Scope declarations

Check that:

- the scope name is unique in its module namespace;
- each protocol-row entry names a scoped effect family;
- each entry supplies all non-scope type arguments;
- at most one protocol-row entry appears in v0.

### Effect rows

Check that:

- `State Counter Int` names a scoped effect family;
- `Counter` resolves to a scope identity;
- `Int` satisfies the family argument kind;
- `State s Int` introduces or references a scope variable `s`;
- duplicate exact members collapse according to existing row behavior;
- distinct scopes remain distinct members.

Examples:

```text
<State Counter Int, State Session Int>
```

contains two different requirements.

```text
<State Counter Int, State Counter Text>
```

contains two different requirements because the protocol arguments differ.

### Operation calls

An explicit call:

```fx
get(Counter)
```

performs `State Counter Int` when the chosen operation is
`State.get : () -> Int`.

An elided call:

```fx
within Counter
  get()
```

performs the same effect after elaboration.

Elaboration should produce typed spans that still point to the source call, but
carry the resolved family and scope for hover and go-to-definition.

### Handlers

A scoped handler removes exactly one scoped row member:

```fx
with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
```

Inside the handler body:

- clauses are matched by operation within the `State` family;
- the handler's scope parameter determines which row member is removed;
- requests for `State other a` are forwarded like any unhandled effect;
- clause effects are added to the resulting row using Phase 3 handler rules.

Concrete specialization:

```fx
with with-state(Counter, 0)
  get(Other)
```

must still require `State Other Int`.

## LSP behavior

Required typed metadata:

- `scoped effect` declarations appear in document and workspace symbols;
- scoped operations appear as operation symbols under their family;
- `scope` declarations appear as scope symbols;
- hover for `Counter` shows `scope Counter with <State Int>`;
- hover for `State Counter Int` shows it as a scoped effect row member;
- hover for `get()` inside `within Counter` shows the elaborated scope;
- go-to-definition from scope use goes to the `scope` declaration;
- go-to-definition from scoped operation use goes to the operation declaration.

Completion can remain index-based initially, but operation completion inside
`within Counter` should later prefer operations supported by `Counter`.

## Diagnostics

Use scoped-effect-specific diagnostics instead of generic type mismatch text
where possible.

Required cases:

- unknown scope:
  `unknown scope 'Counter'`
- protocol row names an ordinary effect:
  `scope protocol row must name a scoped effect family`
- missing protocol arguments:
  `scope 'Counter' protocol row for 'State' expects 1 type argument`
- scoped row with value name:
  `expected a scope in 'State Counter Int'`
- wrong scope:
  `expected 'State Counter Int' but found 'State Session Int'`
- missing effect family:
  `unhandled scoped effect 'State Counter Int'`
- ambiguous elided operation:
  `operation 'get' is ambiguous for active scopes Counter and Session`
- unsupported runtime:
  `unsupported scoped-effect runtime`

Diagnostics should distinguish:

- missing effect family;
- known family with missing scope;
- known family and wrong scope;
- ambiguous operation name;
- ambiguous default scope.

## Compiler behavior

Phase 4 checking should succeed for accepted scoped-effect examples.

Unless JavaScript execution is explicitly added in the same phase,
`fx compile --target js` should reject scoped-effect programs with a compiler
diagnostic distinct from parser and checker diagnostics:

```text
unsupported scoped-effect runtime
```

Pure programs and Phase 1 through Phase 3.1 executable examples must continue
to compile and run as before.

## Acceptance examples

The files in `examples/phase-4/` are the goal target:

- `state.fx` demonstrates concrete scope rows, explicit calls, elision through
  `within`, and exact scoped handler removal.
- `polymorphic.fx` demonstrates helpers polymorphic in a scope variable.
- `diagnostics.fx` documents rejected cases and expected diagnostics.

Before implementation they may fail to parse or check. After implementation,
the accepted examples should parse and check, and the diagnostic examples
should become negative fixtures with stable messages.

## Later design hooks

The internal representation should keep enough structure for:

- fresh generated scope identities;
- non-escape checking;
- masking;
- multiple protocols per scope;
- scoped operations that take subcomputations;
- capability-style interpretation of scope availability.

Do not expose those features in Phase 4 syntax.

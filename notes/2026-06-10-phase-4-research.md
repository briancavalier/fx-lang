# Phase 4 scoped effects research

Date: 2026-06-10

## Purpose

This note records the research needed before implementing Phase 4 of
`notes/2026-05-23-implementation-phases.md`.

Phase 4 should add named scopes, scoped effect families, and
scope-polymorphic effect rows. It should not add fresh scope creation, scope
masking, first-class handler names, non-escape checking, or coeffects.

## Existing repo context

The phase plan says Phase 4 includes:

- `scoped effect Effect s ...` declarations;
- `scope Name` declarations and optional protocol rows;
- scope identities as type-level values in effect rows;
- concrete rows such as `a with <State Counter Int>`;
- scope-polymorphic rows such as `a with <State s Int, ...e>`;
- explicit scoped operation calls first;
- `within Scope` as a default-scope provider for operation elision;
- handler typing that removes only `Effect Scope`;
- diagnostics for unknown scopes, invalid scope protocol rows, ambiguous
  operations, and scope mismatches.

`notes/2026-05-23-named-scopes-v0.md` already selected scoped effect
declarations as the v0 direction. Treat that note as historical design input,
but update examples and rules to the Phase 3 `with <...>` effect-row spelling
rather than the older `! <...>` spelling.

## Research sources

### Formal scoped and higher-order effects

- "A Calculus for Scoped Effects & Handlers"
  <https://arxiv.org/abs/2304.09697>
- "Structured Handling of Scoped Effects"
  <https://arxiv.org/abs/2201.10287>
- "Scoped Effects as Parameterized Algebraic Theories"
  <https://arxiv.org/abs/2402.03103>
- "A Framework for Higher-Order Effects & Handlers"
  <https://arxiv.org/abs/2302.01415>

Use these papers to understand the semantic pressure that appears when an
operation contains or delimits a subcomputation. The useful lesson for Phase 4
is mostly negative: do not accidentally implement full scoped higher-order
operations while trying to add named instances of ordinary operations.

Phase 4's `State Counter Int` can be implemented as an indexed ordinary effect
requirement. Later features such as bracketing, transactions, fresh resources,
and non-escape checking should be informed by the formal scoped-effect work,
but they are not required for the v0 named-scope slice.

### Row-polymorphic effect typing

- "Koka: Programming with Row Polymorphic Effect Types"
  <https://arxiv.org/abs/1406.2061>
- Koka book, especially effect typing, handlers, masking, and named/scoped
  handler sections: <https://koka-lang.github.io/koka/doc/book.html>

Koka is the closest practical reference because it combines row-polymorphic
effects, handlers, masking, state isolation, and named/scoped handler work. It
also demonstrates that implementation and tooling details matter as much as the
surface notation: hovers, inferred effects, and unique resolution are part of
the user model.

Phase 4 should borrow the row-polymorphic lesson, not the whole Koka surface.
The v0 design should keep `scope` names explicit and avoid masking until there
is a separate design for it.

### Capabilities versus rows

- "Rows and Capabilities as Modal Effects"
  <https://arxiv.org/abs/2507.10301>
- Effekt language overview: <https://effekt-lang.org/>
- Effekt docs: <https://effekt-lang.org/docs/>

Named scopes are capability-like: a scope name is an authority to perform a
particular protocol's operations. Effekt's contextual effect polymorphism is a
useful contrast because it makes effects available from lexical context rather
than forcing every higher-order type to name an explicit row variable.

For fxl, keep Phase 4 row-based because Phase 2 and Phase 3 already built the
language around rows. Internally, however, scope identities should be modeled
as a distinct kind of type-level value so a later capability interpretation can
reuse the same facts instead of replacing the checker.

### Abilities and practical diagnostics

- Unison abilities and ability handlers:
  <https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/>
- Flix effects overview: <https://doc.flix.dev/effects.html>

Unison is useful for user-facing ability availability rules: a call is legal
when the surrounding context provides at least the required abilities. Flix is
useful for a simpler user-facing type/effect story, especially around
higher-order effect polymorphism, editor integration, and precise diagnostics.

Neither language directly settles named scoped rows for fxl. The design lesson
is that Phase 4 diagnostics should explain availability, not unifier internals.

### First-class names and local state isolation

- "First-Class Named Effect Handlers"
  <https://xnning.github.io/papers/oopsla22namedh.pdf>
- Haskell `runST` style state isolation, as a comparison point for fresh local
  scopes and non-escape checking.

These sources are important for later fresh scopes, but they should constrain
Phase 4 only by keeping v0 honest: static named scopes are not fresh scopes.
They distinguish instances, but they do not allocate new hidden identities and
therefore do not require non-escape checking yet.

## Design conclusions for Phase 4

### Treat named scopes as indexed effect rows

Represent a scoped effect requirement as one row member with structure:

```text
family = State
scope = Counter or scope variable s
arguments = [Int]
```

Do not flatten `State Counter Int` into a generated label string. The checker
needs structured access to the effect family, scope identity, and protocol
arguments for diagnostics, hovers, handler removal, and future fresh scopes.

### Give scopes a distinct kind

Scope names and scope variables should not be ordinary value variables.
Introduce a scope kind or an equivalent internal distinction:

```text
Counter : Scope
s : Scope
State : Scope -> Type -> Effect
```

User syntax can stay lightweight, but internal scoping should not conflate:

- type variables such as `a`;
- row variables such as `...e`;
- scope variables such as `s`;
- value variables such as `scope`.

### Keep `within` as elaboration

`within Counter` should create a lexical default for resolving scoped
operation calls. The checker should elaborate an elided call such as `get()`
to an explicit operation request for `State Counter Int` before later phases
lower to JavaScript.

This gives LSP and diagnostics a concrete target:

- go-to-definition from `Counter` goes to the `scope` declaration;
- go-to-definition from `get()` goes to the scoped operation declaration;
- hover can show both the operation type and the chosen scope;
- ambiguous elision can point to the active competing scopes.

### Keep explicit scoped calls available

The phase plan says explicit scoped operation calls come first. The surface
needs one canonical explicit form. Use a function-call-like qualified form in
the design note and examples:

```fx
get(Counter)
put(Counter, n + 1)
```

`within Counter` elides that first scope argument only after operation
resolution chooses exactly one scoped family and active scope.

### Handler removal is exact

A handler for `State Counter Int` removes only that row member. It does not
remove `State Other Int`, `State Counter Text`, or `Console`.

This should be represented as row subtraction by structured scoped member, not
by operation name.

### Fresh scopes remain out of scope

Do not add:

- `new-scope`;
- locally generated scope identities;
- hidden scope binders;
- rank-2 scope polymorphism;
- non-escape checking;
- scope masking.

Those features need the first-class-names and scoped-effect literature. Phase 4
should leave explicit syntax and internal representation hooks for them without
building them.

## Open questions to settle before coding

1. Should a `scope Name` without a protocol row be legal only for explicit
   rows, or can it be used by any scoped effect family?
2. Should a `scope Name with <State Int>` protocol row mean exactly one
   supported protocol, or can later syntax support multiple entries?
3. Should the explicit scoped operation form be `get(Counter)`,
   `Counter.get()`, or `get in Counter`?
4. Are scope variables introduced implicitly by scoped-row use, matching Phase
   2 row variables, or does Phase 4 need explicit generic binders?
5. When two active scopes support the same operation name but different result
   types, should expected type participate in disambiguation or should the
   call remain ambiguous?

## Recommended answer to open questions

For v0:

1. Allow bare `scope Name`, but require enough expected effect-row
   context for operation elision. A protocol row is required when no
   other context can determine the family and arguments.
2. Support at most one protocol-row entry per `scope` declaration.
3. Use `get(Counter)` and `put(Counter, value)` as the explicit form.
4. Introduce scope variables implicitly from scoped effect rows in function
   annotations and scoped rows, but track them with a distinct scope-variable
   kind internally.
5. Do not use expected result type as the primary disambiguator for elided
   operations. Require the scoped effect family and active scope to be
   unambiguous, then check the result type.

These answers keep Phase 4 small, preserve the function-call operation model,
and avoid committing to later masking or fresh-scope semantics prematurely.

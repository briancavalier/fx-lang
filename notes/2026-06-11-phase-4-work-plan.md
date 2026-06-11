# Phase 4 work plan

Date: 2026-06-11

## Purpose

This note breaks Phase 4 from
`notes/2026-05-23-implementation-phases.md` into implementation units. Phase 4
adds named scopes, scoped effect families, scope-polymorphic effect rows,
explicit scoped operation calls, `within` operation elision, and exact scoped
handler removal after ordinary Phase 3 effects and handlers are stable.

The implementation target is the v0 design in
`notes/2026-06-10-phase-4-design.md`. The research context is in
`notes/2026-06-10-phase-4-research.md`.

## Scope

Include:

- `scoped effect Effect s ...` declarations;
- `scope Name` declarations;
- `scope Name with <ProtocolShape>` single-entry protocol rows;
- scoped effect rows such as `a with <State Counter Int>`;
- scope-polymorphic rows such as `a with <State s Int, ...e>`;
- explicit scoped operation calls such as `get(Counter)`;
- `within Counter` operation elision;
- handler typing that removes only `Effect Scope`;
- parser, checker, diagnostics, LSP-adjacent metadata, CLI check support, and
  compile-time runtime gating.

Defer:

- fresh scope creation;
- scope masking;
- first-class handler names;
- non-escape checking for locally generated scopes;
- multiple protocol-row entries per scope;
- coeffects and `given`;
- JavaScript execution for scoped effects unless a later Phase 4.1 slice
  explicitly adds it.

## Surface rules

Scoped effect declarations reserve their first parameter as the scope
parameter:

```fx
export scoped effect State s a
  get() : a
  put(value: a) : Unit
```

Scope protocol rows use `with <...>` and omit the scope parameter because the
declared scope supplies it:

```fx
scope Counter with <State Int>
```

The corresponding effect-row member includes the scope:

```fx
Int with <State Counter Int>
```

Explicit scoped operation calls pass the scope first:

```fx
get(Counter)
put(Counter, n + 1)
```

`within` establishes a lexical default scope for elided scoped operations:

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

## Work units

### 1. Phase 4 setup and goal fixtures

Confirm `examples/phase-4/` covers concrete named scopes,
scope-polymorphic helpers, `within` elision, and diagnostic boundaries. Create
`notes/2026-06-11-phase-4-learnings.md` using the established learning log
shape:

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

Acceptance:

- `examples/phase-4/state.fx`, `examples/phase-4/polymorphic.fx`, and
  `examples/phase-4/diagnostics.fx` use the current `scope Name with <...>`
  and `within Scope` syntax.
- The learning log exists before syntax/checker decisions start accumulating.
- Boundary examples avoid fresh scopes, scope masking, multiple protocols,
  coeffects, and JavaScript runtime expectations.

### 2. Parse `scoped effect` declarations

Extend the parser and AST for top-level `scoped effect` declarations.

Parser behavior:

- parse `export scoped effect State s a`;
- require at least one scoped effect parameter, reserving the first as the
  scope parameter;
- parse indented operation signatures using the existing operation-signature
  syntax from Phase 3;
- preserve spans for the `scoped` keyword, effect name, scope parameter,
  ordinary parameters, operation names, parameters, and result types;
- recover cleanly from missing scope parameters and malformed operation
  signatures.

Acceptance:

- Parser tests cover exported and non-exported scoped effects, parameterized
  scoped effects, multiple operations, and recovery diagnostics.
- Ordinary `effect` declarations continue to parse unchanged.
- The AST distinguishes ordinary effects from scoped effect families without
  requiring checker-side reconstruction.

### 3. Parse `scope` declarations and protocol rows

Extend top-level syntax for scope declarations:

```fx
scope Counter
scope Counter with <State Int>
```

Parser behavior:

- parse bare scope declarations;
- parse `with <...>` protocol rows on scope declarations;
- enforce the v0 single-entry protocol-row shape syntactically if practical,
  or recover and let the checker report multiple entries;
- preserve spans for the scope name, `with`, row delimiters, protocol family,
  arguments, and row entries.

Acceptance:

- Parser tests cover bare scopes, protocol-row scopes, malformed rows,
  multiple entries if parsed for checker diagnostics, and duplicate-looking
  declarations.
- Existing type-level `with <effects>` and expression-level trailing `with`
  continue to parse unchanged.
- `scope Counter : State Int` is rejected or recovered with a diagnostic that
  points toward `scope Counter with <State Int>`.

### 4. Parse scoped effect-row members

Extend effect-row type syntax to accept scoped members such as:

```fx
State Counter Int
State s Int
```

Parser behavior:

- preserve each effect row member as a family name plus ordered arguments;
- do not decide in the parser whether the first argument is a scope or a type;
- keep row-tail behavior from Phase 3 unchanged;
- reject or recover from malformed scoped members with useful spans.

Acceptance:

- Parser tests cover concrete scopes, scope variables, open rows, mixed
  ordinary and scoped entries if the syntax permits both, and malformed
  arguments.
- Display and formatter-facing syntax can reconstruct `with <State Counter
  Int, ...e>`.
- Ordinary Phase 3 effect rows such as `with <Raise, ...e>` remain valid.

### 5. Add scope and scoped-effect symbols

Extend the module symbol model for:

- scope symbols;
- scoped effect family symbols;
- scoped operation symbols.

Acceptance:

- Duplicate scope names produce module-owned diagnostics.
- Duplicate scoped effect names and duplicate operation names within a scoped
  effect family produce diagnostics.
- Exported scoped effects and scopes appear in module exports.
- Imports and qualified names can address scoped effect families, scoped
  operations, and scopes.
- Go-to-definition has stable declaration targets for scopes and scoped
  operations.

### 6. Add internal scope kind and scoped effect members

Extend checker types and row entries with a distinct scope identity/kind.

Expected model:

- scope constants for declared scopes;
- scope variables for scope-polymorphic annotations;
- structured scoped effect-row members with family, scope, and ordinary
  arguments;
- distinct row kind behavior so scoped effect rows remain effect rows, not
  record, variant, tuple, or future context rows.

Acceptance:

- `State Counter Int` is represented structurally, not as a formatted string.
- Scope variables do not collide with ordinary type variables or effect-row
  variables.
- Row display prints compact scoped members such as `with <State Counter Int>`.
- Existing Phase 3 effect-row display and unification keep working.

### 7. Lower `scoped effect` and `scope` declarations

Lower parsed declarations into checker metadata.

Checker behavior:

- `scoped effect State s a` records `State : Scope -> Type -> Effect`;
- operations in a scoped effect family carry an implicit first scope slot for
  explicit scoped operation calls;
- `scope Counter` records a scope identity without protocol metadata;
- `scope Counter with <State Int>` records a single protocol-row entry that
  elaborates to `State Counter Int`;
- invalid protocol rows name scoped-effect-specific diagnostics.

Acceptance:

- `scope Counter with <State Int>` checks when `State` is a scoped effect
  family requiring one ordinary type argument.
- `scope Bad with <Console>` reports that the protocol row must name a scoped
  effect family.
- Missing or extra protocol arguments are reported at the protocol row.
- Multiple protocol-row entries are rejected in v0 with a diagnostic that
  leaves the future extension open.

### 8. Check scoped effect rows and scope polymorphism

Teach effect-row lowering and compatibility to understand scoped members.

Acceptance:

- Concrete rows such as `Int with <State Counter Int>` check.
- Scope-polymorphic rows such as `Int with <State s Int, ...e>` check.
- `State Counter Int` and `State OtherCounter Int` remain distinct members.
- `State Counter Int` and `State Counter Text` remain distinct members.
- Closed rows reject extra scoped effects, and open rows bind tails without
  conflating scope variables with row variables.
- Diagnostics distinguish missing effect family, unknown scope, wrong scope,
  and wrong protocol arguments.

### 9. Resolve explicit scoped operation calls

Extend operation resolution for explicit scoped calls:

```fx
get(Counter)
put(Counter, n + 1)
```

Resolution behavior:

- identify the first argument as a scope candidate only for scoped operation
  candidates;
- resolve operation name and scope identity together;
- use the expected effect row and any scope protocol row to select the family
  and ordinary type arguments;
- report ambiguity when more than one scoped operation candidate remains.

Acceptance:

- `get(Counter)` performs `State Counter Int` when `Counter` supports
  `<State Int>` or the expected row requires `State Counter Int`.
- A bare `scope Counter` can still work for explicit calls when expected
  effect-row context supplies the family and arguments.
- Unknown scope and unknown operation diagnostics are distinct.
- Go-to-definition from the operation name reaches the scoped operation
  declaration, and from the scope argument reaches the scope declaration.

### 10. Parse and check `within`

Add expression/block syntax for:

```fx
within Counter
  body
```

Checker behavior:

- resolve `Counter` as a scope identity;
- add it to a lexical default-scope stack while checking the body;
- allow nested `within` blocks;
- preserve spans for the keyword and scope for diagnostics and hover.

Acceptance:

- `within Counter` checks when `Counter` is a known scope.
- Unknown scopes report at the `within` scope use.
- Nested `within` blocks do not affect code outside their body.
- Pure bodies without scoped operation elision behave like the body expression.

### 11. Elaborate elided scoped operations inside `within`

Use active default scopes to resolve operation calls that omit the explicit
scope argument:

```fx
within Counter
  get()
```

Elaboration behavior:

- consider only active `within` scopes;
- use scope protocol rows and expected effect rows to choose a scoped effect
  family and ordinary type arguments;
- require the scoped effect family and active scope to be unambiguous;
- elaborate the call to the same internal operation request as
  `get(Counter)`.

Acceptance:

- `within Counter { get(); put(...) }` checks against
  `with <State Counter Int>`.
- If two active scopes can satisfy the same elided operation, report an
  ambiguity diagnostic naming the scopes.
- If no active scope can satisfy the operation, report a missing scoped effect
  or missing default scope diagnostic as appropriate.
- Hover on an elided operation shows the resolved scope and operation type.

### 12. Extend handler checking for scoped effects

Teach handler checking and row subtraction to remove exactly one scoped effect
member.

Acceptance:

- A handler typed with `State s a` removes only `State s a`.
- Handling `State Counter Int` leaves `State OtherCounter Int` unhandled.
- Handler clauses for scoped effects reuse Phase 3 payload, resume, return,
  and clause-effect rules.
- Clause effects are added to the resulting row without losing unhandled
  scoped effects from other scopes.
- Diagnostics point at the handler/action boundary when the handled scope does
  not match the action's required scope.

### 13. LSP metadata and editor behavior

Wire scoped effect metadata into the analysis output consumed by the language
server.

Acceptance:

- Document and workspace symbols include scopes, scoped effect families, and
  scoped operations.
- Hover on a scope declaration shows `scope Counter with <State Int>`.
- Hover on a scoped row member shows `State Counter Int`.
- Hover on an elided operation inside `within Counter` shows the elaborated
  explicit scope.
- Go-to-definition works from scope declarations, scope uses, explicit scoped
  calls, and elided scoped calls.
- Completion can remain index-based initially, but operation candidates from
  scoped effect declarations are available.

### 14. Compiler runtime gating

Preserve `fx check` support while keeping JavaScript execution out of Phase 4
unless explicitly added.

Acceptance:

- `fx compile --target js` rejects checked scoped-effect constructs with a
  compiler diagnostic such as `unsupported scoped-effect runtime`.
- The diagnostic is distinct from parser and checker diagnostics and points at
  the scoped construct preventing emission.
- Pure Phase 1 through Phase 2.6 examples and Phase 3.1 supported ordinary
  effect examples keep compiling and running.

### 15. Promote examples and finish validation

Promote `examples/phase-4/` from goal targets into test coverage as support
lands.

Acceptance:

- `examples/phase-4/state.fx` and `examples/phase-4/polymorphic.fx` parse and
  check successfully.
- `examples/phase-4/diagnostics.fx` is split into focused negative fixtures
  with stable messages.
- The final sweep includes formatting, parser tests, checker tests, CLI checks
  for examples, LSP-adjacent tests, and compiler runtime-gating tests.
- `notes/2026-06-11-phase-4-learnings.md` records settled decisions,
  implementation surprises, rejected approaches, and validation evidence.

## Suggested implementation order

1. Setup and learning log.
2. Parser slices: `scoped effect`, `scope ... with <...>`, scoped row members,
   and `within`.
3. Symbol/index slices for scopes and scoped effect families.
4. Checker representation: scope kind, scoped members, declaration lowering.
5. Effect-row checking and explicit scoped operation calls.
6. `within` checking and elided operation elaboration.
7. Scoped handler row subtraction.
8. LSP metadata and diagnostics.
9. Compiler runtime gating.
10. Example promotion and final validation.

Keep each slice narrow enough that examples, diagnostics, and learnings can be
updated immediately after the behavior lands.

## Completion status

Completed on 2026-06-11 for the v0 non-runtime scope:

- syntax, formatter, parser recovery, and syntax fixtures for `scoped effect`,
  `scope Name with <...>`, scoped rows, and `within`;
- module symbols, resolution, typed spans, and LSP-adjacent symbol kind mapping
  for scopes and scoped operations;
- checker support for explicit scoped calls, `within` elision, scope-polymorphic
  rows, protocol-row validation, ambiguity diagnostics, and scoped handlers
  that remove only the handled scope family member;
- positive examples in `examples/phase-4/state.fx` and
  `examples/phase-4/polymorphic.fx`;
- negative diagnostics in `examples/phase-4/diagnostics.fx`;
- compiler gating for accepted scoped-effect programs.

Still deferred by design:

- JavaScript execution for scoped effects;
- fresh scope creation;
- scope masking;
- first-class handler names;
- multiple protocol-row entries per scope;
- coeffects and `given`.

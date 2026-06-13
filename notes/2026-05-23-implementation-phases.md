# Implementation phases

Date: 2026-05-23

## Purpose

This plan breaks the current language design into implementation phases. The
goal is to reach a useful type checker and language server incrementally,
without forcing the parser, row system, effects, coeffects, and named scopes to
land all at once.

Each phase should leave the language in a coherent state with runnable examples,
diagnostics, hover, and go-to-definition for the features implemented so far.

## Sequencing rationale

Start with the ordinary language core, then add the shared row machinery, then
effects, then named scopes, then coeffects.

Rows come before effects because records, variants, effect rows, and context
rows all depend on the same underlying row-kinding and row-unification ideas.

Named scopes come before coeffects because they are an extension of the effect
system itself: scoped effect families are still effect requirements. Coeffects
add a second requirement system and should wait until ordinary effects and
scoped effects are stable.

## Phase 1: Basic language core

Goal: working parser, type checker, and language server for the non-effect
language.

This phase should establish the concrete source grammar, module graph, symbol
index, core AST, and annotation-driven type checker.

Include:

- source files as modules;
- imports and inline exports;
- top-level value and function declarations;
- local immutable bindings;
- lambdas and function calls;
- `if` expressions;
- `match` expressions;
- primitive types: `Unit`, `Bool`, `Int`, and `Text`;
- list values and `List a`;
- records, record field access, and record update;
- closed variant types and constructors;
- basic pattern checking for literals, variables, lists, records, variants, and
  tuples if tuples are included in the initial grammar;
- explicit top-level type annotations;
- local type inference inside annotated declarations;
- a small prelude with the operators and functions needed by examples.

Defer:

- open record rows;
- open variant rows;
- variadic tuple packs;
- algebraic effects;
- handlers;
- coeffects and `given`;
- named scopes;
- refs and mutation unless needed for a minimal example.

Type checker success criteria:

- rejects unknown names, duplicate definitions, unresolved imports, arity
  mismatches, and ordinary type mismatches;
- checks top-level declarations against their annotations;
- infers local binding types where enough information is available;
- resolves imported names and fully qualified names;
- produces stable types for hovers.

Language server success criteria:

- parse diagnostics with useful spans;
- type diagnostics with useful spans;
- hover for local bindings, top-level declarations, types, constructors, and
  imported symbols;
- go-to-definition for local names, imported names, constructors, and types;
- document symbols and workspace symbols for modules.

Example target:

```fx
import std.io

export type Todo = {
  id: Int,
  title: Text,
  done: Bool
}

export type TodoResult a = < Ok(a), NotFound(Int) >

summarize(todo: Todo) : Text
  status =
    if todo.done then "done" else "open"

  todo.title ++ " [" ++ status ++ "]"

find(wanted: Int, todos: List Todo) : TodoResult Todo
  match todos
    [] ->
      NotFound(wanted)

    [todo, ...rest] ->
      if todo.id == wanted then Ok(todo) else find(wanted, rest)
```

## Phase 2: Rows

Goal: add the shared row machinery before effects depend on it.

This phase should make records and variants meaningfully row-polymorphic while
keeping effects out of scope.

Include:

- distinct row kinds for records, variants, and tuples if tuple rows are kept in
  v0;
- open record types such as `{ name: Text, ...r }`;
- open variant types such as `< Ok(a), Error(Text), ...r >`;
- final-only row spreads;
- row variable scoping and generalization;
- row unification;
- record field access against open rows;
- record update against open rows;
- variant construction and matching against open rows;
- diagnostics for missing fields, unknown fields, unknown cases, duplicate row
  labels, and incompatible row kinds.

Defer:

- effect rows;
- context rows;
- variadic tuple packs if they complicate row unification too much;
- advanced exhaustiveness and redundancy checking beyond a conservative first
  pass.

Type checker success criteria:

- checks functions polymorphic over record tails;
- checks functions polymorphic over variant tails;
- prevents record rows, variant rows, tuple rows, effect rows, and context rows
  from being accidentally unified across kinds;
- reports row errors in terms users can understand.

Language server success criteria:

- hover shows compact open-row types;
- diagnostics identify the specific missing or incompatible row member;
- go-to-definition works for variant constructors across modules.

Example target:

```fx
greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

message(result: < Ok(a), Missing(), Error(Text), ...r >) : Text
  match result
    Ok(_) -> "ok"
    Missing() -> "missing"
    Error(text) -> text
    other -> "unknown"
```

## Phase 2.5: Executable core and JavaScript backend spike

Goal: make the accepted pure language runnable before effects introduce
handler/runtime design pressure.

This phase should add a narrow executable compiler path for the Phase 1 and
Phase 2 language. It should establish a desugared core IR and a first backend
target without trying to make the backend production-ready.

Target JavaScript first. JavaScript gives the shortest path to runnable output
for records, variants, lists, closures, recursion, and modules, and it leaves
room for later effect-handler experiments in direct style, CPS, generators, or
a small runtime protocol. Native targets such as C should wait until memory
management, closure representation, string/list layout, tail calls, and runtime
ownership decisions are better understood.

Include:

- a desugared core IR for the accepted pure language;
- lowering from checked modules into that core IR;
- JavaScript code generation for Phase 1 and Phase 2 examples;
- a small JavaScript runtime for variant values, pattern matching, list helpers,
  and other required primitives;
- a CLI compile command such as `fx compile --target js`;
- runnable smoke tests that execute generated JavaScript with Node;
- example artifacts under `examples/phase-2.5/` or compact acceptance programs
  that exercise pure runtime behavior.

Defer:

- effect declarations, effect rows, handlers, and resumptions;
- scoped effects and coeffects;
- production packaging, bundling, source maps, and optimization;
- C or native code generation;
- full standard-library design;
- advanced tail-call implementation beyond documenting the current runtime
  behavior and preserving tail-position information where practical.

Compiler success criteria:

- compiles accepted Phase 1 and Phase 2 examples to JavaScript;
- generated JavaScript runs under Node and produces expected observable values;
- record field access, record update, variants, match expressions, lists,
  functions, lambdas, and imports/modules behave consistently with checker
  semantics;
- compiler diagnostics preserve the existing parser/checker failure behavior
  before code generation starts.

Tooling success criteria:

- `fx check` remains the validation path for diagnostics and LSP behavior;
- `fx compile --target js` rejects programs with diagnostics before emitting
  code;
- generated output tests are small, deterministic, and separate from parser,
  checker, and LSP tests.

Example target:

```fx
export type Result a = < Ok(a), Missing(), Error(Text) >

greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

message(result: Result Text) : Text
  match result
    Ok(text) -> greet({ name: text, done: false })
    Missing() -> "missing"
    Error(text) -> text

main : Text
  message(Ok("Grace"))
```

## Phase 2.6: Data-last pipeline and prefix `with`

Goal: add small pure surface conveniences before effects.

This phase introduces no effect semantics. It only adds syntax that desugars to
existing calls and lambdas so Phase 3 examples can stay readable without adding
new runtime concepts.

Include:

- data-last pipeline calls, such as `value |> f(arg)`, desugaring to
  `f(arg, value)`;
- prefix trailing-action sugar, such as `with f(args) body`, desugaring to
  `f(args, () -> body)`;
- flat stacked `with` headers that desugar outermost-first;
- examples under `examples/phase-2.6/` that compile and run through the
  JavaScript backend.

Defer:

- effect declarations, effect rows, handlers, resumptions, scoped effects, and
  coeffects;
- postfix `with`;
- placeholders, partial application, function composition, receiver sugar, and
  Koka-style dot application.

Success criteria:

- parser, checker, LSP-adjacent typed spans, compiler lowering, JavaScript
  emission, and CLI compile support the new sugar;
- `|>` and prefix `with` behave like their explicit call/lambda desugarings;
- generated JavaScript for Phase 2.6 examples runs under Node.

## Phase 3: Effects

Goal: ordinary algebraic effect declarations and effect rows.

This phase introduces effect requirements but not scoped effect families or
coeffects.

Use the `with <effects>` notation for v0 effect rows. The older `! <effects>`
spelling should be treated as an abandoned option unless the design is reopened.

Include:

- `effect` declarations;
- operation symbols exported by effect declarations;
- operation call sites using ordinary function-call syntax;
- function result effects, such as `a with <Console>`;
- open effect rows, such as `a with <Console, ...e>`;
- function types with effects, such as `() -> a with <Console, ...e>`;
- operation resolution by imported name, namespace, and expected effect row;
- basic handlers as function-like values;
- explicit `resume` in handler clauses;
- trailing-action sugar: `with f(args) body`;
- diagnostics for unhandled effects, unknown operations, ambiguous operations,
  invalid `with` usage, and handler return mismatches.

Defer:

- scoped effect declarations;
- named scopes;
- coeffects;
- effect masking;
- syntactic distinction between resumable and non-resumable operations;
- handler literals unless needed as an implementation target;
- advanced laws or restrictions around multi-shot resumptions.

Type checker success criteria:

- tracks effect rows through function calls;
- verifies effect-polymorphic functions;
- verifies a handler removes only the effect it handles and adds any effects its
  clauses perform;
- rejects running an effectful expression where a pure expression is required;
- reports missing effect requirements with actionable messages.

Language server success criteria:

- hover shows value type plus effect row;
- go-to-definition from operation calls to operation declarations;
- diagnostics point at the call site that introduces an unhandled effect;
- completion can suggest operations from imported effect declarations.

Example target:

```fx
export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x / y

catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(_message, _resume) ->
      default

    return(value) ->
      value

value : Int
  with catch(0)
    safe-divide(10, 0)
```

## Phase 3.1: JavaScript effects runtime

Goal: make Phase 3 effectful programs executable through the existing
`fx compile --target js` path.

This phase adds runtime support for ordinary algebraic effects after Phase 3
has stabilized the surface syntax, effect-row checking, operation resolution,
handler typing, diagnostics, and language-server behavior. Phase 3 itself may
accept effectful programs for checking while the compiler rejects them with a
clear unsupported-effect-runtime diagnostic.

Include:

- compiler IR and lowering support for operation calls, handlers, `return`
  clauses, and explicit `resume`;
- a small JavaScript runtime protocol for operation requests, handler dispatch,
  resumptions, and ordinary returns;
- JavaScript emission for the supported Phase 3 effect subset;
- runnable Node smoke tests for handled `Raise` and at least one resumptive
  handler;
- compiler diagnostics for accepted-but-not-yet-executable effect constructs.

Defer:

- scoped effects and named scopes;
- coeffects and `given`;
- effect masking;
- handler literals unless needed as an implementation target;
- advanced laws or restrictions around multi-shot resumptions;
- production packaging, bundling, source maps, optimization, and non-JavaScript
  targets.

Compiler success criteria:

- pure Phase 1, Phase 2, Phase 2.5, and Phase 2.6 programs keep compiling and
  running as before;
- `fx compile --target js` rejects unsupported effectful programs before
  emission with compiler diagnostics distinct from parser and checker
  diagnostics;
- supported Phase 3 effect examples compile to JavaScript and run under Node;
- supported generated JavaScript modules with exported zero-argument `main`
  run directly with `node path/to/module.mjs`, print non-`Unit` results, and
  remain inert when imported;
- generated JavaScript correctly handles operation requests, handler dispatch,
  ordinary returns, and explicit `resume` for the supported subset.

Example target:

```fx
export effect Choose
  choose(left: a, right: a) : a

choose-first(action: () -> a with <Choose, ...e>) : a with <...e>
  handle action()
    choose(left, _right, resume) ->
      resume(left)

    return(value) ->
      value

main : Int
  with choose-first()
    choose(1, 2) + 10
```

## Phase 3.5: Formatter tooling

Goal: add an opinionated formatter after effect syntax is accepted, but before
scoped effects and coeffects add more surface area.

This phase should introduce formatter infrastructure deliberately rather than
building a lossy AST pretty-printer. The formatter must preserve comments and
produce stable, idempotent output.

Include:

- lexer or syntax-tree support for comments and other source trivia needed by
  formatting;
- a formatter API in the syntax/tooling layer that can format a parsed module;
- an `fx fmt` command that formats one file or every `.fx` file under a
  directory;
- a check mode, such as `fx fmt --check`, for CI and examples;
- opinionated rules for indentation, blank lines, imports, records, variants,
  pipelines, prefix `with`, effect declarations, effect rows, handler clauses,
  and match arms;
- golden fixture tests covering Phase 1 through Phase 3 syntax;
- idempotence tests that formatting already-formatted source makes no changes.

Defer:

- semantic refactors or code actions;
- import sorting if it could change module resolution behavior;
- formatting for scoped effects, coeffects, and `given` until those phases land;
- preserving arbitrary unusual spacing exactly as written.

Formatter success criteria:

- `fx fmt --check examples` succeeds for accepted examples;
- formatting preserves comments and parses back to the same accepted program
  shape;
- malformed files report diagnostics without overwriting source;
- formatter tests include compact, intentionally messy inputs and expected
  canonical output.

## Phase 4: Named scopes

Goal: scoped effect families and scope-polymorphic effect rows.

This phase implements the named-scopes v0 design after ordinary effect rows and
handlers are stable.

Status as of 2026-06-11: implemented for syntax, formatting, module indexing,
resolution, checking, LSP-adjacent symbol metadata, positive and negative
examples, and compiler unsupported gating. JavaScript execution for scoped
effects remains deferred; `fx compile --target js` reports scoped-effect
unsupported diagnostics for accepted Phase 4 programs.

Include:

- `scoped effect Effect s ...` declarations;
- `scope Name` declarations;
- `scope Name with <ProtocolShape>` protocol rows;
- scope identities as type-level values suitable for effect rows;
- scoped effect rows such as `a with <State Counter Int>`;
- scope-polymorphic rows such as `a with <State s Int, ...e>`;
- explicit scoped operation calls first;
- `within Scope` as a default-scope provider for operation elision;
- operation elision only when the scoped effect family and active scope are
  unambiguous;
- handler typing that removes only `Effect Scope`, leaving other scopes
  unhandled;
- diagnostics for unknown scopes, invalid scope protocol rows, ambiguous
  scoped operations, and scope mismatches.

Defer:

- fresh scope creation;
- scope masking;
- first-class handler names;
- non-escape checking for locally generated scopes;
- multiple protocol-row entries per scope unless the design explicitly chooses
  them.

Type checker success criteria:

- checks concrete scoped effect rows;
- checks reusable helpers polymorphic over a scope variable;
- verifies scoped handlers affect only the named scope they handle;
- reports ambiguity when two active scopes could satisfy the same unqualified
  operation;
- elaborates `within` operation elision to explicit scoped operation calls.

Language server success criteria:

- hover shows scoped effect rows clearly;
- go-to-definition from a scope use to its declaration;
- go-to-definition from a scoped operation call to the operation declaration;
- diagnostics distinguish missing effect family from missing scope;
- code actions can later suggest adding an explicit scope argument.

Example target:

```fx
export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

increment() : Int with <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n
```

## Phase 4.1: Scoped effect runtime

Goal: make checked Phase 4 scoped-effect programs executable by the JavaScript
compiler and runtime.

This phase follows the completed Phase 4 static/tooling work. Phase 4
established syntax, rows, checking, diagnostics, examples, and compiler
unsupported gating. Phase 4.1 replaces that gating with executable scoped
effect semantics where runtime support is covered by tests.

Include:

- lower scope declarations into stable runtime scope identities;
- lower scoped operation requests with both operation identity and scope
  identity;
- compile `within` and elided calls using the checker's scoped-call behavior,
  so runtime lowering sees explicit scoped operation requests;
- extend handler dispatch to match scoped requests by operation or effect
  family plus scope;
- ensure handlers for one scope leave same-family requests from other scopes
  unhandled;
- keep ordinary Phase 3 effect runtime behavior unchanged;
- remove the current scoped-effect compiler unsupported diagnostics only after
  runtime support is covered by tests.

Defer:

- fresh scope creation;
- scope masking;
- first-class handler names;
- non-escape checking for locally generated scopes;
- multiple protocol-row entries per scope;
- coeffects and `given`;
- capability or protocol authorization beyond current checker validation.

Compiler success criteria:

- `fx compile --target js examples/phase-4/state.fx` succeeds;
- generated JavaScript for `state.fx` executes a concrete scoped-state example;
- a handler for `State Counter Int` does not handle `State OtherCounter Int`;
- scope-polymorphic helpers from `examples/phase-4/polymorphic.fx` compile when
  instantiated with concrete scopes;
- existing Phase 1 through Phase 3 executable examples continue to compile and
  run unchanged;
- compiler unsupported diagnostics remain for any scoped construct not
  implemented in Phase 4.1.

Test targets:

- compiler lowering tests for scoped operation requests carrying scope identity;
- generated JavaScript smoke tests for concrete scoped state;
- generated JavaScript smoke tests for `within` elision;
- generated JavaScript smoke tests for explicit scoped calls;
- generated JavaScript smoke tests for the same effect family with two scopes;
- generated JavaScript smoke tests for a scope-polymorphic helper instantiated
  with concrete scopes.

Validation commands:

- `cargo test`;
- `cargo check`;
- `cargo run -p fx-cli -- check examples/phase-4/state.fx`;
- `cargo run -p fx-cli -- check examples/phase-4/polymorphic.fx`;
- `cargo run -p fx-cli -- compile --target js examples/phase-4/state.fx`;
- `cargo run -p fx-cli -- compile --target js examples/phase-4/polymorphic.fx`.

## Phase 5: Coeffects and `given`

Goal: contextual requirements after effects and scoped effects are stable.

This phase adds a second requirement system for contextual values.

Include:

- context rows such as `a given { clock: Clock }`;
- open context rows such as `a given { clock: Clock, ...c }`;
- functions with both effects and context, such as
  `a with <Console, ...e> given { clock: Clock, ...c }`;
- lexical providers: `given { clock = system-clock() } body`;
- name-and-type contextual resolution;
- contextual values visible by name inside the provider body;
- named provider functions using trailing-action sugar;
- diagnostics for missing contextual values, ambiguous contextual values,
  duplicate context fields, and invalid provider expressions.

Defer:

- `given each`;
- resourceful branch-local context;
- module-level contextual exports;
- final `given { ... }` call arguments;
- ordinary named arguments for contextual requirements;
- qualified contextual names.

Type checker success criteria:

- tracks context rows through function calls;
- resolves a required context field by name and type;
- allows scoped providers to satisfy context requirements for their action;
- checks functions polymorphic over context tails;
- reports missing and ambiguous context requirements clearly.

Language server success criteria:

- hover shows context rows in signatures;
- go-to-definition from an implicitly resolved context use to its provider;
- diagnostics point to the call requiring missing context;
- tooling can eventually show inserted contextual arguments.

Example target:

```fx
export type Clock = {
  now: () -> Instant
}

timestamp(message: Text) : Text given { clock: Clock }
  std.instant.to-text(clock.now()) ++ " " ++ message

main() : Unit with <Console>
  given { clock = system-clock() }
    print-line(timestamp("started"))
```

## Phase 5.5: Editor semantic highlighting

Goal: improve `.fx` editor highlighting through LSP semantic tokens after
Phase 5 syntax and semantic metadata are stable.

This phase is tooling-only. It should not change parser, checker, resolver,
compiler, or formatter behavior except to expose already-computed metadata
through the language server.

Include:

- a stable semantic-token legend for comments, keywords, strings, numbers,
  operators, functions, variables, parameters, properties, types, constructors,
  effects, and scopes;
- full-document semantic-token support in the language server;
- lexical token classification from `fx_syntax::lex_source`;
- semantic overlays from indexed symbols, resolved references, and typed spans;
- declaration modifiers where declaration spans are already reliable;
- VS Code extension wiring through the existing language client;
- editor documentation for launching the extension and confirming semantic
  highlighting.

Defer:

- semantic-token delta updates;
- range-only semantic-token requests;
- theme customization;
- code actions;
- inlay hints;
- folding;
- a large TextMate grammar.

Tooling success criteria:

- VS Code receives semantic tokens for `.fx` files from `fx-lsp`;
- generic LSP clients can request full-document semantic tokens;
- examples through Phase 5 produce sorted, non-overlapping semantic tokens;
- lexical highlighting covers comments, keywords, strings, numbers, and
  operators;
- semantic highlighting distinguishes declarations and references for values,
  types, constructors, effects, operations, scopes, parameters, locals, patterns,
  fields, and contextual values.

## Phase 6: Advanced surface

Goal: fill in power features after the core language, rows, effects, scopes,
and coeffects are usable.

Candidates:

- variadic tuple packs;
- richer tuple rows;
- refs and mutation;
- stronger exhaustiveness and redundancy checking;
- handler literals;
- documented multi-shot semantics and any needed static restrictions;
- `given each`;
- resourceful branch-local context;
- fresh scopes and non-escape checking;
- scope masking;
- first-class handler names;
- standard library expansion;
- richer code actions and refactors.

Each candidate should get a short design note before implementation. Several of
these features interact with effect handlers and resumptions, so they should not
be treated as small syntax additions.

## Cross-phase implementation notes

- Prefer an annotation-driven type checker at first: require top-level
  annotations and infer local bindings.
- Keep the parser recovery-friendly for the language server from the start.
- Maintain a desugared core AST so surface choices like trailing-action `with`,
  scoped operation elision, and contextual insertion do not spread throughout
  the checker.
- Keep row kinds distinct internally even when surface syntax is visually
  uniform.
- Add small example programs for each phase and use them as language-server
  fixtures.
- Create phase-focused example artifacts before the type checker supports them
  when useful. Treat those examples as goal targets and acceptance tests, then
  promote them from expected-failing fixtures to expected-passing fixtures as
  parser, checker, diagnostics, hover, and go-to-definition support catches up.
- Treat diagnostics as part of the feature, not polish after the fact.
- Do not add a feature to the accepted grammar until the type checker and LSP
  can explain it well enough for users to recover from mistakes.

## Phase example artifact protocol

Each phase should maintain small `.fx` programs under `examples/phase-N/` that
focus on the features being implemented in that phase.

Example artifacts may lead the implementation. When an example uses features
that are not supported yet, keep it as a documented goal target rather than
weakening it to match the current checker. As support lands, wire the example
into parser, type checker, diagnostics, hover, and go-to-definition tests.

Prefer examples that combine several phase features in realistic but compact
code. Avoid using later-phase features in earlier-phase examples unless the
example is intentionally documenting a boundary that should still fail.

## Phase learning log protocol

During each phase, maintain a dated phase note under `notes/` for decisions,
surprises, rejected approaches, and implications for later phases.

Use this shape:

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

At the end of each implementation session, update the active phase note before
moving on. When a phase ends, summarize any durable decisions that should be
remembered across future sessions.

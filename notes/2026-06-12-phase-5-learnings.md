# Phase 5 learnings

Date: 2026-06-12

## Purpose

This note records implementation learnings and decisions from Phase 5 of the
implementation plan in `notes/2026-05-23-implementation-phases.md`.

Use it for parser, checker, tooling, lowering, runtime, and diagnostic
decisions related to coeffects and `given`. The work plan is
`notes/2026-06-12-phase-5-work-plan.md`; the research context is
`notes/2026-06-12-phase-5-research.md`.

## Entry template

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

## Entries

- Decision: Keep `given` as the Phase 5 source syntax while allowing the
  internal representation to lower contextual requirements as hidden
  context-record parameters.
- Context: Directly exposing contextual requirements as ordinary initial row
  parameters would simplify the implementation model, but it would hide the
  source-level distinction between ordinary arguments and contextual
  requirements. The settled direction keeps the user-facing syntax explicit and
  lets typed core, compiler IR, and JavaScript generation use context-first
  lowering where useful.
- Applies to phase(s): Phase 5.
- Future implication: Parser, formatter, hover, diagnostics, and examples
  should continue to show `given`, not hidden-parameter wording. Lowering and
  generated JavaScript can still choose a hidden context-record ABI.
- Revisit when: A later backend needs a different ABI for contextual functions
  or the language adds explicit final `given` call arguments.

- Decision: Parse context rows and `given` providers as distinct syntax
  nodes before checker semantics land.
- Context: Phase 5 needs context rows to remain separate from record rows, and
  providers to remain separate from ordinary record literals or ordinary
  trailing-action `with` targets. The parser now accepts contextual result
  types such as `a given { clock: Clock, ...c }`, combined effect/context
  result types such as `a with <Console, ...e> given { clock: Clock, ...c }`,
  and provider expressions such as `given { clock = system-clock() }`.
  Until the checker work units land, downstream passes traverse these nodes
  conservatively and lower contextual types/providers through their result/body
  rather than claiming full Phase 5 semantics.
- Applies to phase(s): Phase 5.
- Future implication: Checker and compiler work can consume structured
  `ContextRow` and provider-field data instead of reconstructing context from
  records, calls, or formatted source. The temporary transparent lowering must
  be replaced by explicit context-row checking and context-record insertion.
- Revisit when: Work units 4 through 9 add internal context-row types,
  propagation, provider checking, and contextual insertion.

- Decision: Represent context rows with their own checker type and substitution
  namespace.
- Context: Context rows reuse source row-tail spelling, but they must not share
  substitution state with record rows or effect rows. The checker now lowers
  contextual annotations to `Type::Contextual` with a `ContextRowType`, displays
  closed and open context rows in source-like form, detects duplicate context
  fields, and substitutes open context tails independently from records and
  effects.
- Applies to phase(s): Phase 5 work units 4 and 5.
- Future implication: Context propagation and insertion can bind context tails
  without polluting record-row or effect-row substitutions.
- Revisit when: Work unit 6 introduces expression-level context propagation and
  needs merge/generalization behavior for multiple contextual requirements.

- Decision: Treat declared context fields as synthetic body locals for Phase 5.
- Context: A function annotation such as `Text given { clock: Clock }` now makes
  `clock` available inside the function body. The resolver, checker, hover, and
  local definition lookup all see the field as a local binding at the context-row
  field span; ordinary parameters and locals continue to shadow through the
  existing local-environment rules.
- Applies to phase(s): Phase 5 work unit 7.
- Future implication: Later contextual insertion can use the same declared
  bindings as the hidden context-record parameter fields during lowering.
- Revisit when: The compiler IR gains explicit contextual parameters and LSP
  needs a dedicated contextual-binding span kind.

- Decision: Implement `given` as a lexical checker provider frame.
- Context: Provider values are inferred once, their names are visible in the
  provider body, and contextual call results are reduced by matching required
  field names against the active provider stack. If no matching provider exists,
  the contextual requirement remains on the expression result and is diagnosed
  when it reaches a pure boundary. Provider type mismatches and duplicate
  provider fields now produce checker diagnostics.
- Applies to phase(s): Phase 5 work units 8 and 9.
- Future implication: The checker now has enough semantic information for IR
  lowering to record concrete context insertion at call sites instead of relying
  on runtime dynamic lookup.
- Revisit when: Work units 14 and 15 add explicit core IR and JavaScript
  lowering for hidden context-record parameters.

- Decision: Lower the supported JavaScript subset with a hidden initial context
  record parameter.
- Context: Contextual value declarations now receive a generated first
  parameter containing required context fields. The lowered body extracts each
  field into an ordinary local, and contextual calls prepend a generated record
  built from the active provider frame. `given` lowers provider values once
  into local lets and uses those locals both for direct provider-body references
  and call-site context insertion.
- Applies to phase(s): Phase 5 work units 14 and 15.
- Future implication: The current JS path supports closed context rows for
  executable contextual functions and `given` providers. Open context rows
  still need a richer IR representation before JavaScript lowering can preserve
  and forward arbitrary context tails.
- Revisit when: Context-polymorphic JavaScript examples need to execute, not
  only typecheck.

- Decision: Treat contextual action lambdas as hidden-context ABI functions.
- Context: Existing trailing-action `with` lowering originally lowered the body
  before knowing the action's contextual result type, so a
  context-polymorphic helper such as `with-clock` could typecheck but not lower.
  The lowerer now derives the expected trailing-action type from the `with`
  target, lowers the body under a synthetic context provider when needed, emits
  a hidden context-record parameter on the action lambda, and elides identity
  provider lets such as `given { clock = clock }`.
- Applies to phase(s): Phase 5 work units 10 and 15.
- Future implication: The supported JavaScript subset now includes
  context-polymorphic provider helpers that remove a named context requirement
  and leave an empty context tail at runtime. General open-tail forwarding still
  needs explicit IR metadata before broader execution support.
- Revisit when: Multiple context fields, nested provider helpers, or non-empty
  context tails need to execute through higher-order functions.

- Decision: Treat contextual effectful calls as effectful in the JavaScript
  emitter.
- Context: Handler actions with types such as
  `() -> Text with <Choose> given { clock: Clock }` were previously wrapped in
  `__fx.pure(...)` because the emitter only recognized plain `Effectful`
  results. The emitter now unwraps contextual result types when deciding whether
  a call needs the effect runtime, so handlers can see operations performed
  inside contextual action lambdas. Smoke tests cover handler clause bodies that
  use contextual values and resumptions that share the context captured at the
  operation site.
- Applies to phase(s): Phase 5 work unit 16.
- Future implication: Future effect/runtime changes must preserve
  context-unwrapping in the effectful-call predicates; otherwise contextual
  effect operations can escape their handlers.
- Revisit when: `given each` adds branch-local context semantics.

- Decision: Keep negative Phase 5 diagnostics fixtures out of the promoted
  `.fx` example set.
- Context: Work Unit 17 uses `fx check examples/phase-5` as a promotion gate,
  so intentionally failing diagnostic cases cannot remain as `.fx` files in
  that directory. The diagnostic source remains in
  `examples/phase-5/diagnostics.fx.txt` and parser/checker tests cover the
  corresponding syntax and diagnostics.
- Applies to phase(s): Phase 5 work units 1, 11, and 17.
- Future implication: Negative examples should live in tests or non-compiled
  fixture files whenever a whole-directory `fx check` gate is expected to pass.
- Revisit when: The CLI gains first-class expected-failure fixture support.

- Decision: Check contextual function bodies against the remaining context
  after declared fields are supplied.
- Context: A function annotated `given { clock: Clock, ...c }` makes `clock`
  available in its body and should not require the body to produce `clock`
  again. The checker now strips declared context fields from the body's expected
  context row, leaving open tails such as `...c` to propagate. This keeps
  higher-order helpers carrying context rows checkable while still requiring
  unsupplied context tails to remain visible.
- Applies to phase(s): Phase 5 work units 6, 7, and 10.
- Future implication: More complete context-row subtraction should build on
  this boundary rather than comparing bodies against their full declared
  context requirement.
- Revisit when: Non-empty context tails are forwarded through JavaScript IR
  metadata instead of erased to the currently supported runtime subset.

- Decision: Reject deferred `given` forms with Phase 5-specific parser
  diagnostics.
- Context: The work plan defers `given each`, final call-site `given`
  arguments, and module-level contextual exports. Generic parse errors make it
  unclear whether those forms are intentionally out of scope or accidentally
  half-supported, so the parser now reports the exact Phase 5 boundary messages
  while recovering enough to continue parsing the surrounding file.
- Applies to phase(s): Phase 5 work units 3 and 11.
- Future implication: Later phases should replace these targeted diagnostics at
  the point where each deferred feature gains concrete syntax and semantics.
- Revisit when: `given each`, final call-site `given`, or module-level
  contextual exports enter an implementation phase.

## Verification

- 2026-06-12 parser/setup slice:
  - `cargo test -p fx-syntax`
  - `cargo check`
- 2026-06-12 context-row checker slice:
  - `cargo test -p fx-core context -- --nocapture`
  - `cargo check`
- 2026-06-12 provider checker slice:
  - `cargo test -p fx-core context -- --nocapture`
  - `cargo check`
- 2026-06-12 JavaScript lowering slice:
  - `cargo test -p fx-compiler contextual_function_executes_with_given_provider -- --nocapture`
  - `cargo test -p fx-compiler`
  - `cargo run -p fx-cli -- check examples/phase-5/basic.fx`
  - `cargo run -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase5-basic examples/phase-5/basic.fx`
  - `node /tmp/fxl-phase5-basic/basic.mjs`
- 2026-06-12 context-polymorphic JavaScript slice:
  - `cargo test -p fx-core declared_context_satisfies_nested_contextual_calls -- --nocapture`
  - `cargo test -p fx-compiler context_polymorphic_with_clock_executes -- --nocapture`
  - `cargo test -p fx-compiler`
  - `cargo run -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase5-with-clock examples/phase-5/with-clock.fx`
  - `node /tmp/fxl-phase5-with-clock/with-clock.mjs`
- 2026-06-12 effects and shared-context slice:
  - `cargo test -p fx-compiler handler_clause_body_uses_contextual_value -- --nocapture`
  - `cargo test -p fx-compiler resumed_continuations_share_operation_site_context -- --nocapture`
- 2026-06-12 promoted Phase 5 examples:
  - `cargo run -q -p fx-cli -- check examples/phase-5`
- 2026-06-12 deferred Phase 5 diagnostics:
  - `cargo test -p fx-syntax reports_deferred_phase_5_given_forms -- --nocapture`
- 2026-06-12 Phase 5 promotion/full validation:
  - `cargo fmt --all --check`
  - `cargo check`
  - `cargo test -p fx-syntax`
  - `cargo test -p fx-core`
  - `cargo test -p fx-compiler`
  - `cargo test -p fx-cli`
  - `cargo test -p fx-lsp`
  - `cargo test`
  - `cargo test -p fx-compiler lowers_contextual_calls_with_hidden_context_records -- --nocapture`
  - `cargo run -q -p fx-cli -- fmt --check examples`
  - `cargo run -q -p fx-cli -- check examples/phase-5`
  - `cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase5-basic examples/phase-5/basic.fx`
  - `cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase5-with-clock examples/phase-5/with-clock.fx`
  - `node /tmp/fxl-phase5-basic/basic.mjs`
  - `node /tmp/fxl-phase5-with-clock/with-clock.mjs`

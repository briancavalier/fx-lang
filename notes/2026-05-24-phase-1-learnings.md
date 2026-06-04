# Phase 1 learnings

Date: 2026-05-24

## Purpose

This note records implementation learnings and decisions from Phase 1 of the
implementation plan in `notes/2026-05-23-implementation-phases.md`.

Use it for decisions, surprises, rejected approaches, and implications for
later phases. Keep entries short, concrete, and tied to the implementation
evidence that produced them.

## Entry template

- Decision:
- Context:
- Applies to phase(s):
- Future implication:
- Revisit when:

## Entries

- Decision: Create phase-focused `.fx` example artifacts before full type
  checker support when they clarify the target behavior.
- Context: Phase examples should act as goal targets and acceptance tests, not
  only as documentation after implementation is complete.
- Applies to phase(s): All phases, starting with Phase 1.
- Future implication: Each phase should keep examples under `examples/phase-N/`
  and promote them from expected-failing fixtures to expected-passing fixtures as
  parser, checker, diagnostics, hover, and go-to-definition support lands.
- Revisit when: The test harness needs to distinguish goal-target examples from
  examples expected to pass today.

- Decision: Use a Rust-first workspace for Phase 1, with TypeScript reserved
  only for possible future editor packaging.
- Context: The initial scaffold now has Rust crates for syntax, core semantics,
  CLI, and an LSP binary. `cargo test`, `cargo fmt --all`, and
  `cargo clippy --all-targets -- -D warnings` validate the first slice.
- Applies to phase(s): Phase 1.
- Future implication: Parser, AST, module graph, symbol indexing, checker, and
  LSP behavior should share Rust APIs rather than duplicating semantics in an
  editor wrapper.
- Revisit when: Distribution or editor integration requires packaging that Rust
  alone cannot provide ergonomically.

- Decision: Treat layout inside multiline delimiters as recoverable parser
  trivia for the initial hand-written parser.
- Context: The Phase 1 fixture uses multiline record types and block-form local
  bindings. The parser now consumes indentation around delimited record/variant
  bodies and supports `name =` followed by an indented expression body.
- Applies to phase(s): Phase 1, with parser implications for later phases.
- Future implication: Parser recovery should keep source spans and layout tokens
  available enough for LSP diagnostics, but semantic AST consumers should not
  have to reason about delimiter-internal indentation.
- Revisit when: The grammar adds trailing-action `with`, handlers, or other
  layout-sensitive forms that need stronger recovery rules.

- Decision: Use `fx` for public-facing command, package, and tool names.
- Context: The repository name `fxl` means "fx language", but users should run
  `fx` and `fx-lsp`, not `fxl` or `fxl-lsp`.
- Applies to phase(s): All phases.
- Future implication: Documentation, CLI help, editor setup, and release
  artifacts should consistently say `fx`.
- Revisit when: Internal implementation names need to distinguish the language
  project from the broader `fx` ecosystem.

- Decision: Add resolver as a distinct core phase between symbol indexing and
  type checking.
- Context: The resolver now builds per-module references from local symbols,
  imported exported symbols, and fully qualified names. `check_workspace`
  consumes resolver diagnostics instead of duplicating unknown-name checks.
- Applies to phase(s): Phase 1.
- Future implication: Hover, go-to-definition, and checker diagnostics should
  use the same resolved references rather than rebuilding name lookup in each
  subsystem.
- Revisit when: Type-directed disambiguation is implemented for ambiguous
  imported names.

- Decision: Fully qualified cross-module names resolve only to exported
  symbols; private symbols remain module-local.
- Context: The v0 module design says fully qualified names do not require an
  import, but non-exported declarations remain local to their module.
- Applies to phase(s): Phase 1.
- Future implication: LSP go-to-definition should reject or diagnose
  `other.private-name` the same way the resolver does, even though the symbol is
  present in the internal module index.
- Revisit when: Friend modules, test visibility, or package-private visibility
  are considered.

- Decision: Lower Phase 1 type declarations into checker-visible aliases before
  checking expressions.
- Context: Record aliases such as `Todo` now expand for field access, and
  closed variant constructors get function types such as
  `Ok : a -> TodoResult a`. This lets `examples/phase-1/todo.fx` be checked
  structurally rather than accepted by `Unknown`.
- Applies to phase(s): Phase 1.
- Future implication: The checker should continue to preserve named types for
  user-facing diagnostics while expanding aliases internally where structural
  checks are required.
- Revisit when: Type aliases need recursion checks, privacy-aware exported
  signatures, or row-polymorphic record/variant aliases in Phase 2.

- Decision: Feed LSP hover and go-to-definition through a shared `fx-core`
  analysis API instead of rebuilding semantic lookup in `fx-lsp`.
- Context: `fx-core::analysis` now combines module indexing, resolver
  references, checker diagnostics, and typed spans. `fx-lsp` stores open
  documents, builds a workspace analysis with open-document overlays, and adapts
  byte spans to LSP UTF-16 positions.
- Applies to phase(s): Phase 1.
- Future implication: LSP features such as document symbols, workspace symbols,
  completion, and richer hover should add facts to core analysis first, then
  adapt them in the protocol layer.
- Revisit when: Diagnostics need per-file ownership instead of the current flat
  checker diagnostic list.

- Decision: Own diagnostics by module/file in each compiler phase, and derive
  aggregate diagnostic lists from those owned diagnostics.
- Context: `IndexedModule`, `ResolvedModule`, and `CheckResult` now retain
  module-owned diagnostics. `fx-core::analysis` exposes `diagnostics_by_path()`
  for LSP publication, while the older aggregate diagnostic vectors remain for
  CLI/backcompat behavior.
- Applies to phase(s): Phase 1.
- Future implication: New compiler phases should attach diagnostics to the
  source module/file where the problem appears before flattening. This keeps
  LSP publication precise and avoids converting unrelated file spans with the
  active document's line index.
- Revisit when: Package roots, generated modules, or cross-file diagnostics need
  richer ownership than a single source path.

- Decision: Build initial document/workspace symbols directly from the Phase 1
  symbol index, deriving module symbols in the LSP adapter.
- Context: `fx-lsp` now advertises document and workspace symbol providers.
  Document symbols present the module as a parent with indexed top-level
  values, types, and constructors as children. Workspace symbols include
  derived module entries and indexed symbols filtered by query.
- Applies to phase(s): Phase 1.
- Future implication: The current symbol ranges use indexed identifier spans.
  Richer outline ranges or nested constructor grouping should come from richer
  core symbol facts rather than ad hoc parsing in the LSP layer.
- Revisit when: Core gains declaration ranges, module symbols, or finer symbol
  hierarchy facts.

- Decision: Use spread-first syntax for closed Phase 1 record update:
  `{ ...record, field: value }`.
- Context: Record update now has a distinct surface AST node. The checker
  requires the base to be a known closed record or record alias, rejects unknown
  fields, checks update value types against existing field types, and preserves
  record aliases for successful updates.
- Applies to phase(s): Phase 1.
- Future implication: Phase 2 row-polymorphic update should build on this
  syntax without changing the Phase 1 closed-record behavior. Spread-not-first
  update syntax remains invalid.
- Revisit when: Open row updates or field addition through row variables are
  implemented.

- Decision: Type Phase 1 lambdas contextually from expected function types.
- Context: Lambda syntax now supports `x -> expr`, `(x, y) -> expr`, and
  block bodies after `->`. Parameters are unannotated and receive types from
  an expected `a -> b` or `(a, b) -> c` type. Lambdas without an expected
  function type produce a diagnostic instead of triggering general inference.
- Applies to phase(s): Phase 1.
- Future implication: The checker can remain annotation-driven while still
  supporting higher-order examples. General inference or annotated lambda
  parameters can be added later without changing the basic arrow syntax.
- Revisit when: Pattern lambda parameters, inferred lambda parameter types, or
  richer block parsing around closing delimiters are implemented.

- Decision: Check Phase 1 match patterns from the inferred scrutinee type.
- Context: Pattern checking now validates literals, variables, lists with
  `...rest`, closed record shorthand patterns, and closed variant constructor
  patterns. Pattern-bound names receive checker typed spans for hover and
  remain scoped to the match arm body.
- Applies to phase(s): Phase 1.
- Future implication: Record patterns are closed shorthand binders for now;
  they do not introduce row-polymorphic destructuring or named-field nested
  pattern syntax.
- Revisit when: Exhaustiveness/redundancy checking, row-polymorphic patterns,
  tuple patterns, or pattern lambda parameters are implemented.

- Decision: Keep the Phase 1 prelude compiler-known and non-importable.
- Context: Primitive types, `unit`, and built-in operators are semantic
  compiler data rather than synthesized `std` modules. Operator checking uses
  explicit built-in signatures for `+`, `-`, `++`, and `==`, and the checker
  emits typed spans for operator hovers.
- Applies to phase(s): Phase 1.
- Future implication: Importable standard-library modules can be added later
  without changing Phase 1 operator semantics or document/workspace symbols.
- Revisit when: A source-backed stdlib, typeclass-like equality, numeric
  overloading, or effectful `std.io` support is implemented.

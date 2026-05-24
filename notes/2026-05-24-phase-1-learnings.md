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

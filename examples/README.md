# Examples

This directory contains small `.fx` programs used as phase artifacts for the
implementation plan in `notes/2026-05-23-implementation-phases.md`.

Examples may be written before the parser, type checker, or language server
supports every feature they use. In that state, they are goal targets and
acceptance tests for the current phase. As implementation support lands, promote
them into expected-passing parser, checker, diagnostics, hover, and
go-to-definition fixtures.

Keep examples compact and focused on the phase they belong to. Avoid reaching
for later-phase features unless the example is intentionally documenting a
boundary that should still fail.

## Phase 1

Phase 1 examples are expected to parse and check successfully:

- `phase-1/todo.fx`
- `phase-1/record-update.fx`
- `phase-1/lambda.fx`
- `phase-1/patterns.fx`
- `phase-1/operators.fx`

Try all Phase 1 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-1
```

Try a single file:

```sh
cargo run -q -p fx-cli -- check examples/phase-1/todo.fx
```

Use JSON output for scripts:

```sh
cargo run -q -p fx-cli -- check --json examples/phase-1
```

For VS Code smoke testing, rebuild the language server and open the extension
host:

```sh
cargo build -p fx-lsp
```

Then open the Phase 1 examples and verify diagnostics, hovers,
go-to-definition, document symbols, and workspace symbols.

## Phase 2

Phase 2 examples are goal targets for row-polymorphic records and variants.
They are expected to report Phase 2 diagnostics until the corresponding parser,
checker, diagnostics, hover, and go-to-definition support lands.

- `phase-2/rows.fx`

Try the current Phase 2 goal target:

```sh
cargo run -q -p fx-cli -- check examples/phase-2/rows.fx
```

This command is expected to fail until Phase 2 row support is implemented.

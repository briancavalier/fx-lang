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

Phase 2 examples are expected to parse and check successfully:

- `phase-2/rows.fx`

Try all Phase 2 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-2
```

Try a single file:

```sh
cargo run -q -p fx-cli -- check examples/phase-2/rows.fx
```

## Phase 2.5

Phase 2.5 examples are expected to parse, check, compile, and execute through
the JavaScript compiler path:

- `phase-2.5/domain.fx`
- `phase-2.5/functions.fx`
- `phase-2.5/main.fx`

Check all Phase 2.5 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-2.5
```

Compile all Phase 2.5 examples to JavaScript:

```sh
cargo run -q -p fx-cli -- compile --target js examples/phase-2.5
```

Write executable ESM files for Node:

```sh
cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase-2.5 examples/phase-2.5
node -e "import('/tmp/fxl-phase-2.5/main.mjs').then((m) => console.log(m.__fx_user_main()))"
```

## Phase 2.6

Phase 2.6 examples exercise data-last pipeline calls and prefix `with`
trailing-action sugar:

- `phase-2.6/support.fx`
- `phase-2.6/main.fx`

Check all Phase 2.6 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-2.6
```

Compile all Phase 2.6 examples to JavaScript:

```sh
cargo run -q -p fx-cli -- compile --target js examples/phase-2.6
```

Write executable ESM files for Node:

```sh
cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase-2.6 examples/phase-2.6
node -e "import('/tmp/fxl-phase-2.6/main.mjs').then((m) => console.log(m.__fx_user_main()))"
```

## Phase 3

Phase 3 examples are goal targets for ordinary algebraic effects. They are not
expected to parse or check successfully until Phase 3 support lands:

- `phase-3/effects.fx`
- `phase-3/imported.fx`
- `phase-3/resume.fx`

Once Phase 3 support lands, check all Phase 3 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-3
```

JavaScript execution for effectful programs belongs to Phase 3.1. Until then,
`fx compile --target js` should continue compiling pure earlier-phase programs
and reject effectful Phase 3 programs with a clear unsupported-effect-runtime
compiler diagnostic.

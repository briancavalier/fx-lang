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
node /tmp/fxl-phase-2.5/main.mjs
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
node /tmp/fxl-phase-2.6/main.mjs
```

## Phase 3

Phase 3 examples are check fixtures for ordinary algebraic effects:

- `phase-3/effects.fx`
- `phase-3/imported.fx`
- `phase-3/resume.fx`

Check all Phase 3 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-3
```

Phase 3.1 adds JavaScript execution for the supported ordinary effect subset:
operation calls, handlers, handler `return` clauses, explicit `resume`, and
effectful expression sequencing. `fx compile --target js` should compile these
fixtures and continue reporting `unsupported-effect-runtime` for checked
effectful constructs outside the executable subset.

Write executable ESM files for Node:

```sh
cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl-phase-3 examples/phase-3
node /tmp/fxl-phase-3/resume.mjs
node /tmp/fxl-phase-3/effects.mjs
```

## Phase 4

Phase 4 examples are goal targets for named scopes, scoped effect families,
scope-polymorphic effect rows, explicit scoped operation calls, and
`within` operation elision:

- `phase-4/state.fx`
- `phase-4/polymorphic.fx`
- `phase-4/diagnostics.fx`

Before Phase 4 implementation lands, these examples are not expected to parse
or check. As support lands, promote `state.fx` and `polymorphic.fx` into
expected-passing parser, checker, diagnostics, hover, and go-to-definition
fixtures. Promote `diagnostics.fx` into focused negative fixtures with stable
scoped-effect messages.

Try all Phase 4 examples once implementation begins:

```sh
cargo run -q -p fx-cli -- check examples/phase-4
```

## Phase 5

Phase 5 examples exercise coeffects and lexical `given` providers:

- `phase-5/basic.fx`
- `phase-5/effects.fx`
- `phase-5/higher-order.fx`
- `phase-5/with-clock.fx`

`phase-5/diagnostics.fx.txt` documents expected diagnostics for unsupported
or invalid Phase 5 forms and should not be included in passing directory runs.

Check all expected-passing Phase 5 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-5
```

## Phase 6A

Phase 6A examples document the handler, resumption, operation-site context, and
branch-lifetime semantic contract:

- `phase-6a/resume-once.fx`
- `phase-6a/resume-twice.fx`
- `phase-6a/non-resume.fx`
- `phase-6a/nested-order.fx`
- `phase-6a/scoped-resume.fx`
- `phase-6a/operation-site-context.fx`

Check and compile the expected-passing Phase 6A examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-6a
cargo run -q -p fx-cli -- compile --target js examples/phase-6a
```

## Phase 6B

Phase 6B examples exercise executable resourceful context and lifecycle
cleanup:

- `phase-6b/finally.fx`
- `phase-6b/given-each.fx`
- `phase-6b/given-each-cleanup.fx`
- `phase-6b/repeated-resume.fx`
- `phase-6b/non-resume-cleanup.fx`
- `phase-6b/nested-lifo.fx`
- `phase-6b/resourceful-boundary.fx`

`phase-6b/diagnostics.fx.txt` documents expected diagnostics for invalid
resourceful context forms and should not be included in passing directory runs.

Check and compile the expected-passing Phase 6B examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-6b
cargo run -q -p fx-cli -- compile --target js examples/phase-6b
```

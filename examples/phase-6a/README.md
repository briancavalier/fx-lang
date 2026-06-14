# Phase 6A examples

Phase 6A documents and validates the current handler, resumption, context, and
branch-lifetime contract. The `.fx` files in this directory are expected to
parse, check, compile to JavaScript, and execute with the current runtime.

Expected-passing fixtures:

- `resume-once.fx` shows a handler resuming a continuation once.
- `resume-twice.fx` shows the current multi-resumption-capable behavior.
- `non-resume.fx` shows abortive handling where a clause ignores `resume`.
- `nested-order.fx` shows that handler nesting order is observable.
- `scoped-resume.fx` shows that scoped resumptions re-enter the same scope.
- `operation-site-context.fx` shows that resumed continuations keep the
  operation-site contextual insertion, not a provider visible only at the
  resume site.

Goal-target fixture:

- `resourceful-boundary.fx.txt` documents the future cleanup rule for
  branch-local resourceful context. It intentionally uses unsupported `given
  each` syntax and should not be included in passing `fx check` directory runs.

Try the expected-passing examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-6a
cargo run -q -p fx-cli -- compile --target js examples/phase-6a
```

# fx-lang

fx-lang is an experimental language for exploring scoped effects, named scopes,
and coeffects.

The repository contains a Rust implementation of the parser, formatter, type
checker, JavaScript compiler path, CLI, and language server experiments. It is
research code first: the examples are organized by implementation phase, and
some language features are intentionally narrower than the long-term design.

## Example

This example combines the main ideas:

- `State` is a scoped effect family.
- `Counter` is a named scope for one `State Int` cell.
- `within Counter` lets operations omit their explicit scope argument.
- `given { clock: Clock }` is a coeffect: the function needs contextual data,
  but it is not an ordinary parameter.

```fx
export scoped effect State s a
  get() : a
  put(value: a) : Unit

export type Clock = { now: Text }

scope Counter with <State Int>

increment-and-label() : Text with <State Counter Int> given { clock: Clock }
  within Counter
    n = get()
    put(n + 1)
    clock.now ++ " count=" ++ show(n + 1)
```

The scope name distinguishes one state cell from another at the type level,
while `given` tracks required context separately from ordinary function
arguments and effect rows.

## Try It

Check the promoted Phase 5 examples:

```sh
cargo run -q -p fx-cli -- check examples/phase-5
```

The full `examples` tree also contains intentional diagnostics fixtures.

Format sources:

```sh
cargo run -q -p fx-cli -- fmt --check examples
```

Compile the currently supported JavaScript subset:

```sh
cargo run -q -p fx-cli -- compile --target js --out-dir /tmp/fxl examples/phase-5
node /tmp/fxl/with-clock.mjs
```

## Repository Layout

- `crates/fx-syntax`: lexer, parser, AST, diagnostics, and formatter
- `crates/fx-core`: module loading, resolution, and type checking
- `crates/fx-compiler`: lowering and JavaScript emission
- `crates/fx-cli`: command-line interface
- `crates/fx-lsp`: language server prototype
- `examples`: phase-oriented language examples
- `notes`: design notes, implementation plans, and phase learnings

## Status

fx-lang is pre-release and changing quickly. It is useful for experimenting
with typed effect systems, named effect scopes, contextual requirements, and
compiler/runtime design, but it is not intended for production use.

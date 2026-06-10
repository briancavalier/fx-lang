# fx basic syntax sketch

Date: 2026-05-22

## Research anchors

- Koka is the closest syntax and type-system anchor: lightweight function-oriented syntax, immutable `val`, explicit mutable `var`, first-class heap refs, and row-polymorphic effects.
- Unison is a useful readability anchor: indentation-sensitive definitions, lambdas written as `x -> expr`, simple type signatures, and tuple syntax that stays visually light.
- OCaml polymorphic variants are the main cautionary precedent for open variants: they are powerful and type-safe, but library boundaries need explicit types because inferred open variants can hide mistakes.
- PureScript row types show the uniform model worth copying: records and variants are both row-polymorphic structures over labels.
- TypeScript variadic tuples show the ergonomic type-level surface for tuple packs, spreads, and rest-like typing.
- Koka's newer qualifier model and Unison-style namespaces are better module anchors for fx than selective import/export lists: fully qualified names should always work, and imports should only affect name resolution convenience.

Sources:

- Koka book: https://koka-lang.github.io/koka/doc/book.html
- Koka row-polymorphic effects paper: https://arxiv.org/abs/1406.2061
- Unison functions: https://www.unison-lang.org/docs/fundamentals/values-and-functions/functions/
- Unison types: https://www.unison-lang.org/docs/language-reference/types/
- OCaml polymorphic variants: https://ocaml.org/manual/4.13/polyvariant.html
- TypeScript variadic tuple types: https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-0.html
- OCaml modules: https://ocaml.org/docs/modules
- Elm modules: https://guide.elm-lang.org/webapps/modules

## Initial syntax decisions

fx should be expression-oriented, layout-sensitive, immutable by default, and sparse in punctuation.

Value identifiers may contain internal hyphens, following Koka's visually light `kebab-case` style:

```fx
print-line
map-tuple-tail
todo-count
```

Lexical rule: a hyphen is part of an identifier only when it appears between identifier characters. A hyphen with surrounding whitespace, as in `x - y`, is the subtraction operator. For readability, use spaces around infix operators; write hyphenated names without spaces.

Top-level values and local values use the same shape:

```fx
name: Type = expr
name = expr
```

Functions are values. A named function is just a value bound to a lambda-like equation:

```fx
map(f: a -> b, xs: List a) : List b
  match xs
    [] -> []
    [x, ...rest] -> [f(x), ...map(f, rest)]
```

Named function declarations use parenthesized comma-separated parameters followed by a return type. The indented body is the function body.

Anonymous functions use Unison/Koka-like arrow syntax:

```fx
x -> x + 1
(x, y) -> x + y
```

Proper tail calls are semantic, not syntactic: any call in tail position must run as a jump. That should include direct recursion, mutual recursion, and tail calls through local functions where the compiler can preserve the tail position.

Immutable bindings use plain `=`. Mutable references are explicit values:

```fx
count = ref 0
count := !count + 1
```

No local rebinding mutation in v0. If we add mutable local variables later, make them syntactically different from refs, for example `var i = 0`, and keep them non-escaping like Koka.

The unit type is `Unit`; the unit value is `unit`.

## Uniform rows

Records, variants, and tuples should share a row syntax:

```fx
{ name: Text, age: Int, ...r }     # record row
< Found(a), Missing(), ...r >      # variant row
(head: a, ...tail)                 # tuple row / tuple pack
```

The visual rule:

- `{ ... }` is fields by label.
- `< ... >` is cases by constructor.
- `( ... )` is positions, optionally with names only in type-level documentation or pattern aliases.
- `...r` means the rest of the row.
- The spread tail is final-only in v0. Closed rows omit it.

Record values:

```fx
user = { id: 42, name: "Ada", email: "ada@example.test" }
display = user.name
renamed = { user | name: "Ada Lovelace" }
```

Open record functions:

```fx
greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name
```

Variant constructors are function-like values:

```fx
ok = Ok(42)
missing = Missing()
```

Constructors are exported symbols in the module namespace and use the same resolution rules as functions. Nullary constructors use `()` in type rows, expression positions, and pattern positions. Constructor entries use types only; if a payload needs names, use a record payload.

Open variant functions:

```fx
message(result: < Ok(a), Missing(), Error(Text), ...r >) : Text
  match result
    Ok(_) -> "ok"
    Missing() -> "missing"
    Error(text) -> text
    other -> "unknown"
```

Variadic tuple functions:

```fx
prepend(x: a, xs: (...tail)) : (a, ...tail)
  (x, ...xs)

tail(tuple: (head, ...tail)) : (...tail)
  match tuple
    (_, ...rest) ->
      rest

map-tuple-tail(f: a -> b, tuple: (head, ...tail)) : (head, ...tail)
  match tuple
    (head, ...tail) ->
      (head, ...map(f, tail))
```

Function calls always use parenthesized comma-separated arguments:

```fx
map(f, xs)
print-line("hello")
```

Data-last pipeline sugar appends the piped value to the rightmost ordinary call
argument position:

```fx
xs |> map(render) |> join(", ")
```

desugars to:

```fx
join(", ", map(render, xs))
```

Pipeline does not provide receiver semantics. Koka-style dot receiver
application may be considered later as a separate feature.

Lambdas stay lightweight:

```fx
x -> x + 1
(x, y) -> x + y
(_, ...rest) ->
  rest
```

## Parametric polymorphism

Use lowercase type variables and implicit `forall` by default:

```fx
id(x: a) : a
  x

compose(f: b -> c, g: a -> b, x: a) : c
  f(g(x))
```

Allow explicit quantification before the parameter list when it improves signatures:

```fx
const[a, b](x: a, _: b) : a
  x
```

## Modules

Keep v0 modules as namespaces, not as a separate import/export calculus:

```fx
import std.io
import std.list

export type Todo = {
  id: Int,
  title: Text,
  done: Bool
}

export main() : IO Unit
  print-line("hello")
  std.io.print-line("hello again")
```

Rules:

- Every source file is a module.
- A module's namespace is derived from its file path relative to a package source root.
- For example, `example/todo.fx` defines namespace `example.todo`; `example/todo/index.fx` defines namespace `example.todo.index`.
- Every exported symbol belongs to its module's fully qualified namespace.
- Fully qualified names are always valid and do not require an import.
- `import module.path` brings that namespace into scope so exported symbols can be referenced unqualified when resolution is unambiguous.
- Import aliases are not supported in v0, for either absolute or relative imports.
- If multiple imported namespaces export the same name, the compiler first uses type information to disambiguate.
- If type information is insufficient, the program must write just enough qualifier prefix to disambiguate, such as `io.print-line(...)` or `std.io.print-line(...)`.
- Exports are inline via `export` on type declarations or function/value declarations. Non-exported declarations remain local to the module.
- Exporting a record type exports only the type name. Record field access uses dot syntax, for example `todo.done`; fields are not exported namespace symbols by default.
- Exporting a variant type exports the type name and its constructors/tags together.
- No functors or first-class modules in v0.

## Example program

```fx
import std.io
import std.list

export type Todo = {
  id: Int,
  title: Text,
  done: Bool
}

export type TodoResult a = < Ok(a), NotFound(Int), Invalid(Text) >

export summarize(todo: { title: Text, done: Bool, ...r }) : Text
  status =
    if todo.done then "done" else "open"

  todo.title ++ " [" ++ status ++ "]"

find(wanted: Int, todos: List { id: Int, ...r }) : < Ok({ id: Int, ...r }), NotFound(Int) >
  match todos
    [] ->
      NotFound(wanted)

    [todo, ...rest] ->
      if todo.id == wanted then
        Ok(todo)
      else
        find(wanted, rest)

map-tuple-tail(f: a -> b, tuple: (head, ...tail)) : (head, ...tail)
  match tuple
    (head, ...tail) ->
      (head, ...map(f, tail))

render-id(id: Int) : Text
  std.int.to-text(id)

export main() : IO Unit
  todos = [
    { id: 1, title: "Sketch syntax", done: true },
    { id: 2, title: "Try parser", done: false }
  ]

  visits = ref 0
  visits := !visits + 1

  match find(2, todos)
    Ok(todo) ->
      print-line(summarize(todo))

    NotFound(id) ->
      print-line("missing todo " ++ render-id(id))
```

## Relative imports

Relative imports are feasible if module namespaces are path-derived:

```fx
import .todo
import .todo.validation
import ..shared.text
```

Interpreted from `example/app/main.fx`, these would resolve as:

```txt
import .todo            -> example.app.todo
import .todo.validation -> example.app.todo.validation
import ..shared.text    -> example.shared.text
```

This is useful for local package structure, tests, and examples because nearby modules can move together without rewriting absolute package prefixes.

Relative imports are package-local only in v0. Imports from dependencies use absolute package/module namespaces.

Relative imports behave like absolute imports in v0: they bring exported symbols into unqualified resolution. They do not introduce a separate local qualifier binding.

Exported signatures may use names brought into scope by relative imports. The compiler and generated documentation resolve those names to their fully qualified namespaces.

Tooling normalizes relative-imported names to absolute fully qualified names in hover text, inlay hints, diagnostics, and generated documentation.

The ambiguities are mostly project-layout and readability issues, not parser issues:

- `import .todo` depends on the importing file's namespace, so moving a file can silently change what it imports.
- `import .todo` from `example/app.fx` resolves from the containing namespace of the current file, so it means `example.todo`. Python treats one leading dot as the current package rather than the current module file; fx follows that direction.
- There is no special `index.fx` module name. `example/app/index.fx` is namespace `example.app.index`, and `import .todo` from that file resolves to `example.app.todo`.
- Source roots matter. `src/example/app/main.fx` and `test/example/app/main.fx` may have the same namespace unless package config separates or overlays roots intentionally.
- Relative imports make fully qualified names less visible in source, which can make public API boundaries harder to scan.
- Relative imports interact with partial qualifiers. If both `import .todo` and `import example.shared.todo` are present, an unqualified `validate(...)` may need type-directed disambiguation or a qualifier like `todo.validate(...)`.

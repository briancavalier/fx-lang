# fx VS Code extension

This is a minimal development wrapper for the Rust `fx-lsp` server.

## Try it

```sh
cd editors/vscode-fx
npm install
code .
```

Press `F5`, then open this repository in the Extension Development Host and
open `examples/phase-1/todo.fx`.

The extension starts `fx-lsp` through the VS Code language client. Semantic
highlighting is provided by the language server, so no TextMate grammar is
required for the first pass. If highlighting looks plain, confirm that VS Code's
`editor.semanticHighlighting.enabled` setting is not disabled.

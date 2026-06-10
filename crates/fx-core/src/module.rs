use fx_syntax::ast::{Item, Module, ParsedModule, TypeExpr};
use fx_syntax::diagnostic::Diagnostic;
use fx_syntax::parse_file;
use fx_syntax::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleGraph {
    pub modules: BTreeMap<String, IndexedModule>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexedModule {
    pub name: String,
    pub path: PathBuf,
    pub parsed: ParsedModule,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub module: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub exported: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Value,
    Type,
    Constructor,
    Effect,
    Operation,
}

pub fn index_workspace(root: &Path, files: Vec<SourceFile>) -> ModuleGraph {
    let mut modules = BTreeMap::new();

    for file in files {
        let parsed = parse_file(file.source);
        let mut module_diagnostics = parsed.diagnostics.clone();
        let name = module_name(root, &file.path);
        if modules.contains_key(&name) {
            module_diagnostics.push(Diagnostic::new(
                Span::point(0),
                format!("duplicate module `{name}`"),
            ));
        }
        let (symbols, imports, index_diagnostics) = index_module(&name, &parsed.module);
        module_diagnostics.extend(index_diagnostics);
        modules.insert(
            name.clone(),
            IndexedModule {
                name,
                path: file.path,
                parsed,
                symbols,
                imports,
                diagnostics: module_diagnostics,
            },
        );
    }

    let module_names = modules.keys().cloned().collect::<BTreeSet<_>>();
    for module in modules.values_mut() {
        let mut resolved_imports = Vec::new();
        for import in &module.imports {
            if let Some(resolved) = resolve_import_name(&module.name, import, &module_names) {
                resolved_imports.push(resolved);
            } else {
                module.diagnostics.push(Diagnostic::new(
                    Span::point(0),
                    format!("unresolved import `{import}` in module `{}`", module.name),
                ));
            }
        }
        module.imports = resolved_imports;
    }

    let diagnostics = modules
        .values()
        .flat_map(|module| module.diagnostics.iter().cloned())
        .collect();

    ModuleGraph {
        modules,
        diagnostics,
    }
}

fn index_module(module: &str, ast: &Module) -> (Vec<Symbol>, Vec<String>, Vec<Diagnostic>) {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen = BTreeMap::<String, Span>::new();

    for item in &ast.items {
        match item {
            Item::Import(import) => imports.push(import.path.join(".")),
            Item::Type(decl) => {
                add_symbol(
                    &mut symbols,
                    &mut seen,
                    &mut diagnostics,
                    Symbol {
                        module: module.to_string(),
                        name: decl.name.text.clone(),
                        qualified_name: format!("{module}.{}", decl.name.text),
                        kind: SymbolKind::Type,
                        exported: decl.export,
                        span: decl.name.span,
                    },
                );
                if let TypeExpr::Variant { cases, .. } = &decl.body {
                    for case in cases {
                        add_symbol(
                            &mut symbols,
                            &mut seen,
                            &mut diagnostics,
                            Symbol {
                                module: module.to_string(),
                                name: case.name.text.clone(),
                                qualified_name: format!("{module}.{}", case.name.text),
                                kind: SymbolKind::Constructor,
                                exported: decl.export,
                                span: case.name.span,
                            },
                        );
                    }
                }
            }
            Item::Effect(decl) => {
                add_symbol(
                    &mut symbols,
                    &mut seen,
                    &mut diagnostics,
                    Symbol {
                        module: module.to_string(),
                        name: decl.name.text.clone(),
                        qualified_name: format!("{module}.{}", decl.name.text),
                        kind: SymbolKind::Effect,
                        exported: decl.export,
                        span: decl.name.span,
                    },
                );
                for operation in &decl.operations {
                    add_symbol(
                        &mut symbols,
                        &mut seen,
                        &mut diagnostics,
                        Symbol {
                            module: module.to_string(),
                            name: operation.name.text.clone(),
                            qualified_name: format!("{module}.{}", operation.name.text),
                            kind: SymbolKind::Operation,
                            exported: decl.export,
                            span: operation.name.span,
                        },
                    );
                }
            }
            Item::Value(decl) => add_symbol(
                &mut symbols,
                &mut seen,
                &mut diagnostics,
                Symbol {
                    module: module.to_string(),
                    name: decl.name.text.clone(),
                    qualified_name: format!("{module}.{}", decl.name.text),
                    kind: SymbolKind::Value,
                    exported: decl.export,
                    span: decl.name.span,
                },
            ),
        }
    }

    (symbols, imports, diagnostics)
}

fn add_symbol(
    symbols: &mut Vec<Symbol>,
    seen: &mut BTreeMap<String, Span>,
    diagnostics: &mut Vec<Diagnostic>,
    symbol: Symbol,
) {
    if let Some(first) = seen.get(&symbol.name) {
        diagnostics.push(Diagnostic::new(
            symbol.span,
            format!(
                "duplicate definition `{}`; first defined at byte {}",
                symbol.name, first.start
            ),
        ));
    } else {
        seen.insert(symbol.name.clone(), symbol.span);
    }
    symbols.push(symbol);
}

fn module_name(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let without_ext = relative.with_extension("");
    without_ext
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(|part| part.replace('/', "."))
        .collect::<Vec<_>>()
        .join(".")
}

fn resolve_import_name(
    module_name: &str,
    import: &str,
    module_names: &BTreeSet<String>,
) -> Option<String> {
    if module_names.contains(import) {
        return Some(import.to_string());
    }
    let (parent, _) = module_name.rsplit_once('.')?;
    let sibling = format!("{parent}.{import}");
    module_names.contains(&sibling).then_some(sibling)
}

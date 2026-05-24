use fx_core::{SourceFile, SymbolKind, check_workspace, index_workspace, resolve_workspace};
use std::path::PathBuf;

#[test]
fn indexes_phase_1_todo_symbols() {
    let graph = todo_graph();

    assert_eq!(graph.diagnostics, []);
    let module = graph.modules.get("todo").expect("todo module");
    let names = module
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect::<Vec<_>>();

    assert!(names.contains(&("Todo", SymbolKind::Type)));
    assert!(names.contains(&("TodoResult", SymbolKind::Type)));
    assert!(names.contains(&("Ok", SymbolKind::Constructor)));
    assert!(names.contains(&("NotFound", SymbolKind::Constructor)));
    assert!(names.contains(&("summarize", SymbolKind::Value)));
    assert!(names.contains(&("find", SymbolKind::Value)));
}

#[test]
fn checker_runs_on_phase_1_todo_fixture() {
    let graph = todo_graph();
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
    assert!(
        result
            .module_types
            .get("todo")
            .is_some_and(|types| types.contains_key("find"))
    );
}

#[test]
fn reports_duplicate_definitions() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/dupe.fx"),
            source: "one() : Int\n  1\n\none() : Int\n  2\n".to_string(),
        }],
    );

    assert!(
        graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate definition"))
    );
}

#[test]
fn reports_unresolved_imports() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/imports.fx"),
            source: "import missing.module\n".to_string(),
        }],
    );

    assert!(
        graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved import"))
    );
}

#[test]
fn resolves_imported_unqualified_symbols() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![
            source("examples/phase-1/lib.fx", "export value() : Int\n  1\n"),
            source(
                "examples/phase-1/main.fx",
                "import lib\n\nmain() : Int\n  value()\n",
            ),
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert_eq!(resolved.diagnostics, []);
    let main = resolved.modules.get("main").expect("main module");
    assert!(main.references.iter().any(|reference| {
        reference.name == "value" && reference.target.qualified_name == "lib.value"
    }));
}

#[test]
fn resolves_fully_qualified_symbols_without_imports() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![
            source("examples/phase-1/lib.fx", "export value() : Int\n  1\n"),
            source("examples/phase-1/main.fx", "main() : Int\n  lib.value()\n"),
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert_eq!(resolved.diagnostics, []);
    let main = resolved.modules.get("main").expect("main module");
    assert!(main.references.iter().any(|reference| {
        reference.name == "lib.value" && reference.target.qualified_name == "lib.value"
    }));
}

#[test]
fn reports_ambiguous_imported_symbols() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![
            source("examples/phase-1/one.fx", "export value() : Int\n  1\n"),
            source("examples/phase-1/two.fx", "export value() : Int\n  2\n"),
            source(
                "examples/phase-1/main.fx",
                "import one\nimport two\n\nmain() : Int\n  value()\n",
            ),
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert!(
        resolved
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("ambiguous name `value`"))
    );
}

#[test]
fn does_not_resolve_private_cross_module_symbols() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![
            source("examples/phase-1/lib.fx", "private() : Int\n  1\n"),
            source(
                "examples/phase-1/main.fx",
                "main() : Int\n  lib.private()\n",
            ),
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert!(
        resolved
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown name `lib.private`"))
    );
}

fn todo_graph() -> fx_core::ModuleGraph {
    index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/todo.fx"),
            source: include_str!("../../../examples/phase-1/todo.fx").to_string(),
        }],
    )
}

fn source(path: &str, source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(path),
        source: source.to_string(),
    }
}

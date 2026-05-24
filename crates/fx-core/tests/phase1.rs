use fx_core::{SourceFile, SymbolKind, check_workspace, index_workspace};
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

fn todo_graph() -> fx_core::ModuleGraph {
    index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/todo.fx"),
            source: include_str!("../../../examples/phase-1/todo.fx").to_string(),
        }],
    )
}

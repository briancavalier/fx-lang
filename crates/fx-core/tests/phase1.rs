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

#[test]
fn checks_record_alias_field_access() {
    let result = check_single(
        "record.fx",
        r#"type Todo = {
  id: Int,
  title: Text
}

get-id(todo: Todo) : Int
  todo.id
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_missing_record_fields_through_aliases() {
    let result = check_single(
        "record.fx",
        r#"type Todo = {
  id: Int
}

bad(todo: Todo) : Int
  todo.done
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown record field `done`"))
    );
}

#[test]
fn reports_constructor_arity_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing() >

bad() : Result
  Ok()
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("arity mismatch"))
    );
}

#[test]
fn reports_constructor_payload_type_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing() >

bad() : Result
  Ok("nope")
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `Int`, found `Text`"))
    );
}

#[test]
fn reports_unknown_variant_pattern_constructors() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing() >

bad(result: Result) : Int
  match result
    Nope(value) ->
      value
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("constructor `Nope` is not part of `Result`")
    }));
}

#[test]
fn reports_match_arm_result_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing() >

bad(result: Result) : Int
  match result
    Ok(value) ->
      value

    Missing() ->
      "missing"
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("match arm type mismatch"))
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

fn check_single(path: &str, source_text: &str) -> fx_core::CheckResult {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source(path, source_text)],
    );
    check_workspace(&graph)
}

use fx_core::{
    SourceFile, SymbolKind, analyze_workspace, check_workspace, index_workspace, resolve_workspace,
};
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
fn checker_runs_on_phase_1_record_update_fixture() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/record-update.fx"),
            source: include_str!("../../../examples/phase-1/record-update.fx").to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
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
fn checks_record_alias_updates() {
    let result = check_single(
        "record.fx",
        r#"type Todo = {
  id: Int,
  done: Bool
}

mark-done(todo: Todo) : Todo
  { ...todo, done: true }
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_structural_record_updates() {
    let result = check_single(
        "record.fx",
        r#"mark-done() : { id: Int, done: Bool }
  todo =
    { id: 1, done: false }

  { ...todo, done: true }
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_unknown_record_update_fields() {
    let result = check_single(
        "record.fx",
        r#"type Todo = {
  id: Int
}

bad(todo: Todo) : Todo
  { ...todo, done: true }
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
fn reports_record_update_value_type_mismatches() {
    let result = check_single(
        "record.fx",
        r#"type Todo = {
  done: Bool
}

bad(todo: Todo) : Todo
  { ...todo, done: "yes" }
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `Bool`, found `Text`"))
    );
}

#[test]
fn reports_record_update_non_record_bases() {
    let result = check_single(
        "record.fx",
        r#"bad(value: Int) : Int
  { ...value, done: true }
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record update requires a record")
    }));
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

#[test]
fn hovers_top_level_function_type() {
    let source_text = "answer() : Int\n  42\n";
    let analysis = analyze_single("main.fx", source_text);

    let hover = analysis
        .hover_at("main", offset_of(source_text, "answer"))
        .expect("hover");

    assert_eq!(hover.contents, "answer : () -> Int");
}

#[test]
fn hovers_parameter_local_pattern_and_field_types() {
    let source_text = r#"type Todo = {
  id: Int,
  title: Text
}

pick(todo: Todo, todos: List Todo) : Text
  label =
    todo.title

  match todos
    [first, ...rest] ->
      label
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "todo: Todo") + 1)
            .expect("parameter hover")
            .contents,
        "todo : Todo"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "label ="))
            .expect("local hover")
            .contents,
        "label : Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "todo.title") + "todo.".len())
            .expect("field hover")
            .contents,
        "title : Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "first"))
            .expect("pattern hover")
            .contents,
        "first : Todo"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "rest"))
            .expect("rest hover")
            .contents,
        "rest : List Todo"
    );
}

#[test]
fn hovers_record_update_fields() {
    let source_text = r#"type Todo = {
  done: Bool
}

mark-done(todo: Todo) : Todo
  { ...todo, done: true }
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "done: true"))
            .expect("field hover")
            .contents,
        "done : Bool"
    );
}

#[test]
fn definition_finds_local_parameter_and_binding_targets() {
    let source_text = r#"main(input: Int) : Int
  doubled =
    input + input

  doubled
"#;
    let analysis = analyze_single("main.fx", source_text);

    let input_def = analysis
        .definition_at("main", offset_of(source_text, "input +"))
        .expect("input definition");
    assert_eq!(input_def.span.start, offset_of(source_text, "input:"));

    let doubled_def = analysis
        .definition_at("main", offset_of(source_text, "doubled\n"))
        .expect("doubled definition");
    assert_eq!(doubled_def.span.start, offset_of(source_text, "doubled ="));
}

#[test]
fn definition_finds_record_update_base_targets() {
    let source_text = r#"type Todo = { done: Bool }

mark-done(todo: Todo) : Todo
  { ...todo, done: true }
"#;
    let analysis = analyze_single("main.fx", source_text);

    let todo_def = analysis
        .definition_at("main", offset_of(source_text, "...todo") + "...".len())
        .expect("todo definition");

    assert_eq!(todo_def.span.start, offset_of(source_text, "todo: Todo"));
}

#[test]
fn definition_finds_pattern_binding_targets() {
    let source_text = r#"type Todo = { id: Int }

main(todos: List Todo) : Int
  match todos
    [first, ...rest] ->
      first.id
"#;
    let analysis = analyze_single("main.fx", source_text);

    let first_def = analysis
        .definition_at("main", offset_of(source_text, "first.id"))
        .expect("first definition");
    assert_eq!(first_def.span.start, offset_of(source_text, "first,"));
}

#[test]
fn definition_finds_imported_and_qualified_symbols() {
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![
            source("examples/phase-1/lib.fx", "export value() : Int\n  1\n"),
            source(
                "examples/phase-1/main.fx",
                "import lib\n\none() : Int\n  value()\n\ntwo() : Int\n  lib.value()\n",
            ),
        ],
    );
    let main_source = analysis
        .graph
        .modules
        .get("main")
        .expect("main")
        .parsed
        .source
        .as_str();

    let imported = analysis
        .definition_at("main", offset_of(main_source, "value()"))
        .expect("imported definition");
    assert_eq!(imported.module, "lib");

    let qualified = analysis
        .definition_at("main", offset_of(main_source, "lib.value"))
        .expect("qualified definition");
    assert_eq!(qualified.module, "lib");
}

#[test]
fn definition_ignores_ambiguous_imported_symbols() {
    let analysis = analyze_workspace(
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
    let main_source = analysis
        .graph
        .modules
        .get("main")
        .expect("main")
        .parsed
        .source
        .as_str();

    assert!(
        analysis
            .definition_at("main", offset_of(main_source, "value()"))
            .is_none()
    );
}

#[test]
fn analysis_owns_parse_diagnostics_by_path() {
    let lib_path = PathBuf::from("examples/phase-1/lib.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source("examples/phase-1/lib.fx", "bad() : Int\n  if\n")],
    );

    let diagnostics = analysis.diagnostics_by_path();
    assert!(diagnostics.get(&lib_path).is_some_and(|diagnostics| {
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.module == "lib")
    }));
}

#[test]
fn analysis_owns_unresolved_imports_by_importing_path() {
    let main_path = PathBuf::from("examples/phase-1/main.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source("examples/phase-1/main.fx", "import missing\n")],
    );

    assert_path_has_diagnostic(&analysis, &main_path, "unresolved import");
}

#[test]
fn analysis_owns_duplicate_definitions_by_defining_path() {
    let lib_path = PathBuf::from("examples/phase-1/lib.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source(
            "examples/phase-1/lib.fx",
            "one() : Int\n  1\n\none() : Int\n  2\n",
        )],
    );

    assert_path_has_diagnostic(&analysis, &lib_path, "duplicate definition");
}

#[test]
fn analysis_owns_resolver_ambiguity_by_reference_path() {
    let main_path = PathBuf::from("examples/phase-1/main.fx");
    let analysis = analyze_workspace(
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

    assert_path_has_diagnostic(&analysis, &main_path, "ambiguous name `value`");
}

#[test]
fn analysis_owns_checker_type_errors_by_checked_path() {
    let lib_path = PathBuf::from("examples/phase-1/lib.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source(
            "examples/phase-1/lib.fx",
            "bad() : Int\n  \"nope\"\n",
        )],
    );

    assert_path_has_diagnostic(&analysis, &lib_path, "expected `Int`, found `Text`");
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

fn analyze_single(path: &str, source_text: &str) -> fx_core::Analysis {
    analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source(&format!("examples/phase-1/{path}"), source_text)],
    )
}

fn offset_of(source: &str, pattern: &str) -> usize {
    source.find(pattern).expect("pattern exists")
}

fn assert_path_has_diagnostic(analysis: &fx_core::Analysis, path: &PathBuf, message: &str) {
    let diagnostics = analysis.diagnostics_by_path();
    assert!(
        diagnostics.get(path).is_some_and(|diagnostics| diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(message))),
        "expected diagnostic containing `{message}` for {path:?}, got {diagnostics:?}"
    );
}

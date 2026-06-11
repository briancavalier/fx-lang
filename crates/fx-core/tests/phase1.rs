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
fn indexes_effect_and_operation_symbols() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

effect Emit a
  emit(value: a) : Unit
"#
            .to_string(),
        }],
    );

    assert_eq!(graph.diagnostics, []);
    let module = graph.modules.get("effects").expect("effects module");
    let symbols = module
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.exported))
        .collect::<Vec<_>>();

    assert!(symbols.contains(&("Raise", SymbolKind::Effect, true)));
    assert!(symbols.contains(&("raise", SymbolKind::Operation, true)));
    assert!(symbols.contains(&("Emit", SymbolKind::Effect, false)));
    assert!(symbols.contains(&("emit", SymbolKind::Operation, false)));
}

#[test]
fn resolves_effect_row_names_to_effect_declarations() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![
            SourceFile {
                path: PathBuf::from("examples/phase-3/effects.fx"),
                source: r#"export effect Raise
  raise(message: Text) : a
"#
                .to_string(),
            },
            SourceFile {
                path: PathBuf::from("examples/phase-3/use.fx"),
                source: r#"import effects

safe() : Int with <effects.Raise>
  1
"#
                .to_string(),
            },
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert_eq!(resolved.diagnostics, []);
    let module = resolved.modules.get("use").expect("use module");
    assert!(module.references.iter().any(|reference| {
        reference.name == "effects.Raise" && reference.target.kind == SymbolKind::Effect
    }));
}

#[test]
fn checks_effectful_operation_calls_against_declared_effect_rows() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
    let types = result.module_types.get("effects").expect("module types");
    assert_eq!(
        types.get("safe-divide").map(ToString::to_string),
        Some("(Int, Int) -> Int with < Raise >".to_string())
    );
}

#[test]
fn reports_unhandled_effects_in_pure_annotations() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

bad() : Int
  raise("bad")
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("unhandled effects < Raise >") })
    );
}

#[test]
fn tracks_effects_through_blocks_branches_matches_and_operators() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

type Decision = < Use(), Skip() >

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

through-let() : Int with <Raise>
  value = safe-divide(10, 0)
  value + 1

through-expression-statements() : Int with <Raise>
  safe-divide(10, 0)
  safe-divide(20, 0)
  1

through-if(flag: Bool) : Int with <Raise>
  if flag then safe-divide(10, 0) + 1 else 0

through-match(decision: Decision) : Int with <Raise>
  match decision
    Use() -> safe-divide(10, 0)
    Skip() -> 0
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_effects_from_local_bindings_in_pure_annotations() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

bad() : Int
  value = safe-divide(10, 0)
  value + 1
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("unhandled effects < Raise >") })
    );
}

#[test]
fn checks_basic_handlers_remove_handled_effects_and_type_resume() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

export effect Choose
  choose(left: a, right: a) : a

catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(_message, _resume) ->
      default

    return(value) ->
      value

choose-first(action: () -> a with <Choose, ...e>) : a with <...e>
  handle action()
    choose(left, _right, resume) ->
      resume(left)

    return(value) ->
      value
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
    let spans = result.typed_spans.get("effects").expect("typed spans");
    assert!(
        spans
            .iter()
            .any(|span| { span.label == "resume" && span.display_type == "(a) -> a" })
    );
}

#[test]
fn reports_unhandled_effects_when_handler_leaves_action_effects() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

bad(action: () -> Int with <Raise>) : Int
  handle action()
    return(value) ->
      value
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("missing handler clause for effect `Raise`")
    }));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("unhandled effects < Raise >") })
    );
}

#[test]
fn reports_duplicate_and_unknown_handler_clauses() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

bad(action: () -> Int with <Raise>) : Int
  handle action()
    raise(_message, _resume) ->
      0

    raise(_message, _resume) ->
      1

    missing(_resume) ->
      2

    return(value) ->
      value
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate handler clause `raise`")
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unknown handler operation `missing`")
    }));
}

#[test]
fn reports_handler_clauses_for_effects_not_performed_by_action() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-3"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-3/effects.fx"),
            source: r#"export effect Raise
  raise(message: Text) : a

pure() : Int
  1

bad() : Int
  handle pure()
    raise(_message, _resume) ->
      0

    return(value) ->
      value
"#
            .to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("handler clause `raise` does not handle an effect performed by the action")
    }));
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
fn checker_runs_on_phase_1_lambda_fixture() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/lambda.fx"),
            source: include_str!("../../../examples/phase-1/lambda.fx").to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checker_runs_on_phase_1_patterns_fixture() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/patterns.fx"),
            source: include_str!("../../../examples/phase-1/patterns.fx").to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checker_runs_on_phase_1_operators_fixture() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-1/operators.fx"),
            source: include_str!("../../../examples/phase-1/operators.fx").to_string(),
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
fn resolves_sibling_imports_when_indexing_parent_directory() {
    let graph = index_workspace(
        &PathBuf::from("examples"),
        vec![
            source("examples/nested/lib.fx", "export value() : Int\n  1\n"),
            source(
                "examples/nested/main.fx",
                "import lib\n\nmain() : Int\n  value()\n\nqualified() : Int\n  lib.value()\n",
            ),
        ],
    );
    let resolved = resolve_workspace(&graph);

    assert_eq!(resolved.diagnostics, []);
    let main = resolved.modules.get("nested.main").expect("main module");
    assert!(main.references.iter().any(|reference| {
        reference.name == "value" && reference.target.qualified_name == "nested.lib.value"
    }));
    assert!(main.references.iter().any(|reference| {
        reference.name == "lib.value" && reference.target.qualified_name == "nested.lib.value"
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
fn hovers_closed_record_and_variant_row_types_stably() {
    let source_text = r#"make-record() : { id: Int, done: Bool }
  { id: 1, done: false }

accept-variant(result: < Ok(Int), Missing() >) : Int
  0
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "make-record"))
            .expect("record function hover")
            .contents,
        "make-record : () -> { id: Int, done: Bool }"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "accept-variant"))
            .expect("variant function hover")
            .contents,
        "accept-variant : (< Ok(Int), Missing() >) -> Int"
    );
}

#[test]
fn hovers_open_record_and_variant_row_types() {
    let source_text = r#"greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

message(result: < Ok(a), Error(Text), ...r >) : Text
  "unknown"
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "greet"))
            .expect("record row function hover")
            .contents,
        "greet : ({ name: Text, ...r }) -> Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "person:") + 1)
            .expect("record row parameter hover")
            .contents,
        "person : { name: Text, ...r }"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "message"))
            .expect("variant row function hover")
            .contents,
        "message : (< Ok(a), Error(Text), ...r >) -> Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "result:") + 1)
            .expect("variant row parameter hover")
            .contents,
        "result : < Ok(a), Error(Text), ...r >"
    );
}

#[test]
fn checks_open_record_rows_accept_extra_fields() {
    let result = check_single(
        "rows.fx",
        r#"greet(person: { name: Text, ...r }) : Text
  person.name

main() : Text
  greet({ name: "Ada", age: 1 })
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_open_record_alias_field_access() {
    let result = check_single(
        "rows.fx",
        r#"type Person = { name: Text, ...r }

greet(person: Person) : Text
  person.name
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_open_record_field_access_through_locals() {
    let result = check_single(
        "rows.fx",
        r#"keep(person: { name: Text, ...r }) : { name: Text, ...r }
  person

main() : Text
  person =
    keep({ name: "Ada", age: 1 })

  person.name
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_open_record_field_access_in_contextual_lambdas() {
    let result = check_single(
        "rows.fx",
        r#"apply(f: { name: Text, ...r } -> Text, person: { name: Text, ...r }) : Text
  f(person)

main() : Text
  apply(person -> person.name, { name: "Ada", age: 1 })
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_unknown_open_record_fields_at_field_span() {
    let source_text = r#"bad(person: { name: Text, ...r }) : Text
  person.title
"#;
    let result = check_single("rows.fx", source_text);

    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("unknown record field `title`"))
        .expect("unknown field diagnostic");
    assert_eq!(diagnostic.span.start, offset_of(source_text, "title"));
}

#[test]
fn reports_open_record_required_field_type_mismatches() {
    let result = check_single(
        "rows.fx",
        r#"greet(person: { name: Text, ...r }) : Text
  person.name

bad() : Text
  greet({ name: 1, age: 2 })
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `Text`, found `Int`"))
    );
}

#[test]
fn reports_open_record_missing_required_fields() {
    let result = check_single(
        "rows.fx",
        r#"greet(person: { name: Text, ...r }) : Text
  person.name

bad() : Text
  greet({ age: 1 })
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record row is missing field `name`")
    }));
}

#[test]
fn checks_open_variant_rows_accept_extra_cases() {
    let result = check_single(
        "rows.fx",
        r#"type Result = < Ok(Int), Missing() >

code(result: < Ok(Int), ...r >) : Int
  0

main(result: Result) : Int
  code(result)
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_open_variant_required_case_payload_mismatches() {
    let result = check_single(
        "rows.fx",
        r#"type Result = < Ok(Text), Missing() >

code(result: < Ok(Int), ...r >) : Int
  0

bad(result: Result) : Int
  code(result)
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
fn reports_open_variant_missing_required_cases() {
    let result = check_single(
        "rows.fx",
        r#"type Result = < Missing() >

code(result: < Ok(Int), ...r >) : Int
  0

bad(result: Result) : Int
  code(result)
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("variant row is missing case `Ok`")
    }));
}

#[test]
fn row_missing_member_diagnostics_hide_internal_scopes() {
    let result = check_single(
        "rows.fx",
        r#"bad(person: { name: Text, ...r }) : { title: Text, ...r }
  person
"#,
    );

    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("record row is missing field `title`")
        })
        .expect("missing field diagnostic");
    assert!(!diagnostic.message.contains("::"));
}

#[test]
fn reports_record_expected_variant_found_row_kind_mismatches() {
    let result = check_single(
        "rows.fx",
        r#"type Result = < Ok(Int), Missing() >

bad(result: Result) : { name: Text, ...r }
  result
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "expected record row, found variant row")
    );
}

#[test]
fn reports_variant_expected_record_found_row_kind_mismatches() {
    let result = check_single(
        "rows.fx",
        r#"type Person = { name: Text, age: Int }

bad(person: Person) : < Ok(Int), ...r >
  person
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "expected variant row, found record row")
    );
}

#[test]
fn substitutes_open_record_row_tails_through_function_calls() {
    let result = check_single(
        "rows.fx",
        r#"keep(person: { name: Text, ...r }) : { name: Text, ...r }
  person

main() : { name: Text, age: Int }
  keep({ name: "Ada", age: 1 })
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn keeps_row_variables_declaration_local() {
    let result = check_single(
        "rows.fx",
        r#"keep-person(person: { name: Text, ...r }) : { name: Text, ...r }
  person

keep-todo(todo: { title: Text, ...r }) : { title: Text, ...r }
  todo

main() : { title: Text, done: Bool }
  keep-todo({ title: "Write tests", done: false })
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn keeps_type_and_row_variables_separate_across_declarations() {
    let result = check_single(
        "rows.fx",
        r#"identity(value: a) : a
  value

name(row: { name: Text, ...a }) : Text
  row.name

main() : Text
  name({ name: "Ada", age: 1 })
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn hovers_open_row_aliases_with_source_tail_names() {
    let source_text = r#"type Open = { name: Text, ...r }

same(value: Open) : Open
  value
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "same"))
            .expect("alias function hover")
            .contents,
        "same : (Open) -> Open"
    );
}

#[test]
fn reports_value_annotation_type_and_row_variable_name_conflicts() {
    let result = check_single(
        "rows.fx",
        r#"bad(value: { field: r, ...r }) : r
  value.field
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("`r` cannot be used as both a type variable and a row variable")
    }));
}

#[test]
fn reports_alias_type_and_row_variable_name_conflicts() {
    let result = check_single(
        "rows.fx",
        r#"type Bad r = { field: r, ...r }

value() : Int
  1
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("`r` cannot be used as both a type variable and a row variable")
    }));
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
fn checks_open_record_updates_to_known_fields() {
    let result = check_single(
        "record.fx",
        r#"rename(person: { name: Text, ...r }) : { name: Text, ...r }
  { ...person, name: "Grace" }
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_open_record_alias_updates_to_known_fields() {
    let result = check_single(
        "record.fx",
        r#"type Person = { name: Text, ...r }

rename(person: Person) : Person
  { ...person, name: "Grace" }
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_open_record_updates_after_row_polymorphic_calls() {
    let result = check_single(
        "record.fx",
        r#"keep(person: { name: Text, ...r }) : { name: Text, ...r }
  person

main() : { name: Text, age: Int }
  person =
    keep({ name: "Ada", age: 1 })

  { ...person, name: "Grace" }
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_open_record_update_value_type_mismatches() {
    let result = check_single(
        "record.fx",
        r#"bad(person: { name: Text, ...r }) : { name: Text, ...r }
  { ...person, name: 1 }
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `Text`, found `Int`"))
    );
}

#[test]
fn reports_unknown_open_record_update_fields_at_field_span() {
    let source_text = r#"bad(person: { name: Text, ...r }) : { name: Text, ...r }
  { ...person, age: 1 }
"#;
    let result = check_single("record.fx", source_text);

    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("unknown record field `age`"))
        .expect("unknown update field diagnostic");
    assert_eq!(diagnostic.span.start, offset_of(source_text, "age:"));
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
fn reports_duplicate_record_fields_in_aliases() {
    let result = check_single(
        "record.fx",
        r#"type Person = { name: Text, name: Int }

value() : Int
  1
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate record field `name`"))
    );
}

#[test]
fn reports_duplicate_record_fields_in_annotations() {
    let result = check_single(
        "record.fx",
        r#"bad(person: { name: Text, name: Int }) : { id: Int, id: Text }
  { id: 1 }
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate record field `name`"))
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate record field `id`"))
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
fn checks_phase_1_prelude_operators_and_unit() {
    let result = check_single(
        "operators.fx",
        r#"unit-value() : Unit
  unit

add(left: Int, right: Int) : Int
  left + right

subtract(left: Int, right: Int) : Int
  left - right

join(left: Text, right: Text) : Text
  left ++ right

same(left: Int, right: Int) : Bool
  left == right
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_add_and_subtract_operand_mismatches() {
    let result = check_single(
        "operators.fx",
        r#"bad-add(value: Text) : Int
  value + 1

bad-subtract(value: Bool) : Int
  1 - value
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("`+` expects `Int` operands, found `Text`")
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("`-` expects `Int` operands, found `Bool`")
    }));
}

#[test]
fn reports_concat_operand_mismatches() {
    let result = check_single(
        "operators.fx",
        r#"bad(value: Int) : Text
  "id " ++ value
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("`++` expects `Text` operands, found `Int`")
    }));
}

#[test]
fn reports_equality_operand_mismatches() {
    let result = check_single(
        "operators.fx",
        r#"bad(value: Int) : Bool
  value == "nope"
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot compare `Int` with `Text`")
    }));
}

#[test]
fn checks_lambda_arguments_against_expected_function_types() {
    let result = check_single(
        "lambda.fx",
        r#"apply(f: Int -> Int, value: Int) : Int
  f(value)

main() : Int
  apply(x -> x + 1, 1)
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_data_last_pipeline_calls() {
    let result = check_single(
        "pipeline.fx",
        r#"append-suffix(suffix: Text, value: Text) : Text
  value ++ suffix

wrap(prefix: Text, suffix: Text, value: Text) : Text
  prefix ++ value ++ suffix

main() : Text
  "Grace" |> append-suffix("!") |> wrap("[", "]")
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_pipeline_final_argument_mismatches() {
    let result = check_single(
        "pipeline.fx",
        r#"needs-int(prefix: Text, value: Int) : Text
  prefix

bad() : Text
  "not-int" |> needs-int("value")
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("expected `Int`, found `Text`") })
    );
}

#[test]
fn checks_prefix_with_and_flat_stacked_with() {
    let result = check_single(
        "with.fx",
        r#"surround(prefix: Text, suffix: Text, action: () -> Text) : Text
  prefix ++ action() ++ suffix

surround-value(prefix: Text, value: Text, action: () -> Text) : Text
  prefix ++ value ++ ":" ++ action()

main() : Text
  with "ctx" |> surround-value("outer")
  with surround("inner(", ")")
    "body"
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_invalid_with_targets() {
    let result = check_single(
        "with.fx",
        r#"not-handler(value: Text) : Text
  value

bad() : Text
  with not-handler("value")
    "body"
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
fn checks_calls_through_lambda_valued_locals() {
    let result = check_single(
        "lambda.fx",
        r#"make-increment() : Int -> Int
  x -> x + 1

main() : Int
  increment =
    make-increment()

  increment(1)
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_lambdas_without_expected_function_types() {
    let result = check_single(
        "lambda.fx",
        r#"bad() : Int
  x -> x
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("lambda requires an expected function type")
    }));
}

#[test]
fn reports_lambda_arity_mismatches() {
    let result = check_single(
        "lambda.fx",
        r#"apply(f: (Int, Int) -> Int) : Int
  f(1, 2)

bad() : Int
  apply(x -> x)
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
fn reports_lambda_body_result_mismatches() {
    let result = check_single(
        "lambda.fx",
        r#"apply(f: Int -> Int, value: Int) : Int
  f(value)

bad() : Int
  apply(x -> "nope", 1)
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
fn checks_constructors_against_open_variant_rows() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing(), Error(Text) >

ok() : < Ok(Int), ...r >
  Ok(1)

missing-bare() : < Missing(), ...r >
  Missing

missing-call() : < Missing(), ...r >
  Missing()
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn local_bindings_shadow_zero_arg_constructor_values() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Missing() >

bad() : Result
  Missing =
    1

  Missing
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected `Result`, found `Int`")
    }));
}

#[test]
fn checks_generic_constructors_against_open_variant_rows() {
    let result = check_single(
        "variant.fx",
        r#"type Option a = < Some(a), None() >

some-text() : < Some(Text), ...r >
  Some("text")
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_open_variant_constructor_payload_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Missing() >

bad() : < Ok(Int), ...r >
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
fn reports_constructors_missing_required_open_variant_cases() {
    let result = check_single(
        "variant.fx",
        r#"type OnlyMissing = < Missing() >

bad() : < Ok(Int), ...r >
  Missing()
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("variant row is missing case `Ok`")
    }));
}

#[test]
fn reports_duplicate_variant_cases_in_aliases() {
    let result = check_single(
        "variant.fx",
        r#"type Result = < Ok(Int), Ok(Text) >

value() : Int
  1
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate variant case `Ok`"))
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
fn checks_open_variant_pattern_constructors() {
    let result = check_single(
        "variant.fx",
        r#"message(result: < Ok(a), Missing(), Error(Text), ...r >) : Text
  match result
    Ok(_) ->
      "ok"

    Missing() ->
      "missing"

    Error(text) ->
      text

    other ->
      "unknown"
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checker_runs_on_phase_2_examples() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-2"),
        vec![SourceFile {
            path: PathBuf::from("examples/phase-2/rows.fx"),
            source: include_str!("../../../examples/phase-2/rows.fx").to_string(),
        }],
    );
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checker_runs_on_phase_2_5_examples() {
    let graph = phase_2_5_graph();
    let result = check_workspace(&graph);

    assert_eq!(result.diagnostics, []);
}

#[test]
fn resolves_phase_2_5_imported_symbols() {
    let graph = phase_2_5_graph();
    let resolved = resolve_workspace(&graph);

    assert_eq!(resolved.diagnostics, []);
    let main = resolved.modules.get("main").expect("main module");
    let imported_targets = main
        .references
        .iter()
        .map(|reference| reference.target.qualified_name.as_str())
        .collect::<Vec<_>>();

    assert!(imported_targets.contains(&"domain.mark-done"));
    assert!(imported_targets.contains(&"domain.find-person"));
    assert!(imported_targets.contains(&"domain.render"));
    assert!(imported_targets.contains(&"functions.decorate"));
    assert!(imported_targets.contains(&"functions.computed"));
}

#[test]
fn reports_unknown_open_variant_pattern_constructors() {
    let result = check_single(
        "variant.fx",
        r#"bad(result: < Ok(Int), ...r >) : Int
  match result
    Missing() ->
      0
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("constructor `Missing` is not part of `< Ok(Int), ...r >`")
    }));
}

#[test]
fn reports_unknown_pattern_constructors_when_scrutinee_type_is_unknown() {
    let result = check_single(
        "variant.fx",
        r#"bad() : Int
  match missing
    Nope(value) ->
      0
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown constructor `Nope`"))
    );
}

#[test]
fn reports_open_variant_pattern_arity_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"bad(result: < Ok(Int), ...r >) : Int
  match result
    Ok() ->
      0
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("constructor `Ok` expects 1 arguments, found 0")
    }));
}

#[test]
fn reports_open_variant_pattern_payload_mismatches() {
    let result = check_single(
        "variant.fx",
        r#"bad(result: < Ok(Int), ...r >) : Int
  match result
    Ok("nope") ->
      0
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("pattern expects `Int`, found `Text`")
    }));
}

#[test]
fn checks_record_patterns_and_literal_patterns() {
    let result = check_single(
        "patterns.fx",
        r#"type Todo = {
  title: Text,
  done: Bool
}

label(todo: Todo) : Text
  match todo
    { title, done } ->
      if done then title else "open"

bool-label(value: Bool) : Text
  match value
    true ->
      "yes"

    false ->
      "no"
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_literal_pattern_type_mismatches() {
    let result = check_single(
        "patterns.fx",
        r#"bad(value: Bool) : Text
  match value
    1 ->
      "one"
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("pattern expects `Bool`, found `Int`")
    }));
}

#[test]
fn reports_list_patterns_on_non_lists() {
    let result = check_single(
        "patterns.fx",
        r#"bad(value: Int) : Int
  match value
    [first, ...rest] ->
      first
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("list pattern requires a list"))
    );
}

#[test]
fn reports_record_patterns_on_non_records() {
    let result = check_single(
        "patterns.fx",
        r#"bad(value: Int) : Int
  match value
    { id } ->
      id
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record pattern requires a record")
    }));
}

#[test]
fn reports_record_patterns_on_variants() {
    let result = check_single(
        "patterns.fx",
        r#"type Result = < Ok(Int) >

bad(result: Result) : Int
  match result
    { id } ->
      id
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record pattern requires a record")
    }));
}

#[test]
fn reports_unknown_record_pattern_fields() {
    let result = check_single(
        "patterns.fx",
        r#"type Todo = { id: Int }

bad(todo: Todo) : Int
  match todo
    { done } ->
      done
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
fn reports_variant_patterns_on_non_variants() {
    let result = check_single(
        "patterns.fx",
        r#"type Result = < Ok(Int) >

bad(value: Int) : Int
  match value
    Ok(found) ->
      found
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("variant pattern requires a variant")
    }));
}

#[test]
fn reports_variant_patterns_on_records() {
    let result = check_single(
        "patterns.fx",
        r#"type Result = < Ok(Int) >
type Todo = { id: Int }

bad(todo: Todo) : Int
  match todo
    Ok(value) ->
      value
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("variant pattern requires a variant")
    }));
}

#[test]
fn reports_duplicate_pattern_bindings() {
    let result = check_single(
        "patterns.fx",
        r#"type Pair = < MkPair(Int, Int) >

bad(pair: Pair) : Int
  match pair
    MkPair(value, value) ->
      value
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate pattern binding `value`")
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
            .any(|diagnostic| diagnostic.message.contains("expected `Int`, found `Text`"))
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
fn hovers_effect_declarations_and_operations() {
    let source_text = r#"export effect Raise
  raise(message: Text) : a

safe() : Int with <Raise>
  raise("nope")
"#;
    let analysis = analyze_single("effects.fx", source_text);

    let effect_hover = analysis
        .hover_at("effects", offset_of(source_text, "Raise"))
        .expect("effect hover");
    assert_eq!(effect_hover.contents, "effect Raise");

    let operation_hover = analysis
        .hover_at("effects", offset_of(source_text, "raise(message"))
        .expect("operation declaration hover");
    assert_eq!(
        operation_hover.contents,
        "raise : (Text) -> a with < Raise >"
    );

    let operation_call_hover = analysis
        .hover_at("effects", offset_of(source_text, "raise(\"nope\")"))
        .expect("operation call hover");
    assert_eq!(
        operation_call_hover.contents,
        "raise : (Text) -> a with < Raise >"
    );
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
fn hovers_open_record_field_access() {
    let source_text = r#"greet(person: { name: Text, ...r }) : Text
  person.name
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at(
                "main",
                offset_of(source_text, "person.name") + "person.".len()
            )
            .expect("open record field hover")
            .contents,
        "name : Text"
    );
}

#[test]
fn hovers_open_record_alias_field_access() {
    let source_text = r#"type Person = { name: Text, ...r }

greet(person: Person) : Text
  person.name
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at(
                "main",
                offset_of(source_text, "person.name") + "person.".len()
            )
            .expect("open record alias field hover")
            .contents,
        "name : Text"
    );
}

#[test]
fn hovers_substituted_open_row_call_results() {
    let source_text = r#"keep(person: { name: Text, ...r }) : { name: Text, ...r }
  person

main() : Text
  person =
    keep({ name: "Ada", age: 1 })

  person.name
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "person ="))
            .expect("substituted local hover")
            .contents,
        "person : { name: Text, age: Int }"
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
fn hovers_open_record_update_fields() {
    let source_text = r#"rename(person: { name: Text, ...r }) : { name: Text, ...r }
  { ...person, name: "Grace" }
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "name: \"Grace\""))
            .expect("open update field hover")
            .contents,
        "name : Text"
    );
}

#[test]
fn hovers_record_pattern_fields() {
    let source_text = r#"type Todo = {
  title: Text,
  done: Bool
}

label(todo: Todo) : Text
  match todo
    { title, done } ->
      if done then title else "open"
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "title,"))
            .expect("title hover")
            .contents,
        "title : Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "done }"))
            .expect("done hover")
            .contents,
        "done : Bool"
    );
}

#[test]
fn hovers_variant_pattern_payloads() {
    let source_text = r#"type Result = < Ok(Int), Missing() >

label(result: Result) : Int
  match result
    Ok(value) ->
      value

    Missing() ->
      0
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "value)"))
            .expect("payload hover")
            .contents,
        "value : Int"
    );
}

#[test]
fn hovers_open_variant_pattern_payloads_and_fallbacks() {
    let source_text = r#"message(result: < Ok(a), Error(Text), ...r >) : Text
  match result
    Error(text) ->
      text

    other ->
      "unknown"
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "text)"))
            .expect("payload hover")
            .contents,
        "text : Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "other ->"))
            .expect("fallback hover")
            .contents,
        "other : < Ok(a), Error(Text), ...r >"
    );
}

#[test]
fn hovers_constructor_declarations() {
    let source_text = r#"type Result = < Ok(Int), Missing() >

value() : Result
  Ok(1)
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "Ok(Int)"))
            .expect("constructor hover")
            .contents,
        "Ok : (Int) -> Result"
    );
}

#[test]
fn hovers_constructor_expressions() {
    let source_text = r#"type Result = < Ok(Int), Missing() >

value() : Result
  Ok(1)
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "Ok(1)"))
            .expect("constructor expression hover")
            .contents,
        "Ok : (Int) -> Result"
    );
}

#[test]
fn hovers_phase_1_operators() {
    let source_text = r#"operators(left: Int, right: Int, text: Text) : Bool
  value =
    left + right - 1

  label =
    text ++ "!"

  value == right
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "+"))
            .expect("add hover")
            .contents,
        "+ : (Int, Int) -> Int"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "-"))
            .expect("subtract hover")
            .contents,
        "- : (Int, Int) -> Int"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "++"))
            .expect("concat hover")
            .contents,
        "++ : (Text, Text) -> Text"
    );
    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "=="))
            .expect("equality hover")
            .contents,
        "== : (Int, Int) -> Bool"
    );
}

#[test]
fn hovers_lambda_parameters() {
    let source_text = r#"apply(f: Int -> Int, value: Int) : Int
  f(value)

main() : Int
  apply(x -> x + 1, 1)
"#;
    let analysis = analyze_single("main.fx", source_text);

    assert_eq!(
        analysis
            .hover_at("main", offset_of(source_text, "x ->"))
            .expect("lambda param hover")
            .contents,
        "x : Int"
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
fn definition_finds_lambda_parameter_targets() {
    let source_text = r#"apply(f: Int -> Int, value: Int) : Int
  f(value)

main() : Int
  apply(x -> x + 1, 1)
"#;
    let analysis = analyze_single("main.fx", source_text);

    let x_def = analysis
        .definition_at("main", offset_of(source_text, "x +"))
        .expect("x definition");

    assert_eq!(x_def.span.start, offset_of(source_text, "x ->"));
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
fn definition_finds_record_pattern_binding_targets() {
    let source_text = r#"type Todo = { title: Text }

main(todo: Todo) : Text
  match todo
    { title } ->
      title
"#;
    let analysis = analyze_single("main.fx", source_text);

    let title_def = analysis
        .definition_at("main", offset_of(source_text, "title\n"))
        .expect("title definition");
    assert_eq!(title_def.span.start, offset_of(source_text, "title }"));
}

#[test]
fn definition_finds_constructor_targets() {
    let source_text = r#"type Result = < Ok(Int), Missing() >

main() : Result
  Ok(1)
"#;
    let analysis = analyze_single("main.fx", source_text);

    let ok_def = analysis
        .definition_at("main", offset_of(source_text, "Ok(1)"))
        .expect("constructor definition");
    assert_eq!(ok_def.span.start, offset_of(source_text, "Ok(Int)"));
}

#[test]
fn definition_finds_declared_constructor_pattern_targets() {
    let source_text = r#"type Result = < Ok(Int), Missing() >

main(result: Result) : Int
  match result
    Ok(value) ->
      value

    Missing() ->
      0
"#;
    let analysis = analyze_single("main.fx", source_text);

    let ok_def = analysis
        .definition_at("main", offset_of(source_text, "Ok(value)"))
        .expect("constructor pattern definition");
    assert_eq!(ok_def.span.start, offset_of(source_text, "Ok(Int)"));
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

#[test]
fn analysis_owns_lambda_diagnostics_by_checked_path() {
    let lib_path = PathBuf::from("examples/phase-1/lib.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source("examples/phase-1/lib.fx", "bad() : Int\n  x -> x\n")],
    );

    assert_path_has_diagnostic(
        &analysis,
        &lib_path,
        "lambda requires an expected function type",
    );
}

#[test]
fn analysis_owns_operator_diagnostics_by_checked_path() {
    let lib_path = PathBuf::from("examples/phase-1/lib.fx");
    let analysis = analyze_workspace(
        &PathBuf::from("examples/phase-1"),
        vec![source(
            "examples/phase-1/lib.fx",
            "bad(value: Text) : Int\n  value + 1\n",
        )],
    );

    assert_path_has_diagnostic(&analysis, &lib_path, "`+` expects `Int` operands");
}

#[test]
fn indexes_scope_and_scoped_effect_symbols() {
    let graph = index_workspace(
        &PathBuf::from("examples/phase-4"),
        vec![source(
            "examples/phase-4/state.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>
"#,
        )],
    );

    assert_eq!(graph.diagnostics, []);
    let module = graph.modules.get("state").expect("state module");
    let symbols = module
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.exported))
        .collect::<Vec<_>>();

    assert!(symbols.contains(&("State", SymbolKind::Effect, true)));
    assert!(symbols.contains(&("get", SymbolKind::Operation, true)));
    assert!(symbols.contains(&("put", SymbolKind::Operation, true)));
    assert!(symbols.contains(&("Counter", SymbolKind::Scope, false)));
}

#[test]
fn checks_explicit_scoped_operation_calls() {
    let result = check_single(
        "examples/phase-4/state.fx",
        r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

increment() : Int with <State Counter Int>
  n = get(Counter)
  put(Counter, n + 1)
  n
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_within_elided_scoped_operation_calls() {
    let result = check_single(
        "examples/phase-4/state.fx",
        r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

increment() : Int with <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_phase_4_state_goal_fixture() {
    let result = check_single(
        "examples/phase-4/state.fx",
        include_str!("../../../examples/phase-4/state.fx"),
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn checks_phase_4_polymorphic_goal_fixture() {
    let result = check_single(
        "examples/phase-4/polymorphic.fx",
        include_str!("../../../examples/phase-4/polymorphic.fx"),
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_scope_protocol_must_name_scoped_effect_family() {
    let result = check_single(
        "examples/phase-4/bad-protocol.fx",
        r#"effect Console
  log(message: Text) : Unit

scope BadConsole with <Console>
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scope protocol row must name a scoped effect family")
    }));
}

#[test]
fn reports_scope_protocol_argument_arity() {
    let result = check_single(
        "examples/phase-4/bad-protocol.fx",
        r#"scoped effect State s a
  get() : a

scope Missing with <State>
scope Extra with <State Int Text>
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scope protocol `State` expects 1 argument, found 0")
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scope protocol `State` expects 1 argument, found 2")
    }));
}

#[test]
fn reports_multiple_scope_protocol_entries_for_v0() {
    let result = check_single(
        "examples/phase-4/bad-protocol.fx",
        r#"scoped effect State s a
  get() : a

scoped effect Emit s a
  emit(value: a) : Unit

scope Mixed with <State Int, Emit Text>
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scope protocol rows support one scoped effect family in Phase 4")
    }));
}

#[test]
fn scoped_handlers_remove_only_the_target_scope_effect() {
    let result = check_single(
        "examples/phase-4/state.fx",
        r#"scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>
scope OtherCounter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

handled-counter() : Int
  with with-state(Counter, 0)
    within Counter
      n = get()
      put(n + 1)
      n

other-counter-still-unhandled() : Int with <State OtherCounter Int>
  with with-state(Counter, 0)
    within Counter
      get()
      get(OtherCounter)
"#,
    );

    assert_eq!(result.diagnostics, []);
}

#[test]
fn reports_ambiguous_elided_scoped_operation() {
    let result = check_single(
        "examples/phase-4/ambiguous.fx",
        r#"scoped effect State s a
  get() : a

scope Counter with <State Int>
scope Session with <State Int>

ambiguous-elided-operation() : Int with <State Counter Int, State Session Int>
  within Counter
    within Session
      get()
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("ambiguous elided scoped operation")
    }));
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

fn phase_2_5_graph() -> fx_core::ModuleGraph {
    index_workspace(
        &PathBuf::from("examples/phase-2.5"),
        vec![
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/domain.fx"),
                source: include_str!("../../../examples/phase-2.5/domain.fx").to_string(),
            },
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/functions.fx"),
                source: include_str!("../../../examples/phase-2.5/functions.fx").to_string(),
            },
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/main.fx"),
                source: include_str!("../../../examples/phase-2.5/main.fx").to_string(),
            },
        ],
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

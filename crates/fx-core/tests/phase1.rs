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
            .hover_at("main", offset_of(source_text, "message"))
            .expect("variant row function hover")
            .contents,
        "message : (< Ok(a), Error(Text), ...r >) -> Text"
    );
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

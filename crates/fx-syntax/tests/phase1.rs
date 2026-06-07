use fx_syntax::ast::{Expr, Item, Pattern, TypeExpr};
use fx_syntax::parse_file;

#[test]
fn parses_phase_1_todo_goal_fixture() {
    let parsed = parse_file(include_str!("../../../examples/phase-1/todo.fx"));

    assert_eq!(parsed.diagnostics, []);
    assert_eq!(parsed.module.items.len(), 4);
    assert!(matches!(parsed.module.items[0], Item::Type(_)));
    assert!(matches!(parsed.module.items[1], Item::Type(_)));
    assert!(matches!(parsed.module.items[2], Item::Value(_)));
    assert!(matches!(parsed.module.items[3], Item::Value(_)));
}

#[test]
fn parses_closed_record_and_variant_types() {
    let parsed = parse_file(
        r#"export type Todo = {
  id: Int,
  done: Bool
}

export type TodoResult a = < Ok(a), NotFound(Int) >
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Type(record) = &parsed.module.items[0] else {
        panic!("expected record type");
    };
    assert!(matches!(record.body, TypeExpr::Record { tail: None, .. }));

    let Item::Type(variant) = &parsed.module.items[1] else {
        panic!("expected variant type");
    };
    assert!(matches!(variant.body, TypeExpr::Variant { tail: None, .. }));
}

#[test]
fn parses_open_record_rows() {
    let source = "type Person r = { name: Text, ...r }\n";
    let parsed = parse_file(source);

    assert_eq!(parsed.diagnostics, []);
    let Item::Type(record) = &parsed.module.items[0] else {
        panic!("expected record type");
    };
    let TypeExpr::Record { fields, tail, .. } = &record.body else {
        panic!("expected record body");
    };
    assert_eq!(fields.len(), 1);
    let tail = tail.as_ref().expect("expected row tail");
    assert_eq!(tail.name.text, "r");
    assert_eq!(&source[tail.span.start..tail.span.end], "...r");
    assert_eq!(&source[tail.name.span.start..tail.name.span.end], "r");
}

#[test]
fn parses_open_variant_rows() {
    let source = "type Result a r = < Ok(a), Error(Text), ...r >\n";
    let parsed = parse_file(source);

    assert_eq!(parsed.diagnostics, []);
    let Item::Type(variant) = &parsed.module.items[0] else {
        panic!("expected variant type");
    };
    let TypeExpr::Variant { cases, tail, .. } = &variant.body else {
        panic!("expected variant body");
    };
    assert_eq!(cases.len(), 2);
    let tail = tail.as_ref().expect("expected row tail");
    assert_eq!(tail.name.text, "r");
    assert_eq!(&source[tail.span.start..tail.span.end], "...r");
    assert_eq!(&source[tail.name.span.start..tail.name.span.end], "r");
}

#[test]
fn parses_row_tails_with_trailing_commas() {
    let parsed = parse_file(
        r#"type Person r = { name: Text, ...r, }
type Result r = < Ok(Text), ...r, >
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Type(record) = &parsed.module.items[0] else {
        panic!("expected record type");
    };
    assert!(matches!(
        record.body,
        TypeExpr::Record { tail: Some(_), .. }
    ));

    let Item::Type(variant) = &parsed.module.items[1] else {
        panic!("expected variant type");
    };
    assert!(matches!(
        variant.body,
        TypeExpr::Variant { tail: Some(_), .. }
    ));
}

#[test]
fn reports_non_final_row_spreads() {
    let parsed = parse_file(
        r#"type Person r = { name: Text, ...r, age: Int }
type Result r = < Ok(Text), ...r, Error(Text) >
"#,
    );

    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("row spread must appear last"))
            .count(),
        2
    );
}

#[test]
fn reports_duplicate_row_spreads() {
    let parsed = parse_file(
        r#"type Person r s = { name: Text, ...r, ...s }
type Result r s = < Ok(Text), ...r, ...s >
"#,
    );

    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("duplicate row spread"))
            .count(),
        2
    );
}

#[test]
fn reports_missing_row_tail_names() {
    let parsed = parse_file(
        r#"type Person r = { name: Text, ... }
type Result r = < Ok(Text), ... >
"#,
    );

    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("expected row variable name after `...`"))
            .count(),
        2
    );
}

#[test]
fn reports_standalone_row_spreads() {
    let parsed = parse_file("type Tail r = ...r\n");

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("row spread is only valid in record or variant types")
    }));
}

#[test]
fn parses_phase_2_rows_goal_fixture() {
    let parsed = parse_file(include_str!("../../../examples/phase-2/rows.fx"));

    assert_eq!(parsed.diagnostics, []);
}

#[test]
fn parses_phase_2_5_executable_fixtures() {
    for source in [
        include_str!("../../../examples/phase-2.5/domain.fx"),
        include_str!("../../../examples/phase-2.5/functions.fx"),
        include_str!("../../../examples/phase-2.5/main.fx"),
    ] {
        let parsed = parse_file(source);

        assert_eq!(parsed.diagnostics, []);
    }
}

#[test]
fn parses_match_with_list_rest_pattern() {
    let parsed = parse_file(
        r#"head(xs: List Int) : Int
  match xs
    [x, ...rest] ->
      x
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    assert!(matches!(value.body.result.as_ref(), Expr::Match { .. }));
}

#[test]
fn parses_binary_operator_spans() {
    let source = r#"add(left: Int, right: Int) : Int
  left + right
"#;
    let parsed = parse_file(source);

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::Binary { op_span, .. } = value.body.result.as_ref() else {
        panic!("expected binary expression");
    };
    assert_eq!(&source[op_span.start..op_span.end], "+");
}

#[test]
fn parses_phase_1_pattern_shapes() {
    let parsed = parse_file(
        r#"type Todo = { title: Text, done: Bool }
type Result = < Ok(Todo), Missing(Int) >

label(result: Result) : Text
  match result
    Ok({ title, done }) ->
      title

    Missing(0) ->
      "zero"

    Missing(unit) ->
      "unit"
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[2] else {
        panic!("expected value");
    };
    let Expr::Match { arms, .. } = value.body.result.as_ref() else {
        panic!("expected match");
    };
    assert!(matches!(
        &arms[0].pattern,
        Pattern::Constructor { args, .. } if matches!(args.first(), Some(Pattern::Record { .. }))
    ));
    assert!(matches!(
        &arms[1].pattern,
        Pattern::Constructor { args, .. } if matches!(args.first(), Some(Pattern::Int { .. }))
    ));
    assert!(matches!(
        &arms[2].pattern,
        Pattern::Constructor { args, .. } if matches!(args.first(), Some(Pattern::Unit { .. }))
    ));
}

#[test]
fn parses_record_update() {
    let parsed = parse_file(
        r#"mark-done(todo: Todo) : Todo
  { ...todo, done: true }
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    assert!(matches!(
        value.body.result.as_ref(),
        Expr::RecordUpdate { .. }
    ));
}

#[test]
fn parses_single_param_lambda() {
    let parsed = parse_file(
        r#"apply(f: Int -> Int) : Int
  f(x -> x + 1)
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::Call { args, .. } = value.body.result.as_ref() else {
        panic!("expected call");
    };
    assert!(matches!(args.first(), Some(Expr::Lambda { .. })));
}

#[test]
fn parses_multi_param_lambda() {
    let parsed = parse_file(
        r#"apply(f: (Int, Int) -> Int) : Int
  f((x, y) -> x + y)
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::Call { args, .. } = value.body.result.as_ref() else {
        panic!("expected call");
    };
    assert!(matches!(args.first(), Some(Expr::Lambda { .. })));
}

#[test]
fn parses_block_lambda_body() {
    let parsed = parse_file(
        r#"apply(f: Int -> Int) : Int
  f(x ->
    y =
      x + 1

    y
  )
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::Call { args, .. } = value.body.result.as_ref() else {
        panic!("expected call");
    };
    let Some(Expr::Lambda { body, .. }) = args.first() else {
        panic!("expected lambda");
    };
    assert_eq!(body.statements.len(), 1);
}

#[test]
fn parses_function_type_annotations() {
    let parsed = parse_file(
        r#"compose(f: Int -> Text, g: (Bool, Int) -> Int) : Bool -> Text
  x -> f(g(x, 1))
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    assert!(matches!(value.annotation, TypeExpr::Function { .. }));
}

#[test]
fn reports_unsupported_lambda_rest_params() {
    let parsed = parse_file(
        r#"bad(f: Int -> Int) : Int
  f((_, ...rest) -> 1)
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("lambda rest or pattern parameters are not supported in Phase 1")
    }));
}

#[test]
fn parses_multiline_record_update() {
    let parsed = parse_file(
        r#"mark-done(todo: Todo) : Todo
  {
    ...todo,
    done: true
  }
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    assert!(matches!(
        value.body.result.as_ref(),
        Expr::RecordUpdate { .. }
    ));
}

#[test]
fn reports_record_update_without_fields() {
    let parsed = parse_file(
        r#"same(todo: Todo) : Todo
  { ...todo }
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record update requires at least one field")
    }));
}

#[test]
fn reports_record_update_spread_not_first() {
    let parsed = parse_file(
        r#"bad(todo: Todo) : Todo
  { done: true, ...todo }
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("record update spread must appear first")
    }));
}

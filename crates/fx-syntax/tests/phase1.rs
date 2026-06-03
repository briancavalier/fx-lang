use fx_syntax::ast::{Expr, Item, TypeExpr};
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
    assert!(matches!(record.body, TypeExpr::Record { .. }));

    let Item::Type(variant) = &parsed.module.items[1] else {
        panic!("expected variant type");
    };
    assert!(matches!(variant.body, TypeExpr::Variant { .. }));
}

#[test]
fn reports_later_phase_open_rows() {
    let parsed = parse_file("type Person r = { name: Text, ...r }\n");

    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Phase 2"))
    );
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

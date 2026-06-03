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

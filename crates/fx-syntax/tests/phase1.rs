use fx_syntax::ast::{Expr, Item, Pattern, Stmt, TypeExpr};
use fx_syntax::{SyntaxTokenKind, lex_source, parse_file};

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
fn lexical_stream_exposes_comments_and_trivia_for_tooling() {
    let source = "# file comment\nvalue() : Int  # trailing\n  1\n";
    let lexed = lex_source(source);

    assert_eq!(lexed.diagnostics, []);
    let comments = lexed
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, SyntaxTokenKind::Comment))
        .collect::<Vec<_>>();
    assert_eq!(comments.len(), 2);
    assert_eq!(
        &source[comments[0].span.start..comments[0].span.end],
        "# file comment"
    );
    assert_eq!(
        &source[comments[1].span.start..comments[1].span.end],
        "# trailing"
    );
    assert!(
        lexed
            .tokens
            .iter()
            .any(|token| matches!(token.kind, SyntaxTokenKind::Whitespace))
    );
    assert!(
        lexed
            .tokens
            .iter()
            .any(|token| matches!(token.kind, SyntaxTokenKind::Indent))
    );
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
fn parses_phase_2_6_pipeline_and_with_fixtures() {
    for source in [
        include_str!("../../../examples/phase-2.6/support.fx"),
        include_str!("../../../examples/phase-2.6/main.fx"),
    ] {
        let parsed = parse_file(source);

        assert_eq!(parsed.diagnostics, []);
    }
}

#[test]
fn parses_effect_declarations() {
    let parsed = parse_file(
        r#"export effect Raise
  raise(message: Text) : a

effect Emit a
  emit(value: a) : Unit
  done() : Unit
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Effect(raise) = &parsed.module.items[0] else {
        panic!("expected effect declaration");
    };
    assert!(raise.export);
    assert_eq!(raise.name.text, "Raise");
    assert_eq!(raise.operations.len(), 1);
    assert_eq!(raise.operations[0].name.text, "raise");
    assert_eq!(raise.operations[0].params.len(), 1);

    let Item::Effect(emit) = &parsed.module.items[1] else {
        panic!("expected effect declaration");
    };
    assert!(!emit.export);
    assert_eq!(emit.name.text, "Emit");
    assert_eq!(emit.params.len(), 1);
    assert_eq!(emit.operations.len(), 2);
}

#[test]
fn parses_scoped_effect_and_scope_declarations() {
    let parsed = parse_file(
        r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>
scope Session
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Effect(state) = &parsed.module.items[0] else {
        panic!("expected scoped effect declaration");
    };
    assert!(state.export);
    assert!(state.scoped);
    assert_eq!(state.name.text, "State");
    assert_eq!(state.params.len(), 2);
    assert_eq!(state.params[0].text, "s");
    assert_eq!(state.operations.len(), 2);

    let Item::Scope(counter) = &parsed.module.items[1] else {
        panic!("expected scope declaration");
    };
    assert_eq!(counter.name.text, "Counter");
    let protocol = counter.protocol.as_ref().expect("expected protocol row");
    assert_eq!(protocol.effects.len(), 1);
    assert!(protocol.tail.is_none());

    let Item::Scope(session) = &parsed.module.items[2] else {
        panic!("expected bare scope declaration");
    };
    assert_eq!(session.name.text, "Session");
    assert!(session.protocol.is_none());
}

#[test]
fn reports_scoped_effect_without_scope_parameter() {
    let parsed = parse_file(
        r#"scoped effect Bad
  bad() : Unit
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scoped effect declarations require a scope parameter")
    }));
}

#[test]
fn parses_effect_rows_in_result_and_function_types() {
    let parsed = parse_file(
        r#"safe-divide(x: Int, y: Int) : Int with <Raise>
  x

catch(action: () -> a with <Raise, ...e>) : a with <...e>
  action()
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(divide) = &parsed.module.items[0] else {
        panic!("expected value declaration");
    };
    assert!(matches!(divide.annotation, TypeExpr::Effect { .. }));

    let Item::Value(catch) = &parsed.module.items[1] else {
        panic!("expected value declaration");
    };
    let TypeExpr::Function { result, .. } = &catch.params[0].annotation else {
        panic!("expected function parameter type");
    };
    assert!(matches!(result.as_ref(), TypeExpr::Effect { .. }));
    let TypeExpr::Effect { effects, .. } = &catch.annotation else {
        panic!("expected effectful return type");
    };
    assert_eq!(effects.effects.len(), 0);
    assert!(effects.tail.is_some());
}

#[test]
fn parses_scoped_effect_rows() {
    let parsed = parse_file(
        r#"increment() : Int with <State Counter Int, ...e>
  get(Counter)
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(increment) = &parsed.module.items[0] else {
        panic!("expected value declaration");
    };
    let TypeExpr::Effect { effects, .. } = &increment.annotation else {
        panic!("expected effectful return type");
    };
    assert_eq!(effects.effects.len(), 1);
    assert!(effects.tail.is_some());
    assert!(matches!(effects.effects[0], TypeExpr::App { .. }));
}

#[test]
fn parses_within_expressions() {
    let parsed = parse_file(
        r#"increment() : Int with <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(increment) = &parsed.module.items[0] else {
        panic!("expected value declaration");
    };
    let Expr::Within { scope, body, .. } = increment.body.result.as_ref() else {
        panic!("expected within expression");
    };
    assert_eq!(scope.text, "Counter");
    assert_eq!(body.statements.len(), 2);
}

#[test]
fn reports_old_scope_protocol_colon_spelling() {
    let parsed = parse_file("scope Counter : State Int\n");

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("scope protocol rows use `with <...>`")
    }));
}

#[test]
fn parses_phase_4_goal_fixtures() {
    for source in [
        include_str!("../../../examples/phase-4/state.fx"),
        include_str!("../../../examples/phase-4/polymorphic.fx"),
        include_str!("../../../examples/phase-4/diagnostics.fx"),
    ] {
        let parsed = parse_file(source);

        assert_eq!(parsed.diagnostics, []);
    }
}

#[test]
fn parses_context_rows_in_result_and_function_types() {
    let parsed = parse_file(
        r#"timestamp(message: Text) : Text given { clock: Clock }
  message

with-clock(action: () -> a given { clock: Clock, ...c }) : a given { ...c }
  action()

log-time(message: Text) : Unit with <Console, ...e> given { clock: Clock, ...c }
  unit
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(timestamp) = &parsed.module.items[0] else {
        panic!("expected timestamp value");
    };
    let TypeExpr::Context { context, .. } = &timestamp.annotation else {
        panic!("expected contextual return type");
    };
    assert_eq!(context.fields.len(), 1);
    assert!(context.tail.is_none());

    let Item::Value(with_clock) = &parsed.module.items[1] else {
        panic!("expected with-clock value");
    };
    let TypeExpr::Function { result, .. } = &with_clock.params[0].annotation else {
        panic!("expected contextual function parameter type");
    };
    assert!(matches!(result.as_ref(), TypeExpr::Context { .. }));
    let TypeExpr::Context { context, .. } = &with_clock.annotation else {
        panic!("expected contextual return type");
    };
    assert!(context.tail.is_some());

    let Item::Value(log_time) = &parsed.module.items[2] else {
        panic!("expected log-time value");
    };
    let TypeExpr::Context {
        result, context, ..
    } = &log_time.annotation
    else {
        panic!("expected contextual effectful return type");
    };
    assert!(matches!(result.as_ref(), TypeExpr::Effect { .. }));
    assert_eq!(context.fields.len(), 1);
    assert!(context.tail.is_some());
}

#[test]
fn reports_malformed_context_row_tails() {
    let parsed = parse_file(
        r#"bad() : Text given { clock: Clock, ...c, locale: Locale }
  ""

also-bad() : Text given { clock: Clock, ...c, ...d }
  ""
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("context row spread must appear last")
    }));
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("duplicate context row spread") })
    );
}

#[test]
fn parses_given_provider_expressions() {
    let parsed = parse_file(
        r#"main() : Text
  given { clock = system-clock(), config = read-config() }
    timestamp("started")
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(main) = &parsed.module.items[0] else {
        panic!("expected main value");
    };
    let Expr::Given { fields, body, .. } = main.body.result.as_ref() else {
        panic!("expected given expression");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name.text, "clock");
    assert_eq!(fields[1].name.text, "config");
    assert!(matches!(body.result.as_ref(), Expr::Call { .. }));
}

#[test]
fn reports_deferred_phase_5_given_forms() {
    let parsed = parse_file(
        r#"given clock = system-clock()

main() : Text
  given each { clock = system-clock() }
    timestamp("started")

call-site() : Text
  timestamp("started") given { clock = system-clock() }
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("module-level contextual exports are not supported in Phase 5")
    }));
    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected `(` after `given each`")
    }));
    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("final given arguments are not supported in Phase 5")
    }));
}

#[test]
fn parses_phase_6b_resourceful_boundaries() {
    let parsed = parse_file(
        r#"type Clock = {
  now: Text
}

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ message

plain-finally() : Text
  finally(() -> unit)
    "done"

branch-context() : Text
  given each(() -> { clock: { now: "10:00" } })
    stamp(" started")

branch-context-cleanup() : Text
  given each(() -> { clock: { now: "10:00" } }, context -> unit)
    stamp(" started")
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(finally_decl) = &parsed.module.items[2] else {
        panic!("expected finally value declaration");
    };
    assert!(matches!(
        finally_decl.body.result.as_ref(),
        Expr::Finally { .. }
    ));
    let Item::Value(given_each_decl) = &parsed.module.items[3] else {
        panic!("expected given each value declaration");
    };
    assert!(matches!(
        given_each_decl.body.result.as_ref(),
        Expr::GivenEach { cleanup: None, .. }
    ));
    let Item::Value(cleanup_decl) = &parsed.module.items[4] else {
        panic!("expected given each cleanup value declaration");
    };
    assert!(matches!(
        cleanup_decl.body.result.as_ref(),
        Expr::GivenEach {
            cleanup: Some(_),
            ..
        }
    ));
}

#[test]
fn parses_phase_5_goal_fixtures() {
    for source in [
        include_str!("../../../examples/phase-5/basic.fx"),
        include_str!("../../../examples/phase-5/effects.fx"),
        include_str!("../../../examples/phase-5/with-clock.fx"),
        include_str!("../../../examples/phase-5/higher-order.fx"),
        include_str!("../../../examples/phase-5/diagnostics.fx.txt"),
    ] {
        let parsed = parse_file(source);

        assert_eq!(parsed.diagnostics, []);
    }
}

#[test]
fn reports_malformed_effect_row_tails() {
    let parsed = parse_file(
        r#"bad() : Int with <Raise, ...e, Other>
  1

also-bad() : Int with <Raise, ...e, ...f>
  1
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("effect row spread must appear last")
    }));
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("duplicate effect row spread") })
    );
}

#[test]
fn parses_handle_expressions_with_operation_and_return_clauses() {
    let parsed = parse_file(
        r#"catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(message, resume) ->
      default

    return(value) ->
      value
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value declaration");
    };
    let Expr::Handle {
        action,
        clauses,
        return_clause,
        ..
    } = value.body.result.as_ref()
    else {
        panic!("expected handle expression");
    };
    assert!(matches!(action.as_ref(), Expr::Call { .. }));
    assert_eq!(clauses.len(), 1);
    assert_eq!(clauses[0].operation.text, "raise");
    assert_eq!(clauses[0].params.len(), 2);
    let return_clause = return_clause.as_ref().expect("expected return clause");
    assert_eq!(return_clause.param.text, "value");
}

#[test]
fn reports_malformed_effect_operation_signatures() {
    let parsed = parse_file(
        r#"effect Bad
  missing-colon(value Text) Unit
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected `:` after parameter name")
    }));
    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected operation result type annotation")
    }));
}

#[test]
fn parses_pipeline_as_left_associative_data_last_sugar() {
    let parsed = parse_file(
        r#"main() : Text
  "Grace" |> append-suffix("!") |> wrap("[", "]")
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::Pipeline {
        value: left,
        target,
        ..
    } = value.body.result.as_ref()
    else {
        panic!("expected outer pipeline");
    };
    assert!(matches!(left.as_ref(), Expr::Pipeline { .. }));
    assert!(matches!(target.as_ref(), Expr::Call { .. }));
}

#[test]
fn parses_prefix_with_and_flat_stacked_with_headers() {
    let parsed = parse_file(
        r#"main() : Text
  with "ctx" |> surround-value("outer")
  with surround("inner(", ")")
    "body"
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    let Expr::With { targets, body, .. } = value.body.result.as_ref() else {
        panic!("expected with expression");
    };
    assert_eq!(targets.len(), 2);
    assert!(matches!(targets[0].target, Expr::Pipeline { .. }));
    assert!(matches!(body.result.as_ref(), Expr::Text { .. }));
}

#[test]
fn rejects_postfix_with() {
    let parsed = parse_file(
        r#"main() : Text
  "Grace" |> append-suffix("!") with
    "ignored"
"#,
    );

    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("postfix `with` is not supported")
    }));
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
fn parses_expression_statements_before_block_result() {
    let parsed = parse_file(
        r#"run() : Unit
  first()
  second()
  unit
"#,
    );

    assert_eq!(parsed.diagnostics, []);
    let Item::Value(value) = &parsed.module.items[0] else {
        panic!("expected value");
    };
    assert_eq!(value.body.statements.len(), 2);
    assert!(matches!(value.body.statements[0], Stmt::Expr(_)));
    assert!(matches!(value.body.statements[1], Stmt::Expr(_)));
    assert!(matches!(value.body.result.as_ref(), Expr::Unit { .. }));
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

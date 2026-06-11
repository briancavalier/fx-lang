use crate::ast::{
    BinaryOp, Block, EffectDecl, EffectRow, Expr, FieldExpr, FieldType, HandlerClause,
    HandlerReturnClause, ImportDecl, Item, LambdaParam, LetStmt, MatchArm, Name, OperationDecl,
    Param, Pattern, RowTail, ScopeDecl, Stmt, TypeDecl, TypeExpr, ValueDecl, VariantCaseType,
    WithTarget,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{SyntaxTokenKind, lex_source};
use crate::parser::parse_file;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct FormattedModule {
    pub source: String,
    pub changed: bool,
}

pub fn format_module(source: &str) -> Result<FormattedModule, Vec<Diagnostic>> {
    let parsed = parse_file(source);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }

    let formatted = format_parsed_module(source, &parsed.module.items);
    let changed = formatted != source;
    Ok(FormattedModule {
        source: formatted,
        changed,
    })
}

fn format_parsed_module(source: &str, items: &[Item]) -> String {
    let comments = lex_source(source)
        .tokens
        .into_iter()
        .filter(|token| matches!(token.kind, SyntaxTokenKind::Comment))
        .map(|token| token.span)
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut previous_end = 0usize;
    let mut previous_item = None;
    let mut index = 0usize;

    while index < items.len() {
        let item = &items[index];
        let leading_comments = leading_comments(source, previous_end, item.span().start);
        if !output.is_empty() && should_separate_items(previous_item, item, &leading_comments) {
            output.push('\n');
        }
        if !leading_comments.is_empty() {
            output.push_str(&leading_comments);
        }

        if matches!(item, Item::Import(_)) {
            if !leading_comments.is_empty() {
                let end = collect_contiguous_import_end(items, index);
                format_import_run_preserving_order(&mut output, source, &items[index..end]);
                previous_end = items[end - 1].span().end;
                previous_item = Some(&items[end - 1]);
                index = end;
                continue;
            }
            let end = collect_import_run_end(source, items, index, previous_end);
            if end > index + 1 {
                format_import_run(&mut output, &items[index..end]);
                previous_end = items[end - 1].span().end;
                previous_item = Some(&items[end - 1]);
                index = end;
                continue;
            }
        }

        if comments
            .iter()
            .any(|comment| span_contains(item.span(), *comment))
        {
            let item_source = line_slice_for_span(source, item.span());
            output.push_str(&normalize_file_whitespace(item_source).trim_end());
            output.push('\n');
        } else {
            format_item(&mut output, item);
        }
        previous_end = item.span().end;
        previous_item = Some(item);
        index += 1;
    }
    let trailing_comments = leading_comments(source, previous_end, source.len());
    if !trailing_comments.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&trailing_comments);
    }

    output
}

fn collect_import_run_end(
    source: &str,
    items: &[Item],
    start: usize,
    previous_end: usize,
) -> usize {
    let mut end = start;
    let mut cursor = previous_end;
    while end < items.len() {
        if !matches!(items[end], Item::Import(_)) {
            break;
        }
        if !leading_comments(source, cursor, items[end].span().start).is_empty() {
            break;
        }
        cursor = items[end].span().end;
        end += 1;
    }
    end
}

fn collect_contiguous_import_end(items: &[Item], start: usize) -> usize {
    let mut end = start;
    while end < items.len() && matches!(items[end], Item::Import(_)) {
        end += 1;
    }
    end
}

fn format_import_run_preserving_order(output: &mut String, source: &str, items: &[Item]) {
    let mut previous_end = items.first().map_or(0, |item| item.span().start);
    for item in items {
        let leading_comments = leading_comments(source, previous_end, item.span().start);
        if !leading_comments.is_empty() {
            output.push_str(&leading_comments);
        }
        format_item(output, item);
        previous_end = item.span().end;
    }
}

fn format_import_run(output: &mut String, items: &[Item]) {
    let mut imports = items
        .iter()
        .filter_map(|item| match item {
            Item::Import(import) => Some(import.path.join(".")),
            _ => None,
        })
        .collect::<Vec<_>>();

    if import_basenames_are_unique(&imports) {
        imports.sort();
    }

    for import in imports {
        output.push_str("import ");
        output.push_str(&import);
        output.push('\n');
    }
}

fn import_basenames_are_unique(imports: &[String]) -> bool {
    let mut basenames = std::collections::BTreeSet::new();
    imports
        .iter()
        .all(|import| basenames.insert(import.rsplit('.').next().unwrap_or(import)))
}

fn should_separate_items(previous: Option<&Item>, current: &Item, leading_comments: &str) -> bool {
    if !leading_comments.is_empty() {
        return true;
    }
    !matches!(
        (previous, current),
        (Some(Item::Import(_)), Item::Import(_))
    )
}

fn normalize_file_whitespace(source: &str) -> String {
    let mut output = String::new();
    let mut pending_blank_line = false;

    for raw_line in source.lines() {
        let line = raw_line.trim_end_matches([' ', '\t', '\r']);
        if line.is_empty() {
            pending_blank_line = !output.is_empty();
            continue;
        }

        if pending_blank_line {
            output.push('\n');
            pending_blank_line = false;
        }
        output.push_str(line);
        output.push('\n');
    }

    output
}

fn leading_comments(source: &str, start: usize, end: usize) -> String {
    let mut output = String::new();
    let mut saw_comment = false;
    let mut blank_after_comment = false;

    for line in source[start..end].lines() {
        let line = line.trim_end_matches([' ', '\t', '\r']);
        if line.trim_start().starts_with('#') {
            output.push_str(line);
            output.push('\n');
            saw_comment = true;
            blank_after_comment = false;
        } else if saw_comment && line.trim().is_empty() {
            blank_after_comment = true;
        }
    }

    if blank_after_comment && !output.is_empty() {
        output.push('\n');
    }

    output
}

fn line_slice_for_span(source: &str, span: Span) -> &str {
    let start = line_start(source, span.start);
    let end = line_end_including_newline(source, span.end);
    &source[start..end]
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn line_end_including_newline(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    if offset > 0 && line_start(source, offset) == offset {
        return offset;
    }
    source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index + 1)
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn format_item(output: &mut String, item: &Item) {
    match item {
        Item::Import(item) => format_import(output, item),
        Item::Type(item) => format_type_decl(output, item),
        Item::Effect(item) => format_effect_decl(output, item),
        Item::Scope(item) => format_scope_decl(output, item),
        Item::Value(item) => format_value_decl(output, item),
    }
}

fn format_import(output: &mut String, item: &ImportDecl) {
    output.push_str("import ");
    output.push_str(&item.path.join("."));
    output.push('\n');
}

fn format_type_decl(output: &mut String, item: &TypeDecl) {
    if item.export {
        output.push_str("export ");
    }
    output.push_str("type ");
    output.push_str(&item.name.text);
    format_type_params(output, &item.params);
    output.push_str(" = ");
    output.push_str(&format_type(&item.body));
    output.push('\n');
}

fn format_effect_decl(output: &mut String, item: &EffectDecl) {
    if item.export {
        output.push_str("export ");
    }
    if item.scoped {
        output.push_str("scoped effect ");
    } else {
        output.push_str("effect ");
    }
    output.push_str(&item.name.text);
    format_type_params(output, &item.params);
    output.push('\n');
    for operation in &item.operations {
        output.push_str("  ");
        format_operation_decl(output, operation);
    }
}

fn format_scope_decl(output: &mut String, item: &ScopeDecl) {
    if item.export {
        output.push_str("export ");
    }
    output.push_str("scope ");
    output.push_str(&item.name.text);
    if let Some(protocol) = &item.protocol {
        output.push_str(" with ");
        output.push_str(&format_effect_row(protocol));
    }
    output.push('\n');
}

fn format_operation_decl(output: &mut String, operation: &OperationDecl) {
    output.push_str(&operation.name.text);
    output.push('(');
    output.push_str(&format_params(&operation.params));
    output.push_str(") : ");
    output.push_str(&format_type(&operation.result));
    output.push('\n');
}

fn format_value_decl(output: &mut String, item: &ValueDecl) {
    if item.export {
        output.push_str("export ");
    }
    output.push_str(&item.name.text);
    output.push('(');
    output.push_str(&format_params(&item.params));
    output.push_str(") : ");
    output.push_str(&format_type(&item.annotation));
    output.push('\n');
    output.push_str(&format_block(&item.body, 1));
}

fn format_type_params(output: &mut String, params: &[Name]) {
    for param in params {
        output.push(' ');
        output.push_str(&param.text);
    }
}

fn format_params(params: &[Param]) -> String {
    params
        .iter()
        .map(|param| format!("{}: {}", param.name.text, format_type(&param.annotation)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named { path, .. } => path
            .iter()
            .map(|name| name.text.as_str())
            .collect::<Vec<_>>()
            .join("."),
        TypeExpr::Var(name) => name.text.clone(),
        TypeExpr::App { callee, args, .. } => {
            let args = args
                .iter()
                .map(format_type_atom)
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} {}", format_type_atom(callee), args)
        }
        TypeExpr::Function { params, result, .. } => {
            let params = if params.len() == 1 {
                format_type_atom(&params[0])
            } else {
                format!(
                    "({})",
                    params
                        .iter()
                        .map(format_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!("{params} -> {}", format_type(result))
        }
        TypeExpr::Effect {
            result, effects, ..
        } => {
            format!(
                "{} with {}",
                format_type(result),
                format_effect_row(effects)
            )
        }
        TypeExpr::Record { fields, tail, .. } => format_record_type(fields, tail.as_ref()),
        TypeExpr::Variant { cases, tail, .. } => format_variant_type(cases, tail.as_ref()),
        TypeExpr::Tuple { elems, .. } => {
            format!(
                "({})",
                elems.iter().map(format_type).collect::<Vec<_>>().join(", ")
            )
        }
        TypeExpr::Error { .. } => "<error>".to_string(),
    }
}

fn format_type_atom(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Function { .. } | TypeExpr::Effect { .. } => format!("({})", format_type(ty)),
        _ => format_type(ty),
    }
}

fn format_effect_row(row: &EffectRow) -> String {
    let mut parts = row.effects.iter().map(format_type).collect::<Vec<_>>();
    if let Some(tail) = &row.tail {
        parts.push(format_row_tail(tail));
    }
    format!("<{}>", parts.join(", "))
}

fn format_record_type(fields: &[FieldType], tail: Option<&RowTail>) -> String {
    let mut parts = fields
        .iter()
        .map(|field| format!("{}: {}", field.name.text, format_type(&field.value)))
        .collect::<Vec<_>>();
    if let Some(tail) = tail {
        parts.push(format_row_tail(tail));
    }
    format!("{{ {} }}", parts.join(", "))
}

fn format_variant_type(cases: &[VariantCaseType], tail: Option<&RowTail>) -> String {
    let mut parts = cases
        .iter()
        .map(|case| {
            format!(
                "{}({})",
                case.name.text,
                case.payload
                    .iter()
                    .map(format_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    if let Some(tail) = tail {
        parts.push(format_row_tail(tail));
    }
    format!("< {} >", parts.join(", "))
}

fn format_row_tail(tail: &RowTail) -> String {
    format!("...{}", tail.name.text)
}

fn format_block(block: &Block, indent: usize) -> String {
    let mut output = String::new();
    for statement in &block.statements {
        push_indented(&mut output, indent, &format_statement(statement));
    }
    push_indented(&mut output, indent, &format_expr(&block.result));
    output
}

fn format_statement(statement: &Stmt) -> String {
    match statement {
        Stmt::Let(statement) => format_let_statement(statement),
        Stmt::Expr(expr) => format_expr(expr),
    }
}

fn format_let_statement(statement: &LetStmt) -> String {
    format!(
        "{} =\n{}",
        statement.name.text,
        indent_text(&format_expr(&statement.value), 1)
    )
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Name(name) => name.text.clone(),
        Expr::Int { value, .. } => value.to_string(),
        Expr::Text { value, .. } => format_text(value),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::Unit { .. } => "unit".to_string(),
        Expr::List { items, .. } => {
            format!(
                "[{}]",
                items.iter().map(format_expr).collect::<Vec<_>>().join(", ")
            )
        }
        Expr::Record { fields, .. } => format_record_expr(fields),
        Expr::RecordUpdate { base, fields, .. } => format_record_update_expr(base, fields),
        Expr::Lambda { params, body, .. } => format_lambda_expr(params, body),
        Expr::Field { target, field, .. } => {
            format!("{}.{}", format_expr_atom(target), field.text)
        }
        Expr::Call { callee, args, .. } => {
            format!(
                "{}({})",
                format_expr_atom(callee),
                args.iter().map(format_expr).collect::<Vec<_>>().join(", ")
            )
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            format!(
                "if {} then {} else {}",
                format_expr(condition),
                format_expr(then_branch),
                format_expr(else_branch)
            )
        }
        Expr::Match {
            scrutinee, arms, ..
        } => format_match_expr(scrutinee, arms),
        Expr::Pipeline { .. } => format_pipeline_expr(expr),
        Expr::With { targets, body, .. } => format_with_expr(targets, body),
        Expr::Within { scope, body, .. } => format_within_expr(scope, body),
        Expr::Handle {
            action,
            clauses,
            return_clause,
            ..
        } => format_handle_expr(action, clauses, return_clause.as_ref()),
        Expr::Binary {
            op, left, right, ..
        } => {
            format!(
                "{} {} {}",
                format_expr(left),
                format_binary_op(*op),
                format_expr(right)
            )
        }
        Expr::Error { .. } => "<error>".to_string(),
    }
}

fn format_expr_atom(expr: &Expr) -> String {
    match expr {
        Expr::Name(_)
        | Expr::Int { .. }
        | Expr::Text { .. }
        | Expr::Bool { .. }
        | Expr::Unit { .. }
        | Expr::List { .. }
        | Expr::Record { .. }
        | Expr::RecordUpdate { .. }
        | Expr::Call { .. }
        | Expr::Field { .. } => format_expr(expr),
        _ => format!("({})", format_expr(expr)),
    }
}

fn format_record_expr(fields: &[FieldExpr]) -> String {
    format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|field| format!("{}: {}", field.name.text, format_expr(&field.value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn format_record_update_expr(base: &Expr, fields: &[FieldExpr]) -> String {
    let mut parts = vec![format!("...{}", format_expr(base))];
    parts.extend(
        fields
            .iter()
            .map(|field| format!("{}: {}", field.name.text, format_expr(&field.value))),
    );
    format!("{{ {} }}", parts.join(", "))
}

fn format_lambda_expr(params: &[LambdaParam], body: &Block) -> String {
    let params = if params.len() == 1 {
        format_lambda_param(&params[0])
    } else {
        format!(
            "({})",
            params
                .iter()
                .map(format_lambda_param)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    if body.statements.is_empty() && is_inline_expr(&body.result) {
        format!("{params} -> {}", format_expr(&body.result))
    } else {
        format!("{params} ->\n{}", indent_text(&format_block(body, 0), 1))
    }
}

fn format_lambda_param(param: &LambdaParam) -> String {
    param
        .name
        .as_ref()
        .map_or_else(|| "_".to_string(), |name| name.text.clone())
}

fn format_match_expr(scrutinee: &Expr, arms: &[MatchArm]) -> String {
    let mut output = format!("match {}", format_expr(scrutinee));
    for arm in arms {
        output.push('\n');
        output.push_str("  ");
        output.push_str(&format_pattern(&arm.pattern));
        output.push_str(" ->");
        if is_inline_expr(&arm.value) {
            output.push(' ');
            output.push_str(&format_expr(&arm.value));
        } else {
            output.push('\n');
            output.push_str(&indent_text(&format_expr(&arm.value), 2));
        }
    }
    output
}

fn format_pipeline_expr(expr: &Expr) -> String {
    let mut parts = Vec::new();
    collect_pipeline_parts(expr, &mut parts);
    parts.join(" |> ")
}

fn collect_pipeline_parts<'a>(expr: &'a Expr, parts: &mut Vec<String>) {
    match expr {
        Expr::Pipeline { value, target, .. } => {
            collect_pipeline_parts(value, parts);
            parts.push(format_expr(target));
        }
        other => parts.push(format_expr(other)),
    }
}

fn format_with_expr(targets: &[WithTarget], body: &Block) -> String {
    let mut output = String::new();
    for target in targets {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("with ");
        output.push_str(&format_expr(&target.target));
    }
    output.push('\n');
    output.push_str(&format_block(body, 1));
    output
}

fn format_within_expr(scope: &Name, body: &Block) -> String {
    let mut output = format!("within {}", scope.text);
    output.push('\n');
    output.push_str(&format_block(body, 1));
    output
}

fn format_handle_expr(
    action: &Expr,
    clauses: &[HandlerClause],
    return_clause: Option<&HandlerReturnClause>,
) -> String {
    let mut output = format!("handle {}", format_expr(action));
    for clause in clauses {
        output.push('\n');
        output.push_str(&format_handler_clause(clause));
    }
    if let Some(return_clause) = return_clause {
        output.push('\n');
        output.push_str(&format_handler_return_clause(return_clause));
    }
    output
}

fn format_handler_clause(clause: &HandlerClause) -> String {
    let params = clause
        .params
        .iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "  {}({}) ->\n{}",
        clause.operation.text,
        params,
        indent_text(&format_block(&clause.body, 0), 2)
    )
}

fn format_handler_return_clause(clause: &HandlerReturnClause) -> String {
    format!(
        "  return({}) ->\n{}",
        clause.param.text,
        indent_text(&format_block(&clause.body, 0), 2)
    )
}

fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Name(name) => name.text.clone(),
        Pattern::Wildcard { .. } => "_".to_string(),
        Pattern::Int { value, .. } => value.to_string(),
        Pattern::Text { value, .. } => format_text(value),
        Pattern::Bool { value, .. } => value.to_string(),
        Pattern::Unit { .. } => "unit".to_string(),
        Pattern::List { items, rest, .. } => {
            let mut parts = items.iter().map(format_pattern).collect::<Vec<_>>();
            if let Some(rest) = rest {
                parts.push(format!("...{}", rest.text));
            }
            format!("[{}]", parts.join(", "))
        }
        Pattern::Record { fields, .. } => {
            format!(
                "{{ {} }}",
                fields
                    .iter()
                    .map(|field| field.text.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Pattern::Constructor { name, args, .. } => {
            format!(
                "{}({})",
                name.text,
                args.iter()
                    .map(format_pattern)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Pattern::Error { .. } => "<error>".to_string(),
    }
}

fn is_inline_expr(expr: &Expr) -> bool {
    !matches!(
        expr,
        Expr::Match { .. } | Expr::With { .. } | Expr::Within { .. } | Expr::Handle { .. }
    )
}

fn format_binary_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Equal => "==",
        BinaryOp::Concat => "++",
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
    }
}

fn format_text(value: &str) -> String {
    format!("\"{}\"", value.escape_default())
}

fn push_indented(output: &mut String, indent: usize, text: &str) {
    output.push_str(&indent_text(text, indent));
    output.push('\n');
}

fn indent_text(text: &str, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unchanged_canonical_source() {
        let source = "value() : Int\n  1\n";
        let formatted = format_module(source).expect("formatted");

        assert!(!formatted.changed);
        assert_eq!(formatted.source, source);
    }

    #[test]
    fn normalizes_file_level_whitespace() {
        let source = "value() : Int   \n  1\t\n\n\n";
        let formatted = format_module(source).expect("formatted");

        assert!(formatted.changed);
        assert_eq!(formatted.source, "value() : Int\n  1\n");
    }

    #[test]
    fn preserves_comments_while_normalizing_whitespace() {
        let source = "# module comment   \nvalue() : Int\n  # body comment\n  1\n";
        let formatted = format_module(source).expect("formatted");

        assert_eq!(
            formatted.source,
            "# module comment\nvalue() : Int\n  # body comment\n  1\n"
        );
    }

    #[test]
    fn returns_diagnostics_for_malformed_source() {
        let diagnostics = format_module("value() : Int\n  \"nope\n").expect_err("diagnostics");

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("unterminated string"))
        );
    }

    #[test]
    fn formatting_is_idempotent() {
        let source = "value() : Int   \n  1\t\n";
        let first = format_module(source).expect("first");
        let second = format_module(&first.source).expect("second");

        assert!(first.changed);
        assert!(!second.changed);
        assert_eq!(second.source, first.source);
    }

    #[test]
    fn formats_top_level_declarations_and_types() {
        let source = r#"import   Core.Text
export   type  Person r = { name : Text, age: Int, ...r, }
type  Result a r = < Ok( a ) , Error( Text ) , ...r, >

export effect  Raise e
  raise( message : Text ) : a with < e >

safe-divide( x : Int, y : Int ) : Int with < Raise, ...e >
  x
"#;

        let expected = r#"import Core.Text

export type Person r = { name: Text, age: Int, ...r }

type Result a r = < Ok(a), Error(Text), ...r >

export effect Raise e
  raise(message: Text) : a with <e>

safe-divide(x: Int, y: Int) : Int with <Raise, ...e>
  x
"#;

        let formatted = format_module(source).expect("formatted");
        let second = format_module(&formatted.source).expect("second");

        assert_eq!(formatted.source, expected);
        assert!(!second.changed);
        assert_eq!(second.source, expected);
    }

    #[test]
    fn preserves_leading_comments_while_formatting_following_items() {
        let source = r#"# imports
import   Core.Text

# domain type
type  Person = { name : Text }
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(
            formatted.source,
            r#"# imports
import Core.Text

# domain type
type Person = { name: Text }
"#
        );
    }

    #[test]
    fn preserves_existing_blank_after_file_header_comments() {
        let source = r#"# Phase fixture.

value() : Int
  1
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }

    #[test]
    fn keeps_consecutive_imports_together() {
        let source = r#"import   domain
import   functions
value() : Int
  1
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(
            formatted.source,
            r#"import domain
import functions

value() : Int
  1
"#
        );
    }

    #[test]
    fn sorts_safe_contiguous_import_groups() {
        let source = r#"import zebra
import alpha
import nested.beta

value() : Int
  1
"#;

        let formatted = format_module(source).expect("formatted");
        let second = format_module(&formatted.source).expect("second");

        assert_eq!(
            formatted.source,
            r#"import alpha
import nested.beta
import zebra

value() : Int
  1
"#
        );
        assert!(!second.changed);
    }

    #[test]
    fn sorts_import_groups_without_crossing_declarations() {
        let source = r#"import zebra
import alpha

type Marker = Unit

import support.zed
import support.beta
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(
            formatted.source,
            r#"import alpha
import zebra

type Marker = Unit

import support.beta
import support.zed
"#
        );
    }

    #[test]
    fn preserves_import_order_when_comments_are_attached_to_group() {
        let source = r#"# module imports
import zebra
import alpha

import gamma
# keep next to beta
import beta
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(
            formatted.source,
            r#"# module imports
import zebra
import alpha
import gamma
# keep next to beta
import beta
"#
        );
    }

    #[test]
    fn preserves_import_order_when_basenames_are_duplicated() {
        let source = r#"import b.foo
import a.foo
import c.bar

value() : Int
  1
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }

    #[test]
    fn formats_blocks_expressions_patterns_and_match_arms() {
        let source = r#"type Todo = { title : Text, done : Bool }
summarize(result: < Ok(Todo), Missing() >) : Text
  label =
    result |> inspect( "todo" )
  match result
    Ok( { title, done } ) -> if done then title ++ " [done]" else title
    Missing() ->
      "missing"
"#;

        let expected = r#"type Todo = { title: Text, done: Bool }

summarize(result: < Ok(Todo), Missing() >) : Text
  label =
    result |> inspect("todo")
  match result
    Ok({ title, done }) -> if done then title ++ " [done]" else title
    Missing() -> "missing"
"#;

        let formatted = format_module(source).expect("formatted");
        let second = format_module(&formatted.source).expect("second");

        assert_eq!(formatted.source, expected);
        assert!(!second.changed);
    }

    #[test]
    fn formats_prefix_with_and_handler_clauses() {
        let source = r#"catch(default: a, action: () -> a with < Raise, ...e >) : a with < ...e >
  handle action( )
    raise( message, resume ) -> resume(default)
    return(value) ->
      value

value() : Int
  with catch( 0 )
    1
"#;

        let expected = r#"catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(message, resume) ->
      resume(default)
    return(value) ->
      value

value() : Int
  with catch(0)
    1
"#;

        let formatted = format_module(source).expect("formatted");
        let second = format_module(&formatted.source).expect("second");

        assert_eq!(formatted.source, expected);
        assert!(!second.changed);
    }

    #[test]
    fn preserves_original_item_source_when_comments_are_inside_the_item() {
        let source = "value() : Int # result comment\n  1\n";
        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }

    #[test]
    fn preserves_comments_inside_blocks_match_arms_and_handlers() {
        let source = r#"effect Raise
  raise(message: Text) : a

catch(action: () -> a with <Raise>) : a
  # before handle
  handle action()
    # before operation clause
    raise(message, resume) ->
      # inside operation body
      resume(message)
    return(value) -> # trailing return clause
      value

describe(value: < Ok(Text), Missing() >) : Text
  match value
    # before match arm
    Ok(text) ->
      text # trailing arm body
    Missing() ->
      "missing"
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }

    #[test]
    fn preserves_trailing_file_comments_after_last_item() {
        let source = r#"value() : Int
  1

# Future diagnostic fixture:
# bad() : Int
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }

    #[test]
    fn preserves_with_prefixed_value_bodies_without_consuming_next_item() {
        let source = r#"with-fallback(action: () -> a) : a
  with action()
    action()

export main() : Int
  1
"#;

        let formatted = format_module(source).expect("formatted");

        assert_eq!(formatted.source, source);
    }
}

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{Keyword, Token, TokenKind, lex};
use crate::span::Span;

pub fn parse_file(source: impl Into<String>) -> ParsedModule {
    parse_module(source)
}

pub fn parse_module(source: impl Into<String>) -> ParsedModule {
    let source = source.into();
    let (tokens, mut diagnostics) = lex(&source);
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let module = parser.module(source.len());
    diagnostics.extend(parser.diagnostics);
    ParsedModule {
        source,
        module,
        diagnostics,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn module(&mut self, source_len: usize) -> Module {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKind::Eof) {
            match self.item() {
                Some(item) => items.push(item),
                None => self.recover_item(),
            }
            self.skip_newlines();
        }
        Module {
            items,
            span: Span::new(0, source_len),
        }
    }

    fn item(&mut self) -> Option<Item> {
        let export = self.eat_keyword(Keyword::Export).is_some();
        if self.at_keyword(Keyword::Import) {
            if export {
                self.error_here("imports cannot be exported");
            }
            Some(Item::Import(self.import_decl()))
        } else if self.at_keyword(Keyword::Type) {
            Some(Item::Type(self.type_decl(export)))
        } else if self.at_keyword(Keyword::Effect) {
            Some(Item::Effect(self.effect_decl(export)))
        } else if self.is_later_phase_keyword() {
            let span = self.bump().span;
            self.diagnostics.push(Diagnostic::new(
                span,
                "this syntax belongs to a later phase and is not supported yet",
            ));
            None
        } else if matches!(self.peek().kind, TokenKind::Ident(_)) {
            Some(Item::Value(self.value_decl(export)))
        } else {
            self.error_here("expected import, type, or value declaration");
            None
        }
    }

    fn import_decl(&mut self) -> ImportDecl {
        let start = self
            .expect_keyword(Keyword::Import, "expected `import`")
            .start;
        let mut path = Vec::new();
        let mut end = start;
        if let Some(name) = self.expect_ident("expected module path segment") {
            end = name.span.end;
            path.push(name.text);
            if self.eat(TokenKind::Dot).is_none() {
                return ImportDecl {
                    path,
                    span: Span::new(start, end),
                };
            }
            while let Some(name) = self.expect_ident("expected module path segment") {
                end = name.span.end;
                path.push(name.text);
                if self.eat(TokenKind::Dot).is_none() {
                    break;
                }
            }
        }
        ImportDecl {
            path,
            span: Span::new(start, end),
        }
    }

    fn type_decl(&mut self, export: bool) -> TypeDecl {
        let start = self.expect_keyword(Keyword::Type, "expected `type`").start;
        let name = self
            .expect_ident("expected type name")
            .unwrap_or_else(|| error_name(start));
        let mut params = Vec::new();
        while matches!(self.peek().kind, TokenKind::Ident(_)) {
            if self.peek_is(TokenKind::Equal) {
                break;
            }
            if let Some(param) = self.expect_ident("expected type parameter") {
                params.push(param);
            }
        }
        self.expect(TokenKind::Equal, "expected `=` before type body");
        let body = self.type_expr();
        let span = Span::new(start, body.span().end);
        TypeDecl {
            export,
            name,
            params,
            body,
            span,
        }
    }

    fn effect_decl(&mut self, export: bool) -> EffectDecl {
        let start = self
            .expect_keyword(Keyword::Effect, "expected `effect`")
            .start;
        let name = self
            .expect_ident("expected effect name")
            .unwrap_or_else(|| error_name(start));
        let mut params = Vec::new();
        while matches!(self.peek().kind, TokenKind::Ident(_)) {
            if let Some(param) = self.expect_ident("expected effect type parameter") {
                params.push(param);
            }
        }
        self.expect_newline("expected newline before effect operations");
        self.expect(TokenKind::Indent, "expected indented effect operations");
        let mut operations = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            operations.push(self.operation_decl());
            self.skip_newlines();
        }
        let end = self
            .expect(TokenKind::Dedent, "expected end of effect declaration")
            .end;
        EffectDecl {
            export,
            name,
            params,
            operations,
            span: Span::new(start, end),
        }
    }

    fn operation_decl(&mut self) -> OperationDecl {
        let start = self.peek().span.start;
        let name = self
            .expect_ident("expected operation name")
            .unwrap_or_else(|| error_name(start));
        let params = self.param_list("operation");
        self.expect(
            TokenKind::Colon,
            "expected operation result type annotation",
        );
        let result = self.type_expr();
        let span = Span::new(start, result.span().end);
        OperationDecl {
            name,
            params,
            result,
            span,
        }
    }

    fn value_decl(&mut self, export: bool) -> ValueDecl {
        let start = self.peek().span.start;
        let name = self
            .expect_ident("expected function name")
            .unwrap_or_else(|| error_name(start));
        let params = self.param_list("function");
        self.expect(TokenKind::Colon, "expected return type annotation");
        let annotation = self.type_expr();
        self.expect_newline("expected newline before function body");
        self.expect(TokenKind::Indent, "expected indented function body");
        let body = self.block();
        self.expect(TokenKind::Dedent, "expected end of function body");
        let span = Span::new(start, body.span.end);
        ValueDecl {
            export,
            name,
            params,
            annotation,
            body,
            span,
        }
    }

    fn param_list(&mut self, owner: &str) -> Vec<Param> {
        self.expect(
            TokenKind::LParen,
            &format!("expected `(` after {owner} name"),
        );
        let mut params = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                let param_start = self.peek().span.start;
                let param_name = self
                    .expect_ident("expected parameter name")
                    .unwrap_or_else(|| error_name(param_start));
                self.expect(TokenKind::Colon, "expected `:` after parameter name");
                let annotation = self.type_expr();
                let span = param_name.span.join(annotation.span());
                params.push(Param {
                    name: param_name,
                    annotation,
                    span,
                });
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after parameters");
        params
    }

    fn block(&mut self) -> Block {
        self.skip_newlines();
        let start = self.peek().span.start;
        let mut statements = Vec::new();
        let mut exprs = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            if self.starts_let_stmt() {
                statements.push(Stmt::Let(self.let_stmt()));
            } else {
                exprs.push(self.expr());
            }
            self.skip_newlines();
        }
        let result = exprs.pop().unwrap_or(Expr::Unit {
            span: Span::point(start),
        });
        let end = result.span().end;
        Block {
            statements,
            result: Box::new(result),
            span: Span::new(start, end),
        }
    }

    fn let_stmt(&mut self) -> LetStmt {
        let name = self
            .expect_ident("expected binding name")
            .unwrap_or_else(|| error_name(self.peek().span.start));
        self.expect(TokenKind::Equal, "expected `=` in local binding");
        let value = if self.at(TokenKind::Newline) {
            self.expect_newline("expected newline after local binding `=`");
            self.expect(TokenKind::Indent, "expected indented local binding body");
            let block = self.block();
            self.expect(TokenKind::Dedent, "expected end of local binding body");
            *block.result
        } else {
            self.expr()
        };
        let span = name.span.join(value.span());
        LetStmt { name, value, span }
    }

    fn expr(&mut self) -> Expr {
        if self.starts_lambda_expr() {
            self.lambda_expr()
        } else if self.at_keyword(Keyword::With) {
            self.with_expr()
        } else {
            let expr = self.binary_expr(0);
            if self.at_keyword(Keyword::With) {
                self.error_here("postfix `with` is not supported; use prefix `with target`");
                self.bump();
            }
            expr
        }
    }

    fn binary_expr(&mut self, min_prec: u8) -> Expr {
        let mut left = self.pipeline_expr();
        while let Some((op, prec)) = self.binary_op() {
            if prec < min_prec {
                break;
            }
            let op_span = self.bump().span;
            let right = self.binary_expr(prec + 1);
            let span = left.span().join(right.span());
            left = Expr::Binary {
                op,
                op_span,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        left
    }

    fn pipeline_expr(&mut self) -> Expr {
        let mut left = self.postfix_expr();
        while self.at(TokenKind::PipeGreater) {
            let op_span = self.bump().span;
            let target = self.postfix_expr();
            let span = left.span().join(target.span());
            left = Expr::Pipeline {
                value: Box::new(left),
                op_span,
                target: Box::new(target),
                span,
            };
        }
        left
    }

    fn postfix_expr(&mut self) -> Expr {
        let mut expr = self.primary_expr();
        loop {
            if self.eat(TokenKind::LParen).is_some() {
                let mut args = Vec::new();
                if !self.at(TokenKind::RParen) {
                    loop {
                        args.push(self.expr());
                        if self.eat(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let close = self.expect(TokenKind::RParen, "expected `)` after arguments");
                let span = expr.span().join(close);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.eat(TokenKind::Dot).is_some() {
                let field = self
                    .expect_ident("expected field name after `.`")
                    .unwrap_or_else(|| error_name(self.peek().span.start));
                let span = expr.span().join(field.span);
                expr = Expr::Field {
                    target: Box::new(expr),
                    field,
                    span,
                };
            } else {
                break;
            }
        }
        expr
    }

    fn primary_expr(&mut self) -> Expr {
        match self.peek().kind.clone() {
            TokenKind::Keyword(Keyword::If) => self.if_expr(),
            TokenKind::Keyword(Keyword::Match) => self.match_expr(),
            TokenKind::Keyword(Keyword::Handle) => self.handle_expr(),
            TokenKind::Keyword(Keyword::True) => {
                let span = self.bump().span;
                Expr::Bool { value: true, span }
            }
            TokenKind::Keyword(Keyword::False) => {
                let span = self.bump().span;
                Expr::Bool { value: false, span }
            }
            TokenKind::Ident(text) if text == "unit" => {
                let span = self.bump().span;
                Expr::Unit { span }
            }
            TokenKind::Keyword(Keyword::With) => self.with_expr(),
            TokenKind::Ident(_) => {
                let name = self
                    .expect_ident("expected name")
                    .unwrap_or_else(|| error_name(self.peek().span.start));
                Expr::Name(name)
            }
            TokenKind::Int(value) => {
                let span = self.bump().span;
                Expr::Int { value, span }
            }
            TokenKind::Text(value) => {
                let span = self.bump().span;
                Expr::Text { value, span }
            }
            TokenKind::LBracket => self.list_expr(),
            TokenKind::LBrace => self.record_expr(),
            _ => {
                let span = self.bump().span;
                self.diagnostics
                    .push(Diagnostic::new(span, "expected expression"));
                Expr::Error { span }
            }
        }
    }

    fn with_expr(&mut self) -> Expr {
        let start = self.peek().span.start;
        let mut targets = Vec::new();
        loop {
            let with_span = self.expect_keyword(Keyword::With, "expected `with`");
            let target = self.expr();
            targets.push(WithTarget {
                span: with_span.join(target.span()),
                target,
            });
            self.expect_newline("expected newline after `with` target");
            if !self.at_keyword(Keyword::With) {
                break;
            }
        }
        self.expect(TokenKind::Indent, "expected indented `with` body");
        let body = self.block();
        self.expect(TokenKind::Dedent, "expected end of `with` body");
        let span = Span::new(start, body.span.end);
        Expr::With {
            targets,
            body,
            span,
        }
    }

    fn handle_expr(&mut self) -> Expr {
        let start = self
            .expect_keyword(Keyword::Handle, "expected `handle`")
            .start;
        let action = self.expr();
        self.expect_newline("expected newline before handler clauses");
        self.expect(TokenKind::Indent, "expected indented handler clauses");
        let mut clauses = Vec::new();
        let mut return_clause = None;
        self.skip_newlines();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            let clause_start = self.peek().span.start;
            let name = self
                .expect_ident("expected handler clause name")
                .unwrap_or_else(|| error_name(clause_start));
            if name.text == "return" {
                if return_clause.is_some() {
                    self.diagnostics.push(Diagnostic::new(
                        name.span,
                        "duplicate handler return clause",
                    ));
                }
                let clause = self.handler_return_clause(name);
                return_clause = Some(clause);
            } else {
                clauses.push(self.handler_operation_clause(name));
            }
            self.skip_newlines();
        }
        let end = self
            .expect(TokenKind::Dedent, "expected end of handler clauses")
            .end;
        Expr::Handle {
            action: Box::new(action),
            clauses,
            return_clause,
            span: Span::new(start, end),
        }
    }

    fn handler_operation_clause(&mut self, operation: Name) -> HandlerClause {
        let params = self.handler_param_list();
        self.expect(TokenKind::Arrow, "expected `->` in handler clause");
        let body = self.handler_clause_body("expected newline after handler clause arrow");
        let span = operation.span.join(body.span);
        HandlerClause {
            operation,
            params,
            body,
            span,
        }
    }

    fn handler_return_clause(&mut self, return_name: Name) -> HandlerReturnClause {
        let params = self.handler_param_list();
        let param = params
            .into_iter()
            .next()
            .unwrap_or_else(|| error_name(return_name.span.end));
        self.expect(TokenKind::Arrow, "expected `->` in handler return clause");
        let body = self.handler_clause_body("expected newline after handler return arrow");
        let span = return_name.span.join(body.span);
        HandlerReturnClause { param, body, span }
    }

    fn handler_param_list(&mut self) -> Vec<Name> {
        self.expect(TokenKind::LParen, "expected `(` after handler clause name");
        let mut params = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                let param_start = self.peek().span.start;
                let param = self
                    .expect_ident("expected handler clause parameter")
                    .unwrap_or_else(|| error_name(param_start));
                params.push(param);
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(
            TokenKind::RParen,
            "expected `)` after handler clause parameters",
        );
        params
    }

    fn handler_clause_body(&mut self, newline_message: &str) -> Block {
        if self.at(TokenKind::Newline) {
            self.expect_newline(newline_message);
            self.expect(TokenKind::Indent, "expected indented handler clause body");
            let body = self.block();
            self.expect(TokenKind::Dedent, "expected end of handler clause body");
            body
        } else {
            let expr = self.expr();
            let span = expr.span();
            Block {
                statements: Vec::new(),
                result: Box::new(expr),
                span,
            }
        }
    }

    fn if_expr(&mut self) -> Expr {
        let start = self.expect_keyword(Keyword::If, "expected `if`").start;
        let condition = self.expr();
        self.expect_keyword(Keyword::Then, "expected `then` in if expression");
        let then_branch = self.expr();
        self.expect_keyword(Keyword::Else, "expected `else` in if expression");
        let else_branch = self.expr();
        let span = Span::new(start, else_branch.span().end);
        Expr::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            span,
        }
    }

    fn lambda_expr(&mut self) -> Expr {
        let start = self.peek().span.start;
        let params = if self.at(TokenKind::LParen) {
            self.lambda_param_list()
        } else {
            vec![self.lambda_param()]
        };
        self.expect(TokenKind::Arrow, "expected `->` in lambda");
        let body = self.lambda_body();
        let span = Span::new(start, body.span.end);
        Expr::Lambda { params, body, span }
    }

    fn lambda_param_list(&mut self) -> Vec<LambdaParam> {
        self.expect(TokenKind::LParen, "expected `(` before lambda parameters");
        let mut params = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                params.push(self.lambda_param());
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after lambda parameters");
        params
    }

    fn lambda_param(&mut self) -> LambdaParam {
        match self.peek().kind.clone() {
            TokenKind::Ident(_) => {
                let span = self.peek().span;
                let name = self.expect_ident("expected lambda parameter");
                LambdaParam {
                    name: name.filter(|name| name.text != "_"),
                    span,
                }
            }
            TokenKind::Spread => {
                let span = self.bump().span;
                self.diagnostics.push(Diagnostic::new(
                    span,
                    "lambda rest or pattern parameters are not supported in Phase 1",
                ));
                if matches!(self.peek().kind, TokenKind::Ident(_)) {
                    self.bump();
                }
                LambdaParam { name: None, span }
            }
            _ => {
                let span = self.bump().span;
                self.diagnostics
                    .push(Diagnostic::new(span, "expected lambda parameter"));
                LambdaParam { name: None, span }
            }
        }
    }

    fn lambda_body(&mut self) -> Block {
        if self.at(TokenKind::Newline) {
            self.expect_newline("expected newline after lambda arrow");
            self.expect(TokenKind::Indent, "expected indented lambda body");
            let body = self.block();
            self.expect(TokenKind::Dedent, "expected end of lambda body");
            body
        } else {
            let expr = self.expr();
            let span = expr.span();
            Block {
                statements: Vec::new(),
                result: Box::new(expr),
                span,
            }
        }
    }

    fn match_expr(&mut self) -> Expr {
        let start = self
            .expect_keyword(Keyword::Match, "expected `match`")
            .start;
        let scrutinee = self.expr();
        self.expect_newline("expected newline before match arms");
        self.expect(TokenKind::Indent, "expected indented match arms");
        let mut arms = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            let pattern = self.pattern();
            self.expect(TokenKind::Arrow, "expected `->` in match arm");
            let value = if self.at(TokenKind::Newline) {
                self.expect_newline("expected newline after match arm arrow");
                self.expect(TokenKind::Indent, "expected indented match arm body");
                let block = self.block();
                self.expect(TokenKind::Dedent, "expected end of match arm body");
                *block.result
            } else {
                self.expr()
            };
            let span = pattern.span().join(value.span());
            arms.push(MatchArm {
                pattern,
                value,
                span,
            });
            self.skip_newlines();
        }
        let end = arms.last().map_or(scrutinee.span().end, |arm| arm.span.end);
        self.expect(TokenKind::Dedent, "expected end of match arms");
        Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Span::new(start, end),
        }
    }

    fn list_expr(&mut self) -> Expr {
        let start = self.expect(TokenKind::LBracket, "expected `[`").start;
        let mut items = Vec::new();
        if !self.at(TokenKind::RBracket) {
            loop {
                if self.eat(TokenKind::Spread).is_some() {
                    self.diagnostics.push(Diagnostic::new(
                        self.peek().span,
                        "list value spreads are reserved for a later implementation slice",
                    ));
                }
                items.push(self.expr());
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let end = self
            .expect(TokenKind::RBracket, "expected `]` after list")
            .end;
        Expr::List {
            items,
            span: Span::new(start, end),
        }
    }

    fn record_expr(&mut self) -> Expr {
        let start = self.expect(TokenKind::LBrace, "expected `{`").start;
        self.skip_delimited_ws();
        if self.eat(TokenKind::Spread).is_some() {
            return self.record_update_expr(start);
        }
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Spread) {
                let span = self.bump().span;
                self.diagnostics.push(Diagnostic::new(
                    span,
                    "record update spread must appear first",
                ));
                self.expr();
                self.eat(TokenKind::Comma);
                self.skip_delimited_ws();
                continue;
            }
            let field_start = self.peek().span.start;
            let name = self
                .expect_ident("expected record field name")
                .unwrap_or_else(|| error_name(field_start));
            self.expect(TokenKind::Colon, "expected `:` after field name");
            let value = self.expr();
            let span = name.span.join(value.span());
            fields.push(FieldExpr { name, value, span });
            self.eat(TokenKind::Comma);
            self.skip_delimited_ws();
        }
        let end = self
            .expect(TokenKind::RBrace, "expected `}` after record")
            .end;
        Expr::Record {
            fields,
            span: Span::new(start, end),
        }
    }

    fn record_update_expr(&mut self, start: usize) -> Expr {
        let base = self.expr();
        let mut fields = Vec::new();
        let mut reported_missing_fields = false;
        if self.eat(TokenKind::Comma).is_none() {
            self.diagnostics.push(Diagnostic::new(
                base.span(),
                "record update requires at least one field",
            ));
            reported_missing_fields = true;
        }
        self.skip_delimited_ws();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let field_start = self.peek().span.start;
            let name = self
                .expect_ident("expected record update field name")
                .unwrap_or_else(|| error_name(field_start));
            self.expect(TokenKind::Colon, "expected `:` after field name");
            let value = self.expr();
            let span = name.span.join(value.span());
            fields.push(FieldExpr { name, value, span });
            self.eat(TokenKind::Comma);
            self.skip_delimited_ws();
        }
        if fields.is_empty() && !reported_missing_fields {
            self.diagnostics.push(Diagnostic::new(
                base.span(),
                "record update requires at least one field",
            ));
        }
        let end = self
            .expect(TokenKind::RBrace, "expected `}` after record update")
            .end;
        Expr::RecordUpdate {
            base: Box::new(base),
            fields,
            span: Span::new(start, end),
        }
    }

    fn pattern(&mut self) -> Pattern {
        match self.peek().kind.clone() {
            TokenKind::Ident(text) if text == "_" => {
                let span = self.bump().span;
                Pattern::Wildcard { span }
            }
            TokenKind::Ident(text) if text == "unit" => {
                let span = self.bump().span;
                Pattern::Unit { span }
            }
            TokenKind::Ident(_) => {
                let name = self.expect_ident("expected pattern name").unwrap();
                if self.eat(TokenKind::LParen).is_some() {
                    let mut args = Vec::new();
                    if !self.at(TokenKind::RParen) {
                        loop {
                            args.push(self.pattern());
                            if self.eat(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let close = self.expect(TokenKind::RParen, "expected `)` after pattern");
                    Pattern::Constructor {
                        span: name.span.join(close),
                        name,
                        args,
                    }
                } else {
                    Pattern::Name(name)
                }
            }
            TokenKind::Int(value) => {
                let span = self.bump().span;
                Pattern::Int { value, span }
            }
            TokenKind::Text(value) => {
                let span = self.bump().span;
                Pattern::Text { value, span }
            }
            TokenKind::Keyword(Keyword::True) => {
                let span = self.bump().span;
                Pattern::Bool { value: true, span }
            }
            TokenKind::Keyword(Keyword::False) => {
                let span = self.bump().span;
                Pattern::Bool { value: false, span }
            }
            TokenKind::LBracket => self.list_pattern(),
            TokenKind::LBrace => self.record_pattern(),
            _ => {
                let span = self.bump().span;
                self.diagnostics
                    .push(Diagnostic::new(span, "expected pattern"));
                Pattern::Error { span }
            }
        }
    }

    fn list_pattern(&mut self) -> Pattern {
        let start = self.expect(TokenKind::LBracket, "expected `[`").start;
        let mut items = Vec::new();
        let mut rest = None;
        if !self.at(TokenKind::RBracket) {
            loop {
                if self.eat(TokenKind::Spread).is_some() {
                    rest = self.expect_ident("expected rest pattern name after `...`");
                    break;
                }
                items.push(self.pattern());
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let end = self
            .expect(TokenKind::RBracket, "expected `]` after list pattern")
            .end;
        Pattern::List {
            items,
            rest,
            span: Span::new(start, end),
        }
    }

    fn record_pattern(&mut self) -> Pattern {
        let start = self.expect(TokenKind::LBrace, "expected `{`").start;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if let Some(name) = self.expect_ident("expected record pattern field") {
                fields.push(name);
            }
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self
            .expect(TokenKind::RBrace, "expected `}` after record pattern")
            .end;
        Pattern::Record {
            fields,
            span: Span::new(start, end),
        }
    }

    fn type_expr(&mut self) -> TypeExpr {
        self.effect_type()
    }

    fn effect_type(&mut self) -> TypeExpr {
        let result = self.function_type();
        if !self.at_keyword(Keyword::With) {
            return result;
        }
        let start = result.span().start;
        self.expect_keyword(Keyword::With, "expected `with` before effect row");
        let effects = self.effect_row();
        let span = Span::new(start, effects.span.end);
        TypeExpr::Effect {
            result: Box::new(result),
            effects,
            span,
        }
    }

    fn function_type(&mut self) -> TypeExpr {
        let left = self.type_app();
        if self.eat(TokenKind::Arrow).is_none() {
            return left;
        }
        let result = self.effect_type();
        let start = left.span().start;
        let params = match left {
            TypeExpr::Tuple { elems, .. } => elems,
            other => vec![other],
        };
        let span = Span::new(start, result.span().end);
        TypeExpr::Function {
            params,
            result: Box::new(result),
            span,
        }
    }

    fn effect_row(&mut self) -> EffectRow {
        let start = self
            .expect(TokenKind::LAngle, "expected `<` before effect row")
            .start;
        let mut effects = Vec::new();
        let mut tail = None;
        self.skip_delimited_ws();
        while !self.at(TokenKind::RAngle) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Spread) {
                if tail.is_some() {
                    self.error_here("duplicate effect row spread");
                    self.bump();
                    self.expect_ident("expected effect row variable name after `...`");
                } else {
                    tail = Some(self.effect_row_tail());
                }
                self.eat(TokenKind::Comma);
                self.skip_delimited_ws();
                if !self.at(TokenKind::RAngle) && !self.at(TokenKind::Eof) {
                    self.error_here("effect row spread must appear last");
                }
                continue;
            }
            effects.push(self.type_expr());
            self.eat(TokenKind::Comma);
            self.skip_delimited_ws();
        }
        let end = self
            .expect(TokenKind::RAngle, "expected `>` after effect row")
            .end;
        EffectRow {
            effects,
            tail,
            span: Span::new(start, end),
        }
    }

    fn effect_row_tail(&mut self) -> RowTail {
        let start = self.expect(TokenKind::Spread, "expected `...`").start;
        let name = self
            .expect_ident("expected effect row variable name after `...`")
            .unwrap_or_else(|| error_name(start));
        RowTail {
            span: Span::new(start, name.span.end),
            name,
        }
    }

    fn type_app(&mut self) -> TypeExpr {
        let mut callee = self.type_primary();
        let mut args = Vec::new();
        while self.starts_type_primary() {
            args.push(self.type_primary());
        }
        if args.is_empty() {
            callee
        } else {
            let span = callee
                .span()
                .join(args.last().expect("args is not empty").span());
            callee = TypeExpr::App {
                callee: Box::new(callee),
                args,
                span,
            };
            callee
        }
    }

    fn type_primary(&mut self) -> TypeExpr {
        match self.peek().kind.clone() {
            TokenKind::Ident(_) => {
                let name = self.expect_qualified_name("expected type name");
                if name.text.chars().next().is_some_and(char::is_lowercase)
                    && !name.text.contains('.')
                {
                    TypeExpr::Var(name)
                } else {
                    TypeExpr::Named {
                        span: name.span,
                        path: name
                            .text
                            .split('.')
                            .map(|part| Name {
                                text: part.to_string(),
                                span: name.span,
                            })
                            .collect(),
                    }
                }
            }
            TokenKind::LBrace => self.record_type(),
            TokenKind::LAngle => self.variant_type(),
            TokenKind::LParen => self.tuple_type(),
            TokenKind::Spread => {
                let span = self.bump().span;
                self.diagnostics.push(Diagnostic::new(
                    span,
                    "row spread is only valid in record or variant types",
                ));
                TypeExpr::Error { span }
            }
            _ => {
                let span = self.bump().span;
                self.diagnostics
                    .push(Diagnostic::new(span, "expected type"));
                TypeExpr::Error { span }
            }
        }
    }

    fn record_type(&mut self) -> TypeExpr {
        let start = self.expect(TokenKind::LBrace, "expected `{`").start;
        let mut fields = Vec::new();
        let mut tail = None;
        self.skip_delimited_ws();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Spread) {
                if tail.is_some() {
                    self.error_here("duplicate row spread");
                    self.bump();
                    self.expect_ident("expected row variable name after `...`");
                } else {
                    tail = Some(self.row_tail());
                }
                self.eat(TokenKind::Comma);
                self.skip_delimited_ws();
                if !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                    self.error_here("row spread must appear last");
                }
                continue;
            }
            let field_start = self.peek().span.start;
            let name = self
                .expect_ident("expected record field name")
                .unwrap_or_else(|| error_name(field_start));
            self.expect(TokenKind::Colon, "expected `:` after field name");
            let value = self.type_expr();
            let span = name.span.join(value.span());
            fields.push(FieldType { name, value, span });
            self.eat(TokenKind::Comma);
            self.skip_delimited_ws();
        }
        let end = self
            .expect(TokenKind::RBrace, "expected `}` after record type")
            .end;
        TypeExpr::Record {
            fields,
            tail,
            span: Span::new(start, end),
        }
    }

    fn variant_type(&mut self) -> TypeExpr {
        let start = self.expect(TokenKind::LAngle, "expected `<`").start;
        let mut cases = Vec::new();
        let mut tail = None;
        self.skip_delimited_ws();
        while !self.at(TokenKind::RAngle) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Spread) {
                if tail.is_some() {
                    self.error_here("duplicate row spread");
                    self.bump();
                    self.expect_ident("expected row variable name after `...`");
                } else {
                    tail = Some(self.row_tail());
                }
                self.eat(TokenKind::Comma);
                self.skip_delimited_ws();
                if !self.at(TokenKind::RAngle) && !self.at(TokenKind::Eof) {
                    self.error_here("row spread must appear last");
                }
                continue;
            }
            let case_start = self.peek().span.start;
            let name = self
                .expect_ident("expected variant constructor name")
                .unwrap_or_else(|| error_name(case_start));
            self.expect(TokenKind::LParen, "expected `(` after variant constructor");
            let mut payload = Vec::new();
            if !self.at(TokenKind::RParen) {
                loop {
                    payload.push(self.type_expr());
                    if self.eat(TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            let close = self.expect(TokenKind::RParen, "expected `)` after variant payload");
            cases.push(VariantCaseType {
                span: name.span.join(close),
                name,
                payload,
            });
            self.eat(TokenKind::Comma);
            self.skip_delimited_ws();
        }
        let end = self
            .expect(TokenKind::RAngle, "expected `>` after variant type")
            .end;
        TypeExpr::Variant {
            cases,
            tail,
            span: Span::new(start, end),
        }
    }

    fn row_tail(&mut self) -> RowTail {
        let start = self.expect(TokenKind::Spread, "expected `...`").start;
        let name = self
            .expect_ident("expected row variable name after `...`")
            .unwrap_or_else(|| error_name(start));
        RowTail {
            span: Span::new(start, name.span.end),
            name,
        }
    }

    fn tuple_type(&mut self) -> TypeExpr {
        let start = self.expect(TokenKind::LParen, "expected `(`").start;
        let mut elems = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                elems.push(self.type_expr());
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let end = self
            .expect(TokenKind::RParen, "expected `)` after tuple type")
            .end;
        TypeExpr::Tuple {
            elems,
            span: Span::new(start, end),
        }
    }

    fn starts_let_stmt(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| matches!(token.kind, TokenKind::Equal))
    }

    fn starts_lambda_expr(&self) -> bool {
        match self.peek().kind {
            TokenKind::Ident(_) => self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| matches!(token.kind, TokenKind::Arrow)),
            TokenKind::LParen => self.lambda_param_list_is_followed_by_arrow(),
            _ => false,
        }
    }

    fn lambda_param_list_is_followed_by_arrow(&self) -> bool {
        let mut depth = 0usize;
        for (offset, token) in self.tokens.iter().enumerate().skip(self.pos) {
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self
                            .tokens
                            .get(offset + 1)
                            .is_some_and(|next| matches!(next.kind, TokenKind::Arrow));
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
        }
        false
    }

    fn starts_type_primary(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Ident(_) | TokenKind::LBrace | TokenKind::LAngle | TokenKind::LParen
        )
    }

    fn binary_op(&self) -> Option<(BinaryOp, u8)> {
        match self.peek().kind {
            TokenKind::EqEq => Some((BinaryOp::Equal, 1)),
            TokenKind::PlusPlus => Some((BinaryOp::Concat, 2)),
            TokenKind::Plus => Some((BinaryOp::Add, 2)),
            TokenKind::Minus => Some((BinaryOp::Subtract, 2)),
            _ => None,
        }
    }

    fn expect_qualified_name(&mut self, message: &str) -> Name {
        let Some(first) = self.expect_ident(message) else {
            return error_name(self.peek().span.start);
        };
        let start = first.span.start;
        let mut end = first.span.end;
        let mut text = first.text;
        while self.eat(TokenKind::Dot).is_some() {
            if let Some(part) = self.expect_ident("expected name after `.`") {
                end = part.span.end;
                text.push('.');
                text.push_str(&part.text);
            } else {
                break;
            }
        }
        Name {
            text,
            span: Span::new(start, end),
        }
    }

    fn expect_ident(&mut self, message: &str) -> Option<Name> {
        match self.peek().kind.clone() {
            TokenKind::Ident(text) => {
                let span = self.bump().span;
                Some(Name { text, span })
            }
            _ => {
                self.error_here(message);
                None
            }
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(TokenKind::Newline) {
            self.bump();
        }
    }

    fn skip_delimited_ws(&mut self) {
        while matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.bump();
        }
    }

    fn expect_newline(&mut self, message: &str) -> Span {
        if self.at(TokenKind::Newline) {
            let span = self.bump().span;
            self.skip_newlines();
            span
        } else {
            self.error_here(message);
            self.peek().span
        }
    }

    fn expect_keyword(&mut self, keyword: Keyword, message: &str) -> Span {
        if self.at_keyword(keyword) {
            self.bump().span
        } else {
            self.error_here(message);
            self.peek().span
        }
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Span {
        if self.at(kind) {
            self.bump().span
        } else {
            self.error_here(message);
            self.peek().span
        }
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> Option<Span> {
        if self.at_keyword(keyword) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        matches!(self.peek().kind, TokenKind::Keyword(found) if found == keyword)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn peek_is(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn error_here(&mut self, message: &str) {
        self.diagnostics
            .push(Diagnostic::new(self.peek().span, message));
    }

    fn is_later_phase_keyword(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Keyword(Keyword::Given) | TokenKind::Keyword(Keyword::Scope)
        )
    }

    fn recover_item(&mut self) {
        while !self.at(TokenKind::Newline) && !self.at(TokenKind::Eof) {
            self.bump();
        }
    }
}

fn error_name(offset: usize) -> Name {
    Name {
        text: "<error>".to_string(),
        span: Span::point(offset),
    }
}

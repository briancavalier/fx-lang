use crate::module::{IndexedModule, ModuleGraph, Symbol, SymbolKind};
use crate::prelude::{binary_operator, primitive_value};
use crate::resolve::{
    SymbolTables, qualified_expr_name, qualified_root_is_local, resolve_visible_symbol,
    resolve_workspace,
};
use fx_syntax::ast::*;
use fx_syntax::diagnostic::Diagnostic;
use fx_syntax::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub module_diagnostics: BTreeMap<String, Vec<Diagnostic>>,
    pub module_types: BTreeMap<String, BTreeMap<String, Type>>,
    pub typed_spans: BTreeMap<String, Vec<TypedSpan>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Text,
    List(Box<Type>),
    Named { name: String, args: Vec<Type> },
    Var(String),
    Row(RowType),
    Function(Vec<Type>, Box<Type>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowType {
    pub kind: RowKind,
    pub entries: Vec<RowEntry>,
    pub tail: Option<RowTailType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowKind {
    Record,
    Variant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowEntry {
    Field { name: String, ty: Type },
    Case { name: String, payload: Vec<Type> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowTailType {
    Var(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedSpan {
    pub module: String,
    pub span: Span,
    pub kind: TypedSpanKind,
    pub label: String,
    pub display_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypedSpanKind {
    Value,
    Type,
    Constructor,
    Parameter,
    Local,
    Pattern,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeDef {
    params: Vec<String>,
    body: Type,
}

pub fn check_workspace(graph: &ModuleGraph) -> CheckResult {
    let resolved = resolve_workspace(graph);
    let mut diagnostics = resolved.diagnostics;
    let mut module_diagnostics = BTreeMap::new();
    let mut module_types = BTreeMap::new();
    let mut typed_spans = BTreeMap::new();
    let (type_defs, mut lowering_diagnostics) = collect_type_definitions(graph);
    let symbol_types = collect_symbol_types(graph);
    let constructor_arities = collect_constructor_arities(graph);
    let symbol_tables = SymbolTables::new(graph);

    for module in graph.modules.values() {
        let mut checker = ModuleChecker {
            module,
            diagnostics: lowering_diagnostics
                .remove(&module.name)
                .unwrap_or_default(),
            top_level: collect_visible_types(module, &symbol_tables, &symbol_types),
            symbol_types: &symbol_types,
            type_defs: &type_defs,
            constructor_arities: &constructor_arities,
            symbol_tables: &symbol_tables,
            typed_spans: Vec::new(),
        };
        let checked = checker.check();
        let checked_diagnostics = checker.diagnostics;
        diagnostics.extend(checked_diagnostics.clone());
        module_diagnostics.insert(module.name.clone(), checked_diagnostics);
        module_types.insert(module.name.clone(), checked.types);
        typed_spans.insert(module.name.clone(), checked.typed_spans);
    }

    CheckResult {
        diagnostics,
        module_diagnostics,
        module_types,
        typed_spans,
    }
}

struct CheckedModule {
    types: BTreeMap<String, Type>,
    typed_spans: Vec<TypedSpan>,
}

struct ModuleChecker<'a> {
    module: &'a IndexedModule,
    diagnostics: Vec<Diagnostic>,
    top_level: BTreeMap<String, Type>,
    symbol_types: &'a BTreeMap<String, Type>,
    type_defs: &'a BTreeMap<String, TypeDef>,
    constructor_arities: &'a BTreeMap<String, usize>,
    symbol_tables: &'a SymbolTables,
    typed_spans: Vec<TypedSpan>,
}

impl Type {
    fn closed_record(fields: Vec<(String, Type)>) -> Self {
        Type::Row(RowType {
            kind: RowKind::Record,
            entries: fields
                .into_iter()
                .map(|(name, ty)| RowEntry::Field { name, ty })
                .collect(),
            tail: None,
        })
    }

    fn closed_variant(cases: Vec<(String, Vec<Type>)>) -> Self {
        Type::Row(RowType {
            kind: RowKind::Variant,
            entries: cases
                .into_iter()
                .map(|(name, payload)| RowEntry::Case { name, payload })
                .collect(),
            tail: None,
        })
    }

    fn as_closed_record(&self) -> Option<&RowType> {
        match self {
            Type::Row(row) if row.kind == RowKind::Record && row.tail.is_none() => Some(row),
            _ => None,
        }
    }

    fn as_closed_variant(&self) -> Option<&RowType> {
        match self {
            Type::Row(row) if row.kind == RowKind::Variant && row.tail.is_none() => Some(row),
            _ => None,
        }
    }
}

impl RowType {
    fn field(&self, wanted: &str) -> Option<&Type> {
        if self.kind != RowKind::Record {
            return None;
        }
        self.entries.iter().find_map(|entry| match entry {
            RowEntry::Field { name, ty } if name == wanted => Some(ty),
            _ => None,
        })
    }

    fn field_index(&self, wanted: &str) -> Option<usize> {
        if self.kind != RowKind::Record {
            return None;
        }
        self.entries
            .iter()
            .position(|entry| matches!(entry, RowEntry::Field { name, .. } if name == wanted))
    }

    fn case(&self, wanted: &str) -> Option<&[Type]> {
        if self.kind != RowKind::Variant {
            return None;
        }
        self.entries.iter().find_map(|entry| match entry {
            RowEntry::Case { name, payload } if name == wanted => Some(payload.as_slice()),
            _ => None,
        })
    }
}

impl ModuleChecker<'_> {
    fn check(&mut self) -> CheckedModule {
        let items = self.module.parsed.module.items.clone();
        self.add_top_level_typed_spans();
        for item in &items {
            if let Item::Value(decl) = item {
                self.check_value_decl(decl);
            }
        }
        CheckedModule {
            types: self.top_level.clone(),
            typed_spans: self.typed_spans.clone(),
        }
    }

    fn check_value_decl(&mut self, decl: &ValueDecl) {
        let mut env = self.top_level.clone();
        for param in &decl.params {
            let ty = self.lower_type(&param.annotation);
            self.add_typed_span(
                param.name.span,
                TypedSpanKind::Parameter,
                &param.name.text,
                &ty,
            );
            env.insert(param.name.text.clone(), ty);
        }
        let expected = self.lower_type(&decl.annotation);
        self.check_block(&decl.body, &env, &expected);
    }

    fn check_block(
        &mut self,
        block: &Block,
        env: &BTreeMap<String, Type>,
        expected: &Type,
    ) -> Type {
        let mut env = env.clone();
        for statement in &block.statements {
            match statement {
                Stmt::Let(binding) => {
                    let ty = self.infer_expr(&binding.value, &env);
                    self.add_typed_span(
                        binding.name.span,
                        TypedSpanKind::Local,
                        &binding.name.text,
                        &ty,
                    );
                    env.insert(binding.name.text.clone(), ty);
                }
            }
        }
        self.check_expr(&block.result, &env, expected)
    }

    fn check_expr(&mut self, expr: &Expr, env: &BTreeMap<String, Type>, expected: &Type) -> Type {
        match expr {
            Expr::Lambda { .. } => self.check_lambda(expr, env, expected),
            Expr::List { items, .. } => {
                if let Type::List(item_expected) = self.expand_alias(expected) {
                    for item in items {
                        self.check_expr(item, env, &item_expected);
                    }
                    expected.clone()
                } else {
                    self.check_inferred_expr(expr, env, expected)
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_expr(condition, env, &Type::Bool);
                self.check_expr(then_branch, env, expected);
                self.check_expr(else_branch, env, expected);
                expected.clone()
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrutinee_ty = self.infer_expr(scrutinee, env);
                for arm in arms {
                    let mut arm_env = env.clone();
                    let mut bound_names = BTreeSet::new();
                    self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env, &mut bound_names);
                    self.check_expr(&arm.value, &arm_env, expected);
                }
                expected.clone()
            }
            _ => self.check_inferred_expr(expr, env, expected),
        }
    }

    fn check_inferred_expr(
        &mut self,
        expr: &Expr,
        env: &BTreeMap<String, Type>,
        expected: &Type,
    ) -> Type {
        let found = self.infer_expr(expr, env);
        if !self.compatible(&found, expected) {
            self.diagnostics.push(Diagnostic::new(
                expr.span(),
                format!("expected `{expected}`, found `{found}`"),
            ));
        }
        found
    }

    fn check_lambda(&mut self, expr: &Expr, env: &BTreeMap<String, Type>, expected: &Type) -> Type {
        let Expr::Lambda { params, body, .. } = expr else {
            return Type::Unknown;
        };
        let expanded = self.expand_alias(expected);
        let Type::Function(expected_params, expected_result) = expanded else {
            if !matches!(expanded, Type::Unknown) {
                self.diagnostics.push(Diagnostic::new(
                    expr.span(),
                    "lambda requires an expected function type",
                ));
            }
            return Type::Unknown;
        };

        if params.len() != expected_params.len() {
            self.diagnostics.push(Diagnostic::new(
                expr.span(),
                format!(
                    "arity mismatch: expected {} lambda parameters, found {}",
                    expected_params.len(),
                    params.len()
                ),
            ));
        }

        let mut lambda_env = env.clone();
        for (index, param) in params.iter().enumerate() {
            let param_ty = expected_params.get(index).cloned().unwrap_or(Type::Unknown);
            if let Some(name) = &param.name {
                self.add_typed_span(name.span, TypedSpanKind::Parameter, &name.text, &param_ty);
                lambda_env.insert(name.text.clone(), param_ty);
            }
        }

        self.check_block(body, &lambda_env, &expected_result);
        expected.clone()
    }

    fn infer_expr(&mut self, expr: &Expr, env: &BTreeMap<String, Type>) -> Type {
        if let Some(name) = qualified_expr_name(expr)
            && name.contains('.')
            && !qualified_root_is_local(&name, &env.keys().cloned().collect::<BTreeSet<_>>())
        {
            return self.resolve_expr_name(&name, expr.span(), env);
        }

        match expr {
            Expr::Name(name) => {
                let ty = self.resolve_expr_name(&name.text, name.span, env);
                self.add_typed_span(name.span, TypedSpanKind::Value, &name.text, &ty);
                ty
            }
            Expr::Int { .. } => Type::Int,
            Expr::Text { .. } => Type::Text,
            Expr::Bool { .. } => Type::Bool,
            Expr::Unit { .. } => Type::Unit,
            Expr::List { items, .. } => {
                let elem = items
                    .first()
                    .map(|item| self.infer_expr(item, env))
                    .unwrap_or(Type::Unknown);
                for item in items.iter().skip(1) {
                    let found = self.infer_expr(item, env);
                    if !self.compatible(&found, &elem) {
                        self.diagnostics.push(Diagnostic::new(
                            item.span(),
                            format!("list item type mismatch: expected `{elem}`, found `{found}`"),
                        ));
                    }
                }
                Type::List(Box::new(elem))
            }
            Expr::Record { fields, .. } => Type::closed_record(
                fields
                    .iter()
                    .map(|field| (field.name.text.clone(), self.infer_expr(&field.value, env)))
                    .collect(),
            ),
            Expr::RecordUpdate { base, fields, .. } => self.infer_record_update(base, fields, env),
            Expr::Lambda { .. } => {
                self.diagnostics.push(Diagnostic::new(
                    expr.span(),
                    "lambda requires an expected function type",
                ));
                Type::Unknown
            }
            Expr::Field { target, field, .. } => {
                let target_ty = self.infer_expr(target, env);
                match self.expand_alias(&target_ty) {
                    Type::Row(row) if row.kind == RowKind::Record => row
                        .field(&field.text)
                        .cloned()
                        .inspect(|ty| {
                            self.add_typed_span(field.span, TypedSpanKind::Field, &field.text, ty);
                        })
                        .unwrap_or_else(|| {
                            self.diagnostics.push(Diagnostic::new(
                                field.span,
                                format!("unknown record field `{}`", field.text),
                            ));
                            Type::Unknown
                        }),
                    Type::Unknown => Type::Unknown,
                    other => {
                        self.diagnostics.push(Diagnostic::new(
                            field.span,
                            format!("field access requires a record, found `{other}`"),
                        ));
                        Type::Unknown
                    }
                }
            }
            Expr::Call { callee, args, span } => match self.infer_expr(callee, env) {
                Type::Function(params, result) => {
                    if params.len() != args.len() {
                        self.diagnostics.push(Diagnostic::new(
                            *span,
                            format!(
                                "arity mismatch: expected {} arguments, found {}",
                                params.len(),
                                args.len()
                            ),
                        ));
                    }
                    let mut subst = BTreeMap::new();
                    for (arg, expected) in args.iter().zip(params.iter()) {
                        let found = self.check_expr(arg, env, expected);
                        unify_vars(expected, &found, &mut subst);
                    }
                    substitute(&result, &subst)
                }
                Type::Unknown => Type::Unknown,
                other => {
                    self.diagnostics.push(Diagnostic::new(
                        callee.span(),
                        format!("cannot call non-function value of type `{other}`"),
                    ));
                    Type::Unknown
                }
            },
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_ty = self.infer_expr(condition, env);
                if !self.compatible(&condition_ty, &Type::Bool) {
                    self.diagnostics.push(Diagnostic::new(
                        condition.span(),
                        format!("if condition must be `Bool`, found `{condition_ty}`"),
                    ));
                }
                let then_ty = self.infer_expr(then_branch, env);
                let else_ty = self.infer_expr(else_branch, env);
                if self.compatible(&then_ty, &else_ty) {
                    then_ty
                } else {
                    self.diagnostics.push(Diagnostic::new(
                        else_branch.span(),
                        format!("if branches differ: `{then_ty}` and `{else_ty}`"),
                    ));
                    Type::Unknown
                }
            }
            Expr::Match { arms, .. } => {
                let scrutinee_ty = match expr {
                    Expr::Match { scrutinee, .. } => self.infer_expr(scrutinee, env),
                    _ => Type::Unknown,
                };
                let mut found = Type::Unknown;
                for arm in arms {
                    let mut arm_env = env.clone();
                    let mut bound_names = BTreeSet::new();
                    self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env, &mut bound_names);
                    let arm_ty = self.infer_expr(&arm.value, &arm_env);
                    if matches!(found, Type::Unknown) {
                        found = arm_ty;
                    } else if !self.compatible(&found, &arm_ty) {
                        self.diagnostics.push(Diagnostic::new(
                            arm.value.span(),
                            format!(
                                "match arm type mismatch: expected `{found}`, found `{arm_ty}`"
                            ),
                        ));
                    }
                }
                found
            }
            Expr::Binary {
                op,
                op_span,
                left,
                right,
                ..
            } => self.infer_binary_expr(*op, *op_span, left, right, env),
            Expr::Error { .. } => Type::Unknown,
        }
    }

    fn infer_binary_expr(
        &mut self,
        op: BinaryOp,
        op_span: Span,
        left: &Expr,
        right: &Expr,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let left_ty = self.infer_expr(left, env);
        let right_ty = self.infer_expr(right, env);

        let operator = binary_operator(op, &left_ty, &right_ty);
        self.add_typed_span(
            op_span,
            TypedSpanKind::Value,
            operator.label,
            &operator.display_type,
        );

        if matches!(op, BinaryOp::Equal) {
            if !self.compatible(&left_ty, &right_ty) {
                self.diagnostics.push(Diagnostic::new(
                    right.span(),
                    format!("cannot compare `{left_ty}` with `{right_ty}`"),
                ));
            }
            return operator.result;
        }

        self.check_operator_operand(operator.label, left, &left_ty, &operator.params[0]);
        self.check_operator_operand(operator.label, right, &right_ty, &operator.params[1]);

        operator.result
    }

    fn check_operator_operand(&mut self, label: &str, expr: &Expr, found: &Type, expected: &Type) {
        if !self.compatible(found, expected) {
            self.diagnostics.push(Diagnostic::new(
                expr.span(),
                format!("`{label}` expects `{expected}` operands, found `{found}`"),
            ));
        }
    }

    fn infer_record_update(
        &mut self,
        base: &Expr,
        updates: &[FieldExpr],
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let base_ty = self.infer_expr(base, env);
        let expanded = self.expand_alias(&base_ty);
        let Type::Row(mut row) = expanded else {
            if !matches!(expanded, Type::Unknown) {
                self.diagnostics.push(Diagnostic::new(
                    base.span(),
                    format!("record update requires a record, found `{base_ty}`"),
                ));
            }
            for update in updates {
                self.infer_expr(&update.value, env);
            }
            return Type::Unknown;
        };
        if row.kind != RowKind::Record || row.tail.is_some() {
            self.diagnostics.push(Diagnostic::new(
                base.span(),
                format!("record update requires a record, found `{base_ty}`"),
            ));
            for update in updates {
                self.infer_expr(&update.value, env);
            }
            return Type::Unknown;
        }

        for update in updates {
            if let Some(index) = row.field_index(&update.name.text) {
                let expected = match &row.entries[index] {
                    RowEntry::Field { ty, .. } => ty.clone(),
                    RowEntry::Case { .. } => Type::Unknown,
                };
                let value_ty = self.check_expr(&update.value, env, &expected);
                self.add_typed_span(
                    update.name.span,
                    TypedSpanKind::Field,
                    &update.name.text,
                    &expected,
                );
                row.entries[index] = RowEntry::Field {
                    name: update.name.text.clone(),
                    ty: value_ty,
                };
            } else {
                self.diagnostics.push(Diagnostic::new(
                    update.name.span,
                    format!("unknown record field `{}`", update.name.text),
                ));
            }
        }

        match base_ty {
            Type::Row(_) => Type::Row(row),
            Type::Named { .. } => base_ty,
            _ => Type::Unknown,
        }
    }

    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        expected: &Type,
        env: &mut BTreeMap<String, Type>,
        bound_names: &mut BTreeSet<String>,
    ) {
        match pattern {
            Pattern::Name(name) => {
                self.bind_pattern_name(name, expected, env, bound_names);
            }
            Pattern::Wildcard { .. } => {}
            Pattern::Int { span, .. } => {
                self.check_pattern_type(*span, &Type::Int, expected);
            }
            Pattern::Text { span, .. } => {
                self.check_pattern_type(*span, &Type::Text, expected);
            }
            Pattern::Bool { span, .. } => {
                self.check_pattern_type(*span, &Type::Bool, expected);
            }
            Pattern::Unit { span } => {
                self.check_pattern_type(*span, &Type::Unit, expected);
            }
            Pattern::Constructor { name, args, span } => {
                let expected_variant = self.expand_alias(expected);
                if let Some(row) = expected_variant.as_closed_variant() {
                    if let Some(payload) = row.case(&name.text) {
                        if payload.len() != args.len() {
                            self.diagnostics.push(Diagnostic::new(
                                *span,
                                format!(
                                    "constructor `{}` expects {} arguments, found {}",
                                    name.text,
                                    payload.len(),
                                    args.len()
                                ),
                            ));
                        }
                        for (arg, payload_ty) in args.iter().zip(payload) {
                            self.check_pattern(arg, payload_ty, env, bound_names);
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            name.span,
                            format!("constructor `{}` is not part of `{expected}`", name.text),
                        ));
                        for arg in args {
                            self.check_pattern(arg, &Type::Unknown, env, bound_names);
                        }
                    }
                } else if !matches!(expected_variant, Type::Unknown) {
                    self.diagnostics.push(Diagnostic::new(
                        name.span,
                        format!("variant pattern requires a variant, found `{expected}`"),
                    ));
                    for arg in args {
                        self.check_pattern(arg, &Type::Unknown, env, bound_names);
                    }
                } else {
                    self.constructor_type(&name.text, args.len(), *span);
                    for arg in args {
                        self.check_pattern(arg, &Type::Unknown, env, bound_names);
                    }
                }
            }
            Pattern::List { items, rest, .. } => {
                let expanded = self.expand_alias(expected);
                let item_ty = match &expanded {
                    Type::List(item) => item.as_ref().clone(),
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.diagnostics.push(Diagnostic::new(
                            pattern.span(),
                            format!("list pattern requires a list, found `{expected}`"),
                        ));
                        Type::Unknown
                    }
                };
                for item in items {
                    self.check_pattern(item, &item_ty, env, bound_names);
                }
                if let Some(rest) = rest {
                    let rest_ty = Type::List(Box::new(item_ty));
                    self.bind_pattern_name(rest, &rest_ty, env, bound_names);
                }
            }
            Pattern::Record { fields, .. } => {
                let expanded = self.expand_alias(expected);
                let Some(record_row) = expanded.as_closed_record() else {
                    if !matches!(expanded, Type::Unknown) {
                        self.diagnostics.push(Diagnostic::new(
                            pattern.span(),
                            format!("record pattern requires a record, found `{expected}`"),
                        ));
                    }
                    for field in fields {
                        self.bind_pattern_name(field, &Type::Unknown, env, bound_names);
                    }
                    return;
                };

                for field in fields {
                    if let Some(field_ty) = record_row.field(&field.text) {
                        self.bind_pattern_name(field, field_ty, env, bound_names);
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            field.span,
                            format!("unknown record field `{}`", field.text),
                        ));
                        self.bind_pattern_name(field, &Type::Unknown, env, bound_names);
                    }
                }
            }
            Pattern::Error { .. } => {}
        }
    }

    fn bind_pattern_name(
        &mut self,
        name: &Name,
        ty: &Type,
        env: &mut BTreeMap<String, Type>,
        bound_names: &mut BTreeSet<String>,
    ) {
        if !bound_names.insert(name.text.clone()) {
            self.diagnostics.push(Diagnostic::new(
                name.span,
                format!("duplicate pattern binding `{}`", name.text),
            ));
        }
        self.add_typed_span(name.span, TypedSpanKind::Pattern, &name.text, ty);
        env.insert(name.text.clone(), ty.clone());
    }

    fn check_pattern_type(&mut self, span: Span, found: &Type, expected: &Type) {
        if !self.compatible(found, expected) {
            self.diagnostics.push(Diagnostic::new(
                span,
                format!("pattern expects `{expected}`, found `{found}`"),
            ));
        }
    }

    fn constructor_type(&mut self, name: &str, arg_count: usize, span: Span) -> Option<Type> {
        let expected = self.constructor_arities.get(name)?;
        if *expected != arg_count {
            self.diagnostics.push(Diagnostic::new(
                span,
                format!("constructor `{name}` expects {expected} arguments, found {arg_count}"),
            ));
        }
        Some(Type::Unknown)
    }

    fn expand_alias(&self, ty: &Type) -> Type {
        match ty {
            Type::Named { name, args } => self
                .type_defs
                .get(name)
                .map(|def| {
                    let subst = def
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<BTreeMap<_, _>>();
                    substitute(&def.body, &subst)
                })
                .unwrap_or_else(|| ty.clone()),
            _ => ty.clone(),
        }
    }

    fn compatible(&self, found: &Type, expected: &Type) -> bool {
        compatible(&self.expand_alias(found), &self.expand_alias(expected))
    }

    fn resolve_expr_name(&mut self, name: &str, span: Span, env: &BTreeMap<String, Type>) -> Type {
        if let Some(ty) = env.get(name) {
            return ty.clone();
        }

        if let Some(ty) = primitive_value(name) {
            return ty;
        }

        match resolve_visible_symbol(self.module, self.symbol_tables, name, None) {
            Ok(Some(symbol)) => self.type_for_symbol(&symbol),
            Ok(None) => Type::Unknown,
            Err(_) => Type::Unknown,
        }
        .or_constructor(name, span, self)
    }

    fn type_for_symbol(&self, symbol: &Symbol) -> Type {
        self.symbol_types
            .get(&symbol.qualified_name)
            .cloned()
            .unwrap_or(Type::Unknown)
    }

    fn add_top_level_typed_spans(&mut self) {
        for item in &self.module.parsed.module.items {
            match item {
                Item::Import(_) => {}
                Item::Type(decl) => {
                    let ty = Type::Named {
                        name: decl.name.text.clone(),
                        args: decl
                            .params
                            .iter()
                            .map(|param| Type::Var(param.text.clone()))
                            .collect(),
                    };
                    self.add_typed_span(decl.name.span, TypedSpanKind::Type, &decl.name.text, &ty);
                    if let TypeExpr::Variant { cases, .. } = &decl.body {
                        for case in cases {
                            if let Some(ty) = self
                                .symbol_types
                                .get(&format!("{}.{}", self.module.name, case.name.text))
                            {
                                self.add_typed_span(
                                    case.name.span,
                                    TypedSpanKind::Constructor,
                                    &case.name.text,
                                    ty,
                                );
                            }
                        }
                    }
                }
                Item::Value(decl) => {
                    if let Some(ty) = self
                        .symbol_types
                        .get(&format!("{}.{}", self.module.name, decl.name.text))
                    {
                        self.add_typed_span(
                            decl.name.span,
                            TypedSpanKind::Value,
                            &decl.name.text,
                            ty,
                        );
                    }
                }
            }
        }
    }

    fn add_typed_span(&mut self, span: Span, kind: TypedSpanKind, label: &str, ty: &Type) {
        self.typed_spans.push(TypedSpan {
            module: self.module.name.clone(),
            span,
            kind,
            label: label.to_string(),
            display_type: ty.to_string(),
        });
    }

    fn lower_type(&mut self, ty: &TypeExpr) -> Type {
        let mut lowerer = TypeLowerer::default();
        let ty = lowerer.lower_type(ty);
        self.diagnostics.extend(lowerer.diagnostics);
        ty
    }
}

trait ConstructorFallback {
    fn or_constructor(self, name: &str, span: Span, checker: &mut ModuleChecker<'_>) -> Type;
}

impl ConstructorFallback for Type {
    fn or_constructor(self, name: &str, span: Span, checker: &mut ModuleChecker<'_>) -> Type {
        if !matches!(self, Type::Unknown) {
            return self;
        }
        checker
            .constructor_type(name, 0, span)
            .unwrap_or(Type::Unknown)
    }
}

fn collect_visible_types(
    module: &IndexedModule,
    symbols: &SymbolTables,
    symbol_types: &BTreeMap<String, Type>,
) -> BTreeMap<String, Type> {
    let mut env = collect_local_types(module, symbol_types);

    for import in &module.imports {
        if let Some(exports) = symbols.exported(import) {
            for symbol in exports.values() {
                if let Some(ty) = symbol_types.get(&symbol.qualified_name) {
                    env.entry(symbol.name.clone()).or_insert_with(|| ty.clone());
                }
            }
        }
    }

    for (qualified, ty) in symbol_types {
        env.entry(qualified.clone()).or_insert_with(|| ty.clone());
    }

    env.insert("unit".to_string(), Type::Unit);
    env
}

fn collect_local_types(
    module: &IndexedModule,
    symbol_types: &BTreeMap<String, Type>,
) -> BTreeMap<String, Type> {
    module
        .symbols
        .iter()
        .filter_map(|symbol| {
            symbol_types
                .get(&symbol.qualified_name)
                .map(|ty| (symbol.name.clone(), ty.clone()))
        })
        .collect()
}

fn collect_type_definitions(
    graph: &ModuleGraph,
) -> (BTreeMap<String, TypeDef>, BTreeMap<String, Vec<Diagnostic>>) {
    let mut defs = BTreeMap::new();
    let mut diagnostics = BTreeMap::new();
    for module in graph.modules.values() {
        for item in &module.parsed.module.items {
            if let Item::Type(decl) = item {
                let mut lowerer = TypeLowerer::default();
                let body = lowerer.lower_type(&decl.body);
                diagnostics
                    .entry(module.name.clone())
                    .or_insert_with(Vec::new)
                    .extend(lowerer.diagnostics);
                let def = TypeDef {
                    params: decl.params.iter().map(|param| param.text.clone()).collect(),
                    body,
                };
                defs.insert(decl.name.text.clone(), def.clone());
                defs.insert(format!("{}.{}", module.name, decl.name.text), def);
            }
        }
    }
    (defs, diagnostics)
}

fn collect_symbol_types(graph: &ModuleGraph) -> BTreeMap<String, Type> {
    let mut types = BTreeMap::new();
    for module in graph.modules.values() {
        for item in &module.parsed.module.items {
            match item {
                Item::Type(decl) => {
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Named {
                            name: decl.name.text.clone(),
                            args: Vec::new(),
                        },
                    );
                    if let TypeExpr::Variant { cases, .. } = &decl.body {
                        let result = lower_named_type_app(&decl.name.text, &decl.params);
                        for case in cases {
                            let params = case.payload.iter().map(lower_type).collect::<Vec<_>>();
                            types.insert(
                                format!("{}.{}", module.name, case.name.text),
                                Type::Function(params, Box::new(result.clone())),
                            );
                        }
                    }
                }
                Item::Value(decl) => {
                    let params = decl
                        .params
                        .iter()
                        .map(|param| lower_type(&param.annotation))
                        .collect();
                    let result = lower_type(&decl.annotation);
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Function(params, Box::new(result)),
                    );
                }
                Item::Import(_) => {}
            }
        }
    }
    types
}

fn collect_constructor_arities(graph: &ModuleGraph) -> BTreeMap<String, usize> {
    let mut arities = BTreeMap::new();
    for module in graph.modules.values() {
        for item in &module.parsed.module.items {
            if let Item::Type(decl) = item
                && let TypeExpr::Variant { cases, .. } = &decl.body
            {
                for case in cases {
                    arities.insert(case.name.text.clone(), case.payload.len());
                }
            }
        }
        for symbol in &module.symbols {
            if matches!(symbol.kind, SymbolKind::Constructor) {
                arities.entry(symbol.name.clone()).or_insert(0);
            }
        }
    }
    arities
}

fn lower_type(ty: &TypeExpr) -> Type {
    TypeLowerer::default().lower_type(ty)
}

#[derive(Default)]
struct TypeLowerer {
    diagnostics: Vec<Diagnostic>,
}

impl TypeLowerer {
    fn lower_type(&mut self, ty: &TypeExpr) -> Type {
        match ty {
            TypeExpr::Named { path, .. } => match path
                .iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>()
                .join(".")
                .as_str()
            {
                "Unit" => Type::Unit,
                "Bool" => Type::Bool,
                "Int" => Type::Int,
                "Text" => Type::Text,
                other => Type::Named {
                    name: other.to_string(),
                    args: Vec::new(),
                },
            },
            TypeExpr::Var(name) => Type::Var(name.text.clone()),
            TypeExpr::App { callee, args, .. } => {
                let callee = self.lower_type(callee);
                match callee {
                    Type::Named { name, args: _ } if name == "List" && args.len() == 1 => {
                        Type::List(Box::new(self.lower_type(&args[0])))
                    }
                    Type::Named {
                        name,
                        args: base_args,
                    } if base_args.is_empty() => Type::Named {
                        name,
                        args: args.iter().map(|arg| self.lower_type(arg)).collect(),
                    },
                    _ => Type::Unknown,
                }
            }
            TypeExpr::Function { params, result, .. } => Type::Function(
                params.iter().map(|param| self.lower_type(param)).collect(),
                Box::new(self.lower_type(result)),
            ),
            TypeExpr::Record { fields, tail, .. } => self.lower_record_type(fields, tail),
            TypeExpr::Variant { cases, tail, .. } => self.lower_variant_type(cases, tail),
            TypeExpr::Tuple { .. } | TypeExpr::Error { .. } => Type::Unknown,
        }
    }

    fn lower_record_type(&mut self, fields: &[FieldType], tail: &Option<RowTail>) -> Type {
        let mut seen = BTreeSet::new();
        let fields = fields
            .iter()
            .map(|field| {
                if !seen.insert(field.name.text.clone()) {
                    self.diagnostics.push(Diagnostic::new(
                        field.name.span,
                        format!("duplicate record field `{}`", field.name.text),
                    ));
                }
                (field.name.text.clone(), self.lower_type(&field.value))
            })
            .collect::<Vec<_>>();

        if let Some(tail) = tail {
            Type::Row(RowType {
                kind: RowKind::Record,
                entries: fields
                    .into_iter()
                    .map(|(name, ty)| RowEntry::Field { name, ty })
                    .collect(),
                tail: Some(RowTailType::Var(tail.name.text.clone())),
            })
        } else {
            Type::closed_record(fields)
        }
    }

    fn lower_variant_type(&mut self, cases: &[VariantCaseType], tail: &Option<RowTail>) -> Type {
        let mut seen = BTreeSet::new();
        let cases = cases
            .iter()
            .map(|case| {
                if !seen.insert(case.name.text.clone()) {
                    self.diagnostics.push(Diagnostic::new(
                        case.name.span,
                        format!("duplicate variant case `{}`", case.name.text),
                    ));
                }
                (
                    case.name.text.clone(),
                    case.payload
                        .iter()
                        .map(|payload| self.lower_type(payload))
                        .collect(),
                )
            })
            .collect::<Vec<_>>();

        if let Some(tail) = tail {
            Type::Row(RowType {
                kind: RowKind::Variant,
                entries: cases
                    .into_iter()
                    .map(|(name, payload)| RowEntry::Case { name, payload })
                    .collect(),
                tail: Some(RowTailType::Var(tail.name.text.clone())),
            })
        } else {
            Type::closed_variant(cases)
        }
    }
}

fn lower_named_type_app(name: &str, params: &[Name]) -> Type {
    Type::Named {
        name: name.to_string(),
        args: params
            .iter()
            .map(|param| Type::Var(param.text.clone()))
            .collect(),
    }
}

fn compatible(found: &Type, expected: &Type) -> bool {
    if found == expected
        || matches!(found, Type::Unknown)
        || matches!(expected, Type::Unknown)
        || matches!(expected, Type::Var(_))
        || matches!(found, Type::Var(_))
    {
        return true;
    }

    match (found, expected) {
        (Type::List(a), Type::List(b)) => compatible(a, b),
        (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba }) => {
            a == b && aa.len() == ba.len() && aa.iter().zip(ba).all(|(a, b)| compatible(a, b))
        }
        (Type::Row(a), Type::Row(b)) => compatible_rows(a, b),
        (Type::Function(a_params, a_result), Type::Function(b_params, b_result)) => {
            a_params.len() == b_params.len()
                && a_params.iter().zip(b_params).all(|(a, b)| compatible(a, b))
                && compatible(a_result, b_result)
        }
        _ => false,
    }
}

fn compatible_rows(found: &RowType, expected: &RowType) -> bool {
    if found.kind != expected.kind || found.tail.is_some() || expected.tail.is_some() {
        return false;
    }
    if found.entries.len() != expected.entries.len() {
        return false;
    }

    match found.kind {
        RowKind::Record => found.entries.iter().all(|entry| {
            let RowEntry::Field { name, ty } = entry else {
                return false;
            };
            expected
                .field(name)
                .is_some_and(|expected_ty| compatible(ty, expected_ty))
        }),
        RowKind::Variant => found.entries.iter().all(|entry| {
            let RowEntry::Case { name, payload } = entry else {
                return false;
            };
            expected.case(name).is_some_and(|expected_payload| {
                payload.len() == expected_payload.len()
                    && payload
                        .iter()
                        .zip(expected_payload)
                        .all(|(ty, expected_ty)| compatible(ty, expected_ty))
            })
        }),
    }
}

fn unify_vars(pattern: &Type, found: &Type, subst: &mut BTreeMap<String, Type>) {
    match pattern {
        Type::Var(name) => {
            subst.entry(name.clone()).or_insert_with(|| found.clone());
        }
        Type::List(item) => {
            if let Type::List(found_item) = found {
                unify_vars(item, found_item, subst);
            }
        }
        Type::Named { name, args } => {
            if let Type::Named {
                name: found_name,
                args: found_args,
            } = found
                && name == found_name
            {
                for (arg, found_arg) in args.iter().zip(found_args) {
                    unify_vars(arg, found_arg, subst);
                }
            }
        }
        Type::Row(row) => {
            if let Type::Row(found_row) = found
                && row.kind == found_row.kind
            {
                unify_row_vars(row, found_row, subst);
            }
        }
        Type::Function(params, result) => {
            if let Type::Function(found_params, found_result) = found {
                for (param, found_param) in params.iter().zip(found_params) {
                    unify_vars(param, found_param, subst);
                }
                unify_vars(result, found_result, subst);
            }
        }
        Type::Unit | Type::Bool | Type::Int | Type::Text | Type::Unknown => {}
    }
}

fn unify_row_vars(pattern: &RowType, found: &RowType, subst: &mut BTreeMap<String, Type>) {
    match pattern.kind {
        RowKind::Record => {
            for entry in &pattern.entries {
                let RowEntry::Field { name, ty } = entry else {
                    continue;
                };
                if let Some(found_ty) = found.field(name) {
                    unify_vars(ty, found_ty, subst);
                }
            }
        }
        RowKind::Variant => {
            for entry in &pattern.entries {
                let RowEntry::Case { name, payload } = entry else {
                    continue;
                };
                if let Some(found_payload) = found.case(name) {
                    for (ty, found_ty) in payload.iter().zip(found_payload) {
                        unify_vars(ty, found_ty, subst);
                    }
                }
            }
        }
    }
}

fn substitute(ty: &Type, subst: &BTreeMap<String, Type>) -> Type {
    match ty {
        Type::Var(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(item) => Type::List(Box::new(substitute(item, subst))),
        Type::Named { name, args } => Type::Named {
            name: name.clone(),
            args: args.iter().map(|arg| substitute(arg, subst)).collect(),
        },
        Type::Row(row) => Type::Row(substitute_row(row, subst)),
        Type::Function(params, result) => Type::Function(
            params
                .iter()
                .map(|param| substitute(param, subst))
                .collect(),
            Box::new(substitute(result, subst)),
        ),
        Type::Unit | Type::Bool | Type::Int | Type::Text | Type::Unknown => ty.clone(),
    }
}

fn substitute_row(row: &RowType, subst: &BTreeMap<String, Type>) -> RowType {
    RowType {
        kind: row.kind,
        entries: row
            .entries
            .iter()
            .map(|entry| match entry {
                RowEntry::Field { name, ty } => RowEntry::Field {
                    name: name.clone(),
                    ty: substitute(ty, subst),
                },
                RowEntry::Case { name, payload } => RowEntry::Case {
                    name: name.clone(),
                    payload: payload.iter().map(|ty| substitute(ty, subst)).collect(),
                },
            })
            .collect(),
        tail: row.tail.clone(),
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit => write!(f, "Unit"),
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Text => write!(f, "Text"),
            Type::List(item) => write!(f, "List {item}"),
            Type::Named { name, args } => {
                write!(f, "{name}")?;
                for arg in args {
                    write!(f, " {arg}")?;
                }
                Ok(())
            }
            Type::Var(name) => write!(f, "{name}"),
            Type::Row(row) => write!(f, "{row}"),
            Type::Function(params, result) => {
                write!(f, "(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ") -> {result}")
            }
            Type::Unknown => write!(f, "<unknown>"),
        }
    }
}

impl std::fmt::Display for RowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            RowKind::Record => {
                write!(f, "{{ ")?;
                let mut first = true;
                for entry in &self.entries {
                    let RowEntry::Field { name, ty } = entry else {
                        continue;
                    };
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{name}: {ty}")?;
                }
                if let Some(RowTailType::Var(name)) = &self.tail {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "...{name}")?;
                }
                write!(f, " }}")
            }
            RowKind::Variant => {
                write!(f, "< ")?;
                let mut first = true;
                for entry in &self.entries {
                    let RowEntry::Case { name, payload } = entry else {
                        continue;
                    };
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{name}(")?;
                    for (j, ty) in payload.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{ty}")?;
                    }
                    write!(f, ")")?;
                }
                if let Some(RowTailType::Var(name)) = &self.tail {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "...{name}")?;
                }
                write!(f, " >")
            }
        }
    }
}

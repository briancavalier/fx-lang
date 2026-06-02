use crate::module::{IndexedModule, ModuleGraph, Symbol, SymbolKind};
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
    Record(Vec<(String, Type)>),
    Variant(Vec<(String, Vec<Type>)>),
    Function(Vec<Type>, Box<Type>),
    Unknown,
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
    let type_defs = collect_type_definitions(graph);
    let symbol_types = collect_symbol_types(graph);
    let constructor_arities = collect_constructor_arities(graph);
    let symbol_tables = SymbolTables::new(graph);

    for module in graph.modules.values() {
        let mut checker = ModuleChecker {
            module,
            diagnostics: Vec::new(),
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
            let ty = lower_type(&param.annotation);
            self.add_typed_span(
                param.name.span,
                TypedSpanKind::Parameter,
                &param.name.text,
                &ty,
            );
            env.insert(param.name.text.clone(), ty);
        }
        for statement in &decl.body.statements {
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
        let found = self.infer_expr(&decl.body.result, &env);
        let expected = lower_type(&decl.annotation);
        if !self.compatible(&found, &expected) {
            self.diagnostics.push(Diagnostic::new(
                decl.body.result.span(),
                format!("expected `{expected}`, found `{found}`"),
            ));
        }
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
            Expr::Record { fields, .. } => Type::Record(
                fields
                    .iter()
                    .map(|field| (field.name.text.clone(), self.infer_expr(&field.value, env)))
                    .collect(),
            ),
            Expr::Field { target, field, .. } => {
                let target_ty = self.infer_expr(target, env);
                match self.expand_alias(&target_ty) {
                    Type::Record(fields) => fields
                        .iter()
                        .find_map(|(name, ty)| (name == &field.text).then_some(ty.clone()))
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
                        let found = self.infer_expr(arg, env);
                        unify_vars(expected, &found, &mut subst);
                        if !self.compatible(&found, expected) {
                            self.diagnostics.push(Diagnostic::new(
                                arg.span(),
                                format!("expected `{expected}`, found `{found}`"),
                            ));
                        }
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
                    self.bind_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env);
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
                op, left, right, ..
            } => {
                let left_ty = self.infer_expr(left, env);
                let right_ty = self.infer_expr(right, env);
                match op {
                    BinaryOp::Equal => {
                        if !self.compatible(&left_ty, &right_ty) {
                            self.diagnostics.push(Diagnostic::new(
                                right.span(),
                                format!("cannot compare `{left_ty}` with `{right_ty}`"),
                            ));
                        }
                        Type::Bool
                    }
                    BinaryOp::Concat => {
                        if !self.compatible(&left_ty, &Type::Text)
                            || !self.compatible(&right_ty, &Type::Text)
                        {
                            self.diagnostics
                                .push(Diagnostic::new(expr.span(), "`++` expects `Text` operands"));
                        }
                        Type::Text
                    }
                    BinaryOp::Add | BinaryOp::Subtract => Type::Int,
                }
            }
            Expr::Error { .. } => Type::Unknown,
        }
    }

    fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        expected: &Type,
        env: &mut BTreeMap<String, Type>,
    ) {
        match pattern {
            Pattern::Name(name) => {
                self.add_typed_span(name.span, TypedSpanKind::Pattern, &name.text, expected);
                env.insert(name.text.clone(), expected.clone());
            }
            Pattern::Constructor { name, args, span } => {
                let expected_variant = self.expand_alias(expected);
                if let Type::Variant(cases) = expected_variant {
                    if let Some((_, payload)) =
                        cases.iter().find(|(case_name, _)| case_name == &name.text)
                    {
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
                        for (arg, expected) in args.iter().zip(payload) {
                            self.bind_pattern(arg, expected, env);
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            name.span,
                            format!("constructor `{}` is not part of `{expected}`", name.text),
                        ));
                        for arg in args {
                            self.bind_pattern(arg, &Type::Unknown, env);
                        }
                    }
                } else {
                    self.constructor_type(&name.text, args.len(), *span);
                    for arg in args {
                        self.bind_pattern(arg, &Type::Unknown, env);
                    }
                }
            }
            Pattern::List { items, .. } => {
                let item_ty = match expected {
                    Type::List(item) => item.as_ref().clone(),
                    _ => Type::Unknown,
                };
                for item in items {
                    self.bind_pattern(item, &item_ty, env);
                }
                if let Pattern::List {
                    rest: Some(rest), ..
                } = pattern
                {
                    let rest_ty = Type::List(Box::new(item_ty));
                    self.add_typed_span(rest.span, TypedSpanKind::Pattern, &rest.text, &rest_ty);
                    env.insert(rest.text.clone(), rest_ty);
                }
            }
            Pattern::Record { .. }
            | Pattern::Wildcard { .. }
            | Pattern::Int { .. }
            | Pattern::Text { .. }
            | Pattern::Bool { .. }
            | Pattern::Unit { .. }
            | Pattern::Error { .. } => {}
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

fn collect_type_definitions(graph: &ModuleGraph) -> BTreeMap<String, TypeDef> {
    let mut defs = BTreeMap::new();
    for module in graph.modules.values() {
        for item in &module.parsed.module.items {
            if let Item::Type(decl) = item {
                let def = TypeDef {
                    params: decl.params.iter().map(|param| param.text.clone()).collect(),
                    body: lower_type(&decl.body),
                };
                defs.insert(decl.name.text.clone(), def.clone());
                defs.insert(format!("{}.{}", module.name, decl.name.text), def);
            }
        }
    }
    defs
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
            let callee = lower_type(callee);
            match callee {
                Type::Named { name, args: _ } if name == "List" && args.len() == 1 => {
                    Type::List(Box::new(lower_type(&args[0])))
                }
                Type::Named {
                    name,
                    args: base_args,
                } if base_args.is_empty() => Type::Named {
                    name,
                    args: args.iter().map(lower_type).collect(),
                },
                _ => Type::Unknown,
            }
        }
        TypeExpr::Record { fields, .. } => Type::Record(
            fields
                .iter()
                .map(|field| (field.name.text.clone(), lower_type(&field.value)))
                .collect(),
        ),
        TypeExpr::Variant { cases, .. } => Type::Variant(
            cases
                .iter()
                .map(|case| {
                    (
                        case.name.text.clone(),
                        case.payload.iter().map(lower_type).collect(),
                    )
                })
                .collect(),
        ),
        TypeExpr::Tuple { .. } | TypeExpr::Error { .. } => Type::Unknown,
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
        (Type::Record(a), Type::Record(b)) => {
            a.len() == b.len()
                && a.iter().all(|(name, ty)| {
                    b.iter()
                        .find_map(|(b_name, b_ty)| (b_name == name).then_some(b_ty))
                        .is_some_and(|b_ty| compatible(ty, b_ty))
                })
        }
        (Type::Variant(a), Type::Variant(b)) => {
            a.len() == b.len()
                && a.iter().all(|(name, payload)| {
                    b.iter()
                        .find_map(|(b_name, b_payload)| (b_name == name).then_some(b_payload))
                        .is_some_and(|b_payload| {
                            payload.len() == b_payload.len()
                                && payload
                                    .iter()
                                    .zip(b_payload)
                                    .all(|(ty, b_ty)| compatible(ty, b_ty))
                        })
                })
        }
        (Type::Function(a_params, a_result), Type::Function(b_params, b_result)) => {
            a_params.len() == b_params.len()
                && a_params.iter().zip(b_params).all(|(a, b)| compatible(a, b))
                && compatible(a_result, b_result)
        }
        _ => false,
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
        Type::Record(fields) => {
            if let Type::Record(found_fields) = found {
                for (name, ty) in fields {
                    if let Some((_, found_ty)) = found_fields
                        .iter()
                        .find(|(found_name, _)| found_name == name)
                    {
                        unify_vars(ty, found_ty, subst);
                    }
                }
            }
        }
        Type::Variant(cases) => {
            if let Type::Variant(found_cases) = found {
                for (name, payload) in cases {
                    if let Some((_, found_payload)) = found_cases
                        .iter()
                        .find(|(found_name, _)| found_name == name)
                    {
                        for (ty, found_ty) in payload.iter().zip(found_payload) {
                            unify_vars(ty, found_ty, subst);
                        }
                    }
                }
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

fn substitute(ty: &Type, subst: &BTreeMap<String, Type>) -> Type {
    match ty {
        Type::Var(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(item) => Type::List(Box::new(substitute(item, subst))),
        Type::Named { name, args } => Type::Named {
            name: name.clone(),
            args: args.iter().map(|arg| substitute(arg, subst)).collect(),
        },
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute(ty, subst)))
                .collect(),
        ),
        Type::Variant(cases) => Type::Variant(
            cases
                .iter()
                .map(|(name, payload)| {
                    (
                        name.clone(),
                        payload.iter().map(|ty| substitute(ty, subst)).collect(),
                    )
                })
                .collect(),
        ),
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
            Type::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                write!(f, " }}")
            }
            Type::Variant(cases) => {
                write!(f, "< ")?;
                for (i, (name, payload)) in cases.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}(")?;
                    for (j, ty) in payload.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{ty}")?;
                    }
                    write!(f, ")")?;
                }
                write!(f, " >")
            }
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

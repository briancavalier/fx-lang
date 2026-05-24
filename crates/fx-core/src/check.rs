use crate::module::{IndexedModule, ModuleGraph, SymbolKind};
use fx_syntax::ast::*;
use fx_syntax::diagnostic::Diagnostic;
use fx_syntax::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub module_types: BTreeMap<String, BTreeMap<String, Type>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Text,
    List(Box<Type>),
    Named(String),
    Var(String),
    Record(Vec<(String, Type)>),
    Variant(Vec<(String, Vec<Type>)>),
    Function(Vec<Type>, Box<Type>),
    Unknown,
}

pub fn check_workspace(graph: &ModuleGraph) -> CheckResult {
    let mut diagnostics = graph.diagnostics.clone();
    let mut module_types = BTreeMap::new();
    let constructor_arities = collect_constructor_arities(graph);

    for module in graph.modules.values() {
        let mut checker = ModuleChecker {
            module,
            diagnostics: Vec::new(),
            top_level: collect_top_level_types(module),
            constructor_arities: &constructor_arities,
        };
        let types = checker.check();
        diagnostics.extend(checker.diagnostics);
        module_types.insert(module.name.clone(), types);
    }

    CheckResult {
        diagnostics,
        module_types,
    }
}

struct ModuleChecker<'a> {
    module: &'a IndexedModule,
    diagnostics: Vec<Diagnostic>,
    top_level: BTreeMap<String, Type>,
    constructor_arities: &'a BTreeMap<String, usize>,
}

impl ModuleChecker<'_> {
    fn check(&mut self) -> BTreeMap<String, Type> {
        let items = self.module.parsed.module.items.clone();
        for item in &items {
            if let Item::Value(decl) = item {
                self.check_value_decl(decl);
            }
        }
        self.top_level.clone()
    }

    fn check_value_decl(&mut self, decl: &ValueDecl) {
        let mut env = self.top_level.clone();
        for param in &decl.params {
            env.insert(param.name.text.clone(), lower_type(&param.annotation));
        }
        for statement in &decl.body.statements {
            match statement {
                Stmt::Let(binding) => {
                    let ty = self.infer_expr(&binding.value, &env);
                    env.insert(binding.name.text.clone(), ty);
                }
            }
        }
        let found = self.infer_expr(&decl.body.result, &env);
        let expected = lower_type(&decl.annotation);
        if !compatible(&found, &expected) {
            self.diagnostics.push(Diagnostic::new(
                decl.body.result.span(),
                format!("expected `{expected}`, found `{found}`"),
            ));
        }
    }

    fn infer_expr(&mut self, expr: &Expr, env: &BTreeMap<String, Type>) -> Type {
        match expr {
            Expr::Name(name) => env
                .get(&name.text)
                .cloned()
                .or_else(|| self.constructor_type(&name.text, 0, name.span))
                .unwrap_or_else(|| {
                    self.diagnostics.push(Diagnostic::new(
                        name.span,
                        format!("unknown name `{}`", name.text),
                    ));
                    Type::Unknown
                }),
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
                    if !compatible(&found, &elem) {
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
            Expr::Field { target, field, .. } => match self.infer_expr(target, env) {
                Type::Record(fields) => fields
                    .iter()
                    .find_map(|(name, ty)| (name == &field.text).then_some(ty.clone()))
                    .unwrap_or_else(|| {
                        self.diagnostics.push(Diagnostic::new(
                            field.span,
                            format!("unknown record field `{}`", field.text),
                        ));
                        Type::Unknown
                    }),
                Type::Named(_) | Type::Unknown => Type::Unknown,
                other => {
                    self.diagnostics.push(Diagnostic::new(
                        field.span,
                        format!("field access requires a record, found `{other}`"),
                    ));
                    Type::Unknown
                }
            },
            Expr::Call { callee, args, span } => {
                let callee_name = match callee.as_ref() {
                    Expr::Name(name) => Some(name.text.as_str()),
                    _ => None,
                };
                if let Some(name) = callee_name
                    && let Some(ty) = self.constructor_type(name, args.len(), *span)
                {
                    for arg in args {
                        self.infer_expr(arg, env);
                    }
                    return ty;
                }
                match self.infer_expr(callee, env) {
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
                        for (arg, expected) in args.iter().zip(params) {
                            let found = self.infer_expr(arg, env);
                            if !compatible(&found, &expected) {
                                self.diagnostics.push(Diagnostic::new(
                                    arg.span(),
                                    format!("expected `{expected}`, found `{found}`"),
                                ));
                            }
                        }
                        *result
                    }
                    Type::Unknown => Type::Unknown,
                    other => {
                        self.diagnostics.push(Diagnostic::new(
                            callee.span(),
                            format!("cannot call non-function value of type `{other}`"),
                        ));
                        Type::Unknown
                    }
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_ty = self.infer_expr(condition, env);
                if !compatible(&condition_ty, &Type::Bool) {
                    self.diagnostics.push(Diagnostic::new(
                        condition.span(),
                        format!("if condition must be `Bool`, found `{condition_ty}`"),
                    ));
                }
                let then_ty = self.infer_expr(then_branch, env);
                let else_ty = self.infer_expr(else_branch, env);
                if compatible(&then_ty, &else_ty) {
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
                    } else if !compatible(&found, &arm_ty) {
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
                        if !compatible(&left_ty, &right_ty) {
                            self.diagnostics.push(Diagnostic::new(
                                right.span(),
                                format!("cannot compare `{left_ty}` with `{right_ty}`"),
                            ));
                        }
                        Type::Bool
                    }
                    BinaryOp::Concat => {
                        if !compatible(&left_ty, &Type::Text) || !compatible(&right_ty, &Type::Text)
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
                env.insert(name.text.clone(), expected.clone());
            }
            Pattern::Constructor { name, args, span } => {
                self.constructor_type(&name.text, args.len(), *span);
                for arg in args {
                    self.bind_pattern(arg, &Type::Unknown, env);
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
                    env.insert(rest.text.clone(), Type::List(Box::new(item_ty)));
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
}

fn collect_top_level_types(module: &IndexedModule) -> BTreeMap<String, Type> {
    let mut env = BTreeMap::new();
    for item in &module.parsed.module.items {
        match item {
            Item::Type(decl) => {
                env.insert(decl.name.text.clone(), Type::Named(decl.name.text.clone()));
            }
            Item::Value(decl) => {
                let params = decl
                    .params
                    .iter()
                    .map(|param| lower_type(&param.annotation))
                    .collect();
                let result = lower_type(&decl.annotation);
                env.insert(
                    decl.name.text.clone(),
                    Type::Function(params, Box::new(result)),
                );
            }
            Item::Import(_) => {}
        }
    }
    env.insert("unit".to_string(), Type::Unit);
    env
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
            other => Type::Named(other.to_string()),
        },
        TypeExpr::Var(name) => Type::Var(name.text.clone()),
        TypeExpr::App { callee, args, .. } => {
            let callee = lower_type(callee);
            if matches!(callee, Type::Named(ref name) if name == "List") && args.len() == 1 {
                Type::List(Box::new(lower_type(&args[0])))
            } else {
                Type::Unknown
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

fn compatible(found: &Type, expected: &Type) -> bool {
    found == expected
        || matches!(found, Type::Unknown)
        || matches!(expected, Type::Unknown)
        || matches!(expected, Type::Var(_))
        || matches!(found, Type::Var(_))
        || matches!((found, expected), (Type::Named(a), Type::Named(b)) if a == b)
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit => write!(f, "Unit"),
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Text => write!(f, "Text"),
            Type::List(item) => write!(f, "List {item}"),
            Type::Named(name) | Type::Var(name) => write!(f, "{name}"),
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

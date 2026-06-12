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
    Named {
        name: String,
        args: Vec<Type>,
    },
    Var(TypeVar),
    Row(RowType),
    Effectful {
        result: Box<Type>,
        effects: EffectRowType,
    },
    Contextual {
        result: Box<Type>,
        context: ContextRowType,
    },
    Function(Vec<Type>, Box<Type>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowType {
    pub kind: RowKind,
    pub entries: Vec<RowEntry>,
    pub tail: Option<RowTailType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRowType {
    pub effects: Vec<Type>,
    pub tail: Option<RowTailType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRowType {
    pub fields: Vec<ContextFieldType>,
    pub tail: Option<RowTailType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFieldType {
    pub name: String,
    pub ty: Type,
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
    Var(RowVar),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TypeVar {
    name: String,
    scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowVar {
    name: String,
    scope: Option<String>,
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
    Effect,
    Operation,
    Scope,
    Constructor,
    Parameter,
    Local,
    Pattern,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeDef {
    params: Vec<TypeVar>,
    body: Type,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Substitution {
    types: BTreeMap<TypeVar, Type>,
    rows: BTreeMap<RowVar, RowType>,
    effects: BTreeMap<RowVar, EffectRowType>,
    contexts: BTreeMap<RowVar, ContextRowType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompatibilityError {
    Mismatch,
    TypeMismatch {
        expected: Box<Type>,
        found: Box<Type>,
    },
    RowKind {
        expected: RowKind,
        found: RowKind,
    },
    MissingField {
        name: String,
    },
    MissingCase {
        name: String,
    },
    UnhandledEffects {
        effects: EffectRowType,
    },
    MissingContext {
        context: ContextRowType,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameUse {
    Value,
    Callee,
}

#[derive(Clone)]
enum CallArg<'a> {
    Expr(&'a Expr),
    Synthetic { ty: Type, span: Span },
}

pub fn check_workspace(graph: &ModuleGraph) -> CheckResult {
    let resolved = resolve_workspace(graph);
    let mut diagnostics = resolved.diagnostics;
    let mut module_diagnostics = BTreeMap::new();
    let mut module_types = BTreeMap::new();
    let mut typed_spans = BTreeMap::new();
    let (type_defs, mut lowering_diagnostics) = collect_type_definitions(graph);
    let symbol_types = collect_symbol_types(graph);
    let effect_families = collect_effect_families(graph);
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
            effect_families: &effect_families,
            type_defs: &type_defs,
            constructor_arities: &constructor_arities,
            symbol_tables: &symbol_tables,
            typed_spans: Vec::new(),
            default_scopes: Vec::new(),
            context_providers: Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectFamilyInfo {
    scoped: bool,
    param_count: usize,
}

struct ModuleChecker<'a> {
    module: &'a IndexedModule,
    diagnostics: Vec<Diagnostic>,
    top_level: BTreeMap<String, Type>,
    symbol_types: &'a BTreeMap<String, Type>,
    effect_families: &'a BTreeMap<String, EffectFamilyInfo>,
    type_defs: &'a BTreeMap<String, TypeDef>,
    constructor_arities: &'a BTreeMap<String, usize>,
    symbol_tables: &'a SymbolTables,
    typed_spans: Vec<TypedSpan>,
    default_scopes: Vec<(Name, Type)>,
    context_providers: Vec<BTreeMap<String, Type>>,
}

impl TypeVar {
    pub(crate) fn unscoped(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope: None,
        }
    }

    fn scoped(name: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope: Some(scope.into()),
        }
    }
}

impl RowVar {
    fn unscoped(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope: None,
        }
    }

    fn scoped(name: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope: Some(scope.into()),
        }
    }
}

impl std::fmt::Display for TypeVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl std::fmt::Display for RowVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
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

    fn as_variant_row(&self) -> Option<&RowType> {
        match self {
            Type::Row(row) if row.kind == RowKind::Variant => Some(row),
            _ => None,
        }
    }
}

impl EffectRowType {
    fn empty() -> Self {
        Self {
            effects: Vec::new(),
            tail: None,
        }
    }

    fn singleton(effect: Type) -> Self {
        Self {
            effects: vec![effect],
            tail: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.tail.is_none()
    }

    fn extend(&mut self, other: EffectRowType) {
        for effect in other.effects {
            if !self.effects.iter().any(|existing| existing == &effect) {
                self.effects.push(effect);
            }
        }
        if self.tail.is_none() {
            self.tail = other.tail;
        } else if self.tail == other.tail {
            // Keep the existing equivalent tail.
        }
    }
}

fn split_effectful(ty: Type) -> (Type, EffectRowType) {
    match ty {
        Type::Effectful { result, effects } => (*result, effects),
        Type::Contextual { result, context } => {
            let (result, effects) = split_effectful(*result);
            (attach_context(result, context), effects)
        }
        other => (other, EffectRowType::empty()),
    }
}

fn attach_effects(result: Type, effects: EffectRowType) -> Type {
    if effects.is_empty() {
        result
    } else {
        if let Type::Contextual { result, context } = result {
            return attach_context(attach_effects(*result, effects), context);
        }
        let (result, existing) = split_effectful(result);
        let mut effects = effects;
        effects.extend(existing);
        Type::Effectful {
            result: Box::new(result),
            effects,
        }
    }
}

fn attach_context(result: Type, context: ContextRowType) -> Type {
    if context.is_empty() {
        result
    } else {
        Type::Contextual {
            result: Box::new(result),
            context,
        }
    }
}

fn strip_context_fields(ty: Type, provided: &BTreeSet<String>) -> Type {
    match ty {
        Type::Contextual { result, context } => {
            let context = ContextRowType {
                fields: context
                    .fields
                    .into_iter()
                    .filter(|field| !provided.contains(&field.name))
                    .collect(),
                tail: context.tail,
            };
            attach_context(*result, context)
        }
        other => other,
    }
}

fn remove_effects(row: &mut EffectRowType, handled: &EffectRowType) -> bool {
    let mut removed = false;
    for handled_effect in &handled.effects {
        if let Some(index) = row.effects.iter().position(|effect| {
            unify_compatible(effect, handled_effect, &mut Substitution::default()).is_ok()
        }) {
            row.effects.remove(index);
            removed = true;
        }
    }
    removed
}

fn effect_row_performed_by_action(handled: &EffectRowType, action: &EffectRowType) -> bool {
    handled.effects.iter().all(|handled_effect| {
        action.effects.iter().any(|action_effect| {
            unify_compatible(action_effect, handled_effect, &mut Substitution::default()).is_ok()
        })
    })
}

fn expected_result_type(expected: &Type) -> Type {
    match expected {
        Type::Effectful { result, .. } => result.as_ref().clone(),
        Type::Contextual { result, .. } => expected_result_type(result),
        other => other.clone(),
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

impl ContextRowType {
    fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.tail.is_none()
    }
}

impl ModuleChecker<'_> {
    fn check(&mut self) -> CheckedModule {
        let items = self.module.parsed.module.items.clone();
        self.add_top_level_typed_spans();
        self.check_scope_decls(&items);
        self.check_effect_annotations(&items);
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

    fn check_scope_decls(&mut self, items: &[Item]) {
        for item in items {
            let Item::Scope(decl) = item else {
                continue;
            };
            let Some(protocol) = &decl.protocol else {
                continue;
            };
            if protocol.tail.is_some() {
                self.diagnostics.push(Diagnostic::new(
                    protocol.span,
                    "scope protocol rows cannot use row tails in Phase 4",
                ));
            }
            if protocol.effects.len() != 1 {
                self.diagnostics.push(Diagnostic::new(
                    protocol.span,
                    "scope protocol rows support one scoped effect family in Phase 4",
                ));
            }
            for effect in &protocol.effects {
                let Some((family, arg_count, span)) = effect_row_member_parts(effect) else {
                    self.diagnostics.push(Diagnostic::new(
                        effect.span(),
                        "scope protocol row must name a scoped effect family",
                    ));
                    continue;
                };
                let Some(info) = self.resolve_effect_family_info(&family, span) else {
                    continue;
                };
                if !info.scoped {
                    self.diagnostics.push(Diagnostic::new(
                        span,
                        "scope protocol row must name a scoped effect family",
                    ));
                    continue;
                }
                let expected = info.param_count.saturating_sub(1);
                if arg_count != expected {
                    self.diagnostics.push(Diagnostic::new(
                        effect.span(),
                        format!(
                            "scope protocol `{family}` expects {expected} argument{}, found {}",
                            if expected == 1 { "" } else { "s" },
                            arg_count
                        ),
                    ));
                }
            }
        }
    }

    fn check_effect_annotations(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Value(decl) => {
                    for param in &decl.params {
                        self.check_effect_annotation_type(&param.annotation);
                    }
                    self.check_effect_annotation_type(&decl.annotation);
                }
                Item::Effect(decl) => {
                    for operation in &decl.operations {
                        for param in &operation.params {
                            self.check_effect_annotation_type(&param.annotation);
                        }
                        self.check_effect_annotation_type(&operation.result);
                    }
                }
                Item::Type(decl) => self.check_effect_annotation_type(&decl.body),
                Item::Import(_) | Item::Scope(_) => {}
            }
        }
    }

    fn check_effect_annotation_type(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::App { callee, args, .. } => {
                self.check_effect_annotation_type(callee);
                for arg in args {
                    self.check_effect_annotation_type(arg);
                }
            }
            TypeExpr::Function { params, result, .. } => {
                for param in params {
                    self.check_effect_annotation_type(param);
                }
                self.check_effect_annotation_type(result);
            }
            TypeExpr::Effect {
                result, effects, ..
            } => {
                self.check_effect_annotation_type(result);
                for effect in &effects.effects {
                    self.check_effect_row_member(effect);
                }
            }
            TypeExpr::Context {
                result, context, ..
            } => {
                self.check_effect_annotation_type(result);
                for field in &context.fields {
                    self.check_effect_annotation_type(&field.value);
                }
            }
            TypeExpr::Record { fields, .. } => {
                for field in fields {
                    self.check_effect_annotation_type(&field.value);
                }
            }
            TypeExpr::Variant { cases, .. } => {
                for case in cases {
                    for payload in &case.payload {
                        self.check_effect_annotation_type(payload);
                    }
                }
            }
            TypeExpr::Tuple { elems, .. } => {
                for elem in elems {
                    self.check_effect_annotation_type(elem);
                }
            }
            TypeExpr::Named { .. } | TypeExpr::Var(_) | TypeExpr::Error { .. } => {}
        }
    }

    fn check_effect_row_member(&mut self, effect: &TypeExpr) {
        let Some((family, arg_count, span)) = effect_row_member_parts(effect) else {
            return;
        };
        let Some(info) = self.resolve_effect_family_info(&family, span) else {
            return;
        };
        if arg_count != info.param_count {
            self.diagnostics.push(Diagnostic::new(
                effect.span(),
                format!(
                    "effect row `{family}` expects {} argument{}, found {}",
                    info.param_count,
                    if info.param_count == 1 { "" } else { "s" },
                    arg_count
                ),
            ));
        }
    }

    fn resolve_effect_family_info(&mut self, family: &str, span: Span) -> Option<EffectFamilyInfo> {
        match resolve_visible_symbol(
            self.module,
            self.symbol_tables,
            family,
            Some(SymbolKind::Effect),
        ) {
            Ok(Some(symbol)) => self.effect_families.get(&symbol.qualified_name).cloned(),
            Ok(None) => {
                self.diagnostics.push(Diagnostic::new(
                    span,
                    format!("unknown effect family `{family}`"),
                ));
                None
            }
            Err(symbols) => {
                let candidates = symbols
                    .iter()
                    .map(|symbol| symbol.qualified_name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.push(Diagnostic::new(
                    span,
                    format!("ambiguous effect family `{family}`; candidates: {candidates}"),
                ));
                None
            }
        }
    }

    fn check_value_decl(&mut self, decl: &ValueDecl) {
        let mut env = self.top_level.clone();
        let mut lowerer = TypeLowerer::for_value_decl(&self.module.name, decl);
        let params = decl
            .params
            .iter()
            .map(|param| lowerer.lower_type(&param.annotation))
            .collect::<Vec<_>>();
        let expected = lowerer.lower_type(&decl.annotation);
        self.diagnostics.extend(lowerer.diagnostics);

        let context_bindings = self.add_context_bindings(&decl.annotation, &expected, &mut env);
        for (param, ty) in decl.params.iter().zip(params) {
            self.add_typed_span(
                param.name.span,
                TypedSpanKind::Parameter,
                &param.name.text,
                &ty,
            );
            env.insert(param.name.text.clone(), ty);
        }
        let provided_context = context_bindings
            .as_ref()
            .map(|bindings| bindings.keys().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let body_expected = strip_context_fields(expected.clone(), &provided_context);
        let has_context_bindings = context_bindings.is_some();
        if let Some(bindings) = context_bindings {
            self.context_providers.push(bindings);
        }
        self.check_block(&decl.body, &env, &body_expected);
        if has_context_bindings {
            self.context_providers.pop();
        }
    }

    fn add_context_bindings(
        &mut self,
        annotation: &TypeExpr,
        expected: &Type,
        env: &mut BTreeMap<String, Type>,
    ) -> Option<BTreeMap<String, Type>> {
        let (
            TypeExpr::Context {
                context: source_context,
                ..
            },
            Type::Contextual { context, .. },
        ) = (annotation, expected)
        else {
            return None;
        };

        let mut providers = BTreeMap::new();
        for (source_field, field) in source_context.fields.iter().zip(&context.fields) {
            self.add_typed_span(
                source_field.name.span,
                TypedSpanKind::Parameter,
                &source_field.name.text,
                &field.ty,
            );
            env.insert(source_field.name.text.clone(), field.ty.clone());
            providers.insert(source_field.name.text.clone(), field.ty.clone());
        }
        Some(providers)
    }

    fn check_block(
        &mut self,
        block: &Block,
        env: &BTreeMap<String, Type>,
        expected: &Type,
    ) -> Type {
        let mut env = env.clone();
        let mut effects = EffectRowType::empty();
        for statement in &block.statements {
            match statement {
                Stmt::Let(binding) => {
                    let ty = self.infer_expr(&binding.value, &env);
                    let (ty, binding_effects) = split_effectful(ty);
                    effects.extend(binding_effects);
                    self.add_typed_span(
                        binding.name.span,
                        TypedSpanKind::Local,
                        &binding.name.text,
                        &ty,
                    );
                    env.insert(binding.name.text.clone(), ty);
                }
                Stmt::Expr(expr) => {
                    let ty = self.infer_expr(expr, &env);
                    let (_, expr_effects) = split_effectful(ty);
                    effects.extend(expr_effects);
                }
            }
        }
        let result = self.check_expr(&block.result, &env, expected);
        let (result, result_effects) = split_effectful(result);
        let has_statement_effects = !effects.is_empty();
        effects.extend(result_effects);
        let found = attach_effects(result, effects);
        if has_statement_effects && !self.compatible(&found, expected) {
            let message = self.compatibility_message(
                &found,
                expected,
                format!("expected `{expected}`, found `{found}`"),
            );
            self.diagnostics
                .push(Diagnostic::new(block.result.span(), message));
        }
        found
    }

    fn check_expr(&mut self, expr: &Expr, env: &BTreeMap<String, Type>, expected: &Type) -> Type {
        match expr {
            Expr::Lambda { .. } => self.check_lambda(expr, env, expected),
            Expr::List { items, .. } => {
                if let Type::List(item_expected) = self.expand_alias(expected) {
                    let mut effects = EffectRowType::empty();
                    for item in items {
                        let item_ty = self.check_expr(item, env, &item_expected);
                        let (_, item_effects) = split_effectful(item_ty);
                        effects.extend(item_effects);
                    }
                    attach_effects(expected.clone(), effects)
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
                let condition_ty = self.check_expr(condition, env, &Type::Bool);
                let (_, condition_effects) = split_effectful(condition_ty);
                let then_ty = self.check_expr(then_branch, env, expected);
                let (_, then_effects) = split_effectful(then_ty);
                let else_ty = self.check_expr(else_branch, env, expected);
                let (_, else_effects) = split_effectful(else_ty);
                let mut effects = condition_effects;
                effects.extend(then_effects);
                effects.extend(else_effects);
                attach_effects(expected_result_type(expected), effects)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrutinee_ty = self.infer_expr(scrutinee, env);
                let (scrutinee_ty, scrutinee_effects) = split_effectful(scrutinee_ty);
                let mut effects = scrutinee_effects;
                for arm in arms {
                    let mut arm_env = env.clone();
                    let mut bound_names = BTreeSet::new();
                    self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env, &mut bound_names);
                    let arm_ty = self.check_expr(&arm.value, &arm_env, expected);
                    let (_, arm_effects) = split_effectful(arm_ty);
                    effects.extend(arm_effects);
                }
                attach_effects(expected_result_type(expected), effects)
            }
            Expr::With { .. } => self.check_inferred_expr(expr, env, expected),
            Expr::Given { .. } => self.check_inferred_expr(expr, env, expected),
            Expr::Within { .. } => self.check_inferred_expr(expr, env, expected),
            Expr::Handle { .. } => self.check_handle_expr(expr, env, expected),
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
            let message = self.compatibility_message(
                &found,
                expected,
                format!("expected `{expected}`, found `{found}`"),
            );
            self.diagnostics.push(Diagnostic::new(expr.span(), message));
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
            return self.resolve_expr_name(&name, expr.span(), env, NameUse::Value);
        }

        match expr {
            Expr::Name(name) => {
                let ty = self.resolve_expr_name(&name.text, name.span, env, NameUse::Value);
                self.add_typed_span(name.span, TypedSpanKind::Value, &name.text, &ty);
                ty
            }
            Expr::Int { .. } => Type::Int,
            Expr::Text { .. } => Type::Text,
            Expr::Bool { .. } => Type::Bool,
            Expr::Unit { .. } => Type::Unit,
            Expr::List { items, .. } => {
                let mut effects = EffectRowType::empty();
                let elem = items
                    .first()
                    .map(|item| {
                        let item_ty = self.infer_expr(item, env);
                        let (item_ty, item_effects) = split_effectful(item_ty);
                        effects.extend(item_effects);
                        item_ty
                    })
                    .unwrap_or(Type::Unknown);
                for item in items.iter().skip(1) {
                    let found = self.infer_expr(item, env);
                    let (found, item_effects) = split_effectful(found);
                    effects.extend(item_effects);
                    if !self.compatible(&found, &elem) {
                        let message = self.compatibility_message(
                            &found,
                            &elem,
                            format!("list item type mismatch: expected `{elem}`, found `{found}`"),
                        );
                        self.diagnostics.push(Diagnostic::new(item.span(), message));
                    }
                }
                attach_effects(Type::List(Box::new(elem)), effects)
            }
            Expr::Record { fields, .. } => {
                let mut effects = EffectRowType::empty();
                let fields = fields
                    .iter()
                    .map(|field| {
                        let ty = self.infer_expr(&field.value, env);
                        let (ty, field_effects) = split_effectful(ty);
                        effects.extend(field_effects);
                        (field.name.text.clone(), ty)
                    })
                    .collect();
                attach_effects(Type::closed_record(fields), effects)
            }
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
                let (target_ty, target_effects) = split_effectful(target_ty);
                match self.expand_alias(&target_ty) {
                    Type::Row(row) if row.kind == RowKind::Record => row
                        .field(&field.text)
                        .cloned()
                        .inspect(|ty| {
                            self.add_typed_span(field.span, TypedSpanKind::Field, &field.text, ty);
                        })
                        .map(|ty| attach_effects(ty, target_effects.clone()))
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
            Expr::Call { callee, args, span } => match self.infer_callee_expr(callee, env) {
                callee_ty => self.infer_call_type(callee_ty, callee.span(), args, *span, env),
            },
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_ty = self.infer_expr(condition, env);
                let (condition_ty, condition_effects) = split_effectful(condition_ty);
                if !self.compatible(&condition_ty, &Type::Bool) {
                    self.diagnostics.push(Diagnostic::new(
                        condition.span(),
                        format!("if condition must be `Bool`, found `{condition_ty}`"),
                    ));
                }
                let then_ty = self.infer_expr(then_branch, env);
                let (then_ty, then_effects) = split_effectful(then_ty);
                let else_ty = self.infer_expr(else_branch, env);
                let (else_ty, else_effects) = split_effectful(else_ty);
                if self.compatible(&then_ty, &else_ty) {
                    let mut effects = condition_effects;
                    effects.extend(then_effects);
                    effects.extend(else_effects);
                    attach_effects(then_ty, effects)
                } else {
                    let message = self.compatibility_message(
                        &then_ty,
                        &else_ty,
                        format!("if branches differ: `{then_ty}` and `{else_ty}`"),
                    );
                    self.diagnostics
                        .push(Diagnostic::new(else_branch.span(), message));
                    Type::Unknown
                }
            }
            Expr::Match { arms, .. } => {
                let scrutinee_ty = match expr {
                    Expr::Match { scrutinee, .. } => self.infer_expr(scrutinee, env),
                    _ => Type::Unknown,
                };
                let (scrutinee_ty, scrutinee_effects) = split_effectful(scrutinee_ty);
                let mut effects = scrutinee_effects;
                let mut found = Type::Unknown;
                for arm in arms {
                    let mut arm_env = env.clone();
                    let mut bound_names = BTreeSet::new();
                    self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env, &mut bound_names);
                    let arm_ty = self.infer_expr(&arm.value, &arm_env);
                    let (arm_ty, arm_effects) = split_effectful(arm_ty);
                    effects.extend(arm_effects);
                    if matches!(found, Type::Unknown) {
                        found = arm_ty;
                    } else if !self.compatible(&found, &arm_ty) {
                        let message = self.compatibility_message(
                            &arm_ty,
                            &found,
                            format!(
                                "match arm type mismatch: expected `{found}`, found `{arm_ty}`"
                            ),
                        );
                        self.diagnostics
                            .push(Diagnostic::new(arm.value.span(), message));
                    }
                }
                attach_effects(found, effects)
            }
            Expr::Pipeline {
                value,
                op_span,
                target,
                span,
            } => self.infer_pipeline_expr(value, *op_span, target, *span, env),
            Expr::With {
                targets,
                body,
                span,
            } => self.infer_with_expr(targets, body, *span, env),
            Expr::Given { fields, body, span } => self.infer_given_expr(fields, body, *span, env),
            Expr::Within { scope, body, .. } => {
                let scope_ty = self.resolve_expr_name(&scope.text, scope.span, env, NameUse::Value);
                self.add_typed_span(scope.span, TypedSpanKind::Scope, &scope.text, &scope_ty);
                self.default_scopes.push((scope.clone(), scope_ty));
                let result = self.infer_block(body, env);
                self.default_scopes.pop();
                result
            }
            Expr::Handle { .. } => self.check_handle_expr(expr, env, &Type::Unknown),
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

    fn check_handle_expr(
        &mut self,
        expr: &Expr,
        env: &BTreeMap<String, Type>,
        expected: &Type,
    ) -> Type {
        let Expr::Handle {
            action,
            clauses,
            return_clause,
            ..
        } = expr
        else {
            return Type::Unknown;
        };

        let action_ty = self.infer_expr(action, env);
        let (action_result, mut remaining_effects) = split_effectful(action_ty);
        let action_effects = remaining_effects.clone();
        let handler_result = match expected {
            Type::Unknown => action_result.clone(),
            _ => expected_result_type(expected),
        };
        let mut handler_effects = EffectRowType::empty();
        let mut seen_clauses = BTreeSet::new();

        for clause in clauses {
            if !seen_clauses.insert(clause.operation.text.clone()) {
                self.diagnostics.push(Diagnostic::new(
                    clause.operation.span,
                    format!("duplicate handler clause `{}`", clause.operation.text),
                ));
            }
            let operation_ty =
                self.resolve_handler_operation(&clause.operation.text, clause.operation.span);
            self.add_typed_span(
                clause.operation.span,
                TypedSpanKind::Value,
                &clause.operation.text,
                &operation_ty,
            );
            let (payload_types, operation_result, operation_effects) =
                self.handler_operation_signature(&operation_ty, &action_result, &action_effects);
            let mut expected_params = payload_types;
            expected_params.push(Type::Function(
                vec![operation_result.clone()],
                Box::new(handler_result.clone()),
            ));

            if clause.params.len() != expected_params.len() {
                self.diagnostics.push(Diagnostic::new(
                    clause.operation.span,
                    format!(
                        "handler clause `{}` expects {} parameters, found {}",
                        clause.operation.text,
                        expected_params.len(),
                        clause.params.len()
                    ),
                ));
            }

            let mut clause_env = env.clone();
            for (index, param) in clause.params.iter().enumerate() {
                let param_ty = expected_params.get(index).cloned().unwrap_or(Type::Unknown);
                self.add_typed_span(param.span, TypedSpanKind::Parameter, &param.text, &param_ty);
                clause_env.insert(param.text.clone(), param_ty);
            }
            let clause_ty = self.check_block(&clause.body, &clause_env, &handler_result);
            let (_, clause_effects) = split_effectful(clause_ty);
            handler_effects.extend(clause_effects);
            let removed = remove_effects(&mut remaining_effects, &operation_effects);
            if !removed
                && !operation_effects.effects.is_empty()
                && !effect_row_performed_by_action(&operation_effects, &action_effects)
            {
                self.diagnostics.push(Diagnostic::new(
                    clause.operation.span,
                    format!(
                        "handler clause `{}` does not handle an effect performed by the action",
                        clause.operation.text
                    ),
                ));
            }
        }

        if let Some(return_clause) = return_clause {
            let mut clause_env = env.clone();
            self.add_typed_span(
                return_clause.param.span,
                TypedSpanKind::Parameter,
                &return_clause.param.text,
                &action_result,
            );
            clause_env.insert(return_clause.param.text.clone(), action_result.clone());
            let return_ty = self.check_block(&return_clause.body, &clause_env, &handler_result);
            let (_, return_effects) = split_effectful(return_ty);
            handler_effects.extend(return_effects);
        }

        for effect in &remaining_effects.effects {
            self.diagnostics.push(Diagnostic::new(
                expr.span(),
                format!(
                    "missing handler clause for effect `{}`",
                    effect_label(effect)
                ),
            ));
        }

        remaining_effects.extend(handler_effects);
        let found = attach_effects(handler_result, remaining_effects);
        if !matches!(expected, Type::Unknown) && !self.compatible(&found, expected) {
            let message = self.compatibility_message(
                &found,
                expected,
                format!("expected `{expected}`, found `{found}`"),
            );
            self.diagnostics.push(Diagnostic::new(expr.span(), message));
        }
        found
    }

    fn handler_operation_signature(
        &mut self,
        operation_ty: &Type,
        action_result: &Type,
        action_effects: &EffectRowType,
    ) -> (Vec<Type>, Type, EffectRowType) {
        let Type::Function(params, result) = operation_ty else {
            return (Vec::new(), Type::Unknown, EffectRowType::empty());
        };
        let (operation_result, operation_effects) = split_effectful(result.as_ref().clone());
        let mut subst = Substitution::default();
        let _ = unify_compatible(&operation_result, action_result, &mut subst);
        let mut drop_scope_param = false;
        if scoped_operation_scope_param(params, &operation_effects).is_some()
            && let Some(operation_effect) = operation_effects.effects.first()
        {
            for action_effect in &action_effects.effects {
                let mut candidate = subst.clone();
                if unify_compatible(action_effect, operation_effect, &mut candidate).is_ok() {
                    subst = candidate;
                    drop_scope_param = true;
                    break;
                }
            }
        }
        let params = params
            .iter()
            .skip(if drop_scope_param { 1 } else { 0 })
            .map(|param| substitute(param, &subst))
            .collect::<Vec<_>>();
        (
            params,
            substitute(&operation_result, &subst),
            substitute_effect_row(&operation_effects, &subst),
        )
    }

    fn resolve_handler_operation(&mut self, name: &str, span: Span) -> Type {
        match resolve_visible_symbol(
            self.module,
            self.symbol_tables,
            name,
            Some(SymbolKind::Operation),
        ) {
            Ok(Some(symbol)) => self.type_for_symbol(&symbol),
            Ok(None) => {
                self.diagnostics.push(Diagnostic::new(
                    span,
                    format!("unknown handler operation `{name}`"),
                ));
                Type::Unknown
            }
            Err(symbols) => {
                let candidates = symbols
                    .iter()
                    .map(|symbol| symbol.qualified_name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.push(Diagnostic::new(
                    span,
                    format!("ambiguous handler operation `{name}`; candidates: {candidates}"),
                ));
                Type::Unknown
            }
        }
    }

    fn infer_pipeline_expr(
        &mut self,
        value: &Expr,
        op_span: Span,
        target: &Expr,
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        self.add_typed_span(
            op_span,
            TypedSpanKind::Value,
            "|>",
            &Type::Function(
                vec![
                    Type::Var(TypeVar::unscoped("a")),
                    Type::Function(
                        vec![Type::Var(TypeVar::unscoped("a"))],
                        Box::new(Type::Var(TypeVar::unscoped("b"))),
                    ),
                ],
                Box::new(Type::Var(TypeVar::unscoped("b"))),
            ),
        );
        self.infer_sugared_call(target, vec![CallArg::Expr(value)], span, env)
    }

    fn infer_with_expr(
        &mut self,
        targets: &[WithTarget],
        body: &Block,
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let mut result = self.infer_block(body, env);
        for target in targets.iter().rev() {
            let action_ty = Type::Function(Vec::new(), Box::new(result));
            result = self.infer_sugared_call(
                &target.target,
                vec![CallArg::Synthetic {
                    ty: action_ty,
                    span: target.span,
                }],
                span,
                env,
            );
        }
        result
    }

    fn infer_given_expr(
        &mut self,
        fields: &[FieldExpr],
        body: &Block,
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let mut provider_env = BTreeMap::new();
        let mut body_env = env.clone();
        let mut seen = BTreeSet::new();
        let mut effects = EffectRowType::empty();

        for field in fields {
            if !seen.insert(field.name.text.clone()) {
                self.diagnostics.push(Diagnostic::new(
                    field.name.span,
                    format!("duplicate context provider `{}`", field.name.text),
                ));
            }
            let ty = self.infer_expr(&field.value, env);
            let (ty, field_effects) = split_effectful(ty);
            effects.extend(field_effects);
            self.add_typed_span(
                field.name.span,
                TypedSpanKind::Parameter,
                &field.name.text,
                &ty,
            );
            provider_env.insert(field.name.text.clone(), ty.clone());
            body_env.insert(field.name.text.clone(), ty);
        }

        self.context_providers.push(provider_env);
        let body_ty = self.infer_block(body, &body_env);
        self.context_providers.pop();
        let (body_ty, body_effects) = split_effectful(body_ty);
        effects.extend(body_effects);
        let found = attach_effects(body_ty, effects);
        if matches!(found, Type::Unknown) {
            self.diagnostics
                .push(Diagnostic::new(span, "invalid context provider expression"));
        }
        found
    }

    fn infer_block(&mut self, block: &Block, env: &BTreeMap<String, Type>) -> Type {
        let mut env = env.clone();
        let mut effects = EffectRowType::empty();
        for statement in &block.statements {
            match statement {
                Stmt::Let(binding) => {
                    let ty = self.infer_expr(&binding.value, &env);
                    let (ty, binding_effects) = split_effectful(ty);
                    effects.extend(binding_effects);
                    self.add_typed_span(
                        binding.name.span,
                        TypedSpanKind::Local,
                        &binding.name.text,
                        &ty,
                    );
                    env.insert(binding.name.text.clone(), ty);
                }
                Stmt::Expr(expr) => {
                    let ty = self.infer_expr(expr, &env);
                    let (_, expr_effects) = split_effectful(ty);
                    effects.extend(expr_effects);
                }
            }
        }
        let result = self.infer_expr(&block.result, &env);
        let (result, result_effects) = split_effectful(result);
        effects.extend(result_effects);
        attach_effects(result, effects)
    }

    fn infer_sugared_call(
        &mut self,
        target: &Expr,
        trailing: Vec<CallArg<'_>>,
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        match target {
            Expr::Pipeline {
                value,
                target,
                span,
                ..
            } => {
                let mut args = vec![CallArg::Expr(value)];
                args.extend(trailing);
                self.infer_sugared_call(target, args, *span, env)
            }
            Expr::Call { callee, args, .. } => {
                let args = args
                    .iter()
                    .map(CallArg::Expr)
                    .chain(trailing)
                    .collect::<Vec<_>>();
                let callee_ty = self.infer_callee_expr(callee, env);
                self.infer_sugared_call_type(callee_ty, callee.span(), &args, span, env)
            }
            other => {
                let callee_ty = self.infer_callee_expr(other, env);
                self.infer_sugared_call_type(callee_ty, other.span(), &trailing, span, env)
            }
        }
    }

    fn infer_call_type(
        &mut self,
        callee_ty: Type,
        callee_span: Span,
        args: &[Expr],
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let args = args.iter().map(CallArg::Expr).collect::<Vec<_>>();
        self.infer_sugared_call_type(callee_ty, callee_span, &args, span, env)
    }

    fn infer_sugared_call_type(
        &mut self,
        callee_ty: Type,
        callee_span: Span,
        args: &[CallArg<'_>],
        span: Span,
        env: &BTreeMap<String, Type>,
    ) -> Type {
        match callee_ty {
            Type::Function(params, result) => {
                let scoped_args;
                let args = if params.len() == args.len() + 1 {
                    if self
                        .default_scopes
                        .iter()
                        .filter(|(_, scope_ty)| self.compatible(&scope_ty, &params[0]))
                        .take(2)
                        .count()
                        > 1
                    {
                        self.diagnostics.push(Diagnostic::new(
                            callee_span,
                            "ambiguous elided scoped operation; pass the scope explicitly",
                        ));
                    }
                    if let Some((scope, scope_ty)) = self.default_scopes.last() {
                        scoped_args = std::iter::once(CallArg::Synthetic {
                            ty: scope_ty.clone(),
                            span: scope.span,
                        })
                        .chain(args.iter().cloned())
                        .collect::<Vec<_>>();
                        scoped_args.as_slice()
                    } else {
                        args
                    }
                } else {
                    args
                };

                if params.len() != args.len() {
                    self.diagnostics.push(Diagnostic::new(
                        span,
                        format!(
                            "arity mismatch: expected {} arguments, found {}",
                            params.len(),
                            args.len()
                        ),
                    ));
                }
                let mut subst = Substitution::default();
                let mut effects = EffectRowType::empty();
                for (arg, expected) in args.iter().zip(params.iter()) {
                    let found = match arg {
                        CallArg::Expr(expr) => self.check_expr(expr, env, expected),
                        CallArg::Synthetic { ty, span } => {
                            if !self.compatible(ty, expected) {
                                let message = self.compatibility_message(
                                    ty,
                                    expected,
                                    format!("expected `{expected}`, found `{ty}`"),
                                );
                                self.diagnostics.push(Diagnostic::new(*span, message));
                            }
                            ty.clone()
                        }
                    };
                    let (found, arg_effects) = split_effectful(found);
                    effects.extend(arg_effects);
                    unify_vars(expected, &found, &mut subst);
                }
                let result = self.satisfy_contextual_result(substitute(&result, &subst), span);
                attach_effects(result, effects)
            }
            Type::Unknown => Type::Unknown,
            other => {
                self.diagnostics.push(Diagnostic::new(
                    callee_span,
                    format!("cannot call non-function value of type `{other}`"),
                ));
                Type::Unknown
            }
        }
    }

    fn satisfy_contextual_result(&mut self, ty: Type, span: Span) -> Type {
        match ty {
            Type::Contextual { result, context } => {
                let remaining = self.satisfy_context_row(context, span);
                attach_context(*result, remaining)
            }
            other => other,
        }
    }

    fn satisfy_context_row(&mut self, context: ContextRowType, span: Span) -> ContextRowType {
        let mut remaining = ContextRowType {
            fields: Vec::new(),
            tail: context.tail,
        };
        for field in context.fields {
            let Some(provider_ty) = self.context_provider(&field.name).cloned() else {
                remaining.fields.push(field);
                continue;
            };
            if !self.compatible(&provider_ty, &field.ty) {
                let message = self.compatibility_message(
                    &provider_ty,
                    &field.ty,
                    format!(
                        "context provider `{}` has type `{provider_ty}`, expected `{}`",
                        field.name, field.ty
                    ),
                );
                self.diagnostics.push(Diagnostic::new(span, message));
            }
        }
        remaining
    }

    fn context_provider(&self, name: &str) -> Option<&Type> {
        self.context_providers
            .iter()
            .rev()
            .find_map(|providers| providers.get(name))
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
        let (left_ty, left_effects) = split_effectful(left_ty);
        let right_ty = self.infer_expr(right, env);
        let (right_ty, right_effects) = split_effectful(right_ty);

        let operator = binary_operator(op, &left_ty, &right_ty);
        self.add_typed_span(
            op_span,
            TypedSpanKind::Value,
            operator.label,
            &operator.display_type,
        );

        if matches!(op, BinaryOp::Equal) {
            if !self.compatible(&left_ty, &right_ty) {
                let message = self.compatibility_message(
                    &left_ty,
                    &right_ty,
                    format!("cannot compare `{left_ty}` with `{right_ty}`"),
                );
                self.diagnostics
                    .push(Diagnostic::new(right.span(), message));
            }
            let mut effects = left_effects;
            effects.extend(right_effects);
            return attach_effects(operator.result, effects);
        }

        self.check_operator_operand(operator.label, left, &left_ty, &operator.params[0]);
        self.check_operator_operand(operator.label, right, &right_ty, &operator.params[1]);

        let mut effects = left_effects;
        effects.extend(right_effects);
        attach_effects(operator.result, effects)
    }

    fn infer_callee_expr(&mut self, expr: &Expr, env: &BTreeMap<String, Type>) -> Type {
        if let Some(name) = qualified_expr_name(expr)
            && name.contains('.')
            && !qualified_root_is_local(&name, &env.keys().cloned().collect::<BTreeSet<_>>())
        {
            return self.resolve_expr_name(&name, expr.span(), env, NameUse::Callee);
        }

        if let Expr::Name(name) = expr {
            let ty = self.resolve_expr_name(&name.text, name.span, env, NameUse::Callee);
            self.add_typed_span(name.span, TypedSpanKind::Value, &name.text, &ty);
            return ty;
        }

        self.infer_expr(expr, env)
    }

    fn check_operator_operand(&mut self, label: &str, expr: &Expr, found: &Type, expected: &Type) {
        if !self.compatible(found, expected) {
            let message = self.compatibility_message(
                found,
                expected,
                format!("`{label}` expects `{expected}` operands, found `{found}`"),
            );
            self.diagnostics.push(Diagnostic::new(expr.span(), message));
        }
    }

    fn infer_record_update(
        &mut self,
        base: &Expr,
        updates: &[FieldExpr],
        env: &BTreeMap<String, Type>,
    ) -> Type {
        let base_ty = self.infer_expr(base, env);
        let (base_ty, base_effects) = split_effectful(base_ty);
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
        let mut effects = base_effects;
        if row.kind != RowKind::Record {
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
                let (value_ty, value_effects) = split_effectful(value_ty);
                effects.extend(value_effects);
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
                let value_ty = self.infer_expr(&update.value, env);
                let (_, value_effects) = split_effectful(value_ty);
                effects.extend(value_effects);
            }
        }

        let result = match base_ty {
            Type::Row(_) => Type::Row(row),
            Type::Named { .. } => base_ty,
            _ => Type::Unknown,
        };
        attach_effects(result, effects)
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
                if let Some(row) = expected_variant.as_variant_row() {
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
                    self.constructor_type(&name.text, args.len(), *span, true);
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
            let message = self.compatibility_message(
                found,
                expected,
                format!("pattern expects `{expected}`, found `{found}`"),
            );
            self.diagnostics.push(Diagnostic::new(span, message));
        }
    }

    fn constructor_type(
        &mut self,
        name: &str,
        arg_count: usize,
        span: Span,
        diagnose_missing: bool,
    ) -> Option<Type> {
        let Some(expected) = self.constructor_arities.get(name) else {
            if diagnose_missing {
                self.diagnostics.push(Diagnostic::new(
                    span,
                    format!("unknown constructor `{name}`"),
                ));
            }
            return None;
        };
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
                    let types = def
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<BTreeMap<_, _>>();
                    let subst = Substitution {
                        types,
                        rows: BTreeMap::new(),
                        effects: BTreeMap::new(),
                        contexts: BTreeMap::new(),
                    };
                    substitute(&def.body, &subst)
                })
                .unwrap_or_else(|| ty.clone()),
            _ => ty.clone(),
        }
    }

    fn compatible(&self, found: &Type, expected: &Type) -> bool {
        compatible(&self.expand_alias(found), &self.expand_alias(expected)).is_ok()
    }

    fn compatibility_message(&self, found: &Type, expected: &Type, fallback: String) -> String {
        match compatible(&self.expand_alias(found), &self.expand_alias(expected)) {
            Err(CompatibilityError::RowKind { expected, found }) => {
                format!(
                    "expected {} row, found {} row",
                    row_kind_name(expected),
                    row_kind_name(found)
                )
            }
            Err(CompatibilityError::TypeMismatch { expected, found }) => {
                format!("expected `{expected}`, found `{found}`")
            }
            Err(CompatibilityError::MissingField { name }) => {
                format!("record row is missing field `{name}`")
            }
            Err(CompatibilityError::MissingCase { name }) => {
                format!("variant row is missing case `{name}`")
            }
            Err(CompatibilityError::UnhandledEffects { effects }) => {
                format!("unhandled effects {effects}")
            }
            Err(CompatibilityError::MissingContext { context }) => {
                format!("missing contextual values {context}")
            }
            Err(CompatibilityError::Mismatch) | Ok(()) => fallback,
        }
    }

    fn resolve_expr_name(
        &mut self,
        name: &str,
        span: Span,
        env: &BTreeMap<String, Type>,
        name_use: NameUse,
    ) -> Type {
        if let Some(ty) = env.get(name) {
            if name_use == NameUse::Value
                && self.top_level.get(name).is_some_and(|top| top == ty)
                && self
                    .visible_symbol_kind(name)
                    .is_some_and(|kind| kind == SymbolKind::Constructor)
            {
                return constructor_value_type(ty);
            }
            return ty.clone();
        }

        if let Some(ty) = primitive_value(name) {
            return ty;
        }

        match resolve_visible_symbol(self.module, self.symbol_tables, name, None) {
            Ok(Some(symbol)) => {
                let ty = self.type_for_symbol(&symbol);
                if name_use == NameUse::Value && matches!(symbol.kind, SymbolKind::Constructor) {
                    return constructor_value_type(&ty);
                }
                ty
            }
            Ok(None) => Type::Unknown,
            Err(_) => Type::Unknown,
        }
        .or_constructor(name, span, self)
    }

    fn visible_symbol_kind(&self, name: &str) -> Option<SymbolKind> {
        resolve_visible_symbol(self.module, self.symbol_tables, name, None)
            .ok()
            .flatten()
            .map(|symbol| symbol.kind)
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
                Item::Effect(decl) => {
                    self.add_typed_span_display(
                        decl.name.span,
                        TypedSpanKind::Effect,
                        &decl.name.text,
                        format!("effect {}", decl.name.text),
                    );
                    for operation in &decl.operations {
                        if let Some(ty) = self
                            .symbol_types
                            .get(&format!("{}.{}", self.module.name, operation.name.text))
                        {
                            self.add_typed_span(
                                operation.name.span,
                                TypedSpanKind::Operation,
                                &operation.name.text,
                                ty,
                            );
                        }
                    }
                }
                Item::Scope(decl) => {
                    self.add_typed_span_display(
                        decl.name.span,
                        TypedSpanKind::Scope,
                        &decl.name.text,
                        if let Some(protocol) = &decl.protocol {
                            format!(
                                "scope {} with {}",
                                decl.name.text,
                                protocol_display(protocol)
                            )
                        } else {
                            format!("scope {}", decl.name.text)
                        },
                    );
                }
                Item::Type(decl) => {
                    let ty = Type::Named {
                        name: decl.name.text.clone(),
                        args: decl
                            .params
                            .iter()
                            .map(|param| Type::Var(TypeVar::unscoped(param.text.clone())))
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
        self.add_typed_span_display(span, kind, label, ty.to_string());
    }

    fn add_typed_span_display(
        &mut self,
        span: Span,
        kind: TypedSpanKind,
        label: &str,
        display_type: String,
    ) {
        self.typed_spans.push(TypedSpan {
            module: self.module.name.clone(),
            span,
            kind,
            label: label.to_string(),
            display_type,
        });
    }
}

fn constructor_value_type(ty: &Type) -> Type {
    match ty {
        Type::Function(params, result) if params.is_empty() => result.as_ref().clone(),
        _ => ty.clone(),
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
            .constructor_type(name, 0, span, false)
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
                let mut lowerer = TypeLowerer::for_type_decl(&module.name, decl);
                let body = lowerer.lower_type(&decl.body);
                let params = decl
                    .params
                    .iter()
                    .map(|param| lowerer.type_var(param))
                    .collect();
                diagnostics
                    .entry(module.name.clone())
                    .or_insert_with(Vec::new)
                    .extend(lowerer.diagnostics);
                let def = TypeDef { params, body };
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
                    let mut lowerer = TypeLowerer::for_type_decl(&module.name, decl);
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Named {
                            name: decl.name.text.clone(),
                            args: Vec::new(),
                        },
                    );
                    if let TypeExpr::Variant { cases, .. } = &decl.body {
                        let type_params = decl
                            .params
                            .iter()
                            .map(|param| lowerer.type_var(param))
                            .collect::<Vec<_>>();
                        let result = lower_named_type_app(&decl.name.text, &type_params);
                        for case in cases {
                            let params = case
                                .payload
                                .iter()
                                .map(|payload| lowerer.lower_type(payload))
                                .collect::<Vec<_>>();
                            types.insert(
                                format!("{}.{}", module.name, case.name.text),
                                Type::Function(params, Box::new(result.clone())),
                            );
                        }
                    }
                }
                Item::Value(decl) => {
                    let mut lowerer = TypeLowerer::for_value_decl(&module.name, decl);
                    let params = decl
                        .params
                        .iter()
                        .map(|param| lowerer.lower_type(&param.annotation))
                        .collect();
                    let result = lowerer.lower_type(&decl.annotation);
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Function(params, Box::new(result)),
                    );
                }
                Item::Effect(decl) => {
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Named {
                            name: decl.name.text.clone(),
                            args: decl
                                .params
                                .iter()
                                .map(|param| {
                                    Type::Var(TypeVar::scoped(
                                        param.text.clone(),
                                        format!("effect:{}:{}", module.name, decl.name.text),
                                    ))
                                })
                                .collect(),
                        },
                    );
                    let mut lowerer = TypeLowerer {
                        diagnostics: Vec::new(),
                        scope: Some(AnnotationScope::for_effect_decl(&module.name, decl)),
                    };
                    let effect_args = decl
                        .params
                        .iter()
                        .map(|param| Type::Var(lowerer.type_var(param)))
                        .collect::<Vec<_>>();
                    let effect_ty = Type::Named {
                        name: decl.name.text.clone(),
                        args: effect_args,
                    };
                    for operation in &decl.operations {
                        let mut params: Vec<Type> = operation
                            .params
                            .iter()
                            .map(|param| lowerer.lower_type(&param.annotation))
                            .collect();
                        if decl.scoped
                            && let Some(scope_param) = decl.params.first()
                        {
                            params.insert(0, Type::Var(lowerer.type_var(scope_param)));
                        }
                        let result = lowerer.lower_type(&operation.result);
                        types.insert(
                            format!("{}.{}", module.name, operation.name.text),
                            Type::Function(
                                params,
                                Box::new(Type::Effectful {
                                    result: Box::new(result),
                                    effects: EffectRowType::singleton(effect_ty.clone()),
                                }),
                            ),
                        );
                    }
                }
                Item::Scope(decl) => {
                    types.insert(
                        format!("{}.{}", module.name, decl.name.text),
                        Type::Named {
                            name: decl.name.text.clone(),
                            args: Vec::new(),
                        },
                    );
                }
                Item::Import(_) => {}
            }
        }
    }
    types
}

fn collect_effect_families(graph: &ModuleGraph) -> BTreeMap<String, EffectFamilyInfo> {
    let mut families = BTreeMap::new();
    for module in graph.modules.values() {
        for item in &module.parsed.module.items {
            if let Item::Effect(decl) = item {
                families.insert(
                    format!("{}.{}", module.name, decl.name.text),
                    EffectFamilyInfo {
                        scoped: decl.scoped,
                        param_count: decl.params.len(),
                    },
                );
            }
        }
    }
    families
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

#[derive(Clone, Default)]
struct AnnotationScope {
    owner: String,
    type_vars: BTreeMap<String, TypeVar>,
    row_vars: BTreeMap<String, RowVar>,
    diagnostics: Vec<Diagnostic>,
}

impl AnnotationScope {
    fn for_value_decl(module: &str, decl: &ValueDecl) -> Self {
        let owner = format!("value:{module}:{}", decl.name.text);
        let mut type_spans = BTreeMap::new();
        let mut row_spans = BTreeMap::new();
        for param in &decl.params {
            collect_annotation_vars(&param.annotation, &mut type_spans, &mut row_spans);
        }
        collect_annotation_vars(&decl.annotation, &mut type_spans, &mut row_spans);
        Self::new(owner, type_spans, row_spans)
    }

    fn for_type_decl(module: &str, decl: &TypeDecl) -> Self {
        let owner = format!("type:{module}:{}", decl.name.text);
        let mut type_spans = decl
            .params
            .iter()
            .map(|param| (param.text.clone(), param.span))
            .collect::<BTreeMap<_, _>>();
        let mut row_spans = BTreeMap::new();
        collect_annotation_vars(&decl.body, &mut type_spans, &mut row_spans);
        Self::new(owner, type_spans, row_spans)
    }

    fn for_effect_decl(module: &str, decl: &EffectDecl) -> Self {
        let owner = format!("effect:{module}:{}", decl.name.text);
        let mut type_spans = decl
            .params
            .iter()
            .map(|param| (param.text.clone(), param.span))
            .collect::<BTreeMap<_, _>>();
        let mut row_spans = BTreeMap::new();
        for operation in &decl.operations {
            for param in &operation.params {
                collect_annotation_vars(&param.annotation, &mut type_spans, &mut row_spans);
            }
            collect_annotation_vars(&operation.result, &mut type_spans, &mut row_spans);
        }
        Self::new(owner, type_spans, row_spans)
    }

    fn new(
        owner: String,
        type_spans: BTreeMap<String, Span>,
        row_spans: BTreeMap<String, Span>,
    ) -> Self {
        let type_vars = type_spans
            .keys()
            .map(|name| (name.clone(), TypeVar::scoped(name.clone(), owner.clone())))
            .collect();
        let row_vars = row_spans
            .keys()
            .map(|name| (name.clone(), RowVar::scoped(name.clone(), owner.clone())))
            .collect();
        let diagnostics = type_spans
            .keys()
            .filter_map(|name| {
                row_spans.get(name).map(|span| {
                    Diagnostic::new(
                        *span,
                        format!(
                            "`{name}` cannot be used as both a type variable and a row variable"
                        ),
                    )
                })
            })
            .collect();

        Self {
            owner,
            type_vars,
            row_vars,
            diagnostics,
        }
    }

    fn type_var(&self, name: &Name) -> TypeVar {
        self.type_vars
            .get(&name.text)
            .cloned()
            .unwrap_or_else(|| TypeVar::scoped(name.text.clone(), self.owner.clone()))
    }

    fn row_var(&self, tail: &RowTail) -> RowVar {
        self.row_vars
            .get(&tail.name.text)
            .cloned()
            .unwrap_or_else(|| RowVar::scoped(tail.name.text.clone(), self.owner.clone()))
    }
}

fn collect_annotation_vars(
    ty: &TypeExpr,
    type_spans: &mut BTreeMap<String, Span>,
    row_spans: &mut BTreeMap<String, Span>,
) {
    match ty {
        TypeExpr::Var(name) => {
            type_spans.entry(name.text.clone()).or_insert(name.span);
        }
        TypeExpr::App { callee, args, .. } => {
            collect_annotation_vars(callee, type_spans, row_spans);
            for arg in args {
                collect_annotation_vars(arg, type_spans, row_spans);
            }
        }
        TypeExpr::Function { params, result, .. } => {
            for param in params {
                collect_annotation_vars(param, type_spans, row_spans);
            }
            collect_annotation_vars(result, type_spans, row_spans);
        }
        TypeExpr::Effect {
            result, effects, ..
        } => {
            collect_annotation_vars(result, type_spans, row_spans);
            for effect in &effects.effects {
                collect_annotation_vars(effect, type_spans, row_spans);
            }
            if let Some(tail) = &effects.tail {
                row_spans
                    .entry(tail.name.text.clone())
                    .or_insert(tail.name.span);
            }
        }
        TypeExpr::Context {
            result, context, ..
        } => {
            collect_annotation_vars(result, type_spans, row_spans);
            for field in &context.fields {
                collect_annotation_vars(&field.value, type_spans, row_spans);
            }
            if let Some(tail) = &context.tail {
                row_spans
                    .entry(tail.name.text.clone())
                    .or_insert(tail.name.span);
            }
        }
        TypeExpr::Record { fields, tail, .. } => {
            for field in fields {
                collect_annotation_vars(&field.value, type_spans, row_spans);
            }
            if let Some(tail) = tail {
                row_spans
                    .entry(tail.name.text.clone())
                    .or_insert(tail.name.span);
            }
        }
        TypeExpr::Variant { cases, tail, .. } => {
            for case in cases {
                for payload in &case.payload {
                    collect_annotation_vars(payload, type_spans, row_spans);
                }
            }
            if let Some(tail) = tail {
                row_spans
                    .entry(tail.name.text.clone())
                    .or_insert(tail.name.span);
            }
        }
        TypeExpr::Named { .. } | TypeExpr::Tuple { .. } | TypeExpr::Error { .. } => {}
    }
}

#[derive(Default)]
struct TypeLowerer {
    diagnostics: Vec<Diagnostic>,
    scope: Option<AnnotationScope>,
}

impl TypeLowerer {
    fn for_value_decl(module: &str, decl: &ValueDecl) -> Self {
        let scope = AnnotationScope::for_value_decl(module, decl);
        Self {
            diagnostics: scope.diagnostics.clone(),
            scope: Some(scope),
        }
    }

    fn for_type_decl(module: &str, decl: &TypeDecl) -> Self {
        let scope = AnnotationScope::for_type_decl(module, decl);
        Self {
            diagnostics: scope.diagnostics.clone(),
            scope: Some(scope),
        }
    }

    fn type_var(&self, name: &Name) -> TypeVar {
        self.scope
            .as_ref()
            .map(|scope| scope.type_var(name))
            .unwrap_or_else(|| TypeVar::unscoped(name.text.clone()))
    }

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
            TypeExpr::Var(name) => Type::Var(
                self.scope
                    .as_ref()
                    .map(|scope| scope.type_var(name))
                    .unwrap_or_else(|| TypeVar::unscoped(name.text.clone())),
            ),
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
            TypeExpr::Effect {
                result, effects, ..
            } => Type::Effectful {
                result: Box::new(self.lower_type(result)),
                effects: self.lower_effect_row(effects),
            },
            TypeExpr::Context {
                result, context, ..
            } => Type::Contextual {
                result: Box::new(self.lower_type(result)),
                context: self.lower_context_row(context),
            },
            TypeExpr::Record { fields, tail, .. } => self.lower_record_type(fields, tail),
            TypeExpr::Variant { cases, tail, .. } => self.lower_variant_type(cases, tail),
            TypeExpr::Tuple { .. } | TypeExpr::Error { .. } => Type::Unknown,
        }
    }

    fn lower_effect_row(&mut self, row: &EffectRow) -> EffectRowType {
        let effects = row
            .effects
            .iter()
            .map(|effect| self.lower_type(effect))
            .collect();
        let tail = row.tail.as_ref().map(|tail| {
            RowTailType::Var(
                self.scope
                    .as_ref()
                    .map(|scope| scope.row_var(tail))
                    .unwrap_or_else(|| RowVar::unscoped(tail.name.text.clone())),
            )
        });
        EffectRowType { effects, tail }
    }

    fn lower_context_row(&mut self, row: &ContextRow) -> ContextRowType {
        let mut seen = BTreeSet::new();
        let fields = row
            .fields
            .iter()
            .map(|field| {
                if !seen.insert(field.name.text.clone()) {
                    self.diagnostics.push(Diagnostic::new(
                        field.name.span,
                        format!("duplicate context field `{}`", field.name.text),
                    ));
                }
                ContextFieldType {
                    name: field.name.text.clone(),
                    ty: self.lower_type(&field.value),
                }
            })
            .collect();
        let tail = row.tail.as_ref().map(|tail| {
            RowTailType::Var(
                self.scope
                    .as_ref()
                    .map(|scope| scope.row_var(tail))
                    .unwrap_or_else(|| RowVar::unscoped(tail.name.text.clone())),
            )
        });
        ContextRowType { fields, tail }
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
            let var = self
                .scope
                .as_ref()
                .map(|scope| scope.row_var(tail))
                .unwrap_or_else(|| RowVar::unscoped(tail.name.text.clone()));
            Type::Row(RowType {
                kind: RowKind::Record,
                entries: fields
                    .into_iter()
                    .map(|(name, ty)| RowEntry::Field { name, ty })
                    .collect(),
                tail: Some(RowTailType::Var(var)),
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
            let var = self
                .scope
                .as_ref()
                .map(|scope| scope.row_var(tail))
                .unwrap_or_else(|| RowVar::unscoped(tail.name.text.clone()));
            Type::Row(RowType {
                kind: RowKind::Variant,
                entries: cases
                    .into_iter()
                    .map(|(name, payload)| RowEntry::Case { name, payload })
                    .collect(),
                tail: Some(RowTailType::Var(var)),
            })
        } else {
            Type::closed_variant(cases)
        }
    }
}

fn lower_named_type_app(name: &str, params: &[TypeVar]) -> Type {
    Type::Named {
        name: name.to_string(),
        args: params.iter().cloned().map(Type::Var).collect(),
    }
}

fn compatible(found: &Type, expected: &Type) -> Result<(), CompatibilityError> {
    let mut subst = Substitution::default();
    unify_compatible(found, expected, &mut subst)
}

fn effect_row_member_parts(effect: &TypeExpr) -> Option<(String, usize, Span)> {
    match effect {
        TypeExpr::Named { path, span } => Some((
            path.iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>()
                .join("."),
            0,
            *span,
        )),
        TypeExpr::App { callee, args, .. } => {
            let (family, base_args, span) = effect_row_member_parts(callee)?;
            Some((family, base_args + args.len(), span))
        }
        TypeExpr::Var(_)
        | TypeExpr::Function { .. }
        | TypeExpr::Effect { .. }
        | TypeExpr::Context { .. }
        | TypeExpr::Record { .. }
        | TypeExpr::Variant { .. }
        | TypeExpr::Tuple { .. }
        | TypeExpr::Error { .. } => None,
    }
}

fn scoped_operation_scope_param<'a>(
    params: &'a [Type],
    operation_effects: &EffectRowType,
) -> Option<&'a Type> {
    let scope_param = params.first()?;
    let Type::Named { args, .. } = operation_effects.effects.first()? else {
        return None;
    };
    if args
        .first()
        .is_some_and(|scope_arg| scope_arg == scope_param)
    {
        Some(scope_param)
    } else {
        None
    }
}

fn unify_compatible(
    found: &Type,
    expected: &Type,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    if found == expected || matches!(found, Type::Unknown) || matches!(expected, Type::Unknown) {
        return Ok(());
    }

    match (found, expected) {
        (_, Type::Var(name)) => {
            subst
                .types
                .entry(name.clone())
                .or_insert_with(|| found.clone());
            Ok(())
        }
        (Type::Var(name), _) => {
            subst
                .types
                .entry(name.clone())
                .or_insert_with(|| expected.clone());
            Ok(())
        }
        (Type::List(a), Type::List(b)) => unify_compatible(a, b, subst),
        (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba })
            if a == b && aa.len() == ba.len() =>
        {
            for (a, b) in aa.iter().zip(ba) {
                unify_compatible(a, b, subst)?;
            }
            Ok(())
        }
        (Type::Row(a), Type::Row(b)) => unify_rows(a, b, subst),
        (
            Type::Effectful {
                result: found_result,
                effects: found_effects,
            },
            Type::Effectful {
                result: expected_result,
                effects: expected_effects,
            },
        ) => {
            unify_compatible(found_result, expected_result, subst)?;
            unify_effect_rows(found_effects, expected_effects, subst)
        }
        (
            Type::Contextual {
                result: found_result,
                context: found_context,
            },
            Type::Contextual {
                result: expected_result,
                context: expected_context,
            },
        ) => {
            unify_compatible(found_result, expected_result, subst)?;
            unify_context_rows(found_context, expected_context, subst)
        }
        (found, Type::Contextual { result, .. }) => unify_compatible(found, result, subst),
        (Type::Effectful { result, effects }, expected) => {
            unify_compatible(result, expected, subst)?;
            if effects.is_empty() {
                Ok(())
            } else {
                Err(CompatibilityError::UnhandledEffects {
                    effects: effects.clone(),
                })
            }
        }
        (found, Type::Effectful { result, effects: _ }) => unify_compatible(found, result, subst),
        (Type::Contextual { result, context }, expected) => {
            unify_compatible(result, expected, subst)?;
            if context.is_empty() {
                Ok(())
            } else {
                Err(CompatibilityError::MissingContext {
                    context: context.clone(),
                })
            }
        }
        (Type::Function(a_params, a_result), Type::Function(b_params, b_result))
            if a_params.len() == b_params.len() =>
        {
            for (a, b) in a_params.iter().zip(b_params) {
                unify_compatible(a, b, subst)?;
            }
            unify_compatible(a_result, b_result, subst)
        }
        _ => Err(CompatibilityError::Mismatch),
    }
}

fn unify_effect_rows(
    found: &EffectRowType,
    expected: &EffectRowType,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    let mut remaining = found.effects.clone();
    for expected_effect in &expected.effects {
        let Some(index) = remaining.iter().position(|found_effect| {
            unify_compatible(found_effect, expected_effect, subst).is_ok()
        }) else {
            return Err(CompatibilityError::UnhandledEffects {
                effects: EffectRowType {
                    effects: vec![expected_effect.clone()],
                    tail: None,
                },
            });
        };
        remaining.remove(index);
    }

    remaining.retain(|found_effect| {
        let found_effect = substitute(found_effect, subst);
        !expected
            .effects
            .iter()
            .any(|expected_effect| found_effect == substitute(expected_effect, subst))
    });

    if let Some(RowTailType::Var(name)) = &expected.tail {
        subst.effects.insert(
            name.clone(),
            EffectRowType {
                effects: remaining,
                tail: found.tail.clone(),
            },
        );
        Ok(())
    } else if remaining.is_empty() && found.tail.is_none() {
        Ok(())
    } else {
        Err(CompatibilityError::UnhandledEffects {
            effects: EffectRowType {
                effects: remaining,
                tail: found.tail.clone(),
            },
        })
    }
}

fn unify_context_rows(
    found: &ContextRowType,
    expected: &ContextRowType,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    let mut remaining = found.fields.clone();
    for expected_field in &expected.fields {
        let Some(index) = remaining.iter().position(|found_field| {
            found_field.name == expected_field.name
                && unify_compatible(&found_field.ty, &expected_field.ty, subst).is_ok()
        }) else {
            return Err(CompatibilityError::MissingField {
                name: expected_field.name.clone(),
            });
        };
        remaining.remove(index);
    }

    if let Some(RowTailType::Var(name)) = &expected.tail {
        bind_context_tail(
            name,
            ContextRowType {
                fields: remaining,
                tail: found.tail.clone(),
            },
            subst,
        )
    } else if remaining.is_empty() && found.tail.is_none() {
        Ok(())
    } else {
        Err(CompatibilityError::Mismatch)
    }
}

fn bind_context_tail(
    name: &RowVar,
    row: ContextRowType,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    if row.fields.is_empty() && matches!(&row.tail, Some(RowTailType::Var(tail)) if tail == name) {
        return Ok(());
    }
    if let Some(existing) = subst.contexts.get(name) {
        let mut nested = subst.clone();
        unify_context_rows(&row, existing, &mut nested)?;
    } else {
        subst.contexts.insert(name.clone(), row);
    }
    Ok(())
}

fn unify_rows(
    found: &RowType,
    expected: &RowType,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    if found.kind != expected.kind {
        return Err(CompatibilityError::RowKind {
            expected: expected.kind,
            found: found.kind,
        });
    }

    let remainder = match expected.kind {
        RowKind::Record => unify_record_rows(found, expected, subst)?,
        RowKind::Variant => unify_variant_rows(found, expected, subst)?,
    };

    if let Some(RowTailType::Var(name)) = &expected.tail {
        bind_row_tail(name, remainder, subst)
    } else if found.tail.is_none() && remainder.entries.is_empty() {
        Ok(())
    } else {
        Err(CompatibilityError::Mismatch)
    }
}

fn unify_record_rows(
    found: &RowType,
    expected: &RowType,
    subst: &mut Substitution,
) -> Result<RowType, CompatibilityError> {
    let mut required = BTreeSet::new();
    for entry in &expected.entries {
        let RowEntry::Field { name, ty } = entry else {
            return Err(CompatibilityError::Mismatch);
        };
        required.insert(name.clone());
        let Some(found_ty) = found.field(name) else {
            return Err(CompatibilityError::MissingField { name: name.clone() });
        };
        unify_compatible(found_ty, ty, subst).map_err(|error| match error {
            CompatibilityError::RowKind { .. }
            | CompatibilityError::MissingField { .. }
            | CompatibilityError::MissingCase { .. }
            | CompatibilityError::UnhandledEffects { .. }
            | CompatibilityError::MissingContext { .. } => error,
            CompatibilityError::Mismatch | CompatibilityError::TypeMismatch { .. } => {
                CompatibilityError::TypeMismatch {
                    expected: Box::new(ty.clone()),
                    found: Box::new(found_ty.clone()),
                }
            }
        })?;
    }

    let entries = found
        .entries
        .iter()
        .filter_map(|entry| match entry {
            RowEntry::Field { name, ty } if !required.contains(name) => Some(RowEntry::Field {
                name: name.clone(),
                ty: ty.clone(),
            }),
            RowEntry::Field { .. } => None,
            RowEntry::Case { .. } => Some(entry.clone()),
        })
        .collect();

    Ok(RowType {
        kind: RowKind::Record,
        entries,
        tail: found.tail.clone(),
    })
}

fn unify_variant_rows(
    found: &RowType,
    expected: &RowType,
    subst: &mut Substitution,
) -> Result<RowType, CompatibilityError> {
    let mut required = BTreeSet::new();
    for entry in &expected.entries {
        let RowEntry::Case { name, payload } = entry else {
            return Err(CompatibilityError::Mismatch);
        };
        required.insert(name.clone());
        let Some(found_payload) = found.case(name) else {
            return Err(CompatibilityError::MissingCase { name: name.clone() });
        };
        if payload.len() != found_payload.len() {
            return Err(CompatibilityError::Mismatch);
        }
        for (found_ty, expected_ty) in found_payload.iter().zip(payload) {
            unify_compatible(found_ty, expected_ty, subst).map_err(|error| match error {
                CompatibilityError::RowKind { .. }
                | CompatibilityError::MissingField { .. }
                | CompatibilityError::MissingCase { .. }
                | CompatibilityError::UnhandledEffects { .. }
                | CompatibilityError::MissingContext { .. } => error,
                CompatibilityError::Mismatch | CompatibilityError::TypeMismatch { .. } => {
                    CompatibilityError::TypeMismatch {
                        expected: Box::new(expected_ty.clone()),
                        found: Box::new(found_ty.clone()),
                    }
                }
            })?;
        }
    }

    let entries = found
        .entries
        .iter()
        .filter_map(|entry| match entry {
            RowEntry::Case { name, payload } if !required.contains(name) => Some(RowEntry::Case {
                name: name.clone(),
                payload: payload.clone(),
            }),
            RowEntry::Case { .. } => None,
            RowEntry::Field { .. } => Some(entry.clone()),
        })
        .collect();

    Ok(RowType {
        kind: RowKind::Variant,
        entries,
        tail: found.tail.clone(),
    })
}

fn bind_row_tail(
    name: &RowVar,
    row: RowType,
    subst: &mut Substitution,
) -> Result<(), CompatibilityError> {
    if row.entries.is_empty() && matches!(&row.tail, Some(RowTailType::Var(tail)) if tail == name) {
        return Ok(());
    }
    if let Some(existing) = subst.rows.get(name) {
        let mut nested = subst.clone();
        unify_rows(&row, existing, &mut nested)?;
    } else {
        subst.rows.insert(name.clone(), row);
    }
    Ok(())
}

fn row_kind_name(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Record => "record",
        RowKind::Variant => "variant",
    }
}

fn effect_label(effect: &Type) -> String {
    match effect {
        Type::Named { name, args } if args.is_empty() => name.clone(),
        other => other.to_string(),
    }
}

fn protocol_display(row: &EffectRow) -> String {
    let mut parts = row
        .effects
        .iter()
        .map(type_expr_display)
        .collect::<Vec<_>>();
    if let Some(tail) = &row.tail {
        parts.push(format!("...{}", tail.name.text));
    }
    format!("<{}>", parts.join(", "))
}

fn type_expr_display(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named { path, .. } => path
            .iter()
            .map(|name| name.text.as_str())
            .collect::<Vec<_>>()
            .join("."),
        TypeExpr::Var(name) => name.text.clone(),
        TypeExpr::App { callee, args, .. } => {
            let mut out = type_expr_display(callee);
            for arg in args {
                out.push(' ');
                out.push_str(&type_expr_display(arg));
            }
            out
        }
        TypeExpr::Function { .. }
        | TypeExpr::Effect { .. }
        | TypeExpr::Context { .. }
        | TypeExpr::Record { .. }
        | TypeExpr::Variant { .. }
        | TypeExpr::Tuple { .. }
        | TypeExpr::Error { .. } => "<type>".to_string(),
    }
}

fn unify_vars(pattern: &Type, found: &Type, subst: &mut Substitution) {
    match pattern {
        Type::Var(name) => {
            subst
                .types
                .entry(name.clone())
                .or_insert_with(|| found.clone());
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
                let _ = unify_rows(found_row, row, subst);
            }
        }
        Type::Effectful { result, effects } => {
            if let Type::Effectful {
                result: found_result,
                effects: found_effects,
            } = found
            {
                unify_vars(result, found_result, subst);
                let _ = unify_effect_rows(found_effects, effects, subst);
            } else {
                unify_vars(result, found, subst);
            }
        }
        Type::Contextual { result, context } => {
            if let Type::Contextual {
                result: found_result,
                context: found_context,
            } = found
            {
                unify_vars(result, found_result, subst);
                let _ = unify_context_rows(found_context, context, subst);
            } else {
                unify_vars(result, found, subst);
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

fn substitute(ty: &Type, subst: &Substitution) -> Type {
    match ty {
        Type::Var(name) => subst.types.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(item) => Type::List(Box::new(substitute(item, subst))),
        Type::Named { name, args } => Type::Named {
            name: name.clone(),
            args: args.iter().map(|arg| substitute(arg, subst)).collect(),
        },
        Type::Row(row) => Type::Row(substitute_row(row, subst)),
        Type::Effectful { result, effects } => Type::Effectful {
            result: Box::new(substitute(result, subst)),
            effects: substitute_effect_row(effects, subst),
        },
        Type::Contextual { result, context } => Type::Contextual {
            result: Box::new(substitute(result, subst)),
            context: substitute_context_row(context, subst),
        },
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

fn substitute_effect_row(row: &EffectRowType, subst: &Substitution) -> EffectRowType {
    let mut effects = row
        .effects
        .iter()
        .map(|effect| substitute(effect, subst))
        .collect::<Vec<_>>();

    let tail = match &row.tail {
        Some(RowTailType::Var(name)) => {
            if let Some(bound) = subst.effects.get(name) {
                let bound = substitute_effect_row(bound, subst);
                effects.extend(bound.effects);
                bound.tail
            } else {
                row.tail.clone()
            }
        }
        None => None,
    };

    EffectRowType { effects, tail }
}

fn substitute_context_row(row: &ContextRowType, subst: &Substitution) -> ContextRowType {
    let mut fields = row
        .fields
        .iter()
        .map(|field| ContextFieldType {
            name: field.name.clone(),
            ty: substitute(&field.ty, subst),
        })
        .collect::<Vec<_>>();

    let tail = match &row.tail {
        Some(RowTailType::Var(name)) => {
            if let Some(bound) = subst.contexts.get(name) {
                let bound = substitute_context_row(bound, subst);
                fields.extend(bound.fields);
                bound.tail
            } else {
                row.tail.clone()
            }
        }
        None => None,
    };

    ContextRowType { fields, tail }
}

fn substitute_row(row: &RowType, subst: &Substitution) -> RowType {
    let mut entries = row
        .entries
        .iter()
        .map(|entry| substitute_row_entry(entry, subst))
        .collect::<Vec<_>>();

    let tail = match &row.tail {
        Some(RowTailType::Var(name)) => {
            if let Some(bound) = subst.rows.get(name) {
                let bound = substitute_row(bound, subst);
                entries.extend(bound.entries);
                bound.tail
            } else {
                row.tail.clone()
            }
        }
        None => None,
    };

    RowType {
        kind: row.kind,
        entries,
        tail,
    }
}

fn substitute_row_entry(entry: &RowEntry, subst: &Substitution) -> RowEntry {
    match entry {
        RowEntry::Field { name, ty } => RowEntry::Field {
            name: name.clone(),
            ty: substitute(ty, subst),
        },
        RowEntry::Case { name, payload } => RowEntry::Case {
            name: name.clone(),
            payload: payload.iter().map(|ty| substitute(ty, subst)).collect(),
        },
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
            Type::Effectful { result, effects } => write!(f, "{result} with {effects}"),
            Type::Contextual { result, context } => write!(f, "{result} given {context}"),
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

impl std::fmt::Display for EffectRowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "< ")?;
        let mut first = true;
        for effect in &self.effects {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            write!(f, "{effect}")?;
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

impl std::fmt::Display for ContextRowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ ")?;
        let mut first = true;
        for field in &self.fields {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            write!(f, "{}: {}", field.name, field.ty)?;
        }
        if let Some(RowTailType::Var(name)) = &self.tail {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "...{name}")?;
        }
        write!(f, " }}")
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

use crate::module::{IndexedModule, ModuleGraph, Symbol, SymbolKind};
use fx_syntax::ast::*;
use fx_syntax::diagnostic::Diagnostic;
use fx_syntax::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedWorkspace {
    pub modules: BTreeMap<String, ResolvedModule>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedModule {
    pub name: String,
    pub references: Vec<ResolvedReference>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedReference {
    pub name: String,
    pub span: Span,
    pub target: Symbol,
}

pub fn resolve_workspace(graph: &ModuleGraph) -> ResolvedWorkspace {
    let mut diagnostics = graph.diagnostics.clone();
    let symbols = SymbolTables::new(graph);
    let mut modules = BTreeMap::new();

    for module in graph.modules.values() {
        let mut resolver = ModuleResolver {
            module,
            symbols: &symbols,
            diagnostics: Vec::new(),
            references: Vec::new(),
        };
        resolver.resolve_module();
        let module_diagnostics = resolver.diagnostics;
        diagnostics.extend(module_diagnostics.clone());
        modules.insert(
            module.name.clone(),
            ResolvedModule {
                name: module.name.clone(),
                references: resolver.references,
                diagnostics: module_diagnostics,
            },
        );
    }

    ResolvedWorkspace {
        modules,
        diagnostics,
    }
}

pub(crate) struct SymbolTables {
    by_qualified: BTreeMap<String, Symbol>,
    exported_by_module_name: BTreeMap<String, BTreeMap<String, Symbol>>,
}

impl SymbolTables {
    pub(crate) fn new(graph: &ModuleGraph) -> Self {
        let mut by_qualified = BTreeMap::new();
        let mut exported_by_module_name = BTreeMap::<String, BTreeMap<String, Symbol>>::new();
        for module in graph.modules.values() {
            for symbol in &module.symbols {
                by_qualified.insert(symbol.qualified_name.clone(), symbol.clone());
                if symbol.exported {
                    exported_by_module_name
                        .entry(module.name.clone())
                        .or_default()
                        .insert(symbol.name.clone(), symbol.clone());
                }
            }
        }
        Self {
            by_qualified,
            exported_by_module_name,
        }
    }

    pub(crate) fn qualified(&self, name: &str) -> Option<&Symbol> {
        self.by_qualified.get(name)
    }

    pub(crate) fn exported(&self, module: &str) -> Option<&BTreeMap<String, Symbol>> {
        self.exported_by_module_name.get(module)
    }

    fn imported_candidates<'a>(&'a self, module: &'a IndexedModule, name: &str) -> Vec<&'a Symbol> {
        module
            .imports
            .iter()
            .filter_map(|import| self.exported_by_module_name.get(import))
            .filter_map(|exports| exports.get(name))
            .collect()
    }
}

pub(crate) fn resolve_visible_symbol(
    module: &IndexedModule,
    symbols: &SymbolTables,
    name: &str,
    kind: Option<SymbolKind>,
) -> Result<Option<Symbol>, Vec<Symbol>> {
    if name.contains('.') {
        return Ok(symbols
            .qualified(name)
            .filter(|symbol| symbol.module == module.name || symbol.exported)
            .filter(|symbol| kind.is_none_or(|kind| symbol.kind == kind))
            .cloned());
    }

    if let Some(local) = module
        .symbols
        .iter()
        .find(|symbol| symbol.name == name && kind.is_none_or(|kind| symbol.kind == kind))
    {
        return Ok(Some(local.clone()));
    }

    let candidates = symbols
        .imported_candidates(module, name)
        .into_iter()
        .filter(|symbol| kind.is_none_or(|kind| symbol.kind == kind))
        .cloned()
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Ok(None),
        [single] => Ok(Some(single.clone())),
        _ => Err(candidates),
    }
}

struct ModuleResolver<'a> {
    module: &'a IndexedModule,
    symbols: &'a SymbolTables,
    diagnostics: Vec<Diagnostic>,
    references: Vec<ResolvedReference>,
}

impl ModuleResolver<'_> {
    fn resolve_module(&mut self) {
        for item in &self.module.parsed.module.items {
            match item {
                Item::Import(_) => {}
                Item::Type(decl) => self.resolve_type_decl(decl),
                Item::Value(decl) => self.resolve_value_decl(decl),
            }
        }
    }

    fn resolve_type_decl(&mut self, decl: &TypeDecl) {
        let mut type_params = decl
            .params
            .iter()
            .map(|param| param.text.clone())
            .collect::<BTreeSet<_>>();
        type_params.extend(["a", "b", "c", "r", "e"].map(String::from));
        self.resolve_type_expr(&decl.body, &type_params);
    }

    fn resolve_value_decl(&mut self, decl: &ValueDecl) {
        let mut locals = BTreeSet::new();
        let mut type_params = BTreeSet::new();
        for param in &decl.params {
            locals.insert(param.name.text.clone());
            self.resolve_type_expr(&param.annotation, &type_params);
            collect_type_vars(&param.annotation, &mut type_params);
        }
        self.resolve_type_expr(&decl.annotation, &type_params);
        collect_type_vars(&decl.annotation, &mut type_params);
        self.resolve_block(&decl.body, &mut locals);
    }

    fn resolve_block(&mut self, block: &Block, locals: &mut BTreeSet<String>) {
        for statement in &block.statements {
            match statement {
                Stmt::Let(binding) => {
                    self.resolve_expr(&binding.value, locals);
                    locals.insert(binding.name.text.clone());
                }
            }
        }
        self.resolve_expr(&block.result, locals);
    }

    fn resolve_expr(&mut self, expr: &Expr, locals: &mut BTreeSet<String>) {
        if let Some(name) = qualified_expr_name(expr)
            && name.contains('.')
            && !qualified_root_is_local(&name, locals)
        {
            self.resolve_symbol_name(&name, expr.span(), None);
            return;
        }

        match expr {
            Expr::Name(name) => {
                if !locals.contains(name.text.as_str()) {
                    self.resolve_symbol_name(&name.text, name.span, None);
                }
            }
            Expr::List { items, .. } => {
                for item in items {
                    self.resolve_expr(item, locals);
                }
            }
            Expr::Record { fields, .. } => {
                for field in fields {
                    self.resolve_expr(&field.value, locals);
                }
            }
            Expr::RecordUpdate { base, fields, .. } => {
                self.resolve_expr(base, locals);
                for field in fields {
                    self.resolve_expr(&field.value, locals);
                }
            }
            Expr::Field { target, .. } => self.resolve_expr(target, locals),
            Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee, locals);
                for arg in args {
                    self.resolve_expr(arg, locals);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expr(condition, locals);
                self.resolve_expr(then_branch, locals);
                self.resolve_expr(else_branch, locals);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.resolve_expr(scrutinee, locals);
                for arm in arms {
                    let mut arm_locals = locals.clone();
                    self.resolve_pattern(&arm.pattern, &mut arm_locals);
                    self.resolve_expr(&arm.value, &mut arm_locals);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left, locals);
                self.resolve_expr(right, locals);
            }
            Expr::Int { .. }
            | Expr::Text { .. }
            | Expr::Bool { .. }
            | Expr::Unit { .. }
            | Expr::Error { .. } => {}
        }
    }

    fn resolve_pattern(&mut self, pattern: &Pattern, locals: &mut BTreeSet<String>) {
        match pattern {
            Pattern::Name(name) => {
                locals.insert(name.text.clone());
            }
            Pattern::Constructor { name, args, .. } => {
                self.resolve_symbol_name(&name.text, name.span, Some(SymbolKind::Constructor));
                for arg in args {
                    self.resolve_pattern(arg, locals);
                }
            }
            Pattern::List { items, rest, .. } => {
                for item in items {
                    self.resolve_pattern(item, locals);
                }
                if let Some(rest) = rest {
                    locals.insert(rest.text.clone());
                }
            }
            Pattern::Record { fields, .. } => {
                for field in fields {
                    locals.insert(field.text.clone());
                }
            }
            Pattern::Wildcard { .. }
            | Pattern::Int { .. }
            | Pattern::Text { .. }
            | Pattern::Bool { .. }
            | Pattern::Unit { .. }
            | Pattern::Error { .. } => {}
        }
    }

    fn resolve_type_expr(&mut self, ty: &TypeExpr, type_params: &BTreeSet<String>) {
        match ty {
            TypeExpr::Named { path, span } => {
                let name = path
                    .iter()
                    .map(|name| name.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                if !is_builtin_type(&name) && !type_params.contains(name.as_str()) {
                    self.resolve_symbol_name(&name, *span, Some(SymbolKind::Type));
                }
            }
            TypeExpr::Var(_) => {}
            TypeExpr::App { callee, args, .. } => {
                self.resolve_type_expr(callee, type_params);
                for arg in args {
                    self.resolve_type_expr(arg, type_params);
                }
            }
            TypeExpr::Record { fields, .. } => {
                for field in fields {
                    self.resolve_type_expr(&field.value, type_params);
                }
            }
            TypeExpr::Variant { cases, .. } => {
                for case in cases {
                    for payload in &case.payload {
                        self.resolve_type_expr(payload, type_params);
                    }
                }
            }
            TypeExpr::Tuple { elems, .. } => {
                for elem in elems {
                    self.resolve_type_expr(elem, type_params);
                }
            }
            TypeExpr::Error { .. } => {}
        }
    }

    fn resolve_symbol_name(&mut self, name: &str, span: Span, kind: Option<SymbolKind>) {
        match resolve_visible_symbol(self.module, self.symbols, name, kind) {
            Ok(Some(target)) => self.references.push(ResolvedReference {
                name: name.to_string(),
                span,
                target,
            }),
            Ok(None) => self.diagnostics.push(Diagnostic::new(
                span,
                format!("unknown {}", describe_name(kind, name)),
            )),
            Err(candidates) => self.diagnostics.push(Diagnostic::new(
                span,
                format!(
                    "ambiguous {}; candidates: {}",
                    describe_name(kind, name),
                    candidates
                        .iter()
                        .map(|symbol| symbol.qualified_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        }
    }
}

pub(crate) fn qualified_expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.text.clone()),
        Expr::Field { target, field, .. } => {
            let mut path = qualified_expr_name(target)?;
            path.push('.');
            path.push_str(&field.text);
            Some(path)
        }
        _ => None,
    }
}

pub(crate) fn qualified_root_is_local(name: &str, locals: &BTreeSet<String>) -> bool {
    name.split('.')
        .next()
        .is_some_and(|root| locals.contains(root))
}

fn collect_type_vars(ty: &TypeExpr, vars: &mut BTreeSet<String>) {
    match ty {
        TypeExpr::Var(name) => {
            vars.insert(name.text.clone());
        }
        TypeExpr::App { callee, args, .. } => {
            collect_type_vars(callee, vars);
            for arg in args {
                collect_type_vars(arg, vars);
            }
        }
        TypeExpr::Record { fields, .. } => {
            for field in fields {
                collect_type_vars(&field.value, vars);
            }
        }
        TypeExpr::Variant { cases, .. } => {
            for case in cases {
                for payload in &case.payload {
                    collect_type_vars(payload, vars);
                }
            }
        }
        TypeExpr::Tuple { elems, .. } => {
            for elem in elems {
                collect_type_vars(elem, vars);
            }
        }
        TypeExpr::Named { .. } | TypeExpr::Error { .. } => {}
    }
}

fn is_builtin_type(name: &str) -> bool {
    matches!(name, "Unit" | "Bool" | "Int" | "Text" | "List")
}

fn describe_name(kind: Option<SymbolKind>, name: &str) -> String {
    match kind {
        Some(SymbolKind::Type) => format!("type `{name}`"),
        Some(SymbolKind::Constructor) => format!("constructor `{name}`"),
        Some(SymbolKind::Value) => format!("value `{name}`"),
        None => format!("name `{name}`"),
    }
}

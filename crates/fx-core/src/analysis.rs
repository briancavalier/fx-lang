use crate::check::{CheckResult, TypedSpan, TypedSpanKind};
use crate::module::{ModuleGraph, SourceFile};
use crate::{check_workspace, index_workspace, resolve_workspace};
use fx_syntax::ast::*;
use fx_syntax::diagnostic::Diagnostic;
use fx_syntax::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub graph: ModuleGraph,
    pub check: CheckResult,
    pub resolved: crate::resolve::ResolvedWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    pub module: String,
    pub span: Span,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionInfo {
    pub module: String,
    pub path: PathBuf,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub module: String,
    pub path: PathBuf,
    pub span: Span,
    pub message: String,
}

pub fn analyze_workspace(root: &Path, files: Vec<SourceFile>) -> Analysis {
    let graph = index_workspace(root, files);
    let resolved = resolve_workspace(&graph);
    let check = check_workspace(&graph);
    Analysis {
        graph,
        check,
        resolved,
    }
}

impl Analysis {
    pub fn module_for_path(&self, path: &Path) -> Option<&str> {
        self.graph
            .modules
            .values()
            .find(|module| module.path == path)
            .map(|module| module.name.as_str())
    }

    pub fn diagnostics_by_path(&self) -> BTreeMap<PathBuf, Vec<AnalysisDiagnostic>> {
        let mut by_path = BTreeMap::<PathBuf, Vec<AnalysisDiagnostic>>::new();
        for module in self.graph.modules.values() {
            add_diagnostics(
                &mut by_path,
                &module.name,
                &module.path,
                &module.diagnostics,
            );
            if let Some(resolved) = self.resolved.modules.get(&module.name) {
                add_diagnostics(
                    &mut by_path,
                    &module.name,
                    &module.path,
                    &resolved.diagnostics,
                );
            }
            if let Some(check_diagnostics) = self.check.module_diagnostics.get(&module.name) {
                add_diagnostics(&mut by_path, &module.name, &module.path, check_diagnostics);
            }
        }
        by_path
    }

    pub fn hover_at(&self, module: &str, byte_offset: usize) -> Option<HoverInfo> {
        let typed = self
            .check
            .typed_spans
            .get(module)?
            .iter()
            .filter(|typed| contains_offset(typed.span, byte_offset))
            .min_by_key(|typed| typed.span.end.saturating_sub(typed.span.start))?;
        Some(HoverInfo {
            module: module.to_string(),
            span: typed.span,
            contents: hover_contents(typed),
        })
    }

    pub fn definition_at(&self, module: &str, byte_offset: usize) -> Option<DefinitionInfo> {
        if let Some(definition) = self.local_definition_at(module, byte_offset) {
            return Some(definition);
        }

        let reference = self
            .resolved
            .modules
            .get(module)?
            .references
            .iter()
            .filter(|reference| contains_offset(reference.span, byte_offset))
            .min_by_key(|reference| reference.span.end.saturating_sub(reference.span.start))?;
        let target = self.graph.modules.get(&reference.target.module)?;
        Some(DefinitionInfo {
            module: target.name.clone(),
            path: target.path.clone(),
            span: reference.target.span,
        })
    }

    fn local_definition_at(&self, module_name: &str, byte_offset: usize) -> Option<DefinitionInfo> {
        let module = self.graph.modules.get(module_name)?;
        let mut top_level = BTreeMap::new();
        for symbol in &module.symbols {
            top_level.insert(
                symbol.name.clone(),
                DefinitionInfo {
                    module: module.name.clone(),
                    path: module.path.clone(),
                    span: symbol.span,
                },
            );
        }

        for item in &module.parsed.module.items {
            if let Item::Value(decl) = item {
                let mut locals = top_level.clone();
                if let Some(definition) =
                    add_context_field_definitions(decl, byte_offset, &mut locals, module)
                {
                    return Some(definition);
                }
                for param in &decl.params {
                    let definition = DefinitionInfo {
                        module: module.name.clone(),
                        path: module.path.clone(),
                        span: param.name.span,
                    };
                    if contains_offset(param.name.span, byte_offset) {
                        return Some(definition);
                    }
                    locals.insert(param.name.text.clone(), definition);
                }
                if let Some(definition) =
                    local_definition_in_block(&decl.body, byte_offset, &mut locals, module)
                {
                    return Some(definition);
                }
            }
        }
        None
    }
}

fn add_context_field_definitions(
    decl: &ValueDecl,
    byte_offset: usize,
    locals: &mut BTreeMap<String, DefinitionInfo>,
    module: &crate::module::IndexedModule,
) -> Option<DefinitionInfo> {
    let TypeExpr::Context { context, .. } = &decl.annotation else {
        return None;
    };

    for field in &context.fields {
        let definition = DefinitionInfo {
            module: module.name.clone(),
            path: module.path.clone(),
            span: field.name.span,
        };
        if contains_offset(field.name.span, byte_offset) {
            return Some(definition);
        }
        locals.insert(field.name.text.clone(), definition);
    }
    None
}

fn add_diagnostics(
    by_path: &mut BTreeMap<PathBuf, Vec<AnalysisDiagnostic>>,
    module: &str,
    path: &Path,
    diagnostics: &[Diagnostic],
) {
    by_path
        .entry(path.to_path_buf())
        .or_default()
        .extend(diagnostics.iter().map(|diagnostic| AnalysisDiagnostic {
            module: module.to_string(),
            path: path.to_path_buf(),
            span: diagnostic.span,
            message: diagnostic.message.clone(),
        }));
}

fn local_definition_in_block(
    block: &Block,
    byte_offset: usize,
    locals: &mut BTreeMap<String, DefinitionInfo>,
    module: &crate::module::IndexedModule,
) -> Option<DefinitionInfo> {
    for statement in &block.statements {
        match statement {
            Stmt::Let(binding) => {
                if let Some(definition) =
                    local_definition_in_expr(&binding.value, byte_offset, locals, module)
                {
                    return Some(definition);
                }
                let definition = DefinitionInfo {
                    module: module.name.clone(),
                    path: module.path.clone(),
                    span: binding.name.span,
                };
                if contains_offset(binding.name.span, byte_offset) {
                    return Some(definition);
                }
                locals.insert(binding.name.text.clone(), definition);
            }
            Stmt::Expr(expr) => {
                if let Some(definition) =
                    local_definition_in_expr(expr, byte_offset, locals, module)
                {
                    return Some(definition);
                }
            }
        }
    }
    local_definition_in_expr(&block.result, byte_offset, locals, module)
}

fn local_definition_in_expr(
    expr: &Expr,
    byte_offset: usize,
    locals: &mut BTreeMap<String, DefinitionInfo>,
    module: &crate::module::IndexedModule,
) -> Option<DefinitionInfo> {
    match expr {
        Expr::Name(name) if contains_offset(name.span, byte_offset) => {
            locals.get(&name.text).cloned()
        }
        Expr::Name(_)
        | Expr::Int { .. }
        | Expr::Text { .. }
        | Expr::Bool { .. }
        | Expr::Unit { .. }
        | Expr::Error { .. } => None,
        Expr::List { items, .. } => items
            .iter()
            .find_map(|item| local_definition_in_expr(item, byte_offset, locals, module)),
        Expr::Record { fields, .. } => fields
            .iter()
            .find_map(|field| local_definition_in_expr(&field.value, byte_offset, locals, module)),
        Expr::RecordUpdate { base, fields, .. } => {
            local_definition_in_expr(base, byte_offset, locals, module).or_else(|| {
                fields.iter().find_map(|field| {
                    local_definition_in_expr(&field.value, byte_offset, locals, module)
                })
            })
        }
        Expr::Lambda { params, body, .. } => {
            let mut lambda_locals = locals.clone();
            for param in params {
                if let Some(name) = &param.name {
                    let definition = DefinitionInfo {
                        module: module.name.clone(),
                        path: module.path.clone(),
                        span: name.span,
                    };
                    if contains_offset(name.span, byte_offset) {
                        return Some(definition);
                    }
                    lambda_locals.insert(name.text.clone(), definition);
                }
            }
            local_definition_in_block(body, byte_offset, &mut lambda_locals, module)
        }
        Expr::Field { target, .. } => local_definition_in_expr(target, byte_offset, locals, module),
        Expr::Call { callee, args, .. } => {
            local_definition_in_expr(callee, byte_offset, locals, module).or_else(|| {
                args.iter()
                    .find_map(|arg| local_definition_in_expr(arg, byte_offset, locals, module))
            })
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => local_definition_in_expr(condition, byte_offset, locals, module)
            .or_else(|| local_definition_in_expr(then_branch, byte_offset, locals, module))
            .or_else(|| local_definition_in_expr(else_branch, byte_offset, locals, module)),
        Expr::Match {
            scrutinee, arms, ..
        } => local_definition_in_expr(scrutinee, byte_offset, locals, module).or_else(|| {
            arms.iter().find_map(|arm| {
                let mut arm_locals = locals.clone();
                bind_pattern_definitions(&arm.pattern, byte_offset, &mut arm_locals, module)
                    .or_else(|| {
                        local_definition_in_expr(&arm.value, byte_offset, &mut arm_locals, module)
                    })
            })
        }),
        Expr::Pipeline { value, target, .. } => {
            local_definition_in_expr(value, byte_offset, locals, module)
                .or_else(|| local_definition_in_expr(target, byte_offset, locals, module))
        }
        Expr::With { targets, body, .. } => targets
            .iter()
            .find_map(|target| {
                local_definition_in_expr(&target.target, byte_offset, locals, module)
            })
            .or_else(|| local_definition_in_block(body, byte_offset, &mut locals.clone(), module)),
        Expr::Given { fields, body, .. } => fields
            .iter()
            .find_map(|field| local_definition_in_expr(&field.value, byte_offset, locals, module))
            .or_else(|| {
                let mut body_locals = locals.clone();
                for field in fields {
                    let definition = DefinitionInfo {
                        module: module.name.clone(),
                        path: module.path.clone(),
                        span: field.name.span,
                    };
                    if contains_offset(field.name.span, byte_offset) {
                        return Some(definition);
                    }
                    body_locals.insert(field.name.text.clone(), definition);
                }
                local_definition_in_block(body, byte_offset, &mut body_locals, module)
            }),
        Expr::GivenEach {
            init,
            cleanup,
            body,
            ..
        } => local_definition_in_expr(init, byte_offset, locals, module)
            .or_else(|| {
                let mut body_locals = locals.clone();
                if let Expr::Lambda {
                    body: init_body, ..
                } = init.as_ref()
                    && let Expr::Record { fields, .. } = init_body.result.as_ref()
                {
                    for field in fields {
                        let definition = DefinitionInfo {
                            module: module.name.clone(),
                            path: module.path.clone(),
                            span: field.name.span,
                        };
                        body_locals.insert(field.name.text.clone(), definition);
                    }
                }
                local_definition_in_block(body, byte_offset, &mut body_locals, module)
            })
            .or_else(|| {
                cleanup.as_ref().and_then(|cleanup| {
                    local_definition_in_expr(cleanup, byte_offset, locals, module)
                })
            }),
        Expr::Finally { cleanup, body, .. } => {
            local_definition_in_expr(cleanup, byte_offset, locals, module).or_else(|| {
                local_definition_in_block(body, byte_offset, &mut locals.clone(), module)
            })
        }
        Expr::Within { body, .. } => {
            local_definition_in_block(body, byte_offset, &mut locals.clone(), module)
        }
        Expr::Handle {
            action,
            clauses,
            return_clause,
            ..
        } => local_definition_in_expr(action, byte_offset, locals, module)
            .or_else(|| {
                clauses.iter().find_map(|clause| {
                    let mut clause_locals = locals.clone();
                    for param in &clause.params {
                        let definition = DefinitionInfo {
                            module: module.name.clone(),
                            path: module.path.clone(),
                            span: param.span,
                        };
                        if contains_offset(param.span, byte_offset) {
                            return Some(definition);
                        }
                        clause_locals.insert(param.text.clone(), definition);
                    }
                    local_definition_in_block(&clause.body, byte_offset, &mut clause_locals, module)
                })
            })
            .or_else(|| {
                return_clause.as_ref().and_then(|clause| {
                    let mut clause_locals = locals.clone();
                    let definition = DefinitionInfo {
                        module: module.name.clone(),
                        path: module.path.clone(),
                        span: clause.param.span,
                    };
                    if contains_offset(clause.param.span, byte_offset) {
                        return Some(definition);
                    }
                    clause_locals.insert(clause.param.text.clone(), definition);
                    local_definition_in_block(&clause.body, byte_offset, &mut clause_locals, module)
                })
            }),
        Expr::Binary { left, right, .. } => {
            local_definition_in_expr(left, byte_offset, locals, module)
                .or_else(|| local_definition_in_expr(right, byte_offset, locals, module))
        }
    }
}

fn bind_pattern_definitions(
    pattern: &Pattern,
    byte_offset: usize,
    locals: &mut BTreeMap<String, DefinitionInfo>,
    module: &crate::module::IndexedModule,
) -> Option<DefinitionInfo> {
    match pattern {
        Pattern::Name(name) => {
            let definition = DefinitionInfo {
                module: module.name.clone(),
                path: module.path.clone(),
                span: name.span,
            };
            if contains_offset(name.span, byte_offset) {
                return Some(definition);
            }
            locals.insert(name.text.clone(), definition);
            None
        }
        Pattern::List { items, rest, .. } => {
            for item in items {
                if let Some(definition) =
                    bind_pattern_definitions(item, byte_offset, locals, module)
                {
                    return Some(definition);
                }
            }
            if let Some(rest) = rest {
                let definition = DefinitionInfo {
                    module: module.name.clone(),
                    path: module.path.clone(),
                    span: rest.span,
                };
                if contains_offset(rest.span, byte_offset) {
                    return Some(definition);
                }
                locals.insert(rest.text.clone(), definition);
            }
            None
        }
        Pattern::Record { fields, .. } => {
            for field in fields {
                let definition = DefinitionInfo {
                    module: module.name.clone(),
                    path: module.path.clone(),
                    span: field.span,
                };
                if contains_offset(field.span, byte_offset) {
                    return Some(definition);
                }
                locals.insert(field.text.clone(), definition);
            }
            None
        }
        Pattern::Constructor { args, .. } => args
            .iter()
            .find_map(|arg| bind_pattern_definitions(arg, byte_offset, locals, module)),
        Pattern::Wildcard { .. }
        | Pattern::Int { .. }
        | Pattern::Text { .. }
        | Pattern::Bool { .. }
        | Pattern::Unit { .. }
        | Pattern::Error { .. } => None,
    }
}

fn hover_contents(typed: &TypedSpan) -> String {
    match typed.kind {
        TypedSpanKind::Effect => typed.display_type.clone(),
        _ => format!("{} : {}", typed.label, typed.display_type),
    }
}

fn contains_offset(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

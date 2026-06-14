use crate::CompileSession;
use crate::ir::*;
use fx_core::check::{ContextFieldType, RowEntry, RowKind, RowType, TypedSpan};
use fx_core::{AnalysisDiagnostic, SymbolKind, Type};
use fx_syntax::ast::*;
use fx_syntax::span::Span;
use std::collections::BTreeMap;

pub fn lower_to_core(session: &CompileSession) -> Result<CoreProgram, Vec<AnalysisDiagnostic>> {
    let mut lowerer = CoreLowerer {
        session,
        diagnostics: Vec::new(),
    };
    let program = lowerer.lower_program();
    if lowerer.diagnostics.is_empty() {
        Ok(program)
    } else {
        Err(lowerer.diagnostics)
    }
}

struct CoreLowerer<'a> {
    session: &'a CompileSession,
    diagnostics: Vec<AnalysisDiagnostic>,
}

struct ModuleLowerer<'a, 'b> {
    parent: &'a mut CoreLowerer<'b>,
    module: &'b fx_core::module::IndexedModule,
    typed_spans: &'b [TypedSpan],
    default_scopes: Vec<CoreExpr>,
    context_providers: Vec<BTreeMap<String, CoreExpr>>,
    next_resource_provider: usize,
}

#[derive(Debug, Clone)]
struct OperationInfo {
    family: CoreGlobalName,
    scoped: bool,
}

#[derive(Debug, Clone)]
struct Local {
    ty: Type,
}

impl CoreLowerer<'_> {
    fn lower_program(&mut self) -> CoreProgram {
        let modules = self
            .session
            .analysis
            .graph
            .modules
            .values()
            .map(|module| {
                let typed_spans = self
                    .session
                    .analysis
                    .check
                    .typed_spans
                    .get(&module.name)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                ModuleLowerer {
                    parent: self,
                    module,
                    typed_spans,
                    default_scopes: Vec::new(),
                    context_providers: Vec::new(),
                    next_resource_provider: 0,
                }
                .lower_module()
            })
            .collect();
        CoreProgram { modules }
    }

    fn diagnostic(
        &mut self,
        module: &fx_core::module::IndexedModule,
        span: Span,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(AnalysisDiagnostic {
            module: module.name.clone(),
            path: module.path.clone(),
            span,
            message: message.into(),
        });
    }
}

impl ModuleLowerer<'_, '_> {
    fn lower_module(&mut self) -> CoreModule {
        let imports = self
            .module
            .imports
            .iter()
            .map(|module| CoreImport {
                module: module.clone(),
            })
            .collect();
        let declarations = self
            .module
            .parsed
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Import(_) => None,
                Item::Type(decl) => self.lower_type_decl(decl).map(CoreDecl::Type),
                Item::Effect(_) => None,
                Item::Scope(decl) => Some(CoreDecl::Scope(CoreScopeDecl {
                    name: self.global(&decl.name.text),
                    export: decl.export,
                })),
                Item::Value(decl) => Some(CoreDecl::Value(self.lower_value_decl(decl))),
            })
            .collect();

        CoreModule {
            name: self.module.name.clone(),
            imports,
            declarations,
        }
    }

    fn lower_type_decl(&mut self, decl: &TypeDecl) -> Option<CoreTypeDecl> {
        let shape = match &decl.body {
            TypeExpr::Record { fields, .. } => CoreTypeShape::Record {
                fields: fields
                    .iter()
                    .map(|field| CoreTypeField {
                        name: field.name.text.clone(),
                        ty: self.lower_type_expr(&field.value),
                    })
                    .collect(),
            },
            TypeExpr::Variant { cases, .. } => CoreTypeShape::Variant {
                cases: cases
                    .iter()
                    .map(|case| CoreVariantCase {
                        name: case.name.text.clone(),
                        payload: case
                            .payload
                            .iter()
                            .map(|payload| self.lower_type_expr(payload))
                            .collect(),
                    })
                    .collect(),
            },
            other => {
                self.unsupported(
                    other.span(),
                    "only record and variant type declarations are executable in Phase 2.5",
                );
                return None;
            }
        };

        Some(CoreTypeDecl {
            name: self.global(&decl.name.text),
            export: decl.export,
            params: decl.params.iter().map(|param| param.text.clone()).collect(),
            shape,
        })
    }

    fn lower_value_decl(&mut self, decl: &ValueDecl) -> CoreValueDecl {
        let value_ty = self.top_level_type(&decl.name.text);
        let (param_types, result_type) = match value_ty {
            Type::Function(params, result) => (params, *result),
            other => (Vec::new(), other),
        };
        let (context_fields, result_type) = split_contextual_result(result_type);
        let context_param = if context_fields.is_empty() {
            None
        } else {
            Some(CoreParam {
                name: CoreLocalName("__context".to_string()),
                ty: core_closed_record(
                    context_fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect(),
                ),
            })
        };
        let mut params = context_param.iter().cloned().collect::<Vec<_>>();
        params.extend(
            decl.params
                .iter()
                .enumerate()
                .map(|(index, param)| CoreParam {
                    name: CoreLocalName(param.name.text.clone()),
                    ty: param_types
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| self.lower_type_expr(&param.annotation)),
                })
                .collect::<Vec<_>>(),
        );
        let mut locals = params
            .iter()
            .map(|param| {
                (
                    param.name.0.clone(),
                    Local {
                        ty: param.ty.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        for field in &context_fields {
            locals.insert(
                field.name.clone(),
                Local {
                    ty: field.ty.clone(),
                },
            );
        }
        let context_provider = context_param.as_ref().map(|_| {
            context_fields
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        CoreExpr {
                            ty: field.ty.clone(),
                            span: decl.annotation.span(),
                            tail: TailPosition::NonTail,
                            kind: CoreExprKind::Local(CoreLocalName(field.name.clone())),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>()
        });
        if let Some(provider) = context_provider {
            self.context_providers.push(provider);
        }
        let mut body = self.lower_block(&decl.body, &mut locals, TailPosition::Tail);
        if !context_fields.is_empty() {
            let context_name = CoreLocalName("__context".to_string());
            for field in context_fields.iter().rev() {
                let value = CoreExpr {
                    ty: field.ty.clone(),
                    span: decl.annotation.span(),
                    tail: TailPosition::NonTail,
                    kind: CoreExprKind::Field {
                        target: Box::new(CoreExpr {
                            ty: Type::Unknown,
                            span: decl.annotation.span(),
                            tail: TailPosition::NonTail,
                            kind: CoreExprKind::Local(context_name.clone()),
                        }),
                        field: field.name.clone(),
                    },
                };
                body = CoreExpr {
                    ty: body.ty.clone(),
                    span: decl.body.span,
                    tail: TailPosition::Tail,
                    kind: CoreExprKind::Let {
                        name: CoreLocalName(field.name.clone()),
                        value: Box::new(value),
                        body: Box::new(body),
                    },
                };
            }
            self.context_providers.pop();
        }
        self.validate_effect_runtime(&body);

        CoreValueDecl {
            name: self.global(&decl.name.text),
            export: decl.export,
            params,
            result_type,
            body,
        }
    }

    fn lower_block(
        &mut self,
        block: &Block,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        self.lower_block_from(&block.statements, 0, &block.result, locals, tail)
    }

    fn lower_block_from(
        &mut self,
        statements: &[Stmt],
        index: usize,
        result: &Expr,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let Some(statement) = statements.get(index) else {
            return self.lower_expr(result, locals, tail);
        };

        match statement {
            Stmt::Let(binding) => {
                let value = self.lower_expr(&binding.value, locals, TailPosition::NonTail);
                locals.insert(
                    binding.name.text.clone(),
                    Local {
                        ty: value.ty.clone(),
                    },
                );
                let body = self.lower_block_from(statements, index + 1, result, locals, tail);
                CoreExpr {
                    ty: body.ty.clone(),
                    span: binding.span.join(body.span),
                    tail,
                    kind: CoreExprKind::Let {
                        name: CoreLocalName(binding.name.text.clone()),
                        value: Box::new(value),
                        body: Box::new(body),
                    },
                }
            }
            Stmt::Expr(expr) => {
                let value = self.lower_expr(expr, locals, TailPosition::NonTail);
                let body = self.lower_block_from(statements, index + 1, result, locals, tail);
                CoreExpr {
                    ty: body.ty.clone(),
                    span: value.span.join(body.span),
                    tail,
                    kind: CoreExprKind::Sequence {
                        value: Box::new(value),
                        body: Box::new(body),
                    },
                }
            }
        }
    }

    fn lower_expr(
        &mut self,
        expr: &Expr,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let ty = self.type_for_expr(expr, locals);
        let kind = match expr {
            Expr::Name(name) => self.lower_name_expr(name, locals),
            Expr::Int { value, .. } => CoreExprKind::Int(*value),
            Expr::Text { value, .. } => CoreExprKind::Text(value.clone()),
            Expr::Bool { value, .. } => CoreExprKind::Bool(*value),
            Expr::Unit { .. } => CoreExprKind::Unit,
            Expr::List { items, .. } => CoreExprKind::List(
                items
                    .iter()
                    .map(|item| self.lower_expr(item, locals, TailPosition::NonTail))
                    .collect(),
            ),
            Expr::Record { fields, .. } => CoreExprKind::Record {
                fields: self.lower_record_fields(fields, locals),
            },
            Expr::RecordUpdate { base, fields, .. } => CoreExprKind::RecordUpdate {
                base: Box::new(self.lower_expr(base, locals, TailPosition::NonTail)),
                fields: self.lower_record_fields(fields, locals),
            },
            Expr::Lambda { params, body, .. } => {
                let param_types = match &ty {
                    Type::Function(params, _) => params.as_slice(),
                    _ => &[],
                };
                let mut lambda_locals = locals.clone();
                let params = params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| {
                        let name = param
                            .name
                            .as_ref()
                            .map(|name| name.text.clone())
                            .unwrap_or_else(|| format!("_{}", index));
                        let ty = param_types.get(index).cloned().unwrap_or(Type::Unknown);
                        lambda_locals.insert(name.clone(), Local { ty: ty.clone() });
                        CoreParam {
                            name: CoreLocalName(name),
                            ty,
                        }
                    })
                    .collect();
                CoreExprKind::Lambda {
                    params,
                    body: Box::new(self.lower_block(body, &mut lambda_locals, TailPosition::Tail)),
                }
            }
            Expr::Field { target, field, .. } if self.reference_at(expr.span()).is_none() => {
                CoreExprKind::Field {
                    target: Box::new(self.lower_expr(target, locals, TailPosition::NonTail)),
                    field: field.text.clone(),
                }
            }
            Expr::Field { .. } => self.lower_resolved_global_expr(expr.span()),
            Expr::Call { callee, args, .. } => {
                return self.lower_call_expr(callee, args, Vec::new(), expr.span(), locals, tail);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => CoreExprKind::If {
                condition: Box::new(self.lower_expr(condition, locals, TailPosition::NonTail)),
                then_branch: Box::new(self.lower_expr(then_branch, locals, tail)),
                else_branch: Box::new(self.lower_expr(else_branch, locals, tail)),
            },
            Expr::Match {
                scrutinee, arms, ..
            } => CoreExprKind::Match {
                scrutinee: Box::new(self.lower_expr(scrutinee, locals, TailPosition::NonTail)),
                arms: arms
                    .iter()
                    .map(|arm| {
                        let mut arm_locals = locals.clone();
                        let pattern = self.lower_pattern(&arm.pattern, &mut arm_locals);
                        let value = self.lower_expr(&arm.value, &mut arm_locals, tail);
                        CoreMatchArm { pattern, value }
                    })
                    .collect(),
            },
            Expr::Pipeline { value, target, .. } => {
                let value = self.lower_expr(value, locals, TailPosition::NonTail);
                return self.lower_sugared_call(target, vec![value], expr.span(), locals, tail);
            }
            Expr::With { targets, body, .. } => {
                let mut reversed = targets.iter().rev();
                let Some(first_target) = reversed.next() else {
                    return self.lower_block(body, &mut locals.clone(), tail);
                };
                let action_ty = self.trailing_action_type(&first_target.target, locals);
                let mut current = self.lower_action_body(
                    body,
                    &action_ty,
                    &mut locals.clone(),
                    TailPosition::Tail,
                );
                let action = self.action_lambda(first_target.span, current, action_ty.as_ref());
                current = self.lower_sugared_call(
                    &first_target.target,
                    vec![action],
                    first_target.span,
                    locals,
                    tail,
                );
                for target in reversed {
                    let action_ty = self.trailing_action_type(&target.target, locals);
                    let action = self.action_lambda(target.span, current, action_ty.as_ref());
                    current = self.lower_sugared_call(
                        &target.target,
                        vec![action],
                        target.span,
                        locals,
                        tail,
                    );
                }
                return current;
            }
            Expr::Given { fields, body, .. } => {
                return self.lower_given_expr(fields, body, expr.span(), locals, tail);
            }
            Expr::GivenEach {
                init,
                cleanup,
                body,
                ..
            } => {
                return self.lower_given_each_expr(
                    init,
                    cleanup.as_deref(),
                    body,
                    expr.span(),
                    locals,
                    tail,
                );
            }
            Expr::Finally { cleanup, body, .. } => {
                let cleanup = self.lower_expr(cleanup, locals, TailPosition::NonTail);
                let body = self.lower_block(body, &mut locals.clone(), tail);
                return CoreExpr {
                    ty,
                    span: expr.span(),
                    tail,
                    kind: CoreExprKind::Finally {
                        cleanup: Box::new(cleanup),
                        body: Box::new(body),
                    },
                };
            }
            Expr::Within { scope, body, .. } => {
                let scope = self.lower_scope_name(scope, locals);
                self.default_scopes.push(scope);
                let body = self.lower_block(body, &mut locals.clone(), tail);
                self.default_scopes.pop();
                return body;
            }
            Expr::Handle {
                action,
                clauses,
                return_clause,
                ..
            } => self.lower_handle_expr(action, clauses, return_clause.as_ref(), locals, tail),
            Expr::Binary {
                op, left, right, ..
            } => CoreExprKind::Binary {
                op: lower_binary_op(*op),
                left: Box::new(self.lower_expr(left, locals, TailPosition::NonTail)),
                right: Box::new(self.lower_expr(right, locals, TailPosition::NonTail)),
            },
            Expr::Error { span } => {
                self.unsupported(*span, "cannot lower parser error expression");
                CoreExprKind::Unit
            }
        };

        CoreExpr {
            ty,
            span: expr.span(),
            tail,
            kind,
        }
    }

    fn lower_call_expr(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        trailing: Vec<CoreExpr>,
        span: Span,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let mut lowered_args = args
            .iter()
            .map(|arg| self.lower_expr(arg, locals, TailPosition::NonTail))
            .collect::<Vec<_>>();
        lowered_args.extend(trailing);
        let callee_ty = self.type_for_expr(callee, locals);
        self.insert_context_arg(&callee_ty, &mut lowered_args, span);
        self.insert_elided_scope_arg(&callee_ty, &mut lowered_args);
        let kind = self.lower_call_kind(callee, callee_ty, lowered_args, locals);
        CoreExpr {
            ty: self.type_at(span),
            span,
            tail,
            kind,
        }
    }

    fn lower_given_expr(
        &mut self,
        fields: &[FieldExpr],
        body: &Block,
        span: Span,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let mut lowered_fields = fields
            .iter()
            .map(|field| {
                let value = self.lower_expr(&field.value, locals, TailPosition::NonTail);
                (field.name.text.clone(), value)
            })
            .collect::<Vec<_>>();

        let mut body_locals = locals.clone();
        let mut providers = BTreeMap::new();
        for (name, value) in &lowered_fields {
            body_locals.insert(
                name.clone(),
                Local {
                    ty: value.ty.clone(),
                },
            );
            providers.insert(
                name.clone(),
                CoreExpr {
                    ty: value.ty.clone(),
                    span,
                    tail: TailPosition::NonTail,
                    kind: CoreExprKind::Local(CoreLocalName(name.clone())),
                },
            );
        }

        self.context_providers.push(providers);
        let mut lowered = self.lower_block(body, &mut body_locals, tail);
        self.context_providers.pop();

        while let Some((name, value)) = lowered_fields.pop() {
            if matches!(&value.kind, CoreExprKind::Local(local) if local.0 == name) {
                continue;
            }
            lowered = CoreExpr {
                ty: lowered.ty.clone(),
                span,
                tail,
                kind: CoreExprKind::Let {
                    name: CoreLocalName(name),
                    value: Box::new(value),
                    body: Box::new(lowered),
                },
            };
        }

        lowered
    }

    fn lower_given_each_expr(
        &mut self,
        init: &Expr,
        cleanup: Option<&Expr>,
        body: &Block,
        span: Span,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let syntax_provider_fields = given_each_provider_fields(init, self, locals);
        let init = self.lower_expr(init, locals, TailPosition::NonTail);
        let provider_ty = resource_initializer_result(&init.ty);
        let provider_name = CoreLocalName(format!(
            "__resource_context_{}",
            self.next_resource_provider
        ));
        self.next_resource_provider += 1;
        let provider_fields = {
            let fields = record_fields(&provider_ty);
            if fields.is_empty() {
                syntax_provider_fields
            } else {
                fields
            }
        };

        let mut body_locals = locals.clone();
        let mut providers = BTreeMap::new();
        for (name, ty) in &provider_fields {
            body_locals.insert(name.clone(), Local { ty: ty.clone() });
            providers.insert(
                name.clone(),
                CoreExpr {
                    ty: ty.clone(),
                    span,
                    tail: TailPosition::NonTail,
                    kind: CoreExprKind::Field {
                        target: Box::new(CoreExpr {
                            ty: provider_ty.clone(),
                            span,
                            tail: TailPosition::NonTail,
                            kind: CoreExprKind::Local(provider_name.clone()),
                        }),
                        field: name.clone(),
                    },
                },
            );
        }
        self.context_providers.push(providers);
        let mut lowered_body = self.lower_block(body, &mut body_locals, tail);
        self.context_providers.pop();

        for (name, ty) in provider_fields.iter().rev() {
            lowered_body = CoreExpr {
                ty: lowered_body.ty.clone(),
                span,
                tail,
                kind: CoreExprKind::Let {
                    name: CoreLocalName(name.clone()),
                    value: Box::new(CoreExpr {
                        ty: ty.clone(),
                        span,
                        tail: TailPosition::NonTail,
                        kind: CoreExprKind::Field {
                            target: Box::new(CoreExpr {
                                ty: provider_ty.clone(),
                                span,
                                tail: TailPosition::NonTail,
                                kind: CoreExprKind::Local(provider_name.clone()),
                            }),
                            field: name.clone(),
                        },
                    }),
                    body: Box::new(lowered_body),
                },
            };
        }

        let cleanup = cleanup
            .map(|cleanup| Box::new(self.lower_expr(cleanup, locals, TailPosition::NonTail)));
        CoreExpr {
            ty: self.type_at(span),
            span,
            tail,
            kind: CoreExprKind::GivenEach {
                provider: provider_name,
                init: Box::new(init),
                cleanup,
                body: Box::new(lowered_body),
            },
        }
    }

    fn lower_sugared_call(
        &mut self,
        target: &Expr,
        trailing: Vec<CoreExpr>,
        span: Span,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        match target {
            Expr::Pipeline { value, target, .. } => {
                let mut args = vec![self.lower_expr(value, locals, TailPosition::NonTail)];
                args.extend(trailing);
                self.lower_sugared_call(target, args, span, locals, tail)
            }
            Expr::Call { callee, args, .. } => {
                self.lower_call_expr(callee, args, trailing, span, locals, tail)
            }
            other => {
                let callee_ty = self.type_for_expr(other, locals);
                let mut args = trailing;
                self.insert_context_arg(&callee_ty, &mut args, span);
                self.insert_elided_scope_arg(&callee_ty, &mut args);
                let kind = self.lower_call_kind(other, callee_ty, args, locals);
                CoreExpr {
                    ty: self.type_at(span),
                    span,
                    tail,
                    kind,
                }
            }
        }
    }

    fn insert_context_arg(&mut self, callee_ty: &Type, args: &mut Vec<CoreExpr>, span: Span) {
        let Type::Function(_, result) = callee_ty else {
            return;
        };
        let Type::Contextual { context, .. } = result.as_ref() else {
            return;
        };
        if context.fields.is_empty() {
            return;
        }

        let fields = context
            .fields
            .iter()
            .map(|field| {
                let value = self.context_provider(&field.name).unwrap_or_else(|| {
                    self.unsupported(
                        span,
                        format!("missing context provider `{}` during lowering", field.name),
                    );
                    CoreExpr {
                        ty: Type::Unknown,
                        span,
                        tail: TailPosition::NonTail,
                        kind: CoreExprKind::Unit,
                    }
                });
                CoreRecordField {
                    name: field.name.clone(),
                    value,
                }
            })
            .collect::<Vec<_>>();
        args.insert(
            0,
            CoreExpr {
                ty: core_closed_record(
                    context
                        .fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect(),
                ),
                span,
                tail: TailPosition::NonTail,
                kind: CoreExprKind::Record { fields },
            },
        );
    }

    fn context_provider(&self, name: &str) -> Option<CoreExpr> {
        self.context_providers
            .iter()
            .rev()
            .find_map(|providers| providers.get(name).cloned())
    }

    fn lower_call_kind(
        &mut self,
        callee: &Expr,
        callee_ty: Type,
        args: Vec<CoreExpr>,
        locals: &mut BTreeMap<String, Local>,
    ) -> CoreExprKind {
        if let Some(constructor) = self.constructor_reference(callee.span()) {
            CoreExprKind::Variant { constructor, args }
        } else if let Some(operation) = self.operation_reference(callee.span()) {
            if let Some(info) = self.operation_info(&operation).filter(|info| info.scoped) {
                let expected_params = match &callee_ty {
                    Type::Function(params, _) => params.len(),
                    _ => args.len(),
                };
                let (scope, args) = if args.len() + 1 == expected_params {
                    if let Some(scope) = self.default_scopes.last() {
                        (scope.clone(), args)
                    } else if let Some((scope, args)) = args.split_first() {
                        (scope.clone(), args.to_vec())
                    } else {
                        self.unsupported_effect_runtime(
                            callee.span(),
                            "scoped operation calls need an explicit or `within` scope",
                        );
                        return CoreExprKind::Operation {
                            operation,
                            scoped: None,
                            args,
                        };
                    }
                } else if let Some((scope, args)) = args.split_first() {
                    (scope.clone(), args.to_vec())
                } else if let Some(scope) = self.default_scopes.last() {
                    (scope.clone(), args)
                } else {
                    self.unsupported_effect_runtime(
                        callee.span(),
                        "scoped operation calls need an explicit or `within` scope",
                    );
                    return CoreExprKind::Operation {
                        operation,
                        scoped: None,
                        args,
                    };
                };
                CoreExprKind::Operation {
                    operation,
                    scoped: Some(CoreScopedRequest {
                        family: info.family,
                        scope: Box::new(scope),
                    }),
                    args,
                }
            } else {
                CoreExprKind::Operation {
                    operation,
                    scoped: None,
                    args,
                }
            }
        } else {
            CoreExprKind::Call {
                callee: Box::new(self.lower_expr(callee, locals, TailPosition::NonTail)),
                args,
            }
        }
    }

    fn insert_elided_scope_arg(&self, callee_ty: &Type, args: &mut Vec<CoreExpr>) {
        let Type::Function(params, _) = callee_ty else {
            return;
        };
        if params.len() != args.len() + 1 {
            return;
        }
        let Some(scope) = self.default_scopes.last() else {
            return;
        };
        args.insert(0, scope.clone());
    }

    fn lower_handle_expr(
        &mut self,
        action: &Expr,
        clauses: &[HandlerClause],
        return_clause: Option<&HandlerReturnClause>,
        locals: &BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExprKind {
        let handled_action_ty = self.handler_action_type(action, locals);
        let action = Box::new(self.lower_expr(action, &mut locals.clone(), TailPosition::NonTail));
        let action_ty = action.ty.clone();
        let clauses = clauses
            .iter()
            .map(|clause| {
                let mut clause_locals = locals.clone();
                let mut params = clause
                    .params
                    .iter()
                    .map(|param| {
                        let ty = self.type_at(param.span);
                        clause_locals.insert(param.text.clone(), Local { ty });
                        CoreLocalName(param.text.clone())
                    })
                    .collect::<Vec<_>>();
                let resume = params
                    .pop()
                    .unwrap_or_else(|| CoreLocalName("_resume".to_string()));
                let body = self.lower_block(&clause.body, &mut clause_locals, tail);
                CoreHandlerClause {
                    operation: self
                        .operation_reference(clause.operation.span)
                        .unwrap_or_else(|| self.global(&clause.operation.text)),
                    scoped: self.lower_scoped_handler(
                        clause.operation.span,
                        handled_action_ty.as_ref().unwrap_or(&action_ty),
                        locals,
                    ),
                    params,
                    resume,
                    body: Box::new(body),
                }
            })
            .collect();
        let return_clause = return_clause.map(|clause| {
            let mut clause_locals = locals.clone();
            let ty = self.type_at(clause.param.span);
            clause_locals.insert(clause.param.text.clone(), Local { ty });
            CoreHandlerReturnClause {
                param: CoreLocalName(clause.param.text.clone()),
                body: Box::new(self.lower_block(&clause.body, &mut clause_locals, tail)),
            }
        });
        CoreExprKind::Handle {
            action,
            clauses,
            return_clause,
        }
    }

    fn trailing_action_type(
        &self,
        target: &Expr,
        locals: &BTreeMap<String, Local>,
    ) -> Option<Type> {
        match target {
            Expr::Pipeline { target, .. } => self.trailing_action_type(target, locals),
            Expr::Call { callee, args, .. } => {
                let Type::Function(params, _) = self.type_for_expr(callee, locals) else {
                    return None;
                };
                params.get(args.len()).cloned()
            }
            other => {
                let Type::Function(params, _) = self.type_for_expr(other, locals) else {
                    return None;
                };
                params.first().cloned()
            }
        }
    }

    fn lower_action_body(
        &mut self,
        body: &Block,
        action_ty: &Option<Type>,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let Some((context_fields, _)) = action_context_fields(action_ty.as_ref()) else {
            return self.lower_block(body, locals, tail);
        };

        let context_name = CoreLocalName("__context".to_string());
        locals.insert(
            context_name.0.clone(),
            Local {
                ty: core_closed_record(
                    context_fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect(),
                ),
            },
        );
        for field in context_fields {
            locals.insert(
                field.name.clone(),
                Local {
                    ty: field.ty.clone(),
                },
            );
        }
        let providers = context_fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    CoreExpr {
                        ty: field.ty.clone(),
                        span: body.span,
                        tail: TailPosition::NonTail,
                        kind: CoreExprKind::Local(CoreLocalName(field.name.clone())),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        self.context_providers.push(providers);
        let mut lowered = self.lower_block(body, locals, tail);
        self.context_providers.pop();

        for field in context_fields.iter().rev() {
            let value = CoreExpr {
                ty: field.ty.clone(),
                span: body.span,
                tail: TailPosition::NonTail,
                kind: CoreExprKind::Field {
                    target: Box::new(CoreExpr {
                        ty: Type::Unknown,
                        span: body.span,
                        tail: TailPosition::NonTail,
                        kind: CoreExprKind::Local(context_name.clone()),
                    }),
                    field: field.name.clone(),
                },
            };
            lowered = CoreExpr {
                ty: lowered.ty.clone(),
                span: body.span,
                tail,
                kind: CoreExprKind::Let {
                    name: CoreLocalName(field.name.clone()),
                    value: Box::new(value),
                    body: Box::new(lowered),
                },
            };
        }

        lowered
    }

    fn action_lambda(&mut self, span: Span, body: CoreExpr, action_ty: Option<&Type>) -> CoreExpr {
        let params = action_context_fields(action_ty)
            .map(|(context_fields, _)| {
                vec![CoreParam {
                    name: CoreLocalName("__context".to_string()),
                    ty: core_closed_record(
                        context_fields
                            .iter()
                            .map(|field| (field.name.clone(), field.ty.clone()))
                            .collect(),
                    ),
                }]
            })
            .unwrap_or_default();
        CoreExpr {
            ty: action_ty
                .cloned()
                .unwrap_or_else(|| Type::Function(Vec::new(), Box::new(body.ty.clone()))),
            span,
            tail: TailPosition::NonTail,
            kind: CoreExprKind::Lambda {
                params,
                body: Box::new(body),
            },
        }
    }

    fn lower_name_expr(&mut self, name: &Name, locals: &BTreeMap<String, Local>) -> CoreExprKind {
        if let Some(local) = locals.get(&name.text) {
            let _ = &local.ty;
            return CoreExprKind::Local(CoreLocalName(name.text.clone()));
        }
        if let Some(constructor) = self.constructor_reference(name.span) {
            return CoreExprKind::Variant {
                constructor,
                args: Vec::new(),
            };
        }
        self.lower_resolved_global_expr(name.span)
    }

    fn lower_scope_name(&mut self, name: &Name, locals: &BTreeMap<String, Local>) -> CoreExpr {
        let ty = locals
            .get(&name.text)
            .map(|local| local.ty.clone())
            .unwrap_or_else(|| self.type_at(name.span));
        let kind = if locals.contains_key(&name.text) {
            CoreExprKind::Local(CoreLocalName(name.text.clone()))
        } else if let Some(reference) = self.reference_at(name.span) {
            CoreExprKind::Global(CoreGlobalName {
                module: reference.target.module.clone(),
                name: reference.target.name.clone(),
            })
        } else {
            CoreExprKind::Global(self.global(&name.text))
        };
        CoreExpr {
            ty,
            span: name.span,
            tail: TailPosition::NonTail,
            kind,
        }
    }

    fn handler_action_type(&self, action: &Expr, locals: &BTreeMap<String, Local>) -> Option<Type> {
        let Expr::Call { callee, .. } = action else {
            return None;
        };
        match self.type_for_expr(callee, locals) {
            Type::Function(_, result) => Some(*result),
            _ => None,
        }
    }

    fn lower_resolved_global_expr(&mut self, span: Span) -> CoreExprKind {
        if let Some(reference) = self.reference_at(span) {
            CoreExprKind::Global(CoreGlobalName {
                module: reference.target.module.clone(),
                name: reference.target.name.clone(),
            })
        } else {
            self.unsupported(span, "cannot lower unresolved global reference");
            CoreExprKind::Global(self.global("<unresolved>"))
        }
    }

    fn lower_record_fields(
        &mut self,
        fields: &[FieldExpr],
        locals: &mut BTreeMap<String, Local>,
    ) -> Vec<CoreRecordField> {
        fields
            .iter()
            .map(|field| CoreRecordField {
                name: field.name.text.clone(),
                value: self.lower_expr(&field.value, locals, TailPosition::NonTail),
            })
            .collect()
    }

    fn lower_pattern(
        &mut self,
        pattern: &Pattern,
        locals: &mut BTreeMap<String, Local>,
    ) -> CorePattern {
        let kind = match pattern {
            Pattern::Name(name) => {
                locals.insert(
                    name.text.clone(),
                    Local {
                        ty: self.type_at(name.span),
                    },
                );
                CorePatternKind::Binder(CoreLocalName(name.text.clone()))
            }
            Pattern::Wildcard { .. } => CorePatternKind::Wildcard,
            Pattern::Int { value, .. } => CorePatternKind::Int(*value),
            Pattern::Text { value, .. } => CorePatternKind::Text(value.clone()),
            Pattern::Bool { value, .. } => CorePatternKind::Bool(*value),
            Pattern::Unit { .. } => CorePatternKind::Unit,
            Pattern::List { items, rest, .. } => {
                let items = items
                    .iter()
                    .map(|item| self.lower_pattern(item, locals))
                    .collect();
                let rest = rest.as_ref().map(|name| {
                    locals.insert(
                        name.text.clone(),
                        Local {
                            ty: self.type_at(name.span),
                        },
                    );
                    CoreLocalName(name.text.clone())
                });
                CorePatternKind::List { items, rest }
            }
            Pattern::Record { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        locals.insert(
                            field.text.clone(),
                            Local {
                                ty: self.type_at(field.span),
                            },
                        );
                        CoreLocalName(field.text.clone())
                    })
                    .collect();
                CorePatternKind::Record { fields }
            }
            Pattern::Constructor { name, args, .. } => CorePatternKind::Constructor {
                constructor: self
                    .constructor_reference(name.span)
                    .unwrap_or_else(|| self.global(&name.text)),
                args: args
                    .iter()
                    .map(|arg| self.lower_pattern(arg, locals))
                    .collect(),
            },
            Pattern::Error { span } => {
                self.unsupported(*span, "cannot lower parser error pattern");
                CorePatternKind::Wildcard
            }
        };

        CorePattern {
            span: pattern.span(),
            kind,
        }
    }

    fn lower_type_expr(&mut self, ty: &TypeExpr) -> Type {
        match ty {
            TypeExpr::Named { path, .. } => Type::Named {
                name: path
                    .iter()
                    .map(|name| name.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                args: Vec::new(),
            }
            .into_primitive(),
            TypeExpr::Var(_) => Type::Unknown,
            TypeExpr::App { callee, args, .. } => {
                let name = match callee.as_ref() {
                    TypeExpr::Named { path, .. } => path
                        .iter()
                        .map(|name| name.text.as_str())
                        .collect::<Vec<_>>()
                        .join("."),
                    _ => return Type::Unknown,
                };
                Type::Named {
                    name,
                    args: args.iter().map(|arg| self.lower_type_expr(arg)).collect(),
                }
            }
            TypeExpr::Function { params, result, .. } => Type::Function(
                params
                    .iter()
                    .map(|param| self.lower_type_expr(param))
                    .collect(),
                Box::new(self.lower_type_expr(result)),
            ),
            TypeExpr::Effect { result, .. } => self.lower_type_expr(result),
            TypeExpr::Context { result, .. } => self.lower_type_expr(result),
            TypeExpr::Record { fields, .. } => Type::Row(RowType {
                kind: RowKind::Record,
                entries: fields
                    .iter()
                    .map(|field| RowEntry::Field {
                        name: field.name.text.clone(),
                        ty: self.lower_type_expr(&field.value),
                    })
                    .collect(),
                tail: None,
            }),
            TypeExpr::Variant { cases, .. } => Type::Row(RowType {
                kind: RowKind::Variant,
                entries: cases
                    .iter()
                    .map(|case| RowEntry::Case {
                        name: case.name.text.clone(),
                        payload: case
                            .payload
                            .iter()
                            .map(|payload| self.lower_type_expr(payload))
                            .collect(),
                    })
                    .collect(),
                tail: None,
            }),
            TypeExpr::Tuple { span, .. } => {
                self.unsupported(*span, "tuple types are not executable in Phase 2.5");
                Type::Unknown
            }
            TypeExpr::Error { span } => {
                self.unsupported(*span, "cannot lower parser error type");
                Type::Unknown
            }
        }
    }

    fn top_level_type(&self, name: &str) -> Type {
        self.session_check_types()
            .get(name)
            .cloned()
            .unwrap_or(Type::Unknown)
    }

    fn session_check_types(&self) -> &BTreeMap<String, Type> {
        self.parent
            .session
            .analysis
            .check
            .module_types
            .get(&self.module.name)
            .expect("compile session only exists after checking")
    }

    fn type_at(&self, span: Span) -> Type {
        self.typed_spans
            .iter()
            .find(|typed| typed.span == span)
            .map(|typed| parse_display_type(&typed.display_type))
            .unwrap_or(Type::Unknown)
    }

    fn type_for_expr(&self, expr: &Expr, locals: &BTreeMap<String, Local>) -> Type {
        if let Expr::Name(name) = expr
            && let Some(local) = locals.get(&name.text)
        {
            return local.ty.clone();
        }
        if let Some(reference) = self.reference_at(expr.span())
            && let Some(module_types) = self
                .parent
                .session
                .analysis
                .check
                .module_types
                .get(&reference.target.module)
            && let Some(ty) = module_types.get(&reference.target.name)
        {
            return ty.clone();
        }
        self.type_at(expr.span())
    }

    fn reference_at(&self, span: Span) -> Option<&fx_core::ResolvedReference> {
        self.parent
            .session
            .analysis
            .resolved
            .modules
            .get(&self.module.name)?
            .references
            .iter()
            .find(|reference| reference.span == span)
    }

    fn constructor_reference(&self, span: Span) -> Option<CoreGlobalName> {
        let reference = self.reference_at(span)?;
        if reference.target.kind == SymbolKind::Constructor {
            Some(CoreGlobalName {
                module: reference.target.module.clone(),
                name: reference.target.name.clone(),
            })
        } else {
            None
        }
    }

    fn operation_reference(&self, span: Span) -> Option<CoreGlobalName> {
        let reference = self.reference_at(span)?;
        if reference.target.kind == SymbolKind::Operation {
            Some(CoreGlobalName {
                module: reference.target.module.clone(),
                name: reference.target.name.clone(),
            })
        } else {
            None
        }
    }

    fn operation_info(&self, operation: &CoreGlobalName) -> Option<OperationInfo> {
        let module = self
            .parent
            .session
            .analysis
            .graph
            .modules
            .get(&operation.module)?;
        for item in &module.parsed.module.items {
            let Item::Effect(decl) = item else {
                continue;
            };
            if decl
                .operations
                .iter()
                .any(|candidate| candidate.name.text == operation.name)
            {
                return Some(OperationInfo {
                    family: CoreGlobalName {
                        module: module.name.clone(),
                        name: decl.name.text.clone(),
                    },
                    scoped: decl.scoped,
                });
            }
        }
        None
    }

    fn lower_scoped_handler(
        &mut self,
        operation_span: Span,
        action_ty: &Type,
        locals: &BTreeMap<String, Local>,
    ) -> Option<CoreScopedRequest> {
        let operation = self.operation_reference(operation_span)?;
        let info = self.operation_info(&operation)?;
        if !info.scoped {
            return None;
        }
        let scope_ty = first_scoped_effect_scope(action_ty, &info.family.name)?;
        let scope = self.scope_expr_for_type(&scope_ty, operation_span, locals)?;
        Some(CoreScopedRequest {
            family: info.family,
            scope: Box::new(scope),
        })
    }

    fn scope_expr_for_type(
        &mut self,
        scope_ty: &Type,
        span: Span,
        locals: &BTreeMap<String, Local>,
    ) -> Option<CoreExpr> {
        for (name, local) in locals {
            if &local.ty == scope_ty {
                return Some(CoreExpr {
                    ty: local.ty.clone(),
                    span,
                    tail: TailPosition::NonTail,
                    kind: CoreExprKind::Local(CoreLocalName(name.clone())),
                });
            }
        }
        if let Type::Named { name, args } = scope_ty
            && args.is_empty()
            && let Some(scope) = self.resolve_scope_global(name)
        {
            return Some(CoreExpr {
                ty: scope_ty.clone(),
                span,
                tail: TailPosition::NonTail,
                kind: CoreExprKind::Global(scope),
            });
        }
        self.unsupported_effect_runtime(
            span,
            "scoped handler scope could not be recovered for JavaScript emission",
        );
        None
    }

    fn resolve_scope_global(&self, name: &str) -> Option<CoreGlobalName> {
        if let Some(symbol) = self
            .module
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SymbolKind::Scope && symbol.name == name)
        {
            return Some(CoreGlobalName {
                module: symbol.module.clone(),
                name: symbol.name.clone(),
            });
        }
        for import in &self.module.imports {
            let Some(module) = self.parent.session.analysis.graph.modules.get(import) else {
                continue;
            };
            if let Some(symbol) = module.symbols.iter().find(|symbol| {
                symbol.kind == SymbolKind::Scope && symbol.exported && symbol.name == name
            }) {
                return Some(CoreGlobalName {
                    module: symbol.module.clone(),
                    name: symbol.name.clone(),
                });
            }
        }
        None
    }

    fn global(&self, name: &str) -> CoreGlobalName {
        CoreGlobalName {
            module: self.module.name.clone(),
            name: name.to_string(),
        }
    }

    fn validate_effect_runtime(&mut self, expr: &CoreExpr) {
        match &expr.kind {
            CoreExprKind::Operation { scoped, args, .. } => {
                if let Some(scoped) = scoped {
                    if core_needs_effect_runtime(&scoped.scope) {
                        self.unsupported_effect_runtime(
                            scoped.scope.span,
                            "effectful scoped operation scopes are not executable yet",
                        );
                    }
                    self.validate_effect_runtime(&scoped.scope);
                }
                for arg in args {
                    if core_needs_effect_runtime(arg) {
                        self.unsupported_effect_runtime(
                            arg.span,
                            "effectful operation arguments are not executable yet",
                        );
                    }
                    self.validate_effect_runtime(arg);
                }
            }
            CoreExprKind::Handle {
                action,
                clauses,
                return_clause,
            } => {
                self.validate_effect_runtime(action);
                for clause in clauses {
                    if let Some(scoped) = &clause.scoped {
                        if core_needs_effect_runtime(&scoped.scope) {
                            self.unsupported_effect_runtime(
                                scoped.scope.span,
                                "effectful scoped handler scopes are not executable yet",
                            );
                        }
                        self.validate_effect_runtime(&scoped.scope);
                    }
                    self.validate_effect_runtime(&clause.body);
                }
                if let Some(return_clause) = return_clause {
                    self.validate_effect_runtime(&return_clause.body);
                }
            }
            CoreExprKind::Finally { cleanup, body } => {
                self.validate_effect_runtime(cleanup);
                self.validate_effect_runtime(body);
            }
            CoreExprKind::GivenEach {
                init,
                cleanup,
                body,
                ..
            } => {
                self.validate_effect_runtime(init);
                if let Some(cleanup) = cleanup {
                    self.validate_effect_runtime(cleanup);
                }
                self.validate_effect_runtime(body);
            }
            CoreExprKind::Let { value, body, .. } | CoreExprKind::Sequence { value, body } => {
                self.validate_effect_runtime(value);
                self.validate_effect_runtime(body);
            }
            CoreExprKind::Lambda { body, .. } => self.validate_effect_runtime(body),
            CoreExprKind::Call { callee, args } => {
                if core_needs_effect_runtime(callee) {
                    self.unsupported_effect_runtime(
                        callee.span,
                        "effectful call callees are not executable yet",
                    );
                }
                self.validate_effect_runtime(callee);
                for arg in args {
                    if core_needs_effect_runtime(arg) {
                        self.unsupported_effect_runtime(
                            arg.span,
                            "effectful call arguments are not executable yet",
                        );
                    }
                    self.validate_effect_runtime(arg);
                }
            }
            CoreExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if core_needs_effect_runtime(condition) {
                    self.unsupported_effect_runtime(
                        condition.span,
                        "effectful if conditions are not executable yet",
                    );
                }
                self.validate_effect_runtime(condition);
                self.validate_effect_runtime(then_branch);
                self.validate_effect_runtime(else_branch);
            }
            CoreExprKind::Binary { left, right, .. } => {
                self.validate_effect_runtime(left);
                self.validate_effect_runtime(right);
            }
            CoreExprKind::Match { scrutinee, arms } => {
                if core_needs_effect_runtime(expr) {
                    self.unsupported_effect_runtime(
                        expr.span,
                        "effectful match expressions are not executable yet",
                    );
                }
                self.validate_effect_runtime(scrutinee);
                for arm in arms {
                    self.validate_effect_runtime(&arm.value);
                }
            }
            CoreExprKind::Record { fields } => {
                self.validate_effectful_fields(fields, "effectful record fields");
            }
            CoreExprKind::RecordUpdate { base, fields } => {
                if core_needs_effect_runtime(base) {
                    self.unsupported_effect_runtime(
                        base.span,
                        "effectful record update bases are not executable yet",
                    );
                }
                self.validate_effect_runtime(base);
                self.validate_effectful_fields(fields, "effectful record update fields");
            }
            CoreExprKind::Field { target, .. } => {
                if core_needs_effect_runtime(target) {
                    self.unsupported_effect_runtime(
                        target.span,
                        "effectful field targets are not executable yet",
                    );
                }
                self.validate_effect_runtime(target);
            }
            CoreExprKind::Variant { args, .. } => {
                for arg in args {
                    if core_needs_effect_runtime(arg) {
                        self.unsupported_effect_runtime(
                            arg.span,
                            "effectful variant payloads are not executable yet",
                        );
                    }
                    self.validate_effect_runtime(arg);
                }
            }
            CoreExprKind::List(items) => {
                for item in items {
                    if core_needs_effect_runtime(item) {
                        self.unsupported_effect_runtime(
                            item.span,
                            "effectful list items are not executable yet",
                        );
                    }
                    self.validate_effect_runtime(item);
                }
            }
            CoreExprKind::Unit
            | CoreExprKind::Bool(_)
            | CoreExprKind::Int(_)
            | CoreExprKind::Text(_)
            | CoreExprKind::Local(_)
            | CoreExprKind::Global(_) => {}
        }
    }

    fn validate_effectful_fields(&mut self, fields: &[CoreRecordField], message: &str) {
        for field in fields {
            if core_needs_effect_runtime(&field.value) {
                self.unsupported_effect_runtime(
                    field.value.span,
                    format!("{message} are not executable yet"),
                );
            }
            self.validate_effect_runtime(&field.value);
        }
    }

    fn unsupported(&mut self, span: Span, message: impl Into<String>) {
        self.parent.diagnostic(
            self.module,
            span,
            format!("Phase 2.5 compiler unsupported: {}", message.into()),
        );
    }

    fn unsupported_effect_runtime(&mut self, span: Span, message: impl Into<String>) {
        self.parent.diagnostic(
            self.module,
            span,
            format!("unsupported-effect-runtime: {}", message.into()),
        );
    }
}

fn lower_binary_op(op: BinaryOp) -> CoreBinaryOp {
    match op {
        BinaryOp::Equal => CoreBinaryOp::Equal,
        BinaryOp::Concat => CoreBinaryOp::Concat,
        BinaryOp::Add => CoreBinaryOp::Add,
        BinaryOp::Subtract => CoreBinaryOp::Subtract,
    }
}

fn core_needs_effect_runtime(expr: &CoreExpr) -> bool {
    match &expr.kind {
        CoreExprKind::Operation { .. }
        | CoreExprKind::Handle { .. }
        | CoreExprKind::Finally { .. }
        | CoreExprKind::GivenEach { .. } => true,
        CoreExprKind::Call { callee, args } => {
            core_returns_effectful(&callee.ty)
                || core_needs_effect_runtime(callee)
                || args.iter().any(core_needs_effect_runtime)
        }
        CoreExprKind::Let { value, body, .. } | CoreExprKind::Sequence { value, body } => {
            core_needs_effect_runtime(value) || core_needs_effect_runtime(body)
        }
        CoreExprKind::Lambda { .. } => false,
        CoreExprKind::Record { fields } => fields
            .iter()
            .any(|field| core_needs_effect_runtime(&field.value)),
        CoreExprKind::Field { target, .. } => core_needs_effect_runtime(target),
        CoreExprKind::RecordUpdate { base, fields } => {
            core_needs_effect_runtime(base)
                || fields
                    .iter()
                    .any(|field| core_needs_effect_runtime(&field.value))
        }
        CoreExprKind::Variant { args, .. } | CoreExprKind::List(args) => {
            args.iter().any(core_needs_effect_runtime)
        }
        CoreExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            core_needs_effect_runtime(condition)
                || core_needs_effect_runtime(then_branch)
                || core_needs_effect_runtime(else_branch)
        }
        CoreExprKind::Match { scrutinee, arms } => {
            core_needs_effect_runtime(scrutinee)
                || arms.iter().any(|arm| core_needs_effect_runtime(&arm.value))
        }
        CoreExprKind::Binary { left, right, .. } => {
            core_needs_effect_runtime(left) || core_needs_effect_runtime(right)
        }
        CoreExprKind::Unit
        | CoreExprKind::Bool(_)
        | CoreExprKind::Int(_)
        | CoreExprKind::Text(_)
        | CoreExprKind::Local(_)
        | CoreExprKind::Global(_) => false,
    }
}

fn split_contextual_result(ty: Type) -> (Vec<ContextFieldType>, Type) {
    match ty {
        Type::Contextual { result, context } => (context.fields, *result),
        other => (Vec::new(), other),
    }
}

fn action_context_fields(action_ty: Option<&Type>) -> Option<(&[ContextFieldType], &Type)> {
    let Type::Function(_, result) = action_ty? else {
        return None;
    };
    let Type::Contextual { result, context } = result.as_ref() else {
        return None;
    };
    if context.fields.is_empty() {
        return None;
    }
    Some((context.fields.as_slice(), result.as_ref()))
}

fn core_closed_record(fields: Vec<(String, Type)>) -> Type {
    Type::Row(RowType {
        kind: RowKind::Record,
        entries: fields
            .into_iter()
            .map(|(name, ty)| RowEntry::Field { name, ty })
            .collect(),
        tail: None,
    })
}

fn resource_initializer_result(ty: &Type) -> Type {
    match ty {
        Type::Function(_, result) => match result.as_ref() {
            Type::Effectful { result, .. } => result.as_ref().clone(),
            other => other.clone(),
        },
        Type::Effectful { result, .. } => result.as_ref().clone(),
        other => other.clone(),
    }
}

fn record_fields(ty: &Type) -> Vec<(String, Type)> {
    match ty {
        Type::Row(row) if row.kind == RowKind::Record => row
            .entries
            .iter()
            .filter_map(|entry| match entry {
                RowEntry::Field { name, ty } => Some((name.clone(), ty.clone())),
                RowEntry::Case { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn given_each_provider_fields(
    init: &Expr,
    lowerer: &ModuleLowerer<'_, '_>,
    locals: &BTreeMap<String, Local>,
) -> Vec<(String, Type)> {
    let Expr::Lambda { body, .. } = init else {
        return Vec::new();
    };
    let Expr::Record { fields, .. } = body.result.as_ref() else {
        return Vec::new();
    };
    fields
        .iter()
        .map(|field| {
            (
                field.name.text.clone(),
                lowerer.type_for_expr(&field.value, locals),
            )
        })
        .collect()
}

fn first_scoped_effect_scope(ty: &Type, family: &str) -> Option<Type> {
    let Type::Effectful { effects, .. } = ty else {
        return None;
    };
    effects.effects.iter().find_map(|effect| {
        let Type::Named { name, args } = effect else {
            return None;
        };
        (name == family).then(|| args.first().cloned()).flatten()
    })
}

fn core_returns_effectful(ty: &Type) -> bool {
    matches!(ty, Type::Function(_, result) if core_type_is_effectful(result))
}

fn core_type_is_effectful(ty: &Type) -> bool {
    match ty {
        Type::Effectful { .. } => true,
        Type::Contextual { result, .. } => core_type_is_effectful(result),
        _ => false,
    }
}

fn parse_display_type(display: &str) -> Type {
    match display {
        "Unit" => Type::Unit,
        "Bool" => Type::Bool,
        "Int" => Type::Int,
        "Text" => Type::Text,
        other if let Some(item) = other.strip_prefix("List ") => {
            Type::List(Box::new(parse_display_type(item)))
        }
        other => Type::Named {
            name: other.to_string(),
            args: Vec::new(),
        },
    }
}

trait PrimitiveTypeName {
    fn into_primitive(self) -> Type;
}

impl PrimitiveTypeName for Type {
    fn into_primitive(self) -> Type {
        match self {
            Type::Named { name, args } if args.is_empty() => match name.as_str() {
                "Unit" => Type::Unit,
                "Bool" => Type::Bool,
                "Int" => Type::Int,
                "Text" => Type::Text,
                _ => Type::Named { name, args },
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompileOptions, prepare_compile};
    use fx_core::{SourceFile, analyze_workspace};
    use std::path::PathBuf;

    #[test]
    fn lowers_phase_2_5_examples_to_core_program() {
        let program = lower_to_core(&phase_2_5_session()).expect("core program");
        let tree = program.debug_tree();

        assert!(tree.contains("module domain"));
        assert!(tree.contains("module functions"));
        assert!(tree.contains("module main"));
        assert!(tree.contains("import domain"));
        assert!(tree.contains("import functions"));
        assert!(tree.contains("type export domain.Person record fields=2"));
        assert!(tree.contains("type export domain.Lookup variant cases=3"));
        assert!(tree.contains("value export main.main"));
        assert!(tree.contains("expr record-update"));
        assert!(tree.contains("expr variant"));
        assert!(tree.contains("expr match"));
        assert!(tree.contains("expr list"));
        assert!(tree.contains("expr lambda"));
        assert!(tree.contains("expr call"));
        assert!(tree.contains("expr binary"));
    }

    #[test]
    fn lowers_accepted_phase_1_and_phase_2_examples() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples"),
            vec![
                source(
                    "examples/phase-1/todo.fx",
                    include_str!("../../../examples/phase-1/todo.fx"),
                ),
                source(
                    "examples/phase-1/record-update.fx",
                    include_str!("../../../examples/phase-1/record-update.fx"),
                ),
                source(
                    "examples/phase-1/lambda.fx",
                    include_str!("../../../examples/phase-1/lambda.fx"),
                ),
                source(
                    "examples/phase-1/patterns.fx",
                    include_str!("../../../examples/phase-1/patterns.fx"),
                ),
                source(
                    "examples/phase-1/operators.fx",
                    include_str!("../../../examples/phase-1/operators.fx"),
                ),
                source(
                    "examples/phase-2/rows.fx",
                    include_str!("../../../examples/phase-2/rows.fx"),
                ),
            ],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");

        let program = lower_to_core(&session).expect("core program");

        assert_eq!(program.modules.len(), 6);
        assert!(program.debug_tree().contains("module phase-2.rows"));
    }

    #[test]
    fn marks_tail_positions_on_function_results_and_recursive_calls() {
        let program = lower_to_core(&phase_2_5_session()).expect("core program");
        let domain = program
            .modules
            .iter()
            .find(|module| module.name == "domain")
            .expect("domain module");
        let find_person = domain
            .declarations
            .iter()
            .find_map(|decl| match decl {
                CoreDecl::Value(value) if value.name.name == "find-person" => Some(value),
                _ => None,
            })
            .expect("find-person");

        assert_eq!(find_person.body.tail, TailPosition::Tail);
        assert!(contains_tail_call_to(&find_person.body, "find-person"));
    }

    #[test]
    fn rejects_accepted_but_unsupported_tuple_type_declarations() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-2.5"),
            vec![source(
                "examples/phase-2.5/tuple.fx",
                "type Pair = (Int, Text)\n\nvalue() : Int\n  1\n",
            )],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");

        let diagnostics = lower_to_core(&session).expect_err("compiler diagnostics");

        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic
                    .message
                    .contains("only record and variant type declarations are executable")
            })
            .expect("unsupported type declaration diagnostic");

        assert!(
            diagnostic
                .message
                .starts_with("Phase 2.5 compiler unsupported:")
        );
        assert_eq!(
            diagnostic.path,
            PathBuf::from("examples/phase-2.5/tuple.fx")
        );
        assert!(diagnostic.span.start < diagnostic.span.end);
    }

    #[test]
    fn rejects_accepted_but_unsupported_effectful_list_items() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-3"),
            vec![source(
                "examples/phase-3/unsupported.fx",
                r#"export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

bad() : List Int with <Raise>
  [safe-divide(10, 0)]
"#,
            )],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");

        let diagnostics = lower_to_core(&session).expect_err("compiler diagnostics");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("unsupported-effect-runtime: effectful list items")
        }));
    }

    #[test]
    fn lowers_scoped_effect_requests_with_scope_metadata() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-4"),
            vec![source(
                "examples/phase-4/state.fx",
                r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

increment() : Int with <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n
"#,
            )],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");

        let program = lower_to_core(&session).expect("core program");
        let tree = program.debug_tree();

        assert!(tree.contains("scope state.Counter"));
        assert!(tree.contains("expr operation"));
        assert!(tree.contains("expr global"));
    }

    #[test]
    fn debug_tree_exposes_lowered_effect_control() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-3"),
            vec![source(
                "examples/phase-3/effects.fx",
                r#"export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(_message, _resume) ->
      default

    return(value) ->
      value
"#,
            )],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");
        let program = lower_to_core(&session).expect("core program");
        let tree = program.debug_tree();

        assert!(tree.contains("expr handle"));
        assert!(tree.contains("expr operation"));
    }

    #[test]
    fn lowers_contextual_calls_with_hidden_context_records() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-5"),
            vec![source(
                "examples/phase-5/main.fx",
                r#"type Clock = {
  now: Text
}

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ message

export main() : Text
  given { clock = { now: "10:00" } }
    stamp(" started")
"#,
            )],
        );
        let session = prepare_compile(analysis, CompileOptions::default()).expect("session");
        let program = lower_to_core(&session).expect("core program");
        let module = program
            .modules
            .iter()
            .find(|module| module.name == "main")
            .expect("main module");
        let stamp = value_decl(module, "stamp");
        let main = value_decl(module, "main");

        assert_eq!(stamp.params[0].name.0, "__context");
        assert!(extracts_context_field(&stamp.body, "clock"));
        assert!(contains_context_record_call_to(
            &main.body, "stamp", "clock"
        ));
    }

    fn phase_2_5_session() -> crate::CompileSession {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-2.5"),
            vec![
                source(
                    "examples/phase-2.5/domain.fx",
                    include_str!("../../../examples/phase-2.5/domain.fx"),
                ),
                source(
                    "examples/phase-2.5/functions.fx",
                    include_str!("../../../examples/phase-2.5/functions.fx"),
                ),
                source(
                    "examples/phase-2.5/main.fx",
                    include_str!("../../../examples/phase-2.5/main.fx"),
                ),
            ],
        );
        prepare_compile(analysis, CompileOptions::default()).expect("session")
    }

    fn source(path: &str, source: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from(path),
            source: source.to_string(),
        }
    }

    fn value_decl<'a>(module: &'a CoreModule, name: &str) -> &'a CoreValueDecl {
        module
            .declarations
            .iter()
            .find_map(|decl| match decl {
                CoreDecl::Value(value) if value.name.name == name => Some(value),
                _ => None,
            })
            .expect("value declaration")
    }

    fn extracts_context_field(expr: &CoreExpr, field: &str) -> bool {
        match &expr.kind {
            CoreExprKind::Let { name, value, body } => {
                name.0 == field
                    && matches!(
                        &value.kind,
                        CoreExprKind::Field { target, field: extracted }
                            if extracted == field
                                && matches!(
                                    &target.kind,
                                    CoreExprKind::Local(local) if local.0 == "__context"
                                )
                    )
                    || extracts_context_field(body, field)
            }
            _ => false,
        }
    }

    fn contains_context_record_call_to(expr: &CoreExpr, callee_name: &str, field: &str) -> bool {
        match &expr.kind {
            CoreExprKind::Call { callee, args } => {
                matches!(
                    &callee.kind,
                    CoreExprKind::Global(global) if global.name == callee_name
                ) && matches!(
                    args.first().map(|arg| &arg.kind),
                    Some(CoreExprKind::Record { fields })
                        if fields.iter().any(|record_field| record_field.name == field)
                ) || contains_context_record_call_to(callee, callee_name, field)
                    || args
                        .iter()
                        .any(|arg| contains_context_record_call_to(arg, callee_name, field))
            }
            CoreExprKind::Let { value, body, .. } | CoreExprKind::Sequence { value, body } => {
                contains_context_record_call_to(value, callee_name, field)
                    || contains_context_record_call_to(body, callee_name, field)
            }
            CoreExprKind::Lambda { body, .. } => {
                contains_context_record_call_to(body, callee_name, field)
            }
            CoreExprKind::Record { fields } => fields.iter().any(|field_expr| {
                contains_context_record_call_to(&field_expr.value, callee_name, field)
            }),
            CoreExprKind::Field { target, .. } => {
                contains_context_record_call_to(target, callee_name, field)
            }
            CoreExprKind::RecordUpdate { base, fields } => {
                contains_context_record_call_to(base, callee_name, field)
                    || fields.iter().any(|field_expr| {
                        contains_context_record_call_to(&field_expr.value, callee_name, field)
                    })
            }
            CoreExprKind::Variant { args, .. } | CoreExprKind::List(args) => args
                .iter()
                .any(|arg| contains_context_record_call_to(arg, callee_name, field)),
            CoreExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                contains_context_record_call_to(condition, callee_name, field)
                    || contains_context_record_call_to(then_branch, callee_name, field)
                    || contains_context_record_call_to(else_branch, callee_name, field)
            }
            CoreExprKind::Match { scrutinee, arms } => {
                contains_context_record_call_to(scrutinee, callee_name, field)
                    || arms
                        .iter()
                        .any(|arm| contains_context_record_call_to(&arm.value, callee_name, field))
            }
            CoreExprKind::Binary { left, right, .. } => {
                contains_context_record_call_to(left, callee_name, field)
                    || contains_context_record_call_to(right, callee_name, field)
            }
            CoreExprKind::Handle {
                action,
                clauses,
                return_clause,
            } => {
                contains_context_record_call_to(action, callee_name, field)
                    || clauses.iter().any(|clause| {
                        contains_context_record_call_to(&clause.body, callee_name, field)
                    })
                    || return_clause.as_ref().is_some_and(|clause| {
                        contains_context_record_call_to(&clause.body, callee_name, field)
                    })
            }
            CoreExprKind::Finally { cleanup, body } => {
                contains_context_record_call_to(cleanup, callee_name, field)
                    || contains_context_record_call_to(body, callee_name, field)
            }
            CoreExprKind::GivenEach {
                init,
                cleanup,
                body,
                ..
            } => {
                contains_context_record_call_to(init, callee_name, field)
                    || cleanup.as_ref().is_some_and(|cleanup| {
                        contains_context_record_call_to(cleanup, callee_name, field)
                    })
                    || contains_context_record_call_to(body, callee_name, field)
            }
            CoreExprKind::Operation { scoped, args, .. } => {
                scoped.as_ref().is_some_and(|scoped| {
                    contains_context_record_call_to(&scoped.scope, callee_name, field)
                }) || args
                    .iter()
                    .any(|arg| contains_context_record_call_to(arg, callee_name, field))
            }
            CoreExprKind::Unit
            | CoreExprKind::Bool(_)
            | CoreExprKind::Int(_)
            | CoreExprKind::Text(_)
            | CoreExprKind::Local(_)
            | CoreExprKind::Global(_) => false,
        }
    }

    fn contains_tail_call_to(expr: &CoreExpr, name: &str) -> bool {
        match &expr.kind {
            CoreExprKind::Call { callee, .. } => {
                expr.tail == TailPosition::Tail
                    && matches!(
                        &callee.kind,
                        CoreExprKind::Global(global) if global.name == name
                    )
            }
            CoreExprKind::Operation { args, .. } => {
                args.iter().any(|arg| contains_tail_call_to(arg, name))
            }
            CoreExprKind::Handle {
                action,
                clauses,
                return_clause,
            } => {
                contains_tail_call_to(action, name)
                    || clauses
                        .iter()
                        .any(|clause| contains_tail_call_to(&clause.body, name))
                    || return_clause
                        .as_ref()
                        .is_some_and(|clause| contains_tail_call_to(&clause.body, name))
            }
            CoreExprKind::Finally { cleanup, body } => {
                contains_tail_call_to(cleanup, name) || contains_tail_call_to(body, name)
            }
            CoreExprKind::GivenEach {
                init,
                cleanup,
                body,
                ..
            } => {
                contains_tail_call_to(init, name)
                    || cleanup
                        .as_ref()
                        .is_some_and(|cleanup| contains_tail_call_to(cleanup, name))
                    || contains_tail_call_to(body, name)
            }
            CoreExprKind::Let { value, body, .. } => {
                contains_tail_call_to(value, name) || contains_tail_call_to(body, name)
            }
            CoreExprKind::Sequence { value, body } => {
                contains_tail_call_to(value, name) || contains_tail_call_to(body, name)
            }
            CoreExprKind::Lambda { body, .. } => contains_tail_call_to(body, name),
            CoreExprKind::Record { fields } => fields
                .iter()
                .any(|field| contains_tail_call_to(&field.value, name)),
            CoreExprKind::Field { target, .. } => contains_tail_call_to(target, name),
            CoreExprKind::RecordUpdate { base, fields } => {
                contains_tail_call_to(base, name)
                    || fields
                        .iter()
                        .any(|field| contains_tail_call_to(&field.value, name))
            }
            CoreExprKind::Variant { args, .. } | CoreExprKind::List(args) => {
                args.iter().any(|arg| contains_tail_call_to(arg, name))
            }
            CoreExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                contains_tail_call_to(condition, name)
                    || contains_tail_call_to(then_branch, name)
                    || contains_tail_call_to(else_branch, name)
            }
            CoreExprKind::Match { scrutinee, arms } => {
                contains_tail_call_to(scrutinee, name)
                    || arms
                        .iter()
                        .any(|arm| contains_tail_call_to(&arm.value, name))
            }
            CoreExprKind::Binary { left, right, .. } => {
                contains_tail_call_to(left, name) || contains_tail_call_to(right, name)
            }
            CoreExprKind::Unit
            | CoreExprKind::Bool(_)
            | CoreExprKind::Int(_)
            | CoreExprKind::Text(_)
            | CoreExprKind::Local(_)
            | CoreExprKind::Global(_) => false,
        }
    }
}

use crate::CompileSession;
use crate::ir::*;
use fx_core::check::{RowEntry, RowKind, RowType, TypedSpan};
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
        let params = decl
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| CoreParam {
                name: CoreLocalName(param.name.text.clone()),
                ty: param_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| self.lower_type_expr(&param.annotation)),
            })
            .collect::<Vec<_>>();
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
        let body = self.lower_block(&decl.body, &mut locals, TailPosition::Tail);

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
        }
    }

    fn lower_expr(
        &mut self,
        expr: &Expr,
        locals: &mut BTreeMap<String, Local>,
        tail: TailPosition,
    ) -> CoreExpr {
        let ty = self.type_at(expr.span());
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
                let mut current = self.lower_block(body, &mut locals.clone(), TailPosition::Tail);
                for target in targets.iter().rev() {
                    let action = self.action_lambda(target.span, current);
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
        let kind = self.lower_call_kind(callee, lowered_args, locals);
        CoreExpr {
            ty: self.type_at(span),
            span,
            tail,
            kind,
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
                let kind = self.lower_call_kind(other, trailing, locals);
                CoreExpr {
                    ty: self.type_at(span),
                    span,
                    tail,
                    kind,
                }
            }
        }
    }

    fn lower_call_kind(
        &mut self,
        callee: &Expr,
        args: Vec<CoreExpr>,
        locals: &mut BTreeMap<String, Local>,
    ) -> CoreExprKind {
        if let Some(constructor) = self.constructor_reference(callee.span()) {
            CoreExprKind::Variant { constructor, args }
        } else {
            CoreExprKind::Call {
                callee: Box::new(self.lower_expr(callee, locals, TailPosition::NonTail)),
                args,
            }
        }
    }

    fn action_lambda(&mut self, span: Span, body: CoreExpr) -> CoreExpr {
        CoreExpr {
            ty: Type::Function(Vec::new(), Box::new(body.ty.clone())),
            span,
            tail: TailPosition::NonTail,
            kind: CoreExprKind::Lambda {
                params: Vec::new(),
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

    fn global(&self, name: &str) -> CoreGlobalName {
        CoreGlobalName {
            module: self.module.name.clone(),
            name: name.to_string(),
        }
    }

    fn unsupported(&mut self, span: Span, message: impl Into<String>) {
        self.parent.diagnostic(
            self.module,
            span,
            format!("Phase 2.5 compiler unsupported: {}", message.into()),
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

    fn contains_tail_call_to(expr: &CoreExpr, name: &str) -> bool {
        match &expr.kind {
            CoreExprKind::Call { callee, .. } => {
                expr.tail == TailPosition::Tail
                    && matches!(
                        &callee.kind,
                        CoreExprKind::Global(global) if global.name == name
                    )
            }
            CoreExprKind::Let { value, body, .. } => {
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

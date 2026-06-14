use fx_core::Type;
use fx_syntax::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreProgram {
    pub modules: Vec<CoreModule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreModule {
    pub name: String,
    pub imports: Vec<CoreImport>,
    pub declarations: Vec<CoreDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreImport {
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreDecl {
    Scope(CoreScopeDecl),
    Type(CoreTypeDecl),
    Value(CoreValueDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreScopeDecl {
    pub name: CoreGlobalName,
    pub export: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTypeDecl {
    pub name: CoreGlobalName,
    pub export: bool,
    pub params: Vec<String>,
    pub shape: CoreTypeShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreTypeShape {
    Record { fields: Vec<CoreTypeField> },
    Variant { cases: Vec<CoreVariantCase> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTypeField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreVariantCase {
    pub name: String,
    pub payload: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreValueDecl {
    pub name: CoreGlobalName,
    pub export: bool,
    pub params: Vec<CoreParam>,
    pub result_type: Type,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreParam {
    pub name: CoreLocalName,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreLocalName(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreGlobalName {
    pub module: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreExpr {
    pub ty: Type,
    pub span: Span,
    pub tail: TailPosition,
    pub kind: CoreExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreExprKind {
    Unit,
    Bool(bool),
    Int(i64),
    Text(String),
    Local(CoreLocalName),
    Global(CoreGlobalName),
    Let {
        name: CoreLocalName,
        value: Box<CoreExpr>,
        body: Box<CoreExpr>,
    },
    Sequence {
        value: Box<CoreExpr>,
        body: Box<CoreExpr>,
    },
    Lambda {
        params: Vec<CoreParam>,
        body: Box<CoreExpr>,
    },
    Call {
        callee: Box<CoreExpr>,
        args: Vec<CoreExpr>,
    },
    Operation {
        operation: CoreGlobalName,
        scoped: Option<CoreScopedRequest>,
        args: Vec<CoreExpr>,
    },
    Handle {
        action: Box<CoreExpr>,
        clauses: Vec<CoreHandlerClause>,
        return_clause: Option<CoreHandlerReturnClause>,
    },
    Finally {
        cleanup: Box<CoreExpr>,
        body: Box<CoreExpr>,
    },
    GivenEach {
        provider: CoreLocalName,
        init: Box<CoreExpr>,
        cleanup: Option<Box<CoreExpr>>,
        body: Box<CoreExpr>,
    },
    Record {
        fields: Vec<CoreRecordField>,
    },
    Field {
        target: Box<CoreExpr>,
        field: String,
    },
    RecordUpdate {
        base: Box<CoreExpr>,
        fields: Vec<CoreRecordField>,
    },
    Variant {
        constructor: CoreGlobalName,
        args: Vec<CoreExpr>,
    },
    If {
        condition: Box<CoreExpr>,
        then_branch: Box<CoreExpr>,
        else_branch: Box<CoreExpr>,
    },
    Match {
        scrutinee: Box<CoreExpr>,
        arms: Vec<CoreMatchArm>,
    },
    List(Vec<CoreExpr>),
    Binary {
        op: CoreBinaryOp,
        left: Box<CoreExpr>,
        right: Box<CoreExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreHandlerClause {
    pub operation: CoreGlobalName,
    pub scoped: Option<CoreScopedRequest>,
    pub params: Vec<CoreLocalName>,
    pub resume: CoreLocalName,
    pub body: Box<CoreExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreScopedRequest {
    pub family: CoreGlobalName,
    pub scope: Box<CoreExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreHandlerReturnClause {
    pub param: CoreLocalName,
    pub body: Box<CoreExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRecordField {
    pub name: String,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMatchArm {
    pub pattern: CorePattern,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePattern {
    pub span: Span,
    pub kind: CorePatternKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorePatternKind {
    Wildcard,
    Binder(CoreLocalName),
    Int(i64),
    Text(String),
    Bool(bool),
    Unit,
    List {
        items: Vec<CorePattern>,
        rest: Option<CoreLocalName>,
    },
    Record {
        fields: Vec<CoreLocalName>,
    },
    Constructor {
        constructor: CoreGlobalName,
        args: Vec<CorePattern>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailPosition {
    Tail,
    NonTail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreBinaryOp {
    Equal,
    Concat,
    Add,
    Subtract,
}

impl CoreProgram {
    pub fn debug_tree(&self) -> String {
        let mut output = String::new();
        for module in &self.modules {
            push_line(&mut output, 0, format_args!("module {}", module.name));
            for import in &module.imports {
                push_line(&mut output, 1, format_args!("import {}", import.module));
            }
            for declaration in &module.declarations {
                declaration.push_debug(&mut output, 1);
            }
        }
        output
    }
}

impl CoreDecl {
    fn push_debug(&self, output: &mut String, indent: usize) {
        match self {
            CoreDecl::Scope(declaration) => declaration.push_debug(output, indent),
            CoreDecl::Type(declaration) => declaration.push_debug(output, indent),
            CoreDecl::Value(declaration) => declaration.push_debug(output, indent),
        }
    }
}

impl CoreScopeDecl {
    fn push_debug(&self, output: &mut String, indent: usize) {
        let export = if self.export { " export" } else { "" };
        push_line(
            output,
            indent,
            format_args!("scope{} {}", export, self.name.display()),
        );
    }
}

impl CoreTypeDecl {
    fn push_debug(&self, output: &mut String, indent: usize) {
        let export = if self.export { " export" } else { "" };
        match &self.shape {
            CoreTypeShape::Record { fields } => push_line(
                output,
                indent,
                format_args!(
                    "type{} {} record fields={}",
                    export,
                    self.name.display(),
                    fields.len()
                ),
            ),
            CoreTypeShape::Variant { cases } => push_line(
                output,
                indent,
                format_args!(
                    "type{} {} variant cases={}",
                    export,
                    self.name.display(),
                    cases.len()
                ),
            ),
        }
    }
}

impl CoreValueDecl {
    fn push_debug(&self, output: &mut String, indent: usize) {
        let export = if self.export { " export" } else { "" };
        push_line(
            output,
            indent,
            format_args!("value{} {}", export, self.name.display()),
        );
        self.body.push_debug(output, indent + 1);
    }
}

impl CoreExpr {
    fn push_debug(&self, output: &mut String, indent: usize) {
        push_line(output, indent, format_args!("expr {}", self.kind.label()));
        match &self.kind {
            CoreExprKind::Let { value, body, .. } | CoreExprKind::Sequence { value, body } => {
                value.push_debug(output, indent + 1);
                body.push_debug(output, indent + 1);
            }
            CoreExprKind::Lambda { body, .. } => body.push_debug(output, indent + 1),
            CoreExprKind::Call { callee, args } => {
                callee.push_debug(output, indent + 1);
                for arg in args {
                    arg.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::Operation { scoped, args, .. } => {
                if let Some(scoped) = scoped {
                    scoped.scope.push_debug(output, indent + 1);
                }
                for arg in args {
                    arg.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::Handle {
                action,
                clauses,
                return_clause,
            } => {
                action.push_debug(output, indent + 1);
                for clause in clauses {
                    if let Some(scoped) = &clause.scoped {
                        scoped.scope.push_debug(output, indent + 1);
                    }
                    clause.body.push_debug(output, indent + 1);
                }
                if let Some(return_clause) = return_clause {
                    return_clause.body.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::Finally { cleanup, body } => {
                cleanup.push_debug(output, indent + 1);
                body.push_debug(output, indent + 1);
            }
            CoreExprKind::GivenEach {
                init,
                cleanup,
                body,
                ..
            } => {
                init.push_debug(output, indent + 1);
                if let Some(cleanup) = cleanup {
                    cleanup.push_debug(output, indent + 1);
                }
                body.push_debug(output, indent + 1);
            }
            CoreExprKind::Record { fields } | CoreExprKind::RecordUpdate { fields, .. } => {
                for field in fields {
                    field.value.push_debug(output, indent + 1);
                }
                if let CoreExprKind::RecordUpdate { base, .. } = &self.kind {
                    base.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::Field { target, .. } => target.push_debug(output, indent + 1),
            CoreExprKind::Variant { args, .. } | CoreExprKind::List(args) => {
                for arg in args {
                    arg.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.push_debug(output, indent + 1);
                then_branch.push_debug(output, indent + 1);
                else_branch.push_debug(output, indent + 1);
            }
            CoreExprKind::Match { scrutinee, arms } => {
                scrutinee.push_debug(output, indent + 1);
                for arm in arms {
                    arm.value.push_debug(output, indent + 1);
                }
            }
            CoreExprKind::Binary { left, right, .. } => {
                left.push_debug(output, indent + 1);
                right.push_debug(output, indent + 1);
            }
            CoreExprKind::Unit
            | CoreExprKind::Bool(_)
            | CoreExprKind::Int(_)
            | CoreExprKind::Text(_)
            | CoreExprKind::Local(_)
            | CoreExprKind::Global(_) => {}
        }
    }
}

impl CoreExprKind {
    fn label(&self) -> &'static str {
        match self {
            CoreExprKind::Unit => "unit",
            CoreExprKind::Bool(_) => "bool",
            CoreExprKind::Int(_) => "int",
            CoreExprKind::Text(_) => "text",
            CoreExprKind::Local(_) => "local",
            CoreExprKind::Global(_) => "global",
            CoreExprKind::Let { .. } => "let",
            CoreExprKind::Sequence { .. } => "sequence",
            CoreExprKind::Lambda { .. } => "lambda",
            CoreExprKind::Call { .. } => "call",
            CoreExprKind::Operation { .. } => "operation",
            CoreExprKind::Handle { .. } => "handle",
            CoreExprKind::Finally { .. } => "finally",
            CoreExprKind::GivenEach { .. } => "given-each",
            CoreExprKind::Record { .. } => "record",
            CoreExprKind::Field { .. } => "field",
            CoreExprKind::RecordUpdate { .. } => "record-update",
            CoreExprKind::Variant { .. } => "variant",
            CoreExprKind::If { .. } => "if",
            CoreExprKind::Match { .. } => "match",
            CoreExprKind::List(_) => "list",
            CoreExprKind::Binary { .. } => "binary",
        }
    }
}

impl CoreGlobalName {
    fn display(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

fn push_line(output: &mut String, indent: usize, args: std::fmt::Arguments<'_>) {
    for _ in 0..indent {
        output.push_str("  ");
    }
    output.push_str(&args.to_string());
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn represents_phase_2_5_style_multi_module_program() {
        let program = sample_program();

        assert_eq!(program.modules.len(), 2);
        assert_eq!(program.modules[1].imports[0].module, "domain");
        let CoreDecl::Value(main) = &program.modules[1].declarations[0] else {
            panic!("expected main value");
        };
        assert_eq!(main.name.name, "main");
        assert!(main.export);
        assert_eq!(main.body.tail, TailPosition::Tail);
    }

    #[test]
    fn represents_records_variants_lists_lambdas_and_tail_calls() {
        let body = sample_main_body();
        let CoreExprKind::Let {
            value,
            body: found_body,
            ..
        } = &body.kind
        else {
            panic!("expected let");
        };
        assert!(matches!(value.kind, CoreExprKind::RecordUpdate { .. }));

        let CoreExprKind::Let {
            value,
            body: final_body,
            ..
        } = &found_body.kind
        else {
            panic!("expected second let");
        };
        assert!(matches!(value.kind, CoreExprKind::Match { .. }));

        let CoreExprKind::Call { callee, args } = &final_body.kind else {
            panic!("expected tail call");
        };
        assert_eq!(final_body.tail, TailPosition::Tail);
        assert!(matches!(callee.kind, CoreExprKind::Lambda { .. }));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn debug_tree_reports_stable_high_level_shape() {
        let tree = sample_program().debug_tree();

        assert!(tree.contains("module domain"));
        assert!(tree.contains("module main"));
        assert!(tree.contains("import domain"));
        assert!(tree.contains("type export domain.Person record fields=2"));
        assert!(tree.contains("type export domain.Lookup variant cases=2"));
        assert!(tree.contains("value export main.main"));
        assert!(tree.contains("expr record-update"));
        assert!(tree.contains("expr match"));
        assert!(tree.contains("expr lambda"));
        assert!(tree.contains("expr call"));
    }

    fn sample_program() -> CoreProgram {
        CoreProgram {
            modules: vec![
                CoreModule {
                    name: "domain".to_string(),
                    imports: Vec::new(),
                    declarations: vec![
                        CoreDecl::Type(CoreTypeDecl {
                            name: global("domain", "Person"),
                            export: true,
                            params: Vec::new(),
                            shape: CoreTypeShape::Record {
                                fields: vec![
                                    CoreTypeField {
                                        name: "name".to_string(),
                                        ty: Type::Text,
                                    },
                                    CoreTypeField {
                                        name: "done".to_string(),
                                        ty: Type::Bool,
                                    },
                                ],
                            },
                        }),
                        CoreDecl::Type(CoreTypeDecl {
                            name: global("domain", "Lookup"),
                            export: true,
                            params: vec!["a".to_string()],
                            shape: CoreTypeShape::Variant {
                                cases: vec![
                                    CoreVariantCase {
                                        name: "Found".to_string(),
                                        payload: vec![Type::Unknown],
                                    },
                                    CoreVariantCase {
                                        name: "Missing".to_string(),
                                        payload: Vec::new(),
                                    },
                                ],
                            },
                        }),
                    ],
                },
                CoreModule {
                    name: "main".to_string(),
                    imports: vec![CoreImport {
                        module: "domain".to_string(),
                    }],
                    declarations: vec![CoreDecl::Value(CoreValueDecl {
                        name: global("main", "main"),
                        export: true,
                        params: Vec::new(),
                        result_type: Type::Text,
                        body: sample_main_body(),
                    })],
                },
            ],
        }
    }

    fn sample_main_body() -> CoreExpr {
        expr(
            Type::Text,
            TailPosition::Tail,
            CoreExprKind::Let {
                name: local("checked"),
                value: Box::new(record_update_expr()),
                body: Box::new(expr(
                    Type::Text,
                    TailPosition::Tail,
                    CoreExprKind::Let {
                        name: local("found"),
                        value: Box::new(match_expr()),
                        body: Box::new(lambda_call_expr()),
                    },
                )),
            },
        )
    }

    fn record_update_expr() -> CoreExpr {
        expr(
            person_type(),
            TailPosition::NonTail,
            CoreExprKind::RecordUpdate {
                base: Box::new(expr(
                    person_type(),
                    TailPosition::NonTail,
                    CoreExprKind::Record {
                        fields: vec![
                            CoreRecordField {
                                name: "name".to_string(),
                                value: text("Grace"),
                            },
                            CoreRecordField {
                                name: "done".to_string(),
                                value: bool_expr(false),
                            },
                        ],
                    },
                )),
                fields: vec![CoreRecordField {
                    name: "done".to_string(),
                    value: bool_expr(true),
                }],
            },
        )
    }

    fn match_expr() -> CoreExpr {
        expr(
            Type::Text,
            TailPosition::NonTail,
            CoreExprKind::Match {
                scrutinee: Box::new(expr(
                    lookup_type(),
                    TailPosition::NonTail,
                    CoreExprKind::Variant {
                        constructor: global("domain", "Found"),
                        args: vec![expr(
                            person_type(),
                            TailPosition::NonTail,
                            CoreExprKind::Local(local("checked")),
                        )],
                    },
                )),
                arms: vec![
                    CoreMatchArm {
                        pattern: CorePattern {
                            span: span(),
                            kind: CorePatternKind::Constructor {
                                constructor: global("domain", "Found"),
                                args: vec![CorePattern {
                                    span: span(),
                                    kind: CorePatternKind::Binder(local("person")),
                                }],
                            },
                        },
                        value: field_expr(),
                    },
                    CoreMatchArm {
                        pattern: CorePattern {
                            span: span(),
                            kind: CorePatternKind::Binder(local("other")),
                        },
                        value: text("unknown"),
                    },
                ],
            },
        )
    }

    fn field_expr() -> CoreExpr {
        expr(
            Type::Text,
            TailPosition::NonTail,
            CoreExprKind::Field {
                target: Box::new(expr(
                    person_type(),
                    TailPosition::NonTail,
                    CoreExprKind::Local(local("person")),
                )),
                field: "name".to_string(),
            },
        )
    }

    fn lambda_call_expr() -> CoreExpr {
        expr(
            Type::Text,
            TailPosition::Tail,
            CoreExprKind::Call {
                callee: Box::new(expr(
                    Type::Function(vec![Type::Text], Box::new(Type::Text)),
                    TailPosition::NonTail,
                    CoreExprKind::Lambda {
                        params: vec![CoreParam {
                            name: local("value"),
                            ty: Type::Text,
                        }],
                        body: Box::new(expr(
                            Type::Text,
                            TailPosition::Tail,
                            CoreExprKind::Binary {
                                op: CoreBinaryOp::Concat,
                                left: Box::new(expr(
                                    Type::Text,
                                    TailPosition::NonTail,
                                    CoreExprKind::Local(local("value")),
                                )),
                                right: Box::new(text("!")),
                            },
                        )),
                    },
                )),
                args: vec![expr(
                    Type::Text,
                    TailPosition::NonTail,
                    CoreExprKind::List(vec![text("Grace")]),
                )],
            },
        )
    }

    fn expr(ty: Type, tail: TailPosition, kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            ty,
            span: span(),
            tail,
            kind,
        }
    }

    fn text(value: &str) -> CoreExpr {
        expr(
            Type::Text,
            TailPosition::NonTail,
            CoreExprKind::Text(value.to_string()),
        )
    }

    fn bool_expr(value: bool) -> CoreExpr {
        expr(Type::Bool, TailPosition::NonTail, CoreExprKind::Bool(value))
    }

    fn local(name: &str) -> CoreLocalName {
        CoreLocalName(name.to_string())
    }

    fn global(module: &str, name: &str) -> CoreGlobalName {
        CoreGlobalName {
            module: module.to_string(),
            name: name.to_string(),
        }
    }

    fn span() -> Span {
        Span::point(0)
    }

    fn person_type() -> Type {
        Type::Named {
            name: "domain.Person".to_string(),
            args: Vec::new(),
        }
    }

    fn lookup_type() -> Type {
        Type::Named {
            name: "domain.Lookup".to_string(),
            args: vec![person_type()],
        }
    }
}

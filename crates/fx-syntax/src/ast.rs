use crate::diagnostic::Diagnostic;
use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedModule {
    pub source: String,
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Import(ImportDecl),
    Type(TypeDecl),
    Effect(EffectDecl),
    Scope(ScopeDecl),
    Value(ValueDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Import(item) => item.span,
            Item::Type(item) => item.span,
            Item::Effect(item) => item.span,
            Item::Scope(item) => item.span,
            Item::Value(item) => item.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub export: bool,
    pub name: Name,
    pub params: Vec<Name>,
    pub body: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectDecl {
    pub export: bool,
    pub scoped: bool,
    pub name: Name,
    pub params: Vec<Name>,
    pub operations: Vec<OperationDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeDecl {
    pub export: bool,
    pub name: Name,
    pub protocol: Option<EffectRow>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationDecl {
    pub name: Name,
    pub params: Vec<Param>,
    pub result: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueDecl {
    pub export: bool,
    pub name: Name,
    pub params: Vec<Param>,
    pub annotation: TypeExpr,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: Name,
    pub annotation: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub result: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Let(LetStmt),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetStmt {
    pub name: Name,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    Named {
        path: Vec<Name>,
        span: Span,
    },
    Var(Name),
    App {
        callee: Box<TypeExpr>,
        args: Vec<TypeExpr>,
        span: Span,
    },
    Function {
        params: Vec<TypeExpr>,
        result: Box<TypeExpr>,
        span: Span,
    },
    Effect {
        result: Box<TypeExpr>,
        effects: EffectRow,
        span: Span,
    },
    Context {
        result: Box<TypeExpr>,
        context: ContextRow,
        span: Span,
    },
    Record {
        fields: Vec<FieldType>,
        tail: Option<RowTail>,
        span: Span,
    },
    Variant {
        cases: Vec<VariantCaseType>,
        tail: Option<RowTail>,
        span: Span,
    },
    Tuple {
        elems: Vec<TypeExpr>,
        span: Span,
    },
    Error {
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. }
            | TypeExpr::Var(Name { span, .. })
            | TypeExpr::App { span, .. }
            | TypeExpr::Function { span, .. }
            | TypeExpr::Effect { span, .. }
            | TypeExpr::Context { span, .. }
            | TypeExpr::Record { span, .. }
            | TypeExpr::Variant { span, .. }
            | TypeExpr::Tuple { span, .. }
            | TypeExpr::Error { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectRow {
    pub effects: Vec<TypeExpr>,
    pub tail: Option<RowTail>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextRow {
    pub fields: Vec<FieldType>,
    pub tail: Option<RowTail>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldType {
    pub name: Name,
    pub value: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowTail {
    pub name: Name,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantCaseType {
    pub name: Name,
    pub payload: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Name(Name),
    Int {
        value: i64,
        span: Span,
    },
    Text {
        value: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    Unit {
        span: Span,
    },
    List {
        items: Vec<Expr>,
        span: Span,
    },
    Record {
        fields: Vec<FieldExpr>,
        span: Span,
    },
    RecordUpdate {
        base: Box<Expr>,
        fields: Vec<FieldExpr>,
        span: Span,
    },
    Lambda {
        params: Vec<LambdaParam>,
        body: Block,
        span: Span,
    },
    Field {
        target: Box<Expr>,
        field: Name,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Pipeline {
        value: Box<Expr>,
        op_span: Span,
        target: Box<Expr>,
        span: Span,
    },
    With {
        targets: Vec<WithTarget>,
        body: Block,
        span: Span,
    },
    Given {
        fields: Vec<FieldExpr>,
        body: Block,
        span: Span,
    },
    GivenEach {
        init: Box<Expr>,
        cleanup: Option<Box<Expr>>,
        body: Block,
        span: Span,
    },
    Finally {
        cleanup: Box<Expr>,
        body: Block,
        span: Span,
    },
    Within {
        scope: Name,
        body: Block,
        span: Span,
    },
    Handle {
        action: Box<Expr>,
        clauses: Vec<HandlerClause>,
        return_clause: Option<HandlerReturnClause>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        op_span: Span,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Error {
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Name(Name { span, .. })
            | Expr::Int { span, .. }
            | Expr::Text { span, .. }
            | Expr::Bool { span, .. }
            | Expr::Unit { span }
            | Expr::List { span, .. }
            | Expr::Record { span, .. }
            | Expr::RecordUpdate { span, .. }
            | Expr::Lambda { span, .. }
            | Expr::Field { span, .. }
            | Expr::Call { span, .. }
            | Expr::If { span, .. }
            | Expr::Match { span, .. }
            | Expr::Pipeline { span, .. }
            | Expr::With { span, .. }
            | Expr::Given { span, .. }
            | Expr::GivenEach { span, .. }
            | Expr::Finally { span, .. }
            | Expr::Within { span, .. }
            | Expr::Handle { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Error { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WithTarget {
    pub target: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerClause {
    pub operation: Name,
    pub params: Vec<Name>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerReturnClause {
    pub param: Name,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Equal,
    Concat,
    Add,
    Subtract,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldExpr {
    pub name: Name,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LambdaParam {
    pub name: Option<Name>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    Name(Name),
    Wildcard {
        span: Span,
    },
    Int {
        value: i64,
        span: Span,
    },
    Text {
        value: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    Unit {
        span: Span,
    },
    List {
        items: Vec<Pattern>,
        rest: Option<Name>,
        span: Span,
    },
    Record {
        fields: Vec<Name>,
        span: Span,
    },
    Constructor {
        name: Name,
        args: Vec<Pattern>,
        span: Span,
    },
    Error {
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Name(Name { span, .. })
            | Pattern::Wildcard { span }
            | Pattern::Int { span, .. }
            | Pattern::Text { span, .. }
            | Pattern::Bool { span, .. }
            | Pattern::Unit { span }
            | Pattern::List { span, .. }
            | Pattern::Record { span, .. }
            | Pattern::Constructor { span, .. }
            | Pattern::Error { span } => *span,
        }
    }
}

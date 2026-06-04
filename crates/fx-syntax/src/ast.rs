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
    Value(ValueDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Import(item) => item.span,
            Item::Type(item) => item.span,
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
    Record {
        fields: Vec<FieldType>,
        span: Span,
    },
    Variant {
        cases: Vec<VariantCaseType>,
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
            | TypeExpr::Record { span, .. }
            | TypeExpr::Variant { span, .. }
            | TypeExpr::Tuple { span, .. }
            | TypeExpr::Error { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldType {
    pub name: Name,
    pub value: TypeExpr,
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
            | Expr::Binary { span, .. }
            | Expr::Error { span } => *span,
        }
    }
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

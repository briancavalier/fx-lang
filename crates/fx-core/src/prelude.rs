use crate::check::Type;
use fx_syntax::ast::BinaryOp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinaryOperator {
    pub(crate) label: &'static str,
    pub(crate) params: Vec<Type>,
    pub(crate) result: Type,
    pub(crate) display_type: Type,
}

pub(crate) fn primitive_value(name: &str) -> Option<Type> {
    match name {
        "unit" => Some(Type::Unit),
        _ => None,
    }
}

pub(crate) fn binary_operator(op: BinaryOp, left: &Type, right: &Type) -> BinaryOperator {
    match op {
        BinaryOp::Equal => equality_operator(left, right),
        BinaryOp::Concat => BinaryOperator {
            label: "++",
            params: vec![Type::Text, Type::Text],
            result: Type::Text,
            display_type: Type::Function(vec![Type::Text, Type::Text], Box::new(Type::Text)),
        },
        BinaryOp::Add => int_operator("+"),
        BinaryOp::Subtract => int_operator("-"),
    }
}

fn int_operator(label: &'static str) -> BinaryOperator {
    BinaryOperator {
        label,
        params: vec![Type::Int, Type::Int],
        result: Type::Int,
        display_type: Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
    }
}

fn equality_operator(left: &Type, right: &Type) -> BinaryOperator {
    let operand = if matches!(left, Type::Unknown | Type::Var(_)) {
        right.clone()
    } else {
        left.clone()
    };
    let display_operand = if matches!(operand, Type::Unknown) {
        Type::Var("a".to_string())
    } else {
        operand
    };

    BinaryOperator {
        label: "==",
        params: vec![left.clone(), right.clone()],
        result: Type::Bool,
        display_type: Type::Function(
            vec![display_operand.clone(), display_operand],
            Box::new(Type::Bool),
        ),
    }
}

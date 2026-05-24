use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

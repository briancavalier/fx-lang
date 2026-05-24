pub mod ast;
pub mod diagnostic;
mod lexer;
mod parser;
pub mod span;

pub use parser::{parse_file, parse_module};

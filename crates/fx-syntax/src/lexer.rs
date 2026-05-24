use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    Ident(String),
    Int(i64),
    Text(String),
    Keyword(Keyword),
    Newline,
    Indent,
    Dedent,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LAngle,
    RAngle,
    Colon,
    Comma,
    Dot,
    Equal,
    Arrow,
    PlusPlus,
    EqEq,
    Plus,
    Minus,
    Spread,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keyword {
    Export,
    Import,
    Type,
    If,
    Then,
    Else,
    Match,
    True,
    False,
    Effect,
    Handle,
    Given,
    Scope,
    With,
}

pub(crate) fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut indent_stack = vec![0usize];
    let mut i = 0usize;
    let mut line_start = true;

    while i < bytes.len() {
        if line_start {
            let line_offset = i;
            let mut spaces = 0usize;
            while i < bytes.len() && bytes[i] == b' ' {
                spaces += 1;
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'\n' {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: Span::new(i, i + 1),
                });
                i += 1;
                continue;
            }
            let current = *indent_stack.last().expect("indent stack is never empty");
            if spaces > current {
                indent_stack.push(spaces);
                tokens.push(Token {
                    kind: TokenKind::Indent,
                    span: Span::new(line_offset, i),
                });
            } else {
                while spaces < *indent_stack.last().expect("indent stack is never empty") {
                    indent_stack.pop();
                    tokens.push(Token {
                        kind: TokenKind::Dedent,
                        span: Span::point(i),
                    });
                }
                if spaces != *indent_stack.last().expect("indent stack is never empty") {
                    diagnostics.push(Diagnostic::new(
                        Span::new(line_offset, i),
                        "indentation does not match an earlier block",
                    ));
                }
            }
            line_start = false;
        }

        let start = i;
        match bytes[i] {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: Span::new(i, i + 1),
                });
                i += 1;
                line_start = true;
            }
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => single(&mut tokens, TokenKind::LParen, start, &mut i),
            b')' => single(&mut tokens, TokenKind::RParen, start, &mut i),
            b'{' => single(&mut tokens, TokenKind::LBrace, start, &mut i),
            b'}' => single(&mut tokens, TokenKind::RBrace, start, &mut i),
            b'[' => single(&mut tokens, TokenKind::LBracket, start, &mut i),
            b']' => single(&mut tokens, TokenKind::RBracket, start, &mut i),
            b'<' => single(&mut tokens, TokenKind::LAngle, start, &mut i),
            b'>' => single(&mut tokens, TokenKind::RAngle, start, &mut i),
            b':' => single(&mut tokens, TokenKind::Colon, start, &mut i),
            b',' => single(&mut tokens, TokenKind::Comma, start, &mut i),
            b'=' if bytes.get(i + 1) == Some(&b'=') => {
                i += 2;
                tokens.push(Token {
                    kind: TokenKind::EqEq,
                    span: Span::new(start, i),
                });
            }
            b'=' => single(&mut tokens, TokenKind::Equal, start, &mut i),
            b'+' if bytes.get(i + 1) == Some(&b'+') => {
                i += 2;
                tokens.push(Token {
                    kind: TokenKind::PlusPlus,
                    span: Span::new(start, i),
                });
            }
            b'+' => single(&mut tokens, TokenKind::Plus, start, &mut i),
            b'-' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                tokens.push(Token {
                    kind: TokenKind::Arrow,
                    span: Span::new(start, i),
                });
            }
            b'-' => single(&mut tokens, TokenKind::Minus, start, &mut i),
            b'.' if bytes.get(i + 1) == Some(&b'.') && bytes.get(i + 2) == Some(&b'.') => {
                i += 3;
                tokens.push(Token {
                    kind: TokenKind::Spread,
                    span: Span::new(start, i),
                });
            }
            b'.' => single(&mut tokens, TokenKind::Dot, start, &mut i),
            b'"' => lex_string(source, &mut tokens, &mut diagnostics, start, &mut i),
            c if is_ident_start(c) => lex_ident(source, &mut tokens, start, &mut i),
            c if c.is_ascii_digit() => {
                lex_int(source, &mut tokens, &mut diagnostics, start, &mut i)
            }
            _ => {
                diagnostics.push(Diagnostic::new(
                    Span::new(start, start + 1),
                    format!("unexpected character `{}`", bytes[i] as char),
                ));
                i += 1;
            }
        }
    }

    while indent_stack.len() > 1 {
        indent_stack.pop();
        tokens.push(Token {
            kind: TokenKind::Dedent,
            span: Span::point(source.len()),
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::point(source.len()),
    });
    (tokens, diagnostics)
}

fn single(tokens: &mut Vec<Token>, kind: TokenKind, start: usize, i: &mut usize) {
    *i += 1;
    tokens.push(Token {
        kind,
        span: Span::new(start, *i),
    });
}

fn lex_string(
    source: &str,
    tokens: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
    start: usize,
    i: &mut usize,
) {
    *i += 1;
    let mut value = String::new();
    while *i < source.len() {
        let b = source.as_bytes()[*i];
        if b == b'"' {
            *i += 1;
            tokens.push(Token {
                kind: TokenKind::Text(value),
                span: Span::new(start, *i),
            });
            return;
        }
        if b == b'\n' {
            break;
        }
        value.push(b as char);
        *i += 1;
    }
    diagnostics.push(Diagnostic::new(
        Span::new(start, *i),
        "unterminated string literal",
    ));
}

fn lex_int(
    source: &str,
    tokens: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
    start: usize,
    i: &mut usize,
) {
    while *i < source.len() && source.as_bytes()[*i].is_ascii_digit() {
        *i += 1;
    }
    match source[start..*i].parse::<i64>() {
        Ok(value) => tokens.push(Token {
            kind: TokenKind::Int(value),
            span: Span::new(start, *i),
        }),
        Err(_) => diagnostics.push(Diagnostic::new(
            Span::new(start, *i),
            "integer literal is out of range",
        )),
    }
}

fn lex_ident(source: &str, tokens: &mut Vec<Token>, start: usize, i: &mut usize) {
    while *i < source.len() {
        let b = source.as_bytes()[*i];
        if is_ident_continue(b) || (b == b'-' && can_continue_after_hyphen(source, *i)) {
            *i += 1;
        } else {
            break;
        }
    }
    let text = &source[start..*i];
    let kind = match text {
        "export" => TokenKind::Keyword(Keyword::Export),
        "import" => TokenKind::Keyword(Keyword::Import),
        "type" => TokenKind::Keyword(Keyword::Type),
        "if" => TokenKind::Keyword(Keyword::If),
        "then" => TokenKind::Keyword(Keyword::Then),
        "else" => TokenKind::Keyword(Keyword::Else),
        "match" => TokenKind::Keyword(Keyword::Match),
        "true" => TokenKind::Keyword(Keyword::True),
        "false" => TokenKind::Keyword(Keyword::False),
        "effect" => TokenKind::Keyword(Keyword::Effect),
        "handle" => TokenKind::Keyword(Keyword::Handle),
        "given" => TokenKind::Keyword(Keyword::Given),
        "scope" => TokenKind::Keyword(Keyword::Scope),
        "with" => TokenKind::Keyword(Keyword::With),
        _ => TokenKind::Ident(text.to_string()),
    };
    tokens.push(Token {
        kind,
        span: Span::new(start, *i),
    });
}

fn can_continue_after_hyphen(source: &str, i: usize) -> bool {
    i > 0
        && source
            .as_bytes()
            .get(i + 1)
            .is_some_and(|b| is_ident_continue(*b))
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

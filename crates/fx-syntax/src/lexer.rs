use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexedSource {
    pub tokens: Vec<SyntaxToken>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxToken {
    pub kind: SyntaxTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyntaxTokenKind {
    Ident(String),
    Int(i64),
    Text(String),
    Keyword(Keyword),
    Whitespace,
    Comment,
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
    PipeGreater,
    Equal,
    Arrow,
    PlusPlus,
    EqEq,
    Plus,
    Minus,
    Spread,
    Eof,
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
    PipeGreater,
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
pub enum Keyword {
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
    Scoped,
    Handle,
    Given,
    Scope,
    With,
    Within,
}

pub(crate) fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let lexed = lex_source(source);
    let tokens = lexed
        .tokens
        .iter()
        .filter_map(parser_token)
        .collect::<Vec<_>>();
    (tokens, lexed.diagnostics)
}

pub fn lex_source(source: &str) -> LexedSource {
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
            if spaces > 0 {
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Whitespace,
                    span: Span::new(line_offset, i),
                });
            }
            if i < bytes.len() && bytes[i] == b'\n' {
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Newline,
                    span: Span::new(i, i + 1),
                });
                i += 1;
                continue;
            }
            let current = *indent_stack.last().expect("indent stack is never empty");
            if spaces > current {
                indent_stack.push(spaces);
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Indent,
                    span: Span::new(line_offset, i),
                });
            } else {
                while spaces < *indent_stack.last().expect("indent stack is never empty") {
                    indent_stack.pop();
                    tokens.push(SyntaxToken {
                        kind: SyntaxTokenKind::Dedent,
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
            b' ' | b'\t' | b'\r' => {
                while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r') {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Whitespace,
                    span: Span::new(start, i),
                });
            }
            b'\n' => {
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Newline,
                    span: Span::new(i, i + 1),
                });
                i += 1;
                line_start = true;
            }
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Comment,
                    span: Span::new(start, i),
                });
            }
            b'(' => single(&mut tokens, SyntaxTokenKind::LParen, start, &mut i),
            b')' => single(&mut tokens, SyntaxTokenKind::RParen, start, &mut i),
            b'{' => single(&mut tokens, SyntaxTokenKind::LBrace, start, &mut i),
            b'}' => single(&mut tokens, SyntaxTokenKind::RBrace, start, &mut i),
            b'[' => single(&mut tokens, SyntaxTokenKind::LBracket, start, &mut i),
            b']' => single(&mut tokens, SyntaxTokenKind::RBracket, start, &mut i),
            b'<' => single(&mut tokens, SyntaxTokenKind::LAngle, start, &mut i),
            b'>' => single(&mut tokens, SyntaxTokenKind::RAngle, start, &mut i),
            b':' => single(&mut tokens, SyntaxTokenKind::Colon, start, &mut i),
            b',' => single(&mut tokens, SyntaxTokenKind::Comma, start, &mut i),
            b'|' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::PipeGreater,
                    span: Span::new(start, i),
                });
            }
            b'=' if bytes.get(i + 1) == Some(&b'=') => {
                i += 2;
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::EqEq,
                    span: Span::new(start, i),
                });
            }
            b'=' => single(&mut tokens, SyntaxTokenKind::Equal, start, &mut i),
            b'+' if bytes.get(i + 1) == Some(&b'+') => {
                i += 2;
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::PlusPlus,
                    span: Span::new(start, i),
                });
            }
            b'+' => single(&mut tokens, SyntaxTokenKind::Plus, start, &mut i),
            b'-' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Arrow,
                    span: Span::new(start, i),
                });
            }
            b'-' => single(&mut tokens, SyntaxTokenKind::Minus, start, &mut i),
            b'.' if bytes.get(i + 1) == Some(&b'.') && bytes.get(i + 2) == Some(&b'.') => {
                i += 3;
                tokens.push(SyntaxToken {
                    kind: SyntaxTokenKind::Spread,
                    span: Span::new(start, i),
                });
            }
            b'.' => single(&mut tokens, SyntaxTokenKind::Dot, start, &mut i),
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
        tokens.push(SyntaxToken {
            kind: SyntaxTokenKind::Dedent,
            span: Span::point(source.len()),
        });
    }
    tokens.push(SyntaxToken {
        kind: SyntaxTokenKind::Eof,
        span: Span::point(source.len()),
    });
    LexedSource {
        tokens,
        diagnostics,
    }
}

fn single(tokens: &mut Vec<SyntaxToken>, kind: SyntaxTokenKind, start: usize, i: &mut usize) {
    *i += 1;
    tokens.push(SyntaxToken {
        kind,
        span: Span::new(start, *i),
    });
}

fn lex_string(
    source: &str,
    tokens: &mut Vec<SyntaxToken>,
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
            tokens.push(SyntaxToken {
                kind: SyntaxTokenKind::Text(value),
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
    tokens: &mut Vec<SyntaxToken>,
    diagnostics: &mut Vec<Diagnostic>,
    start: usize,
    i: &mut usize,
) {
    while *i < source.len() && source.as_bytes()[*i].is_ascii_digit() {
        *i += 1;
    }
    match source[start..*i].parse::<i64>() {
        Ok(value) => tokens.push(SyntaxToken {
            kind: SyntaxTokenKind::Int(value),
            span: Span::new(start, *i),
        }),
        Err(_) => diagnostics.push(Diagnostic::new(
            Span::new(start, *i),
            "integer literal is out of range",
        )),
    }
}

fn lex_ident(source: &str, tokens: &mut Vec<SyntaxToken>, start: usize, i: &mut usize) {
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
        "export" => SyntaxTokenKind::Keyword(Keyword::Export),
        "import" => SyntaxTokenKind::Keyword(Keyword::Import),
        "type" => SyntaxTokenKind::Keyword(Keyword::Type),
        "if" => SyntaxTokenKind::Keyword(Keyword::If),
        "then" => SyntaxTokenKind::Keyword(Keyword::Then),
        "else" => SyntaxTokenKind::Keyword(Keyword::Else),
        "match" => SyntaxTokenKind::Keyword(Keyword::Match),
        "true" => SyntaxTokenKind::Keyword(Keyword::True),
        "false" => SyntaxTokenKind::Keyword(Keyword::False),
        "effect" => SyntaxTokenKind::Keyword(Keyword::Effect),
        "scoped" => SyntaxTokenKind::Keyword(Keyword::Scoped),
        "handle" => SyntaxTokenKind::Keyword(Keyword::Handle),
        "given" => SyntaxTokenKind::Keyword(Keyword::Given),
        "scope" => SyntaxTokenKind::Keyword(Keyword::Scope),
        "with" => SyntaxTokenKind::Keyword(Keyword::With),
        "within" => SyntaxTokenKind::Keyword(Keyword::Within),
        _ => SyntaxTokenKind::Ident(text.to_string()),
    };
    tokens.push(SyntaxToken {
        kind,
        span: Span::new(start, *i),
    });
}

fn parser_token(token: &SyntaxToken) -> Option<Token> {
    let kind = match &token.kind {
        SyntaxTokenKind::Ident(text) => TokenKind::Ident(text.clone()),
        SyntaxTokenKind::Int(value) => TokenKind::Int(*value),
        SyntaxTokenKind::Text(value) => TokenKind::Text(value.clone()),
        SyntaxTokenKind::Keyword(keyword) => TokenKind::Keyword(*keyword),
        SyntaxTokenKind::Whitespace | SyntaxTokenKind::Comment => return None,
        SyntaxTokenKind::Newline => TokenKind::Newline,
        SyntaxTokenKind::Indent => TokenKind::Indent,
        SyntaxTokenKind::Dedent => TokenKind::Dedent,
        SyntaxTokenKind::LParen => TokenKind::LParen,
        SyntaxTokenKind::RParen => TokenKind::RParen,
        SyntaxTokenKind::LBrace => TokenKind::LBrace,
        SyntaxTokenKind::RBrace => TokenKind::RBrace,
        SyntaxTokenKind::LBracket => TokenKind::LBracket,
        SyntaxTokenKind::RBracket => TokenKind::RBracket,
        SyntaxTokenKind::LAngle => TokenKind::LAngle,
        SyntaxTokenKind::RAngle => TokenKind::RAngle,
        SyntaxTokenKind::Colon => TokenKind::Colon,
        SyntaxTokenKind::Comma => TokenKind::Comma,
        SyntaxTokenKind::Dot => TokenKind::Dot,
        SyntaxTokenKind::PipeGreater => TokenKind::PipeGreater,
        SyntaxTokenKind::Equal => TokenKind::Equal,
        SyntaxTokenKind::Arrow => TokenKind::Arrow,
        SyntaxTokenKind::PlusPlus => TokenKind::PlusPlus,
        SyntaxTokenKind::EqEq => TokenKind::EqEq,
        SyntaxTokenKind::Plus => TokenKind::Plus,
        SyntaxTokenKind::Minus => TokenKind::Minus,
        SyntaxTokenKind::Spread => TokenKind::Spread,
        SyntaxTokenKind::Eof => TokenKind::Eof,
    };
    Some(Token {
        kind,
        span: token.span,
    })
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

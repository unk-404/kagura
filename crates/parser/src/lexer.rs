use kagura_support::Span;

use crate::error::ParseError;
use crate::token::{Token, TokenKind};

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte.is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if byte == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            pos += 2;
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            let start = pos;
            pos += 2;
            while pos + 1 < bytes.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            if pos + 1 >= bytes.len() {
                return Err(ParseError::new(
                    "unterminated block comment",
                    Span::new(start as u32, bytes.len() as u32),
                ));
            }
            pos += 2;
            continue;
        }
        if is_ident_start(byte) {
            let start = pos;
            pos += 1;
            while pos < bytes.len() && is_ident_continue(bytes[pos]) {
                pos += 1;
            }
            tokens.push(Token::new(
                keyword_or_ident(&source[start..pos]),
                span(start, pos),
            ));
            continue;
        }
        if byte.is_ascii_digit() {
            let start = pos;
            if bytes.get(pos) == Some(&b'0')
                && let Some(prefix) = bytes.get(pos + 1).copied()
                && matches!(prefix, b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
            {
                let radix = match prefix {
                    b'x' | b'X' => 16,
                    b'b' | b'B' => 2,
                    _ => 8,
                };
                pos += 2;
                let digits = pos;
                while pos < bytes.len()
                    && (bytes[pos].is_ascii_hexdigit()
                        || bytes[pos].is_ascii_digit()
                        || bytes[pos] == b'_')
                {
                    pos += 1;
                }
                if digits == pos {
                    return Err(ParseError::new(
                        "expected digits after numeric prefix",
                        Span::new(start as u32, pos as u32),
                    ));
                }
                let text = source[digits..pos].replace('_', "");
                let value = u64::from_str_radix(&text, radix).map_err(|_| {
                    ParseError::new(
                        "invalid number literal",
                        Span::new(start as u32, pos as u32),
                    )
                })? as f64;
                tokens.push(Token::new(TokenKind::Number(value), span(start, pos)));
                continue;
            }
            while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                pos += 1;
            }
            if pos < bytes.len() && bytes[pos] == b'.' {
                pos += 1;
                while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                    pos += 1;
                }
            }
            if pos < bytes.len() && matches!(bytes[pos], b'e' | b'E') {
                let exp = pos;
                pos += 1;
                if pos < bytes.len() && matches!(bytes[pos], b'+' | b'-') {
                    pos += 1;
                }
                let digits = pos;
                while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                    pos += 1;
                }
                if digits == pos {
                    return Err(ParseError::new(
                        "expected exponent digits",
                        Span::new(exp as u32, pos as u32),
                    ));
                }
            }
            let text = &source[start..pos];
            let normalized = text.replace('_', "");
            let value = normalized.parse::<f64>().map_err(|_| {
                ParseError::new(
                    "invalid number literal",
                    Span::new(start as u32, pos as u32),
                )
            })?;
            tokens.push(Token::new(TokenKind::Number(value), span(start, pos)));
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let (token, next) = lex_string(source, pos, byte)?;
            tokens.push(token);
            pos = next;
            continue;
        }

        let (kind, len) = match (
            byte,
            bytes.get(pos + 1).copied(),
            bytes.get(pos + 2).copied(),
        ) {
            (b'=', Some(b'>'), _) => (TokenKind::Arrow, 2),
            (b'=', Some(b'='), Some(b'=')) => (TokenKind::StrictEq, 3),
            (b'!', Some(b'='), Some(b'=')) => (TokenKind::StrictNotEq, 3),
            (b'=', Some(b'='), _) => (TokenKind::EqEq, 2),
            (b'!', Some(b'='), _) => (TokenKind::NotEq, 2),
            (b'<', Some(b'='), _) => (TokenKind::Le, 2),
            (b'>', Some(b'='), _) => (TokenKind::Ge, 2),
            (b'&', Some(b'&'), Some(b'=')) => (TokenKind::AndEq, 3),
            (b'|', Some(b'|'), Some(b'=')) => (TokenKind::OrEq, 3),
            (b'?', Some(b'?'), Some(b'=')) => (TokenKind::NullishEq, 3),
            (b'?', Some(b'?'), _) => (TokenKind::Nullish, 2),
            (b'&', Some(b'&'), _) => (TokenKind::AndAnd, 2),
            (b'|', Some(b'|'), _) => (TokenKind::OrOr, 2),
            (b'+', Some(b'='), _) => (TokenKind::PlusEq, 2),
            (b'-', Some(b'='), _) => (TokenKind::MinusEq, 2),
            (b'*', Some(b'*'), _) => (TokenKind::StarStar, 2),
            (b'*', Some(b'='), _) => (TokenKind::StarEq, 2),
            (b'/', Some(b'='), _) => (TokenKind::SlashEq, 2),
            (b'%', Some(b'='), _) => (TokenKind::PercentEq, 2),
            (b'<', Some(b'<'), _) => (TokenKind::Shl, 2),
            (b'>', Some(b'>'), Some(b'>')) => (TokenKind::UShr, 3),
            (b'>', Some(b'>'), _) => (TokenKind::Shr, 2),
            (b'&', ..) => (TokenKind::Amp, 1),
            (b'|', ..) => (TokenKind::Pipe, 1),
            (b'^', ..) => (TokenKind::Caret, 1),
            (b'~', ..) => (TokenKind::Tilde, 1),
            (b'(', ..) => (TokenKind::LParen, 1),
            (b')', ..) => (TokenKind::RParen, 1),
            (b'{', ..) => (TokenKind::LBrace, 1),
            (b'}', ..) => (TokenKind::RBrace, 1),
            (b'[', ..) => (TokenKind::LBracket, 1),
            (b']', ..) => (TokenKind::RBracket, 1),
            (b';', ..) => (TokenKind::Semi, 1),
            (b',', ..) => (TokenKind::Comma, 1),
            (b':', ..) => (TokenKind::Colon, 1),
            (b'?', ..) => (TokenKind::Question, 1),
            (b'=', ..) => (TokenKind::Assign, 1),
            (b'+', ..) => (TokenKind::Plus, 1),
            (b'-', ..) => (TokenKind::Minus, 1),
            (b'*', ..) => (TokenKind::Star, 1),
            (b'/', ..) => (TokenKind::Slash, 1),
            (b'%', ..) => (TokenKind::Percent, 1),
            (b'<', ..) => (TokenKind::Lt, 1),
            (b'>', ..) => (TokenKind::Gt, 1),
            (b'!', ..) => (TokenKind::Bang, 1),
            (b'.', Some(b'.'), Some(b'.')) => (TokenKind::DotDotDot, 3),
            (b'.', ..) => (TokenKind::Dot, 1),
            _ => {
                return Err(ParseError::new(
                    format!(
                        "unexpected character '{}'",
                        source[pos..].chars().next().unwrap_or('\0')
                    ),
                    Span::new(pos as u32, pos as u32 + 1),
                ));
            }
        };
        tokens.push(Token::new(kind, Span::new(pos as u32, (pos + len) as u32)));
        pos += len;
    }
    tokens.push(Token::new(
        TokenKind::Eof,
        Span::new(bytes.len() as u32, bytes.len() as u32),
    ));
    Ok(tokens)
}

fn lex_string(source: &str, start: usize, quote: u8) -> Result<(Token, usize), ParseError> {
    let bytes = source.as_bytes();
    let mut pos = start + 1;
    let mut value = String::new();
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte == quote {
            return Ok((
                Token::new(
                    TokenKind::Str(value),
                    Span::new(start as u32, (pos + 1) as u32),
                ),
                pos + 1,
            ));
        }
        if byte == b'\\' {
            pos += 1;
            let Some(escaped) = source[pos..].chars().next() else {
                break;
            };
            pos += escaped.len_utf8();
            match escaped {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                '\'' => value.push('\''),
                '0' => value.push('\0'),
                'x' => {
                    if let Some(digits) = source[pos..].get(..2)
                        && let Ok(code) = u32::from_str_radix(digits, 16)
                        && let Some(ch) = char::from_u32(code)
                    {
                        value.push(ch);
                        pos += 2;
                    }
                }
                'u' => {
                    if source[pos..].starts_with('{') {
                        if let Some(end) = source[pos + 1..].find('}') {
                            let digits = &source[pos + 1..pos + 1 + end];
                            if let Ok(code) = u32::from_str_radix(digits, 16)
                                && let Some(ch) = char::from_u32(code)
                            {
                                value.push(ch);
                            }
                            pos += end + 2;
                        }
                    } else if let Some(digits) = source[pos..].get(..4)
                        && let Ok(code) = u32::from_str_radix(digits, 16)
                        && let Some(ch) = char::from_u32(code)
                    {
                        value.push(ch);
                        pos += 4;
                    }
                }
                '\r' => {
                    if source[pos..].starts_with('\n') {
                        pos += 1;
                    }
                }
                '\n' => {}
                other => value.push(other),
            }
            continue;
        }
        let ch = source[pos..].chars().next().ok_or_else(|| {
            ParseError::new(
                "invalid string literal",
                Span::new(start as u32, pos as u32),
            )
        })?;
        value.push(ch);
        pos += ch.len_utf8();
    }
    Err(ParseError::new(
        "unterminated string literal",
        Span::new(start as u32, bytes.len() as u32),
    ))
}

fn keyword_or_ident(text: &str) -> TokenKind {
    match text {
        "var" => TokenKind::Var,
        "let" => TokenKind::Let,
        "const" => TokenKind::Const,
        "function" => TokenKind::Function,
        "class" => TokenKind::Class,
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "while" => TokenKind::While,
        "do" => TokenKind::Do,
        "for" => TokenKind::For,
        "return" => TokenKind::Return,
        "throw" => TokenKind::Throw,
        "switch" => TokenKind::Switch,
        "case" => TokenKind::Case,
        "default" => TokenKind::Default,
        "try" => TokenKind::Try,
        "catch" => TokenKind::Catch,
        "finally" => TokenKind::Finally,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "null" => TokenKind::Null,
        "new" => TokenKind::New,
        "undefined" => TokenKind::Undefined,
        "this" => TokenKind::This,
        "super" => TokenKind::Super,
        "type" => TokenKind::Type,
        "interface" => TokenKind::Interface,
        "as" => TokenKind::As,
        "instanceof" => TokenKind::InstanceOf,
        "in" => TokenKind::In,
        "typeof" => TokenKind::TypeOf,
        "void" => TokenKind::Void,
        "delete" => TokenKind::Delete,
        _ => TokenKind::Ident(text.to_owned()),
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn span(start: usize, end: usize) -> Span {
    Span::new(start as u32, end as u32)
}

//! Tokens for the OpenCL C formula subset, after a minimal preprocessor.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TokenKind {
    Ident(String),
    Number { value: f64, float: bool },
    Punct(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

/// Longest punctuators first, so `<<=` wins over `<<` and `<`.
const PUNCTUATORS: [&str; 47] = [
    "<<=", ">>=", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=",
    "*=", "/=", "%=", "&=", "|=", "^=", "(", ")", "{", "}", "[", "]", ";", ",", ".", "?", ":", "+",
    "-", "*", "/", "%", "<", ">", "=", "!", "~", "&", "|", "^", "#", "\\",
];

/// Macros the formula sources test: none are defined (single precision).
const DEFINED_MACROS: [&str; 0] = [];

pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let text = preprocess(source)?;
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else if text[i..].starts_with("//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if text[i..].starts_with("/*") {
            let end = text[i + 2..]
                .find("*/")
                .ok_or_else(|| format!("line {line}: unterminated comment"))?;
            line += text[i..i + 2 + end].matches('\n').count();
            i += end + 4;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident(text[start..i].to_owned()),
                line,
            });
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let (kind, length) =
                number(&text[i..]).map_err(|error| format!("line {line}: {error}"))?;
            tokens.push(Token { kind, line });
            i += length;
        } else {
            let punct = PUNCTUATORS
                .iter()
                .find(|p| text[i..].starts_with(**p))
                .ok_or_else(|| format!("line {line}: unexpected character {:?}", c as char))?;
            tokens.push(Token {
                kind: TokenKind::Punct(punct),
                line,
            });
            i += punct.len();
        }
    }
    Ok(tokens)
}

/// Numeric literal at the start of `text`: decimal or hex integers (with
/// `u`/`U` suffixes) and floats with optional exponent and `f`/`F` suffix.
fn number(text: &str) -> Result<(TokenKind, usize), String> {
    let bytes = text.as_bytes();
    if text.starts_with("0x") || text.starts_with("0X") {
        let digits = text[2..].bytes().take_while(u8::is_ascii_hexdigit).count();
        let value = u64::from_str_radix(&text[2..2 + digits], 16).map_err(|e| e.to_string())?;
        let mut length = 2 + digits;
        while bytes
            .get(length)
            .is_some_and(|b| matches!(b, b'u' | b'U' | b'l' | b'L'))
        {
            length += 1;
        }
        return Ok((
            TokenKind::Number {
                value: value as f64,
                float: false,
            },
            length,
        ));
    }
    let mut length = 0;
    let mut float = false;
    while bytes.get(length).is_some_and(u8::is_ascii_digit) {
        length += 1;
    }
    if bytes.get(length) == Some(&b'.') {
        float = true;
        length += 1;
        while bytes.get(length).is_some_and(u8::is_ascii_digit) {
            length += 1;
        }
    }
    if bytes.get(length).is_some_and(|b| matches!(b, b'e' | b'E')) {
        let mut exponent = length + 1;
        if bytes
            .get(exponent)
            .is_some_and(|b| matches!(b, b'+' | b'-'))
        {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            float = true;
            length = exponent;
            while bytes.get(length).is_some_and(u8::is_ascii_digit) {
                length += 1;
            }
        }
    }
    let value: f64 = text[..length]
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if bytes.get(length).is_some_and(|b| matches!(b, b'f' | b'F')) {
        float = true;
        length += 1;
    }
    while bytes
        .get(length)
        .is_some_and(|b| matches!(b, b'u' | b'U' | b'l' | b'L'))
    {
        length += 1;
    }
    Ok((TokenKind::Number { value, float }, length))
}

/// Resolves `#ifdef`/`#ifndef`/`#else`/`#endif` against `DEFINED_MACROS` and
/// blanks every directive line, keeping line numbers.
fn preprocess(source: &str) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    let mut active = vec![true];
    for (number, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(directive) = trimmed.strip_prefix('#') {
            let mut words = directive.split_whitespace();
            let keyword = words.next().unwrap_or("");
            let name = words.next().unwrap_or("");
            let enclosing = *active.last().unwrap_or(&true);
            match keyword {
                "ifdef" => active.push(enclosing && DEFINED_MACROS.contains(&name)),
                "ifndef" => active.push(enclosing && !DEFINED_MACROS.contains(&name)),
                "else" => {
                    let current = active
                        .pop()
                        .ok_or_else(|| format!("line {}: #else without #if", number + 1))?;
                    let parent = *active.last().unwrap_or(&true);
                    active.push(parent && !current);
                }
                "endif" => {
                    active.pop();
                    if active.is_empty() {
                        return Err(format!("line {}: #endif without #if", number + 1));
                    }
                }
                other => {
                    return Err(format!(
                        "line {}: unsupported directive #{other}",
                        number + 1
                    ));
                }
            }
        } else if *active.last().unwrap_or(&true) {
            output.push_str(line);
        }
        output.push('\n');
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn numbers_keep_float_suffixes_and_exponents() {
        assert_eq!(
            kinds("1.0f 2 1e-3f 0.5 3u .25"),
            vec![
                TokenKind::Number {
                    value: 1.0,
                    float: true
                },
                TokenKind::Number {
                    value: 2.0,
                    float: false
                },
                TokenKind::Number {
                    value: 1e-3,
                    float: true
                },
                TokenKind::Number {
                    value: 0.5,
                    float: true
                },
                TokenKind::Number {
                    value: 3.0,
                    float: false
                },
                TokenKind::Number {
                    value: 0.25,
                    float: true
                },
            ]
        );
    }

    #[test]
    fn longest_punctuator_wins_and_comments_vanish() {
        assert_eq!(
            kinds("a->b /* x */ c <<= 1 // y"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Punct("->"),
                TokenKind::Ident("b".into()),
                TokenKind::Ident("c".into()),
                TokenKind::Punct("<<="),
                TokenKind::Number {
                    value: 1.0,
                    float: false
                },
            ]
        );
    }

    #[test]
    fn single_precision_branch_survives_preprocessing() {
        let source = "#ifndef DOUBLE_PRECISION\nfloat a;\n#else\ndouble a;\n#endif\n";
        assert_eq!(
            kinds(source),
            vec![
                TokenKind::Ident("float".into()),
                TokenKind::Ident("a".into()),
                TokenKind::Punct(";"),
            ]
        );
    }
}

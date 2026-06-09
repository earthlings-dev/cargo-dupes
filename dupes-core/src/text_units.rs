use std::path::Path;

use crate::code_unit::{CodeUnit, CodeUnitKind, DetectionDimension};
use crate::config::Config;
use crate::fingerprint::Fingerprint;
use crate::node::{NodeKind, NormalizedNode};

/// Generic token and line units extracted from a text-like file.
#[derive(Debug, Default)]
pub struct TextUnits {
    /// Identifier/literal-normalized token windows.
    pub normalized_tokens: Vec<CodeUnit>,
    /// Raw token windows.
    pub raw_tokens: Vec<CodeUnit>,
    /// Normalized line windows.
    pub lines: Vec<CodeUnit>,
}

/// Extract generic token and line units from a source file.
#[must_use]
pub fn extract(path: &Path, source: &str, config: &Config) -> TextUnits {
    let tokens = tokenize(source);
    let normalized_tokens = if config.dimension_enabled(DetectionDimension::TokenNormalized) {
        token_windows(
            path,
            &tokens,
            config.token_min_tokens,
            TokenMode::Normalized,
        )
    } else {
        Vec::new()
    };
    let raw_tokens = if config.dimension_enabled(DetectionDimension::TokenRaw) {
        token_windows(path, &tokens, config.token_min_tokens, TokenMode::Raw)
    } else {
        Vec::new()
    };
    let lines = if config.dimension_enabled(DetectionDimension::Line) {
        line_windows(path, source, config.line_min_lines)
    } else {
        Vec::new()
    };
    TextUnits {
        normalized_tokens,
        raw_tokens,
        lines,
    }
}

/// Token representation with source line information.
#[derive(Debug, Clone)]
struct Token {
    /// Whitespace-insensitive source token.
    raw: String,
    /// Identifier/literal-normalized token.
    normalized: String,
    /// One-based source line.
    line: usize,
}

/// Token window extraction mode.
#[derive(Debug, Clone, Copy)]
enum TokenMode {
    /// Use normalized tokens.
    Normalized,
    /// Use raw tokens.
    Raw,
}

/// Convert source into coarse language-agnostic tokens.
fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    let mut line = 1_usize;

    while let Some((_, ch)) = chars.next() {
        if ch == '\n' {
            line += 1;
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }

        if is_ident_start(ch) {
            let mut raw = String::from(ch);
            while let Some((_, next)) = chars.peek().copied() {
                if is_ident_continue(next) {
                    raw.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            let normalized = if is_keyword(&raw) {
                raw.clone()
            } else {
                "IDENT".to_string()
            };
            tokens.push(Token {
                raw,
                normalized,
                line,
            });
            continue;
        }

        if ch.is_ascii_digit() {
            let mut raw = String::from(ch);
            while let Some((_, next)) = chars.peek().copied() {
                if next.is_ascii_alphanumeric() || matches!(next, '_' | '.') {
                    raw.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                raw,
                normalized: "NUMBER".to_string(),
                line,
            });
            continue;
        }

        if matches!(ch, '"' | '\'' | '`') {
            let quote = ch;
            let mut raw = String::from(ch);
            let mut escaped = false;
            for (_, next) in chars.by_ref() {
                raw.push(next);
                if next == '\n' {
                    line += 1;
                }
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == quote {
                    break;
                }
            }
            tokens.push(Token {
                raw,
                normalized: "STRING".to_string(),
                line,
            });
            continue;
        }

        let raw = String::from(ch);
        tokens.push(Token {
            raw: raw.clone(),
            normalized: raw,
            line,
        });
    }

    tokens
}

/// Return true if `ch` can start an identifier-like token.
const fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

/// Return true if `ch` can continue an identifier-like token.
const fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch == '-' || ch.is_ascii_alphanumeric()
}

/// Keep common language keywords structurally meaningful in normalized tokens.
fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "as" | "async"
            | "await"
            | "break"
            | "class"
            | "const"
            | "continue"
            | "def"
            | "else"
            | "enum"
            | "false"
            | "fn"
            | "for"
            | "from"
            | "if"
            | "impl"
            | "import"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "mut"
            | "pub"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "type"
            | "use"
            | "where"
            | "while"
            | "yield"
    )
}

/// Build token windows.
fn token_windows(
    path: &Path,
    tokens: &[Token],
    min_tokens: usize,
    mode: TokenMode,
) -> Vec<CodeUnit> {
    if min_tokens == 0 || tokens.len() < min_tokens {
        return Vec::new();
    }
    let stride = min_tokens.div_ceil(2).max(1);
    let mut units = Vec::new();
    let mut start = 0_usize;
    while start + min_tokens <= tokens.len() {
        let end = start + min_tokens;
        let slice = &tokens[start..end];
        let values: Vec<String> = slice
            .iter()
            .map(|token| match mode {
                TokenMode::Normalized => token.normalized.clone(),
                TokenMode::Raw => token.raw.clone(),
            })
            .collect();
        units.push(window_unit(
            path,
            match mode {
                TokenMode::Normalized => "normalized token window",
                TokenMode::Raw => "raw token window",
            },
            CodeUnitKind::TokenWindow,
            slice.first().map_or(1, |token| token.line),
            slice.last().map_or(1, |token| token.line),
            &values,
        ));
        start += stride;
    }
    units
}

/// Build normalized line windows.
fn line_windows(path: &Path, source: &str, min_lines: usize) -> Vec<CodeUnit> {
    if min_lines == 0 {
        return Vec::new();
    }
    let normalized: Vec<(usize, String)> = source
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if trimmed.is_empty() {
                None
            } else {
                Some((idx + 1, trimmed))
            }
        })
        .collect();
    if normalized.len() < min_lines {
        return Vec::new();
    }
    let mut units = Vec::new();
    for start in 0..=(normalized.len() - min_lines) {
        let end = start + min_lines;
        let slice = &normalized[start..end];
        let values: Vec<String> = slice.iter().map(|(_, line)| line.clone()).collect();
        units.push(window_unit(
            path,
            "line window",
            CodeUnitKind::LineWindow,
            slice.first().map_or(1, |(line, _)| *line),
            slice.last().map_or(1, |(line, _)| *line),
            &values,
        ));
    }
    units
}

/// Build a code unit from generic text values.
fn window_unit(
    path: &Path,
    name: &str,
    kind: CodeUnitKind,
    line_start: usize,
    line_end: usize,
    values: &[String],
) -> CodeUnit {
    let body = NormalizedNode::with_children(
        NodeKind::Block,
        values
            .iter()
            .map(|value| NormalizedNode::leaf(NodeKind::Token(value.clone())))
            .collect(),
    );
    CodeUnit {
        kind,
        name: name.to_string(),
        file: path.to_path_buf(),
        line_start,
        line_end,
        signature: NormalizedNode::leaf(NodeKind::Opaque),
        fingerprint: Fingerprint::from_node(&body),
        node_count: values.len(),
        body,
        parent_name: None,
        is_test: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_tokens_ignore_identifier_names() {
        let first = tokenize("fn alpha(x: i32) { let beta = x + 1; }");
        let second = tokenize("fn gamma(y: i32) { let delta = y + 1; }");
        let first_norm: Vec<_> = first
            .iter()
            .map(|token| token.normalized.as_str())
            .collect();
        let second_norm: Vec<_> = second
            .iter()
            .map(|token| token.normalized.as_str())
            .collect();
        assert_eq!(first_norm, second_norm);
    }

    #[test]
    fn token_windows_use_source_lines() {
        let config = Config {
            token_min_tokens: 4,
            ..Config::default()
        };
        let units = extract(Path::new("sample.rs"), "fn a() {\nlet x = 1;\n}", &config);
        assert!(!units.normalized_tokens.is_empty());
        assert!(units.normalized_tokens[0].line_end >= units.normalized_tokens[0].line_start);
    }

    #[test]
    fn line_windows_normalize_whitespace() {
        let config = Config {
            line_min_lines: 2,
            ..Config::default()
        };
        let units = extract(Path::new("README.md"), "a   b\n\n c d \n", &config);
        assert_eq!(units.lines.len(), 1);
        assert_eq!(units.lines[0].line_start, 1);
        assert_eq!(units.lines[0].line_end, 3);
    }
}
